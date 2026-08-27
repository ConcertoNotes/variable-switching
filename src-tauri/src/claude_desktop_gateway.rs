//! Claude Desktop 专用本地 Gateway 运行态与请求路由。

use crate::claude_desktop_provider::{ClaudeDesktopConnectionMode, ClaudeDesktopProfile};
use crate::claude_proxy::{ProxyTarget, ProxyUpstream, UpstreamMode};
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

pub(crate) const SONNET_ROUTE_ID: &str = "claude-sonnet-4-6";
pub(crate) const OPUS_ROUTE_ID: &str = "claude-opus-4-6";
pub(crate) const HAIKU_ROUTE_ID: &str = "claude-haiku-4-5-20251001";

#[derive(Clone)]
pub(crate) struct ClaudeDesktopRuntime {
    pub(crate) profile: ClaudeDesktopProfile,
    pub(crate) token: String,
    pub(crate) primary: ProxyTarget,
    pub(crate) failover: Vec<ProxyTarget>,
    failover_profiles: Vec<ClaudeDesktopProfile>,
}

static DESKTOP_RUNTIME: OnceLock<Mutex<Option<ClaudeDesktopRuntime>>> = OnceLock::new();
static DESKTOP_HEALTH: OnceLock<Mutex<HashMap<String, DesktopUpstreamHealth>>> = OnceLock::new();
static DESKTOP_FAILOVER_COUNT: AtomicU64 = AtomicU64::new(0);

const BREAKER_FAILURE_THRESHOLD: u32 = 3;
const BREAKER_OPEN_SECS: u64 = 60;

#[derive(Clone, Copy, PartialEq, Eq)]
enum DesktopBreakerState {
    Closed,
    Open,
    HalfOpen,
}

impl DesktopBreakerState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Closed => "closed",
            Self::Open => "open",
            Self::HalfOpen => "half_open",
        }
    }
}

struct DesktopUpstreamHealth {
    state: DesktopBreakerState,
    consecutive_failures: u32,
    total_requests: u64,
    total_failures: u64,
    last_error: Option<String>,
    last_error_ts: Option<u64>,
    last_success_ts: Option<u64>,
    state_changed_at: u64,
}

impl DesktopUpstreamHealth {
    fn new() -> Self {
        Self {
            state: DesktopBreakerState::Closed,
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

fn runtime_cell() -> &'static Mutex<Option<ClaudeDesktopRuntime>> {
    DESKTOP_RUNTIME.get_or_init(|| Mutex::new(None))
}

fn target_from_profile(profile: &ClaudeDesktopProfile) -> ProxyTarget {
    ProxyTarget {
        upstream: ProxyUpstream {
            base_url: profile.base_url.clone(),
            api_key: profile.api_key.clone(),
            model: profile.model_id.clone(),
        },
        mode: UpstreamMode::from_api_format(&profile.api_format),
    }
}

pub(crate) fn runtime_from_profiles(
    profile: ClaudeDesktopProfile,
    token: String,
    failover_profiles: Vec<ClaudeDesktopProfile>,
) -> ClaudeDesktopRuntime {
    ClaudeDesktopRuntime {
        primary: target_from_profile(&profile),
        failover: failover_profiles.iter().map(target_from_profile).collect(),
        profile,
        token,
        failover_profiles,
    }
}

pub(crate) fn set_desktop_runtime(runtime: Option<ClaudeDesktopRuntime>) {
    if let Ok(mut guard) = runtime_cell().lock() {
        *guard = runtime;
    }
    reset_desktop_health_state();
}

pub(crate) fn current_desktop_runtime() -> Option<ClaudeDesktopRuntime> {
    runtime_cell().lock().ok().and_then(|guard| guard.clone())
}

pub(crate) fn clear_desktop_runtime() {
    set_desktop_runtime(None);
}

fn desktop_health_cell() -> &'static Mutex<HashMap<String, DesktopUpstreamHealth>> {
    DESKTOP_HEALTH.get_or_init(|| Mutex::new(HashMap::new()))
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn desktop_upstream_key(upstream: &ProxyUpstream) -> String {
    let mut hasher = Sha1::new();
    hasher.update(upstream.base_url.trim_end_matches('/').as_bytes());
    hasher.update(b"\n");
    hasher.update(upstream.api_key.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn desktop_breaker_allows(key: &str) -> bool {
    let now = unix_now();
    let Ok(mut map) = desktop_health_cell().lock() else {
        return true;
    };
    let health = map
        .entry(key.to_string())
        .or_insert_with(DesktopUpstreamHealth::new);
    match health.state {
        DesktopBreakerState::Closed => true,
        DesktopBreakerState::Open
            if now.saturating_sub(health.state_changed_at) >= BREAKER_OPEN_SECS =>
        {
            health.state = DesktopBreakerState::HalfOpen;
            health.state_changed_at = now;
            true
        }
        DesktopBreakerState::Open => false,
        DesktopBreakerState::HalfOpen
            if now.saturating_sub(health.state_changed_at) >= BREAKER_OPEN_SECS =>
        {
            health.state_changed_at = now;
            true
        }
        DesktopBreakerState::HalfOpen => false,
    }
}

fn record_desktop_success(key: &str) {
    let now = unix_now();
    if let Ok(mut map) = desktop_health_cell().lock() {
        let health = map
            .entry(key.to_string())
            .or_insert_with(DesktopUpstreamHealth::new);
        health.state = DesktopBreakerState::Closed;
        health.consecutive_failures = 0;
        health.total_requests += 1;
        health.last_success_ts = Some(now);
    }
}

fn record_desktop_failure(key: &str, error: &str, retryable: bool) {
    let now = unix_now();
    if let Ok(mut map) = desktop_health_cell().lock() {
        let health = map
            .entry(key.to_string())
            .or_insert_with(DesktopUpstreamHealth::new);
        health.total_requests += 1;
        health.total_failures += 1;
        health.last_error = Some(error.chars().take(200).collect());
        health.last_error_ts = Some(now);
        if retryable {
            health.consecutive_failures += 1;
            if health.state == DesktopBreakerState::HalfOpen
                || health.consecutive_failures >= BREAKER_FAILURE_THRESHOLD
            {
                health.state = DesktopBreakerState::Open;
                health.state_changed_at = now;
            }
        } else if health.state == DesktopBreakerState::HalfOpen {
            health.state = DesktopBreakerState::Closed;
            health.consecutive_failures = 0;
        }
    }
}

fn reset_desktop_health_state() {
    if let Ok(mut map) = desktop_health_cell().lock() {
        map.clear();
    }
    DESKTOP_FAILOVER_COUNT.store(0, Ordering::Relaxed);
}

fn masked_api_key(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    if chars.is_empty() {
        String::new()
    } else if chars.len() <= 10 {
        "****".into()
    } else {
        format!(
            "{}****{}",
            chars[..6].iter().collect::<String>(),
            chars[chars.len() - 4..].iter().collect::<String>()
        )
    }
}

fn desktop_health_entry(target: &ProxyTarget, role: &str) -> Value {
    let key = desktop_upstream_key(&target.upstream);
    let map = desktop_health_cell().lock().ok();
    let health = map.as_ref().and_then(|map| map.get(&key));
    json!({
        "role": role,
        "baseUrl": target.upstream.base_url,
        "model": target.upstream.model,
        "apiKey": masked_api_key(&target.upstream.api_key),
        "apiFormat": target.mode.as_str(),
        "state": health.map(|health| health.state.as_str()).unwrap_or("closed"),
        "consecutiveFailures": health.map(|health| health.consecutive_failures).unwrap_or(0),
        "totalRequests": health.map(|health| health.total_requests).unwrap_or(0),
        "totalFailures": health.map(|health| health.total_failures).unwrap_or(0),
        "lastError": health.and_then(|health| health.last_error.clone()),
        "lastErrorTs": health.and_then(|health| health.last_error_ts),
        "lastSuccessTs": health.and_then(|health| health.last_success_ts),
    })
}

fn desktop_gateway_health_snapshot(runtime: Option<&ClaudeDesktopRuntime>) -> Value {
    let mut health = Vec::new();
    if let Some(runtime) = runtime {
        health.push(desktop_health_entry(&runtime.primary, "primary"));
        health.extend(
            runtime
                .failover
                .iter()
                .map(|target| desktop_health_entry(target, "failover")),
        );
    }
    json!({
        "running": crate::claude_proxy::is_running(),
        "port": crate::claude_proxy::CLAUDE_PROXY_PORT,
        "configured": runtime.is_some(),
        "health": health,
        "failoverCount": DESKTOP_FAILOVER_COUNT.load(Ordering::Relaxed),
    })
}

#[tauri::command]
pub(crate) fn get_claude_desktop_gateway_health() -> Value {
    let runtime = current_desktop_runtime();
    desktop_gateway_health_snapshot(runtime.as_ref())
}

#[tauri::command]
pub(crate) fn claude_desktop_gateway_reset_breaker() {
    reset_desktop_health_state();
    crate::app_log(
        "INFO",
        "[claude-desktop-gateway] 熔断器与健康统计已手动重置",
    );
}

pub(crate) fn validate_authorization(
    authorization: Option<&str>,
    expected_token: &str,
) -> Result<(), String> {
    if expected_token.is_empty() || expected_token == "PROXY_MANAGED" {
        return Err("Claude Desktop Gateway Token 未配置".into());
    }
    match authorization.and_then(|value| value.strip_prefix("Bearer ")) {
        Some(token) if token == expected_token => Ok(()),
        _ => Err("Claude Desktop Gateway 鉴权失败".into()),
    }
}

fn model_for_role(profile: &ClaudeDesktopProfile, role: &str) -> Option<String> {
    let preferred = match role {
        "sonnet" => &profile.sonnet_model,
        "opus" => &profile.opus_model,
        "haiku" => &profile.haiku_model,
        _ => return None,
    };
    [
        preferred,
        &profile.model_id,
        &profile.sonnet_model,
        &profile.opus_model,
        &profile.haiku_model,
    ]
    .into_iter()
    .map(|model| model.trim())
    .find(|model| !model.is_empty())
    .map(str::to_string)
}

fn role_from_route(model: &str) -> Option<&'static str> {
    let normalized = model.trim().to_ascii_lowercase();
    [
        ("sonnet", SONNET_ROUTE_ID),
        ("opus", OPUS_ROUTE_ID),
        ("haiku", HAIKU_ROUTE_ID),
    ]
    .into_iter()
    .find_map(|(role, route)| {
        (normalized == route || normalized.starts_with(&format!("{route}-"))).then_some(role)
    })
}

fn role_from_model_id(model: &str) -> Option<&'static str> {
    let normalized = model.trim().to_ascii_lowercase();
    ["sonnet", "opus", "haiku"]
        .into_iter()
        .find(|role| normalized.contains(role))
}

fn is_safe_claude_model_id(model: &str) -> bool {
    let normalized = model.trim().to_ascii_lowercase();
    ["claude-sonnet-", "claude-opus-", "claude-haiku-"]
        .into_iter()
        .any(|prefix| {
            normalized
                .strip_prefix(prefix)
                .is_some_and(|tail| !tail.is_empty())
        })
}

fn configured_model_catalog(profile: &ClaudeDesktopProfile) -> Vec<String> {
    let mut models = Vec::new();
    let candidates = if profile.available_models.is_empty() {
        vec![
            profile.model_id.as_str(),
            profile.sonnet_model.as_str(),
            profile.opus_model.as_str(),
            profile.haiku_model.as_str(),
        ]
    } else {
        profile
            .available_models
            .iter()
            .map(String::as_str)
            .collect()
    };
    for model in candidates {
        let model = model.trim();
        if is_safe_claude_model_id(model) && !models.iter().any(|item| item == model) {
            models.push(model.to_string());
        }
    }
    if models.is_empty() {
        for (role, route) in [
            ("sonnet", SONNET_ROUTE_ID),
            ("opus", OPUS_ROUTE_ID),
            ("haiku", HAIKU_ROUTE_ID),
        ] {
            if model_for_role(profile, role).is_some() {
                models.push(route.to_string());
            }
        }
    }
    models
}

#[cfg(test)]
pub(crate) fn map_model(
    requested_model: &str,
    runtime: &ClaudeDesktopRuntime,
) -> Result<String, String> {
    let role = role_from_route(requested_model)
        .ok_or_else(|| format!("未知的 Claude Desktop 模型路由: {requested_model}"))?;
    model_for_role(&runtime.profile, role)
        .ok_or_else(|| format!("Claude Desktop 的 {role} 模型没有可用映射"))
}

pub(crate) fn model_list_response(runtime: &ClaudeDesktopRuntime) -> Value {
    let data: Vec<Value> = configured_model_catalog(&runtime.profile)
        .into_iter()
        .map(|id| {
            json!({
                "id": id,
                "object": "model",
                "created": 0,
                "owned_by": "varswitch",
            })
        })
        .collect();
    json!({"object": "list", "data": data})
}

fn target_for_profile_and_role(profile: &ClaudeDesktopProfile, role: &str) -> Option<ProxyTarget> {
    let mut target = target_from_profile(profile);
    target.upstream.model = model_for_role(profile, role)?;
    Some(target)
}

fn target_for_profile_and_model(profile: &ClaudeDesktopProfile, model: &str) -> ProxyTarget {
    let mut target = target_from_profile(profile);
    target.upstream.model = model.to_string();
    target
}

pub(crate) fn request_targets(
    requested_model: &str,
    runtime: &ClaudeDesktopRuntime,
) -> Result<Vec<ProxyTarget>, String> {
    if let Some(role) = role_from_route(requested_model) {
        let mut targets = vec![target_for_profile_and_role(&runtime.profile, role)
            .ok_or_else(|| format!("Claude Desktop 的 {role} 模型没有可用映射"))?];
        targets.extend(
            runtime
                .failover_profiles
                .iter()
                .filter_map(|profile| target_for_profile_and_role(profile, role)),
        );
        return Ok(targets);
    }

    if !is_safe_claude_model_id(requested_model) {
        return Err(format!("未知的 Claude Desktop 模型路由: {requested_model}"));
    }

    let mut targets = vec![target_for_profile_and_model(&runtime.profile, requested_model)];
    if let Some(role) = role_from_model_id(requested_model) {
        targets.extend(
            runtime
                .failover_profiles
                .iter()
                .filter_map(|profile| target_for_profile_and_role(profile, role)),
        );
    } else {
        targets.extend(
            runtime
                .failover_profiles
                .iter()
                .map(|profile| target_for_profile_and_model(profile, requested_model)),
        );
    }
    Ok(targets)
}

pub(crate) fn runtime_from_profile_pool(
    profile: ClaudeDesktopProfile,
    token: String,
    pool: Vec<ClaudeDesktopProfile>,
) -> Result<ClaudeDesktopRuntime, String> {
    if profile.connection_mode != ClaudeDesktopConnectionMode::Gateway {
        return Err("只有 Gateway 配置可以创建 Claude Desktop 本地运行态".into());
    }
    Ok(runtime_from_profiles(profile, token, pool))
}

fn authorization_header(request: &tiny_http::Request) -> Option<&str> {
    request
        .headers()
        .iter()
        .find(|header| header.field.equiv("Authorization"))
        .map(|header| header.value.as_str())
}

pub(crate) fn handle_request(request: tiny_http::Request) {
    handle_request_with_runtime(request, current_desktop_runtime());
}

fn handle_request_with_runtime(
    mut request: tiny_http::Request,
    runtime: Option<ClaudeDesktopRuntime>,
) {
    let Some(runtime) = runtime else {
        crate::claude_proxy::respond_error(
            request,
            503,
            "api_error",
            "VarSwitch 未启用 Claude Desktop Gateway 配置",
        );
        return;
    };
    if let Err(message) = validate_authorization(authorization_header(&request), &runtime.token) {
        crate::claude_proxy::respond_error(request, 401, "authentication_error", &message);
        return;
    }

    let path = request
        .url()
        .split('?')
        .next()
        .unwrap_or_default()
        .to_string();
    match (request.method(), path.as_str()) {
        (&tiny_http::Method::Get, "/claude-desktop/v1/models") => {
            crate::claude_proxy::respond_json(
                request,
                200,
                model_list_response(&runtime).to_string(),
            );
        }
        (&tiny_http::Method::Post, "/claude-desktop/v1/messages") => {
            let body = match crate::claude_proxy::read_body(&mut request) {
                Ok(body) => body,
                Err(message) => {
                    crate::claude_proxy::respond_error(
                        request,
                        400,
                        "invalid_request_error",
                        &message,
                    );
                    return;
                }
            };
            let requested_model = match body.get("model").and_then(Value::as_str) {
                Some(model) if !model.trim().is_empty() => model,
                _ => {
                    crate::claude_proxy::respond_error(
                        request,
                        400,
                        "invalid_request_error",
                        "请求缺少 model",
                    );
                    return;
                }
            };
            let candidates = match request_targets(requested_model, &runtime) {
                Ok(candidates) => candidates,
                Err(message) => {
                    crate::claude_proxy::respond_error(
                        request,
                        400,
                        "invalid_request_error",
                        &message,
                    );
                    return;
                }
            };
            forward_messages(request, body, candidates);
        }
        _ => crate::claude_proxy::respond_error(
            request,
            404,
            "not_found_error",
            &format!("未知路径：{path}"),
        ),
    }
}

fn forward_messages(request: tiny_http::Request, body: Value, candidates: Vec<ProxyTarget>) {
    let stream = body.get("stream").and_then(Value::as_bool).unwrap_or(false);
    let mut last_error = None;
    let mut primary_attempted = false;
    for (index, target) in candidates.iter().enumerate() {
        let key = desktop_upstream_key(&target.upstream);
        if !desktop_breaker_allows(&key) {
            continue;
        }
        if index == 0 {
            primary_attempted = true;
        } else {
            DESKTOP_FAILOVER_COUNT.fetch_add(1, Ordering::Relaxed);
        }
        match crate::claude_proxy::try_upstream(target, &body) {
            crate::claude_proxy::AttemptOutcome::Success(response) => {
                record_desktop_success(&key);
                crate::claude_proxy::deliver_response(request, response, stream, target.mode);
                return;
            }
            crate::claude_proxy::AttemptOutcome::Retryable(status, message) => {
                record_desktop_failure(&key, &message, true);
                crate::app_log(
                    "WARN",
                    &format!(
                        "[claude-desktop-gateway] 上游 {} 失败（{message}），尝试下一候选",
                        target.upstream.base_url
                    ),
                );
                last_error = Some((status, message));
            }
            crate::claude_proxy::AttemptOutcome::ClientError(status, message) => {
                record_desktop_failure(&key, &message, false);
                crate::claude_proxy::respond_error(request, status, "api_error", &message);
                return;
            }
        }
    }
    if !primary_attempted {
        if let Some(target) = candidates.first() {
            let key = desktop_upstream_key(&target.upstream);
            match crate::claude_proxy::try_upstream(target, &body) {
                crate::claude_proxy::AttemptOutcome::Success(response) => {
                    record_desktop_success(&key);
                    crate::claude_proxy::deliver_response(request, response, stream, target.mode);
                    return;
                }
                crate::claude_proxy::AttemptOutcome::Retryable(status, message) => {
                    record_desktop_failure(&key, &message, true);
                    last_error = Some((status, message));
                }
                crate::claude_proxy::AttemptOutcome::ClientError(status, message) => {
                    record_desktop_failure(&key, &message, false);
                    crate::claude_proxy::respond_error(request, status, "api_error", &message);
                    return;
                }
            }
        }
    }
    let (status, message) = last_error.unwrap_or((503, "所有上游均不可用".into()));
    crate::claude_proxy::respond_error(request, status, "api_error", &message);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude_desktop_provider::{ClaudeDesktopConnectionMode, ClaudeDesktopProfile};

    fn fixture_profile() -> ClaudeDesktopProfile {
        ClaudeDesktopProfile {
            id: "desktop-provider-1".into(),
            name: "Desktop Gateway".into(),
            api_key: "sk-real-upstream".into(),
            base_url: "https://api.example.com".into(),
            connection_mode: ClaudeDesktopConnectionMode::Gateway,
            api_format: "openai_chat".into(),
            model_id: "upstream-default".into(),
            sonnet_model: "upstream-sonnet".into(),
            opus_model: "upstream-opus".into(),
            haiku_model: String::new(),
            available_models: Vec::new(),
            proxy_failover: false,
            is_active: true,
            created_at: "1".into(),
        }
    }

    fn fixture_runtime() -> ClaudeDesktopRuntime {
        runtime_from_profiles(fixture_profile(), "vsd-good".into(), Vec::new())
    }

    #[test]
    fn gateway_auth_accepts_only_matching_bearer() {
        assert!(validate_authorization(Some("Bearer vsd-good"), "vsd-good").is_ok());
        assert!(validate_authorization(Some("Bearer PROXY_MANAGED"), "vsd-good").is_err());
        assert!(validate_authorization(None, "vsd-good").is_err());
        assert!(validate_authorization(Some("Bearer vsd-other"), "vsd-good").is_err());
    }

    #[test]
    fn role_mapping_prefers_role_then_default_and_rejects_unknown_names() {
        let runtime = fixture_runtime();

        assert_eq!(
            map_model("claude-opus-4-6", &runtime).unwrap(),
            "upstream-opus"
        );
        assert_eq!(
            map_model("claude-haiku-4-5-20251001", &runtime).unwrap(),
            "upstream-default"
        );
        assert_eq!(
            map_model("claude-sonnet-4-6-20260801", &runtime).unwrap(),
            "upstream-sonnet"
        );
        assert!(map_model("gpt-5", &runtime).is_err());
    }

    #[test]
    fn model_catalog_includes_configured_real_claude_models() {
        let mut profile = fixture_profile();
        profile.model_id = "claude-opus-5".into();
        profile.sonnet_model = "claude-sonnet-4-6".into();
        let runtime = runtime_from_profiles(profile, "vsd-good".into(), Vec::new());
        let response = model_list_response(&runtime);
        let ids: Vec<&str> = response["data"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|item| item["id"].as_str())
            .collect();

        assert!(ids.contains(&"claude-opus-5"));
        assert!(ids.contains(&"claude-sonnet-4-6"));
    }

    #[test]
    fn real_claude_model_id_is_forwarded_without_role_fallback() {
        let mut profile = fixture_profile();
        profile.model_id = "claude-opus-5".into();
        profile.opus_model = "legacy-opus".into();
        let runtime = runtime_from_profiles(profile, "vsd-good".into(), Vec::new());

        let targets = request_targets("claude-opus-5", &runtime).unwrap();
        assert_eq!(targets[0].upstream.model, "claude-opus-5");
    }

    #[test]
    fn model_catalog_exposes_only_resolvable_safe_role_ids() {
        let runtime = fixture_runtime();

        let response = model_list_response(&runtime);
        let ids: Vec<&str> = response["data"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|item| item["id"].as_str())
            .collect();

        assert_eq!(
            ids,
            vec![
                "claude-sonnet-4-6",
                "claude-opus-4-6",
                "claude-haiku-4-5-20251001"
            ]
        );
        assert!(!response.to_string().contains("upstream-opus"));
    }

    #[test]
    fn desktop_runtime_does_not_replace_claude_code_runtime() {
        crate::claude_proxy::set_upstream(Some(crate::claude_proxy::ProxyUpstream {
            base_url: "https://code.example.com".into(),
            api_key: "sk-code".into(),
            model: "claude-code-model".into(),
        }));

        set_desktop_runtime(Some(fixture_runtime()));

        assert_eq!(
            crate::claude_proxy::current_upstream().unwrap().model,
            "claude-code-model"
        );
        assert_eq!(
            current_desktop_runtime().unwrap().profile.model_id,
            "upstream-default"
        );
        set_desktop_runtime(None);
        crate::claude_proxy::set_upstream(None);
    }

    fn gateway_http_round_trip(
        runtime: ClaudeDesktopRuntime,
        method: reqwest::Method,
        path: &str,
        authorization: Option<&str>,
        body: Option<serde_json::Value>,
    ) -> (u16, String) {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let address = server.server_addr().to_ip().unwrap();
        let worker = std::thread::spawn(move || {
            let request = server.incoming_requests().next().unwrap();
            handle_request_with_runtime(request, Some(runtime));
        });
        let client = reqwest::blocking::Client::new();
        let mut request = client.request(method, format!("http://{address}{path}"));
        if let Some(authorization) = authorization {
            request = request.header("Authorization", authorization);
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().unwrap();
        let status = response.status().as_u16();
        let text = response.text().unwrap();
        worker.join().unwrap();
        (status, text)
    }

    #[test]
    fn http_models_requires_auth_and_returns_safe_catalog() {
        let (status, _) = gateway_http_round_trip(
            fixture_runtime(),
            reqwest::Method::GET,
            "/claude-desktop/v1/models",
            None,
            None,
        );
        assert_eq!(status, 401);

        let (status, body) = gateway_http_round_trip(
            fixture_runtime(),
            reqwest::Method::GET,
            "/claude-desktop/v1/models",
            Some("Bearer vsd-good"),
            None,
        );
        assert_eq!(status, 200);
        assert!(body.contains("claude-sonnet-4-6"));
        assert!(!body.contains("upstream-sonnet"));
    }

    #[test]
    fn http_messages_rejects_unknown_model_before_forwarding() {
        let (status, body) = gateway_http_round_trip(
            fixture_runtime(),
            reqwest::Method::POST,
            "/claude-desktop/v1/messages",
            Some("Bearer vsd-good"),
            Some(serde_json::json!({
                "model": "gpt-5",
                "max_tokens": 16,
                "messages": []
            })),
        );

        assert_eq!(status, 400);
        assert!(body.contains("未知"));
    }

    #[test]
    fn http_messages_replaces_gateway_auth_with_real_upstream_key() {
        let upstream = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let upstream_address = upstream.server_addr().to_ip().unwrap();
        let upstream_worker = std::thread::spawn(move || {
            let mut request = upstream.incoming_requests().next().unwrap();
            let authorization = request
                .headers()
                .iter()
                .find(|header| header.field.equiv("Authorization"))
                .map(|header| header.value.as_str().to_string());
            let mut body = String::new();
            request.as_reader().read_to_string(&mut body).unwrap();
            let body: serde_json::Value = serde_json::from_str(&body).unwrap();
            assert_eq!(authorization.as_deref(), Some("Bearer sk-real-upstream"));
            assert_eq!(body["model"], "upstream-opus");
            request
                .respond(
                    tiny_http::Response::from_string(
                        serde_json::json!({
                            "id": "chatcmpl-test",
                            "object": "chat.completion",
                            "created": 1,
                            "model": "upstream-opus",
                            "choices": [{
                                "index": 0,
                                "message": {"role": "assistant", "content": "ok"},
                                "finish_reason": "stop"
                            }],
                            "usage": {"prompt_tokens": 1, "completion_tokens": 1}
                        })
                        .to_string(),
                    )
                    .with_header(
                        tiny_http::Header::from_bytes("Content-Type", "application/json").unwrap(),
                    ),
                )
                .unwrap();
        });
        let mut profile = fixture_profile();
        profile.base_url = format!("http://{upstream_address}/v1");
        let runtime = runtime_from_profiles(profile, "vsd-good".into(), Vec::new());

        let (status, body) = gateway_http_round_trip(
            runtime,
            reqwest::Method::POST,
            "/claude-desktop/v1/messages",
            Some("Bearer vsd-good"),
            Some(serde_json::json!({
                "model": "claude-opus-4-6",
                "max_tokens": 16,
                "messages": [{"role": "user", "content": "hi"}]
            })),
        );

        upstream_worker.join().unwrap();
        assert_eq!(status, 200);
        assert!(body.contains("\"type\":\"message\""));
        assert!(body.contains("ok"));
    }

    #[test]
    fn desktop_failover_and_health_are_independent_from_claude_code_reset() {
        reset_desktop_health_state();
        let primary = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let primary_address = primary.server_addr().to_ip().unwrap();
        let primary_worker = std::thread::spawn(move || {
            let request = primary.incoming_requests().next().unwrap();
            request
                .respond(tiny_http::Response::from_string("primary failed").with_status_code(500))
                .unwrap();
        });
        let fallback = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let fallback_address = fallback.server_addr().to_ip().unwrap();
        let fallback_worker = std::thread::spawn(move || {
            let request = fallback.incoming_requests().next().unwrap();
            request
                .respond(
                    tiny_http::Response::from_string(
                        serde_json::json!({
                            "id": "chatcmpl-fallback",
                            "object": "chat.completion",
                            "created": 1,
                            "model": "fallback-model",
                            "choices": [{
                                "index": 0,
                                "message": {"role": "assistant", "content": "fallback ok"},
                                "finish_reason": "stop"
                            }],
                            "usage": {"prompt_tokens": 1, "completion_tokens": 1}
                        })
                        .to_string(),
                    )
                    .with_header(
                        tiny_http::Header::from_bytes("Content-Type", "application/json").unwrap(),
                    ),
                )
                .unwrap();
        });
        let mut primary_profile = fixture_profile();
        primary_profile.base_url = format!("http://{primary_address}/v1");
        let mut fallback_profile = fixture_profile();
        fallback_profile.id = "desktop-fallback".into();
        fallback_profile.base_url = format!("http://{fallback_address}/v1");
        fallback_profile.opus_model = "fallback-model".into();
        let runtime =
            runtime_from_profiles(primary_profile, "vsd-good".into(), vec![fallback_profile]);

        let (status, body) = gateway_http_round_trip(
            runtime.clone(),
            reqwest::Method::POST,
            "/claude-desktop/v1/messages",
            Some("Bearer vsd-good"),
            Some(serde_json::json!({
                "model": "claude-opus-4-6",
                "max_tokens": 16,
                "messages": [{"role": "user", "content": "hi"}]
            })),
        );
        primary_worker.join().unwrap();
        fallback_worker.join().unwrap();
        assert_eq!(status, 200);
        assert!(body.contains("fallback ok"));
        assert_eq!(
            desktop_gateway_health_snapshot(Some(&runtime))["failoverCount"],
            1
        );

        crate::claude_proxy::set_upstream(None);

        assert_eq!(
            desktop_gateway_health_snapshot(Some(&runtime))["failoverCount"],
            1
        );
        reset_desktop_health_state();
    }
}
