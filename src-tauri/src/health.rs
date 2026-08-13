//! 配置体检：逐个探测已保存配置的 Base URL 还能不能用，并给出具体失效原因。
//!
//! ── 判定依据（来自本机 26 套真实配置的实测回包）─────────────────────────────
//!
//! 1. 401/403 不等于「Key 失效」。被 WAF 整站拦截的站点（实测 superxihe.com 挂在
//!    Cloudflare 后面）同样回 403，但响应体是一整页 HTML 而不是 JSON——此时 Key 多半
//!    还是好的，只是探测请求根本没走到接口。报成失效会诱导用户删掉仍可用的配置，
//!    因此按响应体形态把这两种情况彻底分开。
//! 2. 站点回 JSON 时，把它自己写的原因原样透出，远比归纳成「Key 无效」有用：实测
//!    出现过 insufficient balance（充值即可恢复）、API Key 已被禁用、Invalid token
//!    三类，用户的处置动作完全不同。
//! 3. api.anthropic.com 拒绝 Bearer 鉴权，必须用 x-api-key + anthropic-version，
//!    否则官方配置每次体检都会被误判成 Key 失效。
//!
//! 探测统一打 `{base_url}/v1/models`：五个 CLI 的上游都兼容 OpenAI 风格的模型列表接口，
//! 它足够轻（不消耗额度）又必然经过鉴权，是最能反映「这套配置还能不能发请求」的信号。

use crate::*;
use std::sync::atomic::AtomicUsize;

/// 单次探测超时。体检要跑几十套配置，超时太长整体等待难以忍受，太短又会把
/// 网络本就慢的境外站点误判成已下线，10 秒是实测下来的折中值。
const HEALTH_HTTP_TIMEOUT_SECS: u64 = 10;

/// 同时在飞的探测数上限。用户动辄几十套配置，一次性全发出去会瞬间打开同样多的
/// TCP 连接，既容易触发站点限流，也会把本机网络拖垮。
const HEALTH_MAX_CONCURRENCY: usize = 6;

/// 站点失败原因的截断长度：够看清是余额不足还是 Key 被禁，又不至于把列表撑爆。
const HEALTH_REASON_MAX_CHARS: usize = 80;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProfileHealth {
    pub(crate) app: String,
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) host: String,
    /// "ok" | "warn" | "bad" | "dead" | "skipped"
    pub(crate) level: String,
    pub(crate) message: String,
    pub(crate) latency_ms: Option<u64>,
}

/// 一条待探测的配置。api_key 只在探测线程内用于组装请求头，任何日志都不得携带。
struct ProbeTarget {
    app: &'static str,
    id: String,
    name: String,
    base_url: String,
    api_key: String,
}

// ── 纯判定逻辑（不依赖网络，便于用真实回包做单测）───────────────────────────

/// 探测地址拼接：base 已经指到 /models 就直接用，指到 /v1 只补 /models，
/// 其余情况按 OpenAI 兼容惯例补完整的 /v1/models。
fn models_probe_url(base_url: &str) -> Result<String, String> {
    let normalized = normalize_endpoint_url(base_url)?;
    let lower = normalized.to_ascii_lowercase();
    Ok(if lower.ends_with("/models") {
        normalized
    } else if lower.ends_with("/v1") {
        format!("{normalized}/models")
    } else {
        format!("{normalized}/v1/models")
    })
}

/// 只取开头几个字符判型，避免为了认出一页 HTML 把几百 KB 正文整体转小写。
fn looks_like_html(body: &str) -> bool {
    let head: String = body
        .trim_start()
        .chars()
        .take(16)
        .collect::<String>()
        .to_ascii_lowercase();
    head.starts_with("<!doctype") || head.starts_with("<html")
}

/// 失败原因散落在三个位置：OpenAI 风格放 error.message，one-api 系放顶层 message，
/// 少数站点直接把一句话塞进 error 字符串，按这个顺序取第一个非空的。
fn extract_error_reason(body: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let reason = value
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(|message| message.as_str())
        .or_else(|| value.get("message").and_then(|message| message.as_str()))
        .or_else(|| value.get("error").and_then(|error| error.as_str()))?
        .trim();
    if reason.is_empty() {
        return None;
    }
    // 按字符而非字节截断，否则会把中文原因切成乱码
    Some(reason.chars().take(HEALTH_REASON_MAX_CHARS).collect())
}

/// HTTP 状态码 + 响应体 → (level, message)。
/// 401/403 的分流是整个体检最关键的一步，详见模块头注释第 1、2 条。
fn classify_response(status: u16, body: &str) -> (String, String) {
    if (200..300).contains(&status) {
        return ("ok".to_string(), "可用".to_string());
    }
    if status == 401 || status == 403 {
        if looks_like_html(body) {
            return (
                "warn".to_string(),
                "站点返回网页而非接口，可能是 WAF 拦截，API Key 未必失效".to_string(),
            );
        }
        return (
            "bad".to_string(),
            match extract_error_reason(body) {
                Some(reason) => format!("鉴权失败 HTTP {status}：{reason}"),
                None => format!("鉴权失败 HTTP {status}"),
            },
        );
    }
    ("warn".to_string(), format!("HTTP {status}"))
}

/// 网络层错误按「有没有连上」分级：连不上说明站点自身出了问题（dead）；
/// TLS、解码之类的失败不足以断言站点已下线，只提示用户自行确认（warn）。
/// 取 bool + 文本而不是 reqwest::Error，是为了让这段判定也能被单测覆盖。
fn classify_transport_error(
    is_timeout: bool,
    is_connect: bool,
    description: &str,
) -> (String, String) {
    if is_timeout {
        return (
            "dead".to_string(),
            format!("请求超时（超过 {HEALTH_HTTP_TIMEOUT_SECS} 秒无响应）"),
        );
    }
    let lower = description.to_ascii_lowercase();
    if is_connect || lower.contains("dns") || lower.contains("resolve") {
        return (
            "dead".to_string(),
            "域名无法解析或连接失败（站点可能已下线）".to_string(),
        );
    }
    ("warn".to_string(), "请求失败，无法判定站点状态".to_string())
}

/// Anthropic 官方端点：只认 anthropic.com 及其子域，蹭域名的中转站
/// （fake-anthropic.com 之类）仍按 Bearer 处理。
fn is_anthropic_official(host: &str) -> bool {
    host == "anthropic.com" || host.ends_with(".anthropic.com")
}

/// OpenCode 允许 Base URL 留空表示走该 provider 的官方地址，这里按 models.dev 的
/// provider id 补齐；补不出来的没法探测，只能跳过。
fn opencode_official_base_url(provider_id: &str) -> Option<&'static str> {
    match provider_id.trim().to_ascii_lowercase().as_str() {
        "deepseek" => Some("https://api.deepseek.com"),
        "moonshotai" => Some("https://api.moonshot.cn"),
        "siliconflow" => Some("https://api.siliconflow.cn"),
        "openrouter" => Some("https://openrouter.ai"),
        "anthropic" => Some("https://api.anthropic.com"),
        "openai" => Some("https://api.openai.com/v1"),
        _ => None,
    }
}

/// 展示用 host，与前端 `new URL(x).host` 一致（带端口），同一中转站的多套配置一眼可归组。
fn url_host_with_port(url: &reqwest::Url) -> String {
    let host = url.host_str().unwrap_or_default();
    match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_string(),
    }
}

// ── 探测执行 ────────────────────────────────────────────────────────────────

fn collect_targets(app: &tauri::AppHandle) -> Vec<ProbeTarget> {
    let mut targets = Vec::new();

    for profile in read_profiles(app).profiles {
        targets.push(ProbeTarget {
            app: "claude",
            id: profile.id,
            name: profile.name,
            base_url: profile.base_url,
            api_key: profile.api_key,
        });
    }
    for profile in read_codex_profiles(app).profiles {
        targets.push(ProbeTarget {
            app: "codex",
            id: profile.id,
            name: profile.name,
            base_url: profile.base_url,
            api_key: profile.api_key,
        });
    }
    for profile in read_grok_profiles(app).profiles {
        targets.push(ProbeTarget {
            app: "grok",
            id: profile.id,
            name: profile.name,
            base_url: profile.base_url,
            api_key: profile.api_key,
        });
    }
    for profile in read_gemini_profiles(app).profiles {
        targets.push(ProbeTarget {
            app: "gemini",
            id: profile.id,
            name: profile.name,
            base_url: profile.base_url,
            api_key: profile.api_key,
        });
    }
    for profile in opencode::read_opencode_profiles(app).profiles {
        let base_url = if profile.base_url.trim().is_empty() {
            opencode_official_base_url(&profile.provider_id)
                .unwrap_or_default()
                .to_string()
        } else {
            profile.base_url
        };
        targets.push(ProbeTarget {
            app: "opencode",
            id: profile.id,
            name: profile.name,
            base_url,
            api_key: profile.api_key,
        });
    }

    targets
}

/// 发一次模型列表请求，返回状态码与响应体原文（判定交给 classify_response）。
fn send_models_probe(
    client: &reqwest::blocking::Client,
    url: &str,
    host: &str,
    api_key: &str,
) -> Result<(u16, String), reqwest::Error> {
    let request = client.get(url).header("Accept", "application/json");
    let request = if is_anthropic_official(host) {
        request
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
    } else {
        request.bearer_auth(api_key)
    };
    let response = request.send()?;
    let status = response.status().as_u16();
    // 读不出正文时按空体处理：状态码本身已经够判级，不该因此报成网络错误
    Ok((status, response.text().unwrap_or_default()))
}

fn probe_target(client: &reqwest::blocking::Client, target: &ProbeTarget) -> ProfileHealth {
    let mut health = ProfileHealth {
        app: target.app.to_string(),
        id: target.id.clone(),
        name: target.name.clone(),
        host: String::new(),
        level: "skipped".to_string(),
        message: String::new(),
        latency_ms: None,
    };

    if target.api_key.trim().is_empty() || target.base_url.trim().is_empty() {
        health.message = "未填写 API Key 或 Base URL，跳过检测".to_string();
        return health;
    }

    let Ok(url) = models_probe_url(&target.base_url) else {
        health.message = "Base URL 格式非法".to_string();
        return health;
    };
    let Ok(parsed) = reqwest::Url::parse(&url) else {
        health.message = "Base URL 格式非法".to_string();
        return health;
    };
    health.host = url_host_with_port(&parsed);
    let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();

    let started = Instant::now();
    match send_models_probe(client, &url, &host, &target.api_key) {
        Ok((status, body)) => {
            let (level, message) = classify_response(status, &body);
            // 延迟只在真正通了的时候才有参考价值，失败耗时多半是超时上限
            if level == "ok" {
                health.latency_ms = Some(started.elapsed().as_millis() as u64);
            }
            health.level = level;
            health.message = message;
        }
        Err(error) => {
            let (level, message) = classify_transport_error(
                error.is_timeout(),
                error.is_connect(),
                &error.to_string(),
            );
            health.level = level;
            health.message = message;
        }
    }
    health
}

/// 体检要对每套配置发一次 HTTP 请求，26 套配置最坏情况会占用调用线程几十秒。
/// Tauri 的同步命令跑在主线程上，直接同步执行会冻结窗口，因此命令声明为 async，
/// 连读档带探测一并交给阻塞线程池（与 query_provider_balance 的处理一致）。
#[tauri::command]
pub(crate) async fn check_profiles_health(
    app: tauri::AppHandle,
) -> Result<Vec<ProfileHealth>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        check_profiles_health_blocking(collect_targets(&app))
    })
    .await
    .map_err(|e| format!("配置体检任务异常退出: {e}"))?
}

fn check_profiles_health_blocking(targets: Vec<ProbeTarget>) -> Result<Vec<ProfileHealth>, String> {
    if targets.is_empty() {
        return Ok(Vec::new());
    }

    let client = build_http_client(HEALTH_HTTP_TIMEOUT_SECS)?;
    let mut slots: Vec<Option<ProfileHealth>> = vec![None; targets.len()];
    let worker_count = targets.len().min(HEALTH_MAX_CONCURRENCY);
    // 固定数量的线程轮流从共享游标取任务，而不是按批切分：几十套配置里只要有一两个
    // 卡到 10 秒超时，分批就会让整批一起等它，共享队列则让其余线程继续往下跑。
    let cursor = AtomicUsize::new(0);
    let targets = &targets;
    let cursor = &cursor;

    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..worker_count)
            .map(|_| {
                let client = client.clone();
                scope.spawn(move || {
                    let mut done = Vec::new();
                    loop {
                        let index = cursor.fetch_add(1, Ordering::Relaxed);
                        let Some(target) = targets.get(index) else {
                            break;
                        };
                        done.push((index, probe_target(&client, target)));
                    }
                    done
                })
            })
            .collect();
        for handle in handles {
            // 单个探测线程异常退出不该让整次体检失败，丢掉它那几条结果即可
            for (index, health) in handle.join().unwrap_or_default() {
                slots[index] = Some(health);
            }
        }
    });

    let results: Vec<ProfileHealth> = slots.into_iter().flatten().collect();
    let count_of = |level: &str| results.iter().filter(|item| item.level == level).count();
    log_info!(
        "[health] 配置体检完成：共 {} 项，可用 {}，鉴权失败 {}，不可达 {}，存疑 {}，跳过 {}",
        results.len(),
        count_of("ok"),
        count_of("bad"),
        count_of("dead"),
        count_of("warn"),
        count_of("skipped"),
    );
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn success_status_is_ok() {
        let (level, message) = classify_response(200, r#"{"data":[]}"#);
        assert_eq!(level, "ok");
        assert_eq!(message, "可用");
        assert_eq!(classify_response(204, "").0, "ok");
    }

    #[test]
    fn html_body_on_403_must_not_be_reported_as_key_failure() {
        // 实测 superxihe.com 被 Cloudflare 整站拦截，403 回的是一整页 HTML。
        // 判成 bad 会让用户删掉其实还能用的配置，这里必须降级成 warn。
        let body = "<!DOCTYPE html>\n<html><head><title>Just a moment...</title></head></html>";
        let (level, message) = classify_response(403, body);
        assert_eq!(level, "warn");
        assert!(message.contains("WAF"));

        // 前导空白与大小写都不该影响判型
        assert_eq!(
            classify_response(401, "  <HTML><body>Forbidden</body></HTML>").0,
            "warn"
        );
    }

    #[test]
    fn json_403_surfaces_site_reason_verbatim() {
        // 余额不足只需充值，绝不能让用户误以为 Key 废了
        let body = r#"{"error":{"message":"insufficient balance, please top up and retry","type":"invalid_request_error"}}"#;
        let (level, message) = classify_response(403, body);
        assert_eq!(level, "bad");
        assert_eq!(
            message,
            "鉴权失败 HTTP 403：insufficient balance, please top up and retry"
        );
    }

    #[test]
    fn json_401_reads_reason_from_alternative_fields() {
        let (level, message) = classify_response(401, r#"{"message":"API Key 已被禁用"}"#);
        assert_eq!(level, "bad");
        assert_eq!(message, "鉴权失败 HTTP 401：API Key 已被禁用");

        let (_, message) = classify_response(401, r#"{"error":"Invalid token"}"#);
        assert_eq!(message, "鉴权失败 HTTP 401：Invalid token");

        // 站点什么都不肯说时只报状态码，不要替它编原因
        let (level, message) = classify_response(401, "");
        assert_eq!(level, "bad");
        assert_eq!(message, "鉴权失败 HTTP 401");
    }

    #[test]
    fn other_failures_are_warnings() {
        assert_eq!(
            classify_response(500, "upstream error"),
            ("warn".to_string(), "HTTP 500".to_string())
        );
        assert_eq!(classify_response(429, "{}").0, "warn");
        assert_eq!(classify_response(404, "{}").1, "HTTP 404");
    }

    #[test]
    fn long_site_reason_is_truncated_by_characters() {
        let body = format!(r#"{{"error":{{"message":"{}"}}}}"#, "x".repeat(200));
        let (_, message) = classify_response(403, &body);
        assert_eq!(
            message,
            format!("鉴权失败 HTTP 403：{}", "x".repeat(HEALTH_REASON_MAX_CHARS))
        );

        // 中文按字符截断，不能把多字节字符切坏
        let body = format!(r#"{{"message":"{}"}}"#, "余".repeat(200));
        let (_, message) = classify_response(403, &body);
        assert_eq!(
            message,
            format!(
                "鉴权失败 HTTP 403：{}",
                "余".repeat(HEALTH_REASON_MAX_CHARS)
            )
        );
    }

    #[test]
    fn probe_url_respects_existing_path_suffix() {
        assert_eq!(
            models_probe_url("https://relay.test/v1").unwrap(),
            "https://relay.test/v1/models"
        );
        assert_eq!(
            models_probe_url("https://relay.test/v1/models").unwrap(),
            "https://relay.test/v1/models"
        );
        assert_eq!(
            models_probe_url("https://relay.test").unwrap(),
            "https://relay.test/v1/models"
        );
        // 尾部斜杠由 normalize_endpoint_url 统一去掉，不能拼出 //models
        assert_eq!(
            models_probe_url("https://relay.test/v1/").unwrap(),
            "https://relay.test/v1/models"
        );
        assert!(models_probe_url("").is_err());
        assert!(models_probe_url("not a url").is_err());
    }

    #[test]
    fn transport_errors_separate_unreachable_from_unknown() {
        let (level, message) = classify_transport_error(true, false, "operation timed out");
        assert_eq!(level, "dead");
        assert_eq!(message, "请求超时（超过 10 秒无响应）");

        let (level, message) = classify_transport_error(false, true, "tcp connect error");
        assert_eq!(level, "dead");
        assert_eq!(message, "域名无法解析或连接失败（站点可能已下线）");

        let (level, _) = classify_transport_error(
            false,
            false,
            "dns error: failed to lookup address information",
        );
        assert_eq!(level, "dead");

        // 说不清原因的失败不能断言站点已下线
        assert_eq!(
            classify_transport_error(false, false, "invalid gzip body").0,
            "warn"
        );
    }

    #[test]
    fn only_official_anthropic_hosts_use_x_api_key() {
        assert!(is_anthropic_official("api.anthropic.com"));
        assert!(is_anthropic_official("anthropic.com"));
        assert!(!is_anthropic_official("relay.example.com"));
        // 蹭域名的中转站不能被当成官方端点，否则会漏掉 Bearer 头
        assert!(!is_anthropic_official("fake-anthropic.com"));
    }

    #[test]
    fn opencode_blank_base_url_falls_back_to_official_endpoint() {
        assert_eq!(
            opencode_official_base_url("deepseek"),
            Some("https://api.deepseek.com")
        );
        assert_eq!(
            opencode_official_base_url("openai"),
            Some("https://api.openai.com/v1")
        );
        assert_eq!(opencode_official_base_url("zhipuai"), None);
    }

    #[test]
    fn html_detection_ignores_json_bodies() {
        assert!(looks_like_html("<!doctype html><html></html>"));
        assert!(!looks_like_html(r#"{"error":{"message":"nope"}}"#));
        assert!(!looks_like_html(""));
    }
}
