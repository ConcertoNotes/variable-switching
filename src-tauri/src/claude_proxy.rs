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
//! 限制：VarSwitch 退出后代理停止，Claude Code 将无法连接；
//! 切回 anthropic 直连配置即可脱离代理。

use serde_json::{json, Map, Value};
use std::io::{BufRead, BufReader, Read};
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

static UPSTREAM: OnceLock<Mutex<Option<ProxyUpstream>>> = OnceLock::new();
static SERVER_RUNNING: OnceLock<Mutex<bool>> = OnceLock::new();

fn upstream_cell() -> &'static Mutex<Option<ProxyUpstream>> {
    UPSTREAM.get_or_init(|| Mutex::new(None))
}

pub fn set_upstream(upstream: Option<ProxyUpstream>) {
    if let Ok(mut guard) = upstream_cell().lock() {
        *guard = upstream;
    }
}

pub fn current_upstream() -> Option<ProxyUpstream> {
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

// ── HTTP 处理 ─────────────────────────────────────────

fn json_content_type() -> tiny_http::Header {
    tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
        .expect("static header")
}

fn anthropic_error_body(error_type: &str, message: &str) -> String {
    json!({"type": "error", "error": {"type": error_type, "message": message}}).to_string()
}

fn respond_error(request: tiny_http::Request, status: u16, error_type: &str, message: &str) {
    let response = tiny_http::Response::from_string(anthropic_error_body(error_type, message))
        .with_status_code(status)
        .with_header(json_content_type());
    let _ = request.respond(response);
}

fn respond_json(request: tiny_http::Request, status: u16, body: String) {
    let response = tiny_http::Response::from_string(body)
        .with_status_code(status)
        .with_header(json_content_type());
    let _ = request.respond(response);
}

fn read_body(request: &mut tiny_http::Request) -> Result<Value, String> {
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

/// Claude Code 用 count_tokens 做上下文估算；上游没有等价接口，
/// 按「JSON 字符数 / 4」给一个数量级正确的估算，失败也不影响主链路。
fn handle_count_tokens(mut request: tiny_http::Request) {
    let mut body = String::new();
    let _ = request.as_reader().read_to_string(&mut body);
    let approx = (body.chars().count() / 4).max(1);
    respond_json(request, 200, json!({"input_tokens": approx}).to_string());
}

fn handle_messages(mut request: tiny_http::Request) {
    let upstream = match current_upstream() {
        Some(upstream) if !upstream.base_url.trim().is_empty() => upstream,
        _ => {
            respond_error(
                request,
                503,
                "api_error",
                "VarSwitch 本地代理未配置上游。请在 VarSwitch 中启用一个 OpenAI 格式的 Claude 配置。",
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
    let openai_body = anthropic_request_to_openai(&body, &upstream.model);
    let url = format!(
        "{}/chat/completions",
        upstream.base_url.trim_end_matches('/')
    );

    let sent = proxy_client()
        .post(&url)
        .header("Authorization", format!("Bearer {}", upstream.api_key))
        .header("Content-Type", "application/json")
        .body(openai_body.to_string())
        .send();
    let upstream_resp = match sent {
        Ok(resp) => resp,
        Err(e) => {
            crate::app_log("WARN", &format!("[claude-proxy] 上游连接失败：{e}"));
            respond_error(request, 502, "api_error", &format!("上游连接失败：{e}"));
            return;
        }
    };

    let status = upstream_resp.status().as_u16();
    if status >= 400 {
        let text = upstream_resp.text().unwrap_or_default();
        let brief: String = text.chars().take(600).collect();
        crate::app_log(
            "WARN",
            &format!("[claude-proxy] 上游返回 {status}：{brief}"),
        );
        respond_error(
            request,
            status,
            "api_error",
            &format!("上游返回 {status}：{brief}"),
        );
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
}
