//! Claude Code 本地协议转换代理（与 cc-switch 的本地路由同思路）
//!
//! Claude Code CLI 只会说 Anthropic Messages 协议。要接入仅提供
//! OpenAI Chat Completions 接口的上游（DeepSeek 官方、Kimi、OneAPI 等），
//! 需要一层本地代理做双向翻译：
//!
//! - 常驻监听 `127.0.0.1:25789`（仅本机回环，不对外暴露）
//! - 切换到 `apiFormat = "openai_chat"` 的 Claude 配置时，
//!   `ANTHROPIC_BASE_URL` 指向本代理，真实上游写入代理全局状态
//! - 收到 `/v1/messages` 请求后：Anthropic 请求体 → OpenAI 请求体，
//!   转发到上游 `{base_url}/chat/completions`，再把响应
//!   （含流式 SSE、工具调用、思考内容）翻译回 Anthropic 格式
//!
//! 本地路由增强（对标 cc-switch）：支持备用上游池、自动故障转移与
//! 每上游独立的熔断器。请求候选序列 = [主上游] + 备用池（按序）；
//! 在尚未向客户端写回任何字节前，连接失败 / 5xx / 429 会自动换下一个
//! 候选重试，4xx 视为配置问题直接翻译回客户端。SSE 一旦开始转发即
//! 不再切换。健康统计见 claude_proxy_health / claude_proxy_reset_breaker。
//!
//! 限制：VarSwitch 退出后代理停止，Claude Code 将无法连接；
//! 切回 anthropic 直连配置即可脱离代理。

use serde_json::{json, Map, Value};
use sha1::{Digest, Sha1};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

pub const CLAUDE_PROXY_PORT: u16 = 25789;

/// 代理转发目标。model 非空时会重写请求里的模型名
/// （Claude Code 发来的是 claude-* 系列名，上游不认识）。
#[derive(Clone, Default)]
pub struct ProxyUpstream {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

/// 上游说什么协议，决定代理是「翻译」还是「透传」。
///
/// - `OpenAiChat`：上游只有 OpenAI Chat Completions 接口，需双向翻译协议。
/// - `Anthropic`：上游本身就是 Anthropic Messages 端点，原样透传即可。
///   透传模式让直连中转站也能用上故障转移与熔断，且切换上游时终端无感
///   （`ANTHROPIC_BASE_URL` 始终指向本地代理，换配置只改代理内部指向）。
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum UpstreamMode {
    #[default]
    OpenAiChat,
    Anthropic,
}

impl UpstreamMode {
    pub fn as_str(self) -> &'static str {
        match self {
            UpstreamMode::OpenAiChat => "openai_chat",
            UpstreamMode::Anthropic => "anthropic",
        }
    }

    /// 与 Profile.api_format 的取值对应；未知值按翻译模式处理（与旧行为一致）
    pub fn from_api_format(raw: &str) -> Self {
        match raw.trim() {
            "anthropic" => UpstreamMode::Anthropic,
            _ => UpstreamMode::OpenAiChat,
        }
    }
}

/// 一个候选转发目标：上游地址 + 它说的协议
#[derive(Clone, Default)]
pub struct ProxyTarget {
    pub upstream: ProxyUpstream,
    pub mode: UpstreamMode,
}

static UPSTREAM: OnceLock<Mutex<Option<ProxyTarget>>> = OnceLock::new();
static SERVER_RUNNING: OnceLock<Mutex<bool>> = OnceLock::new();

fn upstream_cell() -> &'static Mutex<Option<ProxyTarget>> {
    UPSTREAM.get_or_init(|| Mutex::new(None))
}

/// 设置主上游（协议翻译模式）。保留此签名供既有调用点使用。
pub fn set_upstream(upstream: Option<ProxyUpstream>) {
    set_upstream_with_mode(upstream, UpstreamMode::OpenAiChat);
}

/// 设置主上游并指定协议模式
pub fn set_upstream_with_mode(upstream: Option<ProxyUpstream>, mode: UpstreamMode) {
    if let Ok(mut guard) = upstream_cell().lock() {
        *guard = upstream.map(|upstream| ProxyTarget { upstream, mode });
    }
    // 配置切换（热更新）：全量重置熔断与统计，避免旧上游的熔断状态
    // 误伤新配置。按「简单可靠优先」不做增量清理。
    reset_health_state();
}

pub fn current_upstream() -> Option<ProxyUpstream> {
    current_target().map(|target| target.upstream)
}

pub fn current_target() -> Option<ProxyTarget> {
    upstream_cell().lock().ok().and_then(|guard| guard.clone())
}

pub fn is_running() -> bool {
    SERVER_RUNNING
        .get()
        .and_then(|cell| cell.lock().ok().map(|guard| *guard))
        .unwrap_or(false)
}

pub fn local_base_url() -> String {
    format!("http://127.0.0.1:{CLAUDE_PROXY_PORT}")
}

/// 启动代理服务器（幂等）。端口被占用等监听失败时返回 Err。
pub fn ensure_server() -> Result<(), String> {
    let running = SERVER_RUNNING.get_or_init(|| Mutex::new(false));
    let mut guard = running
        .lock()
        .map_err(|_| "代理状态锁获取失败".to_string())?;
    if *guard {
        return Ok(());
    }
    let server = tiny_http::Server::http(("127.0.0.1", CLAUDE_PROXY_PORT)).map_err(|e| {
        format!("本地代理监听 127.0.0.1:{CLAUDE_PROXY_PORT} 失败（端口可能被占用）：{e}")
    })?;
    std::thread::spawn(move || {
        for request in server.incoming_requests() {
            // LLM 请求持续时间长，逐请求开线程保证并发
            std::thread::spawn(move || handle_request(request));
        }
    });
    *guard = true;
    crate::app_log(
        "INFO",
        &format!("[claude-proxy] 本地协议转换代理已启动 127.0.0.1:{CLAUDE_PROXY_PORT}"),
    );
    Ok(())
}

// ── 备用上游池 / 熔断器 / 健康统计 ────────────────────

/// 连续失败达到该次数后熔断该上游（open）
const BREAKER_FAILURE_THRESHOLD: u32 = 3;
/// 熔断开启时长（秒）；到期进入 half-open，放行一个探测请求
const BREAKER_OPEN_SECS: u64 = 60;
/// last_error 保留的最大字符数
const LAST_ERROR_MAX_CHARS: usize = 200;

/// 熔断器状态机：closed（正常）→ 连续失败 ≥ 3 次 open（60 秒内跳过该上游）
/// → 到期 half-open（放行一个探测请求）→ 成功回 closed / 失败重回 open
#[derive(Clone, Copy, PartialEq)]
enum BreakerState {
    Closed,
    Open,
    HalfOpen,
}

impl BreakerState {
    // 仅被暂未注册的 claude_proxy_health 使用，集成后可移除 allow
    #[allow(dead_code)]
    fn as_str(self) -> &'static str {
        match self {
            BreakerState::Closed => "closed",
            BreakerState::Open => "open",
            BreakerState::HalfOpen => "half_open",
        }
    }
}

/// 单个上游的熔断状态与健康统计（键 = base_url + api_key 的 SHA1）
struct UpstreamHealth {
    state: BreakerState,
    consecutive_failures: u32,
    total_requests: u64,
    total_failures: u64,
    last_error: Option<String>,
    last_error_ts: Option<u64>,
    last_success_ts: Option<u64>,
    /// 进入 open 的时刻，或 half-open 放行探测的时刻（驱动 60 秒窗口）
    state_changed_at: u64,
}

impl UpstreamHealth {
    fn new() -> Self {
        UpstreamHealth {
            state: BreakerState::Closed,
            consecutive_failures: 0,
            total_requests: 0,
            total_failures: 0,
            last_error: None,
            last_error_ts: None,
            last_success_ts: None,
            state_changed_at: 0,
        }
    }
}

static FAILOVER_POOL: OnceLock<Mutex<Vec<ProxyTarget>>> = OnceLock::new();
static HEALTH_MAP: OnceLock<Mutex<HashMap<String, UpstreamHealth>>> = OnceLock::new();
/// 自动转移累计次数：请求被实际发往备用上游（非首选候选）的次数
static FAILOVER_COUNT: AtomicU64 = AtomicU64::new(0);

fn failover_pool_cell() -> &'static Mutex<Vec<ProxyTarget>> {
    FAILOVER_POOL.get_or_init(|| Mutex::new(Vec::new()))
}

fn health_map() -> &'static Mutex<HashMap<String, UpstreamHealth>> {
    HEALTH_MAP.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 设置备用上游池（有序，优先级从高到低），每项自带协议模式。
/// 请求处理时的候选序列 = [主上游] + 备用池；池为空时行为与单上游一致。
pub fn set_failover_targets(targets: Vec<ProxyTarget>) {
    if let Ok(mut guard) = failover_pool_cell().lock() {
        *guard = targets;
    }
    // 与 set_upstream 同理：配置变更即全量重置熔断/统计
    reset_health_state();
}

/// 当前备用上游池快照（含协议模式）
pub fn failover_targets() -> Vec<ProxyTarget> {
    failover_pool_cell()
        .lock()
        .map(|guard| guard.clone())
        .unwrap_or_default()
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 熔断键：base_url + api_key 的 SHA1（同地址不同 key 视为不同上游）
fn upstream_key(upstream: &ProxyUpstream) -> String {
    let mut hasher = Sha1::new();
    hasher.update(upstream.base_url.trim_end_matches('/').as_bytes());
    hasher.update(b"\n");
    hasher.update(upstream.api_key.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// 检查熔断器是否放行该上游，可能触发 open → half-open 转换。
/// 锁获取失败时放行——宁可请求失败也不能让代理无路可走。
fn breaker_allows(key: &str) -> bool {
    let now = unix_now();
    let Ok(mut map) = health_map().lock() else {
        return true;
    };
    let entry = map
        .entry(key.to_string())
        .or_insert_with(UpstreamHealth::new);
    match entry.state {
        BreakerState::Closed => true,
        BreakerState::Open => {
            if now.saturating_sub(entry.state_changed_at) >= BREAKER_OPEN_SECS {
                // 熔断到期 → half-open，放行本请求作为探测；
                // 同步刷新时间戳，避免并发请求同时放行多个探测
                entry.state = BreakerState::HalfOpen;
                entry.state_changed_at = now;
                true
            } else {
                false
            }
        }
        BreakerState::HalfOpen => {
            // 已有探测在途时不再放行；若探测悬挂超过一个窗口
            //（LLM 请求可能极慢），再放行一个新探测兜底
            if now.saturating_sub(entry.state_changed_at) >= BREAKER_OPEN_SECS {
                entry.state_changed_at = now;
                true
            } else {
                false
            }
        }
    }
}

/// 记录一次成功请求（以上游返回 2xx 为判据）：闭合熔断并清零连续失败
fn record_success(key: &str) {
    let now = unix_now();
    if let Ok(mut map) = health_map().lock() {
        let entry = map
            .entry(key.to_string())
            .or_insert_with(UpstreamHealth::new);
        entry.state = BreakerState::Closed;
        entry.consecutive_failures = 0;
        entry.total_requests += 1;
        entry.last_success_ts = Some(now);
    }
}

/// 记录一次可转移失败（连接失败 / 5xx / 429）并推进熔断状态机
fn record_failure(key: &str, error: &str) {
    let now = unix_now();
    if let Ok(mut map) = health_map().lock() {
        let entry = map
            .entry(key.to_string())
            .or_insert_with(UpstreamHealth::new);
        entry.total_requests += 1;
        entry.total_failures += 1;
        entry.consecutive_failures += 1;
        entry.last_error = Some(error.chars().take(LAST_ERROR_MAX_CHARS).collect());
        entry.last_error_ts = Some(now);
        // half-open 探测失败 → 立即重新熔断；连续失败达阈值 → 熔断
        if entry.state == BreakerState::HalfOpen
            || entry.consecutive_failures >= BREAKER_FAILURE_THRESHOLD
        {
            entry.state = BreakerState::Open;
            entry.state_changed_at = now;
        }
    }
}

/// 记录一次 4xx 配置类错误：计入统计与 last_error，但不推进熔断
///（上游本身可达，熔断/转移解决不了鉴权或参数问题）
fn record_config_error(key: &str, error: &str) {
    let now = unix_now();
    if let Ok(mut map) = health_map().lock() {
        let entry = map
            .entry(key.to_string())
            .or_insert_with(UpstreamHealth::new);
        entry.total_requests += 1;
        entry.total_failures += 1;
        entry.last_error = Some(error.chars().take(LAST_ERROR_MAX_CHARS).collect());
        entry.last_error_ts = Some(now);
        // half-open 探测收到 4xx 说明链路已恢复，视为探测通过
        if entry.state == BreakerState::HalfOpen {
            entry.state = BreakerState::Closed;
            entry.consecutive_failures = 0;
        }
    }
}

/// 清空全部熔断/健康统计状态（配置热更新与手动重置共用）
fn reset_health_state() {
    if let Ok(mut map) = health_map().lock() {
        map.clear();
    }
    FAILOVER_COUNT.store(0, Ordering::Relaxed);
}

/// api_key 脱敏：只留前 6 后 4；长度不足时全部打码，避免变相泄露
// 仅被暂未注册的 claude_proxy_health 使用，集成后可移除 allow
#[allow(dead_code)]
fn mask_api_key(key: &str) -> String {
    if key.is_empty() {
        return String::new();
    }
    let chars: Vec<char> = key.chars().collect();
    if chars.len() <= 10 {
        return "****".to_string();
    }
    let head: String = chars[..6].iter().collect();
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{head}****{tail}")
}

/// 上游脱敏摘要（健康面板展示用）
fn masked_target(target: &ProxyTarget) -> Value {
    json!({
        "baseUrl": target.upstream.base_url,
        "model": target.upstream.model,
        "apiKey": mask_api_key(&target.upstream.api_key),
        "apiFormat": target.mode.as_str(),
    })
}

/// 单个上游的健康统计条目（尚无记录时返回全零默认值）
fn health_entry(target: &ProxyTarget, role: &str) -> Value {
    let key = upstream_key(&target.upstream);
    let map = health_map().lock().ok();
    let stats = map.as_ref().and_then(|m| m.get(&key));
    json!({
        "role": role,
        "baseUrl": target.upstream.base_url,
        "model": target.upstream.model,
        "apiFormat": target.mode.as_str(),
        "state": stats.map(|s| s.state.as_str()).unwrap_or("closed"),
        "consecutiveFailures": stats.map(|s| s.consecutive_failures).unwrap_or(0),
        "totalRequests": stats.map(|s| s.total_requests).unwrap_or(0),
        "totalFailures": stats.map(|s| s.total_failures).unwrap_or(0),
        "lastError": stats.and_then(|s| s.last_error.clone()),
        "lastErrorTs": stats.and_then(|s| s.last_error_ts),
        "lastSuccessTs": stats.and_then(|s| s.last_success_ts),
    })
}

/// 健康监控快照：运行状态、主/备上游（脱敏）、每上游熔断统计、
/// 自动转移次数。字段名 camelCase。命令暂未注册，由后续集成接线。
#[allow(dead_code)]
#[tauri::command]
pub fn claude_proxy_health() -> serde_json::Value {
    let primary = current_target();
    let pool = failover_targets();
    let mut health: Vec<Value> = Vec::new();
    if let Some(p) = primary.as_ref() {
        health.push(health_entry(p, "primary"));
    }
    for target in &pool {
        health.push(health_entry(target, "failover"));
    }
    json!({
        "running": is_running(),
        "port": CLAUDE_PROXY_PORT,
        "upstream": primary.as_ref().map(masked_target),
        "failoverPool": pool.iter().map(masked_target).collect::<Vec<Value>>(),
        "health": health,
        "failoverCount": FAILOVER_COUNT.load(Ordering::Relaxed),
    })
}

/// 手动清空所有熔断/统计状态（上游恢复后想立即重试时用）
#[allow(dead_code)]
#[tauri::command]
pub fn claude_proxy_reset_breaker() {
    reset_health_state();
    crate::app_log("INFO", "[claude-proxy] 熔断器与健康统计已手动重置");
}

// ── HTTP 处理 ─────────────────────────────────────────

fn json_content_type() -> tiny_http::Header {
    tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
        .expect("static header")
}

fn anthropic_error_body(error_type: &str, message: &str) -> String {
    json!({"type": "error", "error": {"type": error_type, "message": message}}).to_string()
}

pub(crate) fn respond_error(
    request: tiny_http::Request,
    status: u16,
    error_type: &str,
    message: &str,
) {
    let response = tiny_http::Response::from_string(anthropic_error_body(error_type, message))
        .with_status_code(status)
        .with_header(json_content_type());
    let _ = request.respond(response);
}

pub(crate) fn respond_json(request: tiny_http::Request, status: u16, body: String) {
    let response = tiny_http::Response::from_string(body)
        .with_status_code(status)
        .with_header(json_content_type());
    let _ = request.respond(response);
}

pub(crate) fn read_body(request: &mut tiny_http::Request) -> Result<Value, String> {
    let mut body = String::new();
    request
        .as_reader()
        .read_to_string(&mut body)
        .map_err(|e| format!("读取请求体失败：{e}"))?;
    serde_json::from_str(&body).map_err(|e| format!("请求体不是有效 JSON：{e}"))
}

fn handle_request(request: tiny_http::Request) {
    let path = request
        .url()
        .split('?')
        .next()
        .unwrap_or_default()
        .to_string();
    if path.starts_with("/claude-desktop/") {
        crate::claude_desktop_gateway::handle_request(request);
        return;
    }
    if *request.method() != tiny_http::Method::Post {
        respond_error(
            request,
            404,
            "not_found_error",
            "VarSwitch Claude 代理仅支持 POST /v1/messages",
        );
        return;
    }
    match path.as_str() {
        "/v1/messages" => handle_messages(request),
        "/v1/messages/count_tokens" => handle_count_tokens(request),
        _ => respond_error(
            request,
            404,
            "not_found_error",
            &format!("未知路径：{path}"),
        ),
    }
}

/// Claude Code 用 count_tokens 做上下文估算。
/// 透传模式下上游有同名接口，直接转发拿精确值；翻译模式下上游没有等价接口，
/// 按「JSON 字符数 / 4」给一个数量级正确的估算。两种情况都不影响主链路。
fn handle_count_tokens(mut request: tiny_http::Request) {
    let mut body = String::new();
    let _ = request.as_reader().read_to_string(&mut body);

    if let Some(target) = current_target() {
        if target.mode == UpstreamMode::Anthropic && !target.upstream.base_url.trim().is_empty() {
            let base = target.upstream.base_url.trim_end_matches('/');
            let sent = proxy_client()
                .post(format!("{base}/v1/messages/count_tokens"))
                .header("x-api-key", target.upstream.api_key.as_str())
                .header("anthropic-version", "2023-06-01")
                .header("Content-Type", "application/json")
                .body(body.clone())
                .send();
            if let Ok(resp) = sent {
                let status = resp.status().as_u16();
                if status < 400 {
                    if let Ok(text) = resp.text() {
                        respond_json(request, 200, text);
                        return;
                    }
                }
            }
            // 上游不支持或临时失败：回落到本地估算，不让上下文统计阻断主链路
        }
    }

    let approx = (body.chars().count() / 4).max(1);
    respond_json(request, 200, json!({"input_tokens": approx}).to_string());
}

/// 单次上游尝试的结果分类
pub(crate) enum AttemptOutcome {
    /// 2xx：拿到可转发的上游响应
    Success(reqwest::blocking::Response),
    /// 可转移失败（TCP 连接失败/超时、5xx、429）：记熔断并尝试下一候选
    Retryable(u16, String),
    /// 4xx 配置类错误（401/403/404 等）：不转移，直接翻译回客户端
    ClientError(u16, String),
}

/// 覆写请求体里的模型名（上游不认识 Claude Code 发来的 claude-* 名字时用）
fn rewrite_model(body: &Value, model: &str) -> Value {
    let mut out = body.clone();
    if !model.trim().is_empty() {
        if let Some(map) = out.as_object_mut() {
            map.insert("model".to_string(), json!(model));
        }
    }
    out
}

/// 向单个上游发起一次转发尝试。此阶段尚未向客户端写回任何字节，
/// 因此失败可以安全地换下一个候选重试。
pub(crate) fn try_upstream(target: &ProxyTarget, body: &Value) -> AttemptOutcome {
    let upstream = &target.upstream;
    let base = upstream.base_url.trim_end_matches('/');
    // 各候选的重写模型可能不同，请求体按候选分别生成
    let sent = match target.mode {
        UpstreamMode::Anthropic => {
            // 透传：上游本就是 Anthropic Messages 端点，只换地址与鉴权头
            proxy_client()
                .post(format!("{base}/v1/messages"))
                .header("x-api-key", upstream.api_key.as_str())
                .header("anthropic-version", "2023-06-01")
                .header("Content-Type", "application/json")
                .body(rewrite_model(body, &upstream.model).to_string())
                .send()
        }
        UpstreamMode::OpenAiChat => proxy_client()
            .post(format!("{base}/chat/completions"))
            .header("Authorization", format!("Bearer {}", upstream.api_key))
            .header("Content-Type", "application/json")
            .body(anthropic_request_to_openai(body, &upstream.model).to_string())
            .send(),
    };
    let resp = match sent {
        Ok(resp) => resp,
        // 传输层错误（连接失败/超时等）→ 可转移
        Err(e) => return AttemptOutcome::Retryable(502, format!("上游连接失败：{e}")),
    };
    let status = resp.status().as_u16();
    if status < 400 {
        return AttemptOutcome::Success(resp);
    }
    let text = resp.text().unwrap_or_default();
    let brief: String = text.chars().take(600).collect();
    let message = format!("上游返回 {status}：{brief}");
    if status >= 500 || status == 429 {
        // 服务端故障或限流 → 可转移
        AttemptOutcome::Retryable(status, message)
    } else {
        AttemptOutcome::ClientError(status, message)
    }
}

/// 把上游成功响应转发给客户端（流式 / 非流式两条路径）。
/// 从这里开始向客户端写字节，之后不能再切换上游。
pub(crate) fn deliver_response(
    request: tiny_http::Request,
    upstream_resp: reqwest::blocking::Response,
    stream: bool,
    mode: UpstreamMode,
) {
    // 透传模式：上游响应已经是 Anthropic 格式，原样回给客户端
    if mode == UpstreamMode::Anthropic {
        if stream {
            passthrough_stream(request, upstream_resp);
        } else {
            match upstream_resp.text() {
                Ok(text) => respond_json(request, 200, text),
                Err(e) => respond_error(request, 502, "api_error", &format!("上游响应读取失败：{e}")),
            }
        }
        return;
    }
    if stream {
        stream_response(request, upstream_resp);
    } else {
        let value: Value = match upstream_resp.json() {
            Ok(v) => v,
            Err(e) => {
                respond_error(request, 502, "api_error", &format!("上游响应解析失败：{e}"));
                return;
            }
        };
        respond_json(request, 200, openai_response_to_anthropic(&value).to_string());
    }
}

fn handle_messages(mut request: tiny_http::Request) {
    let primary = match current_target() {
        Some(target) if !target.upstream.base_url.trim().is_empty() => target,
        _ => {
            respond_error(
                request,
                503,
                "api_error",
                "VarSwitch 本地代理未配置上游。请在 VarSwitch 中激活一个由代理接管的 Claude 配置。",
            );
            return;
        }
    };
    let body = match read_body(&mut request) {
        Ok(value) => value,
        Err(message) => {
            respond_error(request, 400, "invalid_request_error", &message);
            return;
        }
    };
    let stream = body.get("stream").and_then(Value::as_bool).unwrap_or(false);

    // 候选序列 = [主上游] + 备用池（有序），按熔断键去重。
    // 备用池为空时序列只有主上游，行为与单上游版本一致（仅多统计记录）。
    let mut candidates: Vec<ProxyTarget> = vec![primary];
    for target in failover_targets() {
        if target.upstream.base_url.trim().is_empty() {
            continue;
        }
        if candidates
            .iter()
            .all(|c| upstream_key(&c.upstream) != upstream_key(&target.upstream))
        {
            candidates.push(target);
        }
    }

    let mut last_error: Option<(u16, String)> = None;
    // 主上游是否被实际发起过请求（熔断跳过不算），决定最后是否兜底强试
    let mut primary_attempted = false;

    for (idx, target) in candidates.iter().enumerate() {
        let upstream = &target.upstream;
        let key = upstream_key(upstream);
        if !breaker_allows(&key) {
            crate::app_log(
                "WARN",
                &format!(
                    "[claude-proxy] 上游 {} 处于熔断状态，跳过",
                    upstream.base_url
                ),
            );
            continue;
        }
        if idx == 0 {
            primary_attempted = true;
        } else {
            // 请求被实际发往备用上游 = 发生一次自动转移
            FAILOVER_COUNT.fetch_add(1, Ordering::Relaxed);
            crate::app_log(
                "INFO",
                &format!(
                    "[claude-proxy] 自动转移：尝试备用上游 {}",
                    upstream.base_url
                ),
            );
        }
        match try_upstream(target, &body) {
            AttemptOutcome::Success(resp) => {
                record_success(&key);
                deliver_response(request, resp, stream, target.mode);
                return;
            }
            AttemptOutcome::Retryable(status, message) => {
                record_failure(&key, &message);
                crate::app_log(
                    "WARN",
                    &format!(
                        "[claude-proxy] 上游 {} 失败（{message}），尝试下一候选",
                        upstream.base_url
                    ),
                );
                last_error = Some((status, message));
            }
            AttemptOutcome::ClientError(status, message) => {
                // 4xx 是配置问题，换上游解决不了：按现状直接翻译回客户端
                record_config_error(&key, &message);
                crate::app_log("WARN", &format!("[claude-proxy] {message}"));
                respond_error(request, status, "api_error", &message);
                return;
            }
        }
    }

    // 全部候选都被熔断跳过或失败。若主上游整轮都没被实际尝试过
    //（被熔断跳过），忽略熔断状态强行尝试一次——宁可失败也不能无路可走；
    // 若主上游已实际失败过，则不再重复请求，直接返回该错误。
    if !primary_attempted {
        let target = &candidates[0];
        let key = upstream_key(&target.upstream);
        crate::app_log(
            "WARN",
            &format!(
                "[claude-proxy] 所有候选均不可用，忽略熔断强行尝试主上游 {}",
                target.upstream.base_url
            ),
        );
        match try_upstream(target, &body) {
            AttemptOutcome::Success(resp) => {
                record_success(&key);
                deliver_response(request, resp, stream, target.mode);
                return;
            }
            AttemptOutcome::Retryable(status, message) => {
                record_failure(&key, &message);
                last_error = Some((status, message));
            }
            AttemptOutcome::ClientError(status, message) => {
                record_config_error(&key, &message);
                respond_error(request, status, "api_error", &message);
                return;
            }
        }
    }

    let (status, message) = last_error.unwrap_or((503, "所有上游均不可用".to_string()));
    crate::app_log("WARN", &format!("[claude-proxy] 请求最终失败：{message}"));
    respond_error(request, status, "api_error", &message);
}

// ── 流式响应管线 ──────────────────────────────────────

/// 把 mpsc 通道适配成 std::io::Read，交给 tiny_http 做 chunked 输出。
struct ChannelReader {
    rx: Receiver<Vec<u8>>,
    buf: Vec<u8>,
    pos: usize,
}

impl Read for ChannelReader {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        if self.pos >= self.buf.len() {
            match self.rx.recv() {
                Ok(chunk) => {
                    self.buf = chunk;
                    self.pos = 0;
                }
                Err(_) => return Ok(0), // 生产者结束 → EOF
            }
        }
        let n = std::cmp::min(out.len(), self.buf.len() - self.pos);
        out[..n].copy_from_slice(&self.buf[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

/// 透传模式的流式转发：上游 SSE 已是 Anthropic 格式，按字节原样搬运，
/// 不解析、不重组，避免翻译层引入的语义损失。
fn passthrough_stream(request: tiny_http::Request, upstream_resp: reqwest::blocking::Response) {
    let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        let mut reader = upstream_resp;
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if tx.send(buf[..n].to_vec()).is_err() {
                        return; // 客户端已断开
                    }
                }
                Err(_) => break,
            }
        }
    });

    let reader = ChannelReader {
        rx,
        buf: Vec::new(),
        pos: 0,
    };
    let headers = vec![
        tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/event-stream; charset=utf-8"[..])
            .expect("static header"),
        tiny_http::Header::from_bytes(&b"Cache-Control"[..], &b"no-cache"[..])
            .expect("static header"),
    ];
    let response = tiny_http::Response::new(tiny_http::StatusCode(200), headers, reader, None, None);
    let _ = request.respond(response);
}

fn stream_response(request: tiny_http::Request, upstream_resp: reqwest::blocking::Response) {
    let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        let mut state = SseState::new();
        let reader = BufReader::new(upstream_resp);
        for line in reader.lines() {
            let Ok(line) = line else { break };
            let trimmed = line.trim();
            let Some(data) = trimmed.strip_prefix("data:") else {
                continue;
            };
            let data = data.trim();
            if data == "[DONE]" {
                break;
            }
            let Ok(chunk) = serde_json::from_str::<Value>(data) else {
                continue;
            };
            for frame in state.ingest(&chunk) {
                if tx.send(frame.into_bytes()).is_err() {
                    return; // 客户端已断开
                }
            }
        }
        for frame in state.finish() {
            let _ = tx.send(frame.into_bytes());
        }
    });

    let reader = ChannelReader {
        rx,
        buf: Vec::new(),
        pos: 0,
    };
    let headers = vec![
        tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/event-stream; charset=utf-8"[..])
            .expect("static header"),
        tiny_http::Header::from_bytes(&b"Cache-Control"[..], &b"no-cache"[..])
            .expect("static header"),
    ];
    let response = tiny_http::Response::new(tiny_http::StatusCode(200), headers, reader, None, None);
    let _ = request.respond(response);
}

fn proxy_client() -> &'static reqwest::blocking::Client {
    static CLIENT: OnceLock<reqwest::blocking::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::blocking::Client::builder()
            .user_agent("VarSwitch/claude-proxy")
            .connect_timeout(Duration::from_secs(15))
            // LLM 流式响应可能持续数分钟，禁用总超时
            .timeout(Option::<Duration>::None)
            .build()
            .expect("build claude proxy http client")
    })
}

// ── Anthropic → OpenAI 请求转换 ───────────────────────

fn tool_result_text(block: &Value) -> String {
    let is_error = block
        .get("is_error")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let body = match block.get("content") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .map(|part| match part.get("type").and_then(Value::as_str).unwrap_or("") {
                "text" => part
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                other => format!("[{other} 内容已省略]"),
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Some(other) => other.to_string(),
        None => String::new(),
    };
    if is_error {
        format!("[tool error] {body}")
    } else {
        body
    }
}

pub fn anthropic_request_to_openai(body: &Value, upstream_model: &str) -> Value {
    let mut messages: Vec<Value> = Vec::new();

    let system_text = match body.get("system") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter_map(|b| b.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    };
    if !system_text.is_empty() {
        messages.push(json!({"role": "system", "content": system_text}));
    }

    let empty = Vec::new();
    let source_messages = body
        .get("messages")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    for msg in source_messages {
        let role = msg.get("role").and_then(Value::as_str).unwrap_or("user");
        match msg.get("content") {
            Some(Value::String(text)) => {
                messages.push(json!({"role": role, "content": text}));
            }
            Some(Value::Array(blocks)) => {
                if role == "assistant" {
                    let mut text_parts: Vec<String> = Vec::new();
                    let mut tool_calls: Vec<Value> = Vec::new();
                    for block in blocks {
                        match block.get("type").and_then(Value::as_str).unwrap_or("") {
                            "text" => {
                                if let Some(t) = block.get("text").and_then(Value::as_str) {
                                    text_parts.push(t.to_string());
                                }
                            }
                            "tool_use" => {
                                let args =
                                    block.get("input").cloned().unwrap_or_else(|| json!({}));
                                tool_calls.push(json!({
                                    "id": block.get("id").and_then(Value::as_str).unwrap_or(""),
                                    "type": "function",
                                    "function": {
                                        "name": block.get("name").and_then(Value::as_str).unwrap_or(""),
                                        "arguments": args.to_string()
                                    }
                                }));
                            }
                            // thinking / redacted_thinking：OpenAI 格式无对应角色，丢弃
                            _ => {}
                        }
                    }
                    if text_parts.is_empty() && tool_calls.is_empty() {
                        continue;
                    }
                    let mut converted = Map::new();
                    converted.insert("role".into(), json!("assistant"));
                    converted.insert(
                        "content".into(),
                        if text_parts.is_empty() {
                            Value::Null
                        } else {
                            json!(text_parts.join(""))
                        },
                    );
                    if !tool_calls.is_empty() {
                        converted.insert("tool_calls".into(), json!(tool_calls));
                    }
                    messages.push(Value::Object(converted));
                } else {
                    // user 消息：tool_result 必须转成独立的 role=tool 消息
                    let mut text_parts: Vec<String> = Vec::new();
                    for block in blocks {
                        match block.get("type").and_then(Value::as_str).unwrap_or("") {
                            "tool_result" => {
                                messages.push(json!({
                                    "role": "tool",
                                    "tool_call_id": block.get("tool_use_id").and_then(Value::as_str).unwrap_or(""),
                                    "content": tool_result_text(block)
                                }));
                            }
                            "text" => {
                                if let Some(t) = block.get("text").and_then(Value::as_str) {
                                    text_parts.push(t.to_string());
                                }
                            }
                            "image" => {
                                text_parts.push("[图片已省略：当前上游不支持图像输入]".into());
                            }
                            _ => {}
                        }
                    }
                    if !text_parts.is_empty() {
                        messages.push(json!({"role": "user", "content": text_parts.join("\n")}));
                    }
                }
            }
            _ => {}
        }
    }

    let mut out = Map::new();
    let model = if upstream_model.trim().is_empty() {
        body.get("model")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    } else {
        upstream_model.trim().to_string()
    };
    out.insert("model".into(), json!(model));
    out.insert("messages".into(), json!(messages));
    if let Some(n) = body.get("max_tokens").and_then(Value::as_u64) {
        out.insert("max_tokens".into(), json!(n));
    }
    if let Some(t) = body.get("temperature").filter(|v| v.is_number()) {
        out.insert("temperature".into(), t.clone());
    }
    if let Some(t) = body.get("top_p").filter(|v| v.is_number()) {
        out.insert("top_p".into(), t.clone());
    }
    if let Some(stops) = body.get("stop_sequences").and_then(Value::as_array) {
        if !stops.is_empty() {
            out.insert("stop".into(), json!(stops));
        }
    }
    if body.get("stream").and_then(Value::as_bool).unwrap_or(false) {
        out.insert("stream".into(), json!(true));
        out.insert("stream_options".into(), json!({"include_usage": true}));
    }
    if let Some(tools) = body.get("tools").and_then(Value::as_array) {
        let converted: Vec<Value> = tools
            .iter()
            .filter_map(|tool| {
                let name = tool.get("name").and_then(Value::as_str)?;
                Some(json!({
                    "type": "function",
                    "function": {
                        "name": name,
                        "description": tool.get("description").and_then(Value::as_str).unwrap_or(""),
                        "parameters": tool.get("input_schema").cloned().unwrap_or_else(|| json!({"type": "object"}))
                    }
                }))
            })
            .collect();
        if !converted.is_empty() {
            out.insert("tools".into(), json!(converted));
        }
    }
    if let Some(choice) = body.get("tool_choice") {
        let mapped = match choice.get("type").and_then(Value::as_str).unwrap_or("") {
            "any" => json!("required"),
            "tool" => json!({
                "type": "function",
                "function": {"name": choice.get("name").and_then(Value::as_str).unwrap_or("")}
            }),
            "none" => json!("none"),
            _ => json!("auto"),
        };
        out.insert("tool_choice".into(), mapped);
    }
    Value::Object(out)
}

// ── OpenAI → Anthropic 响应转换（非流式） ─────────────

fn map_finish_reason(reason: &str) -> &'static str {
    match reason {
        "length" => "max_tokens",
        "tool_calls" | "function_call" => "tool_use",
        _ => "end_turn",
    }
}

pub fn openai_response_to_anthropic(body: &Value) -> Value {
    let choice = body
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .cloned()
        .unwrap_or(Value::Null);
    let message = choice.get("message").cloned().unwrap_or(Value::Null);

    let mut content: Vec<Value> = Vec::new();
    let reasoning = message
        .get("reasoning_content")
        .and_then(Value::as_str)
        .or_else(|| message.get("reasoning").and_then(Value::as_str))
        .unwrap_or("");
    if !reasoning.is_empty() {
        content.push(json!({"type": "thinking", "thinking": reasoning, "signature": ""}));
    }
    if let Some(text) = message.get("content").and_then(Value::as_str) {
        if !text.is_empty() {
            content.push(json!({"type": "text", "text": text}));
        }
    }
    let mut has_tool = false;
    if let Some(calls) = message.get("tool_calls").and_then(Value::as_array) {
        for (i, call) in calls.iter().enumerate() {
            has_tool = true;
            let args_raw = call
                .get("function")
                .and_then(|f| f.get("arguments"))
                .and_then(Value::as_str)
                .unwrap_or("{}");
            let input: Value = serde_json::from_str(args_raw).unwrap_or_else(|_| json!({}));
            content.push(json!({
                "type": "tool_use",
                "id": call.get("id").and_then(Value::as_str).map(String::from)
                    .unwrap_or_else(|| format!("toolu_proxy_{i}")),
                "name": call.get("function").and_then(|f| f.get("name")).and_then(Value::as_str).unwrap_or(""),
                "input": input
            }));
        }
    }

    let finish = choice
        .get("finish_reason")
        .and_then(Value::as_str)
        .unwrap_or("stop");
    let stop_reason = if has_tool && finish == "stop" {
        "tool_use"
    } else {
        map_finish_reason(finish)
    };
    let usage = body.get("usage").cloned().unwrap_or(Value::Null);
    json!({
        "id": body.get("id").and_then(Value::as_str).unwrap_or("msg_varswitch_proxy"),
        "type": "message",
        "role": "assistant",
        "model": body.get("model").and_then(Value::as_str).unwrap_or(""),
        "content": content,
        "stop_reason": stop_reason,
        "stop_sequence": null,
        "usage": {
            "input_tokens": usage.get("prompt_tokens").and_then(Value::as_u64).unwrap_or(0),
            "output_tokens": usage.get("completion_tokens").and_then(Value::as_u64).unwrap_or(0)
        }
    })
}

// ── OpenAI → Anthropic 流式转换状态机 ─────────────────

fn sse_frame(event: &str, data: &Value) -> String {
    format!("event: {event}\ndata: {data}\n\n")
}

#[derive(PartialEq, Clone, Copy)]
enum BlockKind {
    Thinking,
    Text,
    Tool,
}

/// 把 OpenAI chunk 流转换为 Anthropic 事件流：
/// message_start → content_block_start/delta/stop（thinking / text / tool_use
/// 各占一个块，按内容切换）→ message_delta（stop_reason + usage）→ message_stop
pub struct SseState {
    message_started: bool,
    finished: bool,
    open_block: Option<BlockKind>,
    next_index: u64,
    current_index: u64,
    current_tool_slot: Option<u64>,
    saw_tool_call: bool,
    stop_reason: String,
    input_tokens: u64,
    output_tokens: u64,
}

impl SseState {
    pub fn new() -> Self {
        SseState {
            message_started: false,
            finished: false,
            open_block: None,
            next_index: 0,
            current_index: 0,
            current_tool_slot: None,
            saw_tool_call: false,
            stop_reason: String::new(),
            input_tokens: 0,
            output_tokens: 0,
        }
    }

    fn emit_message_start(&mut self, chunk: Option<&Value>, frames: &mut Vec<String>) {
        let id = chunk
            .and_then(|c| c.get("id"))
            .and_then(Value::as_str)
            .unwrap_or("msg_varswitch_proxy");
        let model = chunk
            .and_then(|c| c.get("model"))
            .and_then(Value::as_str)
            .unwrap_or("");
        frames.push(sse_frame(
            "message_start",
            &json!({
                "type": "message_start",
                "message": {
                    "id": id, "type": "message", "role": "assistant",
                    "model": model, "content": [],
                    "stop_reason": null, "stop_sequence": null,
                    "usage": {"input_tokens": 0, "output_tokens": 0}
                }
            }),
        ));
        self.message_started = true;
    }

    fn close_block(&mut self, frames: &mut Vec<String>) {
        let Some(kind) = self.open_block else { return };
        if kind == BlockKind::Thinking {
            // Anthropic thinking 块以 signature_delta 收尾；第三方内容无签名，补空值
            frames.push(sse_frame(
                "content_block_delta",
                &json!({
                    "type": "content_block_delta", "index": self.current_index,
                    "delta": {"type": "signature_delta", "signature": ""}
                }),
            ));
        }
        frames.push(sse_frame(
            "content_block_stop",
            &json!({"type": "content_block_stop", "index": self.current_index}),
        ));
        self.open_block = None;
        self.current_tool_slot = None;
    }

    fn start_block(
        &mut self,
        kind: BlockKind,
        tool: Option<(u64, &Value)>,
        frames: &mut Vec<String>,
    ) {
        self.close_block(frames);
        self.current_index = self.next_index;
        self.next_index += 1;
        self.open_block = Some(kind);
        let content_block = match kind {
            BlockKind::Thinking => json!({"type": "thinking", "thinking": "", "signature": ""}),
            BlockKind::Text => json!({"type": "text", "text": ""}),
            BlockKind::Tool => {
                let (slot, call) = tool.expect("tool block requires tool call info");
                self.current_tool_slot = Some(slot);
                self.saw_tool_call = true;
                let id = call
                    .get("id")
                    .and_then(Value::as_str)
                    .map(String::from)
                    .unwrap_or_else(|| format!("toolu_proxy_{}", self.current_index));
                let name = call
                    .get("function")
                    .and_then(|f| f.get("name"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                json!({"type": "tool_use", "id": id, "name": name, "input": {}})
            }
        };
        frames.push(sse_frame(
            "content_block_start",
            &json!({
                "type": "content_block_start", "index": self.current_index,
                "content_block": content_block
            }),
        ));
    }

    pub fn ingest(&mut self, chunk: &Value) -> Vec<String> {
        let mut frames = Vec::new();
        if self.finished {
            return frames;
        }
        if !self.message_started {
            self.emit_message_start(Some(chunk), &mut frames);
        }
        if let Some(usage) = chunk.get("usage").filter(|u| u.is_object()) {
            if let Some(n) = usage.get("prompt_tokens").and_then(Value::as_u64) {
                self.input_tokens = n;
            }
            if let Some(n) = usage.get("completion_tokens").and_then(Value::as_u64) {
                self.output_tokens = n;
            }
        }
        let Some(choice) = chunk
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
        else {
            return frames;
        };

        if let Some(delta) = choice.get("delta") {
            let reasoning = delta
                .get("reasoning_content")
                .and_then(Value::as_str)
                .or_else(|| delta.get("reasoning").and_then(Value::as_str))
                .unwrap_or("");
            if !reasoning.is_empty() {
                if self.open_block != Some(BlockKind::Thinking) {
                    self.start_block(BlockKind::Thinking, None, &mut frames);
                }
                frames.push(sse_frame(
                    "content_block_delta",
                    &json!({
                        "type": "content_block_delta", "index": self.current_index,
                        "delta": {"type": "thinking_delta", "thinking": reasoning}
                    }),
                ));
            }

            let content = delta.get("content").and_then(Value::as_str).unwrap_or("");
            if !content.is_empty() {
                if self.open_block != Some(BlockKind::Text) {
                    self.start_block(BlockKind::Text, None, &mut frames);
                }
                frames.push(sse_frame(
                    "content_block_delta",
                    &json!({
                        "type": "content_block_delta", "index": self.current_index,
                        "delta": {"type": "text_delta", "text": content}
                    }),
                ));
            }

            if let Some(tool_calls) = delta.get("tool_calls").and_then(Value::as_array) {
                for call in tool_calls {
                    let slot = call.get("index").and_then(Value::as_u64).unwrap_or(0);
                    if self.open_block != Some(BlockKind::Tool)
                        || self.current_tool_slot != Some(slot)
                    {
                        self.start_block(BlockKind::Tool, Some((slot, call)), &mut frames);
                    }
                    let args = call
                        .get("function")
                        .and_then(|f| f.get("arguments"))
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    if !args.is_empty() {
                        frames.push(sse_frame(
                            "content_block_delta",
                            &json!({
                                "type": "content_block_delta", "index": self.current_index,
                                "delta": {"type": "input_json_delta", "partial_json": args}
                            }),
                        ));
                    }
                }
            }
        }

        if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
            if !reason.is_empty() {
                self.stop_reason = if self.saw_tool_call && reason == "stop" {
                    "tool_use".to_string()
                } else {
                    map_finish_reason(reason).to_string()
                };
            }
        }
        frames
    }

    pub fn finish(&mut self) -> Vec<String> {
        let mut frames = Vec::new();
        if self.finished {
            return frames;
        }
        if !self.message_started {
            self.emit_message_start(None, &mut frames);
        }
        self.close_block(&mut frames);
        if self.stop_reason.is_empty() {
            self.stop_reason = "end_turn".to_string();
        }
        frames.push(sse_frame(
            "message_delta",
            &json!({
                "type": "message_delta",
                "delta": {"stop_reason": self.stop_reason, "stop_sequence": null},
                "usage": {"input_tokens": self.input_tokens, "output_tokens": self.output_tokens}
            }),
        ));
        frames.push(sse_frame("message_stop", &json!({"type": "message_stop"})));
        self.finished = true;
        frames
    }
}

// ── 单元测试 ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// 下列测试读写进程级的代理上游状态，默认并行执行会互相踩踏，故串行化
    static PROXY_STATE_LOCK: Mutex<()> = Mutex::new(());

    fn lock_proxy_state() -> std::sync::MutexGuard<'static, ()> {
        PROXY_STATE_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn upstream_mode_maps_profile_api_format() {
        assert_eq!(
            UpstreamMode::from_api_format("anthropic"),
            UpstreamMode::Anthropic
        );
        assert_eq!(
            UpstreamMode::from_api_format(" anthropic "),
            UpstreamMode::Anthropic
        );
        assert_eq!(
            UpstreamMode::from_api_format("openai_chat"),
            UpstreamMode::OpenAiChat
        );
        // 未知/空值按翻译模式处理，与加入透传前的行为一致
        assert_eq!(UpstreamMode::from_api_format(""), UpstreamMode::OpenAiChat);
        assert_eq!(
            UpstreamMode::from_api_format("something-else"),
            UpstreamMode::OpenAiChat
        );
    }

    #[test]
    fn set_upstream_without_mode_defaults_to_translation() {
        let _guard = lock_proxy_state();
        // set_upstream 不带协议参数，必须继续按 OpenAI 翻译模式工作
        set_upstream(Some(ProxyUpstream {
            base_url: "https://legacy.example.com".into(),
            api_key: "sk-legacy".into(),
            model: "gpt-x".into(),
        }));
        let target = current_target().expect("主上游已设置");
        assert_eq!(target.mode, UpstreamMode::OpenAiChat);
        assert_eq!(target.upstream.base_url, "https://legacy.example.com");

        set_upstream(None);
        set_failover_targets(Vec::new());
    }

    #[test]
    fn mixed_protocol_pool_keeps_each_target_mode() {
        let _guard = lock_proxy_state();
        set_upstream_with_mode(
            Some(ProxyUpstream {
                base_url: "https://relay.example.com".into(),
                api_key: "sk-primary".into(),
                model: String::new(),
            }),
            UpstreamMode::Anthropic,
        );
        set_failover_targets(vec![
            ProxyTarget {
                upstream: ProxyUpstream {
                    base_url: "https://backup-anthropic.example.com".into(),
                    api_key: "sk-a".into(),
                    model: String::new(),
                },
                mode: UpstreamMode::Anthropic,
            },
            ProxyTarget {
                upstream: ProxyUpstream {
                    base_url: "https://backup-openai.example.com".into(),
                    api_key: "sk-b".into(),
                    model: "deepseek-chat".into(),
                },
                mode: UpstreamMode::OpenAiChat,
            },
        ]);

        let target = current_target().expect("主上游已设置");
        assert_eq!(target.mode, UpstreamMode::Anthropic);
        let pool = failover_targets();
        assert_eq!(pool[0].mode, UpstreamMode::Anthropic);
        assert_eq!(pool[1].mode, UpstreamMode::OpenAiChat);

        // 健康面板要能区分每个上游说的协议
        let snapshot = claude_proxy_health();
        assert_eq!(snapshot["upstream"]["apiFormat"], "anthropic");
        assert_eq!(snapshot["failoverPool"][1]["apiFormat"], "openai_chat");

        set_upstream(None);
        set_failover_targets(Vec::new());
    }

    #[test]
    fn rewrite_model_only_overrides_when_configured() {
        let body = json!({"model": "claude-sonnet-5", "messages": []});
        // 未配置模型时保留客户端原始模型名，交给上游自行解析
        assert_eq!(rewrite_model(&body, "")["model"], "claude-sonnet-5");
        assert_eq!(rewrite_model(&body, "   ")["model"], "claude-sonnet-5");
        // 配置了就覆盖，其余字段原样保留
        let rewritten = rewrite_model(&body, "claude-opus-4");
        assert_eq!(rewritten["model"], "claude-opus-4");
        assert!(rewritten.get("messages").is_some());
    }

    #[test]
    fn request_converts_system_messages_tools_and_model() {
        let body = json!({
            "model": "claude-sonnet-5",
            "max_tokens": 4096,
            "stream": true,
            "system": [{"type": "text", "text": "You are helpful."}],
            "messages": [
                {"role": "user", "content": "hi"},
                {"role": "assistant", "content": [
                    {"type": "thinking", "thinking": "hmm", "signature": "x"},
                    {"type": "text", "text": "let me check"},
                    {"type": "tool_use", "id": "toolu_1", "name": "get_weather", "input": {"city": "sh"}}
                ]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "toolu_1", "content": [{"type": "text", "text": "sunny"}]},
                    {"type": "text", "text": "continue"}
                ]}
            ],
            "tools": [{"name": "get_weather", "description": "d", "input_schema": {"type": "object"}}],
            "tool_choice": {"type": "auto"}
        });
        let out = anthropic_request_to_openai(&body, "deepseek-chat");

        assert_eq!(out["model"], "deepseek-chat");
        assert_eq!(out["max_tokens"], 4096);
        assert_eq!(out["stream"], true);
        assert_eq!(out["stream_options"]["include_usage"], true);
        assert_eq!(out["tool_choice"], "auto");

        let messages = out["messages"].as_array().unwrap();
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[0]["content"], "You are helpful.");
        assert_eq!(messages[1]["role"], "user");
        // assistant：thinking 丢弃，text 保留，tool_use → tool_calls
        assert_eq!(messages[2]["role"], "assistant");
        assert_eq!(messages[2]["content"], "let me check");
        assert_eq!(messages[2]["tool_calls"][0]["id"], "toolu_1");
        assert_eq!(messages[2]["tool_calls"][0]["function"]["name"], "get_weather");
        // tool_result → 独立 role=tool 消息，text 合并为 user
        assert_eq!(messages[3]["role"], "tool");
        assert_eq!(messages[3]["tool_call_id"], "toolu_1");
        assert_eq!(messages[3]["content"], "sunny");
        assert_eq!(messages[4]["role"], "user");
        assert_eq!(messages[4]["content"], "continue");

        assert_eq!(out["tools"][0]["type"], "function");
        assert_eq!(out["tools"][0]["function"]["name"], "get_weather");
    }

    #[test]
    fn request_keeps_original_model_when_upstream_model_empty() {
        let body = json!({"model": "claude-x", "messages": [], "max_tokens": 1});
        let out = anthropic_request_to_openai(&body, "");
        assert_eq!(out["model"], "claude-x");
    }

    #[test]
    fn response_converts_text_reasoning_and_tool_calls() {
        let body = json!({
            "id": "chatcmpl-1",
            "model": "deepseek-chat",
            "choices": [{
                "finish_reason": "tool_calls",
                "message": {
                    "role": "assistant",
                    "reasoning_content": "think...",
                    "content": "hello",
                    "tool_calls": [{
                        "id": "call_1", "type": "function",
                        "function": {"name": "f", "arguments": "{\"a\":1}"}
                    }]
                }
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5}
        });
        let out = openai_response_to_anthropic(&body);
        assert_eq!(out["type"], "message");
        assert_eq!(out["role"], "assistant");
        assert_eq!(out["stop_reason"], "tool_use");
        assert_eq!(out["usage"]["input_tokens"], 10);
        assert_eq!(out["usage"]["output_tokens"], 5);
        let content = out["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "thinking");
        assert_eq!(content[0]["thinking"], "think...");
        assert_eq!(content[1]["type"], "text");
        assert_eq!(content[1]["text"], "hello");
        assert_eq!(content[2]["type"], "tool_use");
        assert_eq!(content[2]["input"]["a"], 1);
    }

    #[test]
    fn finish_reason_maps_to_anthropic_values() {
        assert_eq!(map_finish_reason("stop"), "end_turn");
        assert_eq!(map_finish_reason("length"), "max_tokens");
        assert_eq!(map_finish_reason("tool_calls"), "tool_use");
        assert_eq!(map_finish_reason("content_filter"), "end_turn");
    }

    fn frame_events(frames: &[String]) -> Vec<String> {
        frames
            .iter()
            .map(|f| {
                f.lines()
                    .next()
                    .unwrap_or("")
                    .trim_start_matches("event: ")
                    .to_string()
            })
            .collect()
    }

    #[test]
    fn sse_text_stream_produces_anthropic_event_sequence() {
        let mut state = SseState::new();
        let mut frames = Vec::new();
        frames.extend(state.ingest(&json!({
            "id": "chatcmpl-1", "model": "m",
            "choices": [{"delta": {"content": "he"}}]
        })));
        frames.extend(state.ingest(&json!({
            "choices": [{"delta": {"content": "llo"}}]
        })));
        frames.extend(state.ingest(&json!({
            "choices": [{"delta": {}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 3, "completion_tokens": 2}
        })));
        frames.extend(state.finish());

        assert_eq!(
            frame_events(&frames),
            vec![
                "message_start",
                "content_block_start",
                "content_block_delta",
                "content_block_delta",
                "content_block_stop",
                "message_delta",
                "message_stop",
            ]
        );
        assert!(frames[1].contains("\"text\""));
        assert!(frames[5].contains("\"end_turn\""));
        assert!(frames[5].contains("\"output_tokens\":2"));
    }

    #[test]
    fn sse_reasoning_then_text_then_tool_switches_blocks() {
        let mut state = SseState::new();
        let mut frames = Vec::new();
        frames.extend(state.ingest(&json!({
            "id": "c", "model": "m",
            "choices": [{"delta": {"reasoning_content": "think"}}]
        })));
        frames.extend(state.ingest(&json!({
            "choices": [{"delta": {"content": "answer"}}]
        })));
        frames.extend(state.ingest(&json!({
            "choices": [{"delta": {"tool_calls": [{
                "index": 0, "id": "call_1",
                "function": {"name": "f", "arguments": "{\"a\":"}
            }]}}]
        })));
        frames.extend(state.ingest(&json!({
            "choices": [{"delta": {"tool_calls": [{
                "index": 0, "function": {"arguments": "1}"}
            }]}}]
        })));
        frames.extend(state.ingest(&json!({
            "choices": [{"delta": {}, "finish_reason": "tool_calls"}]
        })));
        frames.extend(state.finish());

        let events = frame_events(&frames);
        assert_eq!(
            events,
            vec![
                "message_start",
                "content_block_start",  // thinking
                "content_block_delta",  // thinking_delta
                "content_block_delta",  // signature_delta（关闭 thinking 前）
                "content_block_stop",
                "content_block_start",  // text
                "content_block_delta",  // text_delta
                "content_block_stop",
                "content_block_start",  // tool_use
                "content_block_delta",  // input_json_delta "{\"a\":"
                "content_block_delta",  // input_json_delta "1}"
                "content_block_stop",   // finish() 关闭打开的 tool 块
                "message_delta",
                "message_stop",
            ]
        );
        assert!(frames[8].contains("\"tool_use\""));
        assert!(frames[8].contains("\"call_1\""));
        let joined = frames.join("");
        assert!(joined.contains("\"stop_reason\":\"tool_use\""));
    }

    #[test]
    fn sse_finish_without_chunks_emits_complete_empty_message() {
        let mut state = SseState::new();
        let frames = state.finish();
        assert_eq!(
            frame_events(&frames),
            vec!["message_start", "message_delta", "message_stop"]
        );
    }

    #[test]
    fn tool_result_text_handles_string_blocks_and_errors() {
        assert_eq!(
            tool_result_text(&json!({"content": "plain"})),
            "plain"
        );
        assert_eq!(
            tool_result_text(&json!({"content": [{"type": "text", "text": "a"}, {"type": "text", "text": "b"}]})),
            "a\nb"
        );
        assert_eq!(
            tool_result_text(&json!({"content": "boom", "is_error": true})),
            "[tool error] boom"
        );
    }

    // ── 熔断器状态机测试（各测试用独立 key，避免全局状态互扰；
    //    不调用 reset_health_state，防止清掉并行测试的状态） ──

    /// 手动把状态时间拨回窗口之前，模拟熔断到期（测试不能真等 60 秒）
    fn rewind_state_time(key: &str) {
        if let Ok(mut map) = health_map().lock() {
            map.get_mut(key).unwrap().state_changed_at = unix_now() - BREAKER_OPEN_SECS - 1;
        }
    }

    #[test]
    fn breaker_opens_after_three_failures_then_probes_and_recovers() {
        let _guard = lock_proxy_state();
        let key = "test_breaker_open_probe";
        assert!(breaker_allows(key));
        record_failure(key, "err1");
        record_failure(key, "err2");
        // 2 次连续失败尚未达到阈值
        assert!(breaker_allows(key));
        record_failure(key, "err3");
        // 第 3 次失败 → open，跳过该上游
        assert!(!breaker_allows(key));

        rewind_state_time(key);
        // open 到期 → half-open，放行一个探测；在途期间不再放行
        assert!(breaker_allows(key));
        assert!(!breaker_allows(key));
        // 探测成功 → closed，连续失败清零
        record_success(key);
        assert!(breaker_allows(key));
        if let Ok(map) = health_map().lock() {
            let entry = map.get(key).unwrap();
            assert_eq!(entry.consecutive_failures, 0);
            assert_eq!(entry.total_requests, 4);
            assert_eq!(entry.total_failures, 3);
            assert!(entry.last_success_ts.is_some());
        }
    }

    #[test]
    fn breaker_reopens_when_half_open_probe_fails() {
        let _guard = lock_proxy_state();
        let key = "test_breaker_reopen";
        for i in 0..3 {
            record_failure(key, &format!("err{i}"));
        }
        rewind_state_time(key);
        // half-open 探测放行后失败 → 立即重新 open
        assert!(breaker_allows(key));
        record_failure(key, "probe failed");
        assert!(!breaker_allows(key));
    }

    #[test]
    fn success_resets_consecutive_failures() {
        let _guard = lock_proxy_state();
        let key = "test_breaker_reset_on_success";
        record_failure(key, "err1");
        record_failure(key, "err2");
        record_success(key);
        record_failure(key, "err3");
        record_failure(key, "err4");
        // 成功清零后重新累计，2 < 3 不触发熔断
        assert!(breaker_allows(key));
    }

    #[test]
    fn config_error_records_stats_without_tripping_breaker() {
        let _guard = lock_proxy_state();
        let key = "test_breaker_config_error";
        for i in 0..5 {
            record_config_error(key, &format!("401 err{i}"));
        }
        // 4xx 属配置问题，不推进熔断
        assert!(breaker_allows(key));
        if let Ok(map) = health_map().lock() {
            let entry = map.get(key).unwrap();
            assert_eq!(entry.total_failures, 5);
            assert_eq!(entry.consecutive_failures, 0);
        }
    }

    #[test]
    fn last_error_is_truncated_to_limit() {
        let _guard = lock_proxy_state();
        let key = "test_breaker_truncate";
        record_failure(key, &"x".repeat(500));
        if let Ok(map) = health_map().lock() {
            let stored = map.get(key).unwrap().last_error.clone().unwrap();
            assert_eq!(stored.chars().count(), LAST_ERROR_MAX_CHARS);
        }
    }

    #[test]
    fn mask_api_key_keeps_head_and_tail_only() {
        assert_eq!(mask_api_key("sk-abcdefghijklmnop"), "sk-abc****mnop");
        assert_eq!(mask_api_key("short"), "****");
        assert_eq!(mask_api_key(""), "");
    }
}
