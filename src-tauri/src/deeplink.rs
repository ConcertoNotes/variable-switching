//! 深链导入 varswitch://（从 lib.rs 拆分，逻辑未改动）。

use crate::*;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

// ── Deep Link（varswitch:// 一键导入）─────────────────
// URL 契约：
//   varswitch://import/profile?app=<claude|codex|gemini|grok>&payload=<base64url(JSON)>
//     payload JSON：{ name, apiKey, baseUrl?, model?, 其他应用特有字段可选 }
//   varswitch://import/mcp?payload=<base64url(JSON)>
//     payload JSON：{ name, config, apps? }
// 安全流程：后端只解析校验，通过 deeplink-import 事件交前端弹窗确认，
// 用户点「确认导入」后才调用 apply_deep_link_import 真正写入；旧版仅新增，
// v1 同名可按确认选择重命名新增或授权覆盖，所有导入都不激活、不切换。

/// 深链导入请求的解析结果（emit 给前端的 payload 结构）
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct DeepLinkImport {
    /// "profile" | "mcp"
    pub(crate) kind: String,
    /// profile 时为 claude|codex|gemini|grok；mcp 时为空串
    pub(crate) app: String,
    /// 解码后的 payload JSON
    pub(crate) data: serde_json::Value,
    pub(crate) source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) conflict: Option<DeepLinkConflict>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DeepLinkConflict {
    pub(crate) existing_name: String,
    pub(crate) suggested_name: String,
    pub(crate) confirmation_token: String,
}

const OVERWRITE_CONFIRMATION_TTL: Duration = Duration::from_secs(10 * 60);
const MAX_PENDING_OVERWRITE_CONFIRMATIONS: usize = 128;

#[derive(Debug, Clone)]
struct PendingOverwriteConfirmation {
    app: String,
    profile_id: String,
    name: String,
    data_fingerprint: (u64, u64),
    created_at: Instant,
}

impl PendingOverwriteConfirmation {
    fn matches_target(&self, profile_id: &str, name: &str) -> bool {
        self.profile_id == profile_id && self.name == name
    }
}

type PendingOverwriteConfirmations = HashMap<String, PendingOverwriteConfirmation>;

static PENDING_OVERWRITE_CONFIRMATIONS: OnceLock<Arc<Mutex<PendingOverwriteConfirmations>>> =
    OnceLock::new();

fn pending_overwrite_confirmations() -> Arc<Mutex<PendingOverwriteConfirmations>> {
    PENDING_OVERWRITE_CONFIRMATIONS
        .get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
        .clone()
}

fn append_canonical_json(value: &serde_json::Value, output: &mut String) {
    match value {
        serde_json::Value::Null => output.push_str("null"),
        serde_json::Value::Bool(value) => output.push_str(if *value { "true" } else { "false" }),
        serde_json::Value::Number(value) => output.push_str(&value.to_string()),
        serde_json::Value::String(value) => {
            output.push_str(&serde_json::to_string(value).expect("JSON 字符串序列化不应失败"));
        }
        serde_json::Value::Array(values) => {
            output.push('[');
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                append_canonical_json(value, output);
            }
            output.push(']');
        }
        serde_json::Value::Object(values) => {
            output.push('{');
            let mut keys: Vec<&String> = values.keys().collect();
            keys.sort_unstable();
            for (index, key) in keys.into_iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                output.push_str(&serde_json::to_string(key).expect("JSON key 序列化不应失败"));
                output.push(':');
                append_canonical_json(&values[key], output);
            }
            output.push('}');
        }
    }
}

fn canonical_import_data_fingerprint(data: &serde_json::Value) -> (u64, u64) {
    let mut canonical = String::new();
    append_canonical_json(data, &mut canonical);

    let mut first = std::collections::hash_map::DefaultHasher::new();
    0x6f76_6572_7772_6974_u64.hash(&mut first);
    canonical.hash(&mut first);
    let mut second = std::collections::hash_map::DefaultHasher::new();
    0x636f_6e66_6972_6d21_u64.hash(&mut second);
    canonical.hash(&mut second);
    (first.finish(), second.finish())
}

fn insert_pending_overwrite_confirmation(
    pending: &mut PendingOverwriteConfirmations,
    token: String,
    confirmation: PendingOverwriteConfirmation,
    now: Instant,
) {
    pending.retain(|_, existing| {
        now.saturating_duration_since(existing.created_at) <= OVERWRITE_CONFIRMATION_TTL
    });
    while pending.len() >= MAX_PENDING_OVERWRITE_CONFIRMATIONS {
        let Some(oldest) = pending
            .iter()
            .min_by_key(|(_, existing)| existing.created_at)
            .map(|(token, _)| token.clone())
        else {
            break;
        };
        pending.remove(&oldest);
    }
    pending.entry(token).or_insert(confirmation);
}

fn register_overwrite_confirmation_at(
    app: &str,
    profile_id: &str,
    name: &str,
    data: &serde_json::Value,
    now: Instant,
) -> String {
    let store = pending_overwrite_confirmations();
    let mut pending = store
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    register_overwrite_confirmation_in(&mut pending, app, profile_id, name, data, now)
}

fn register_overwrite_confirmation_in(
    pending: &mut PendingOverwriteConfirmations,
    app: &str,
    profile_id: &str,
    name: &str,
    data: &serde_json::Value,
    now: Instant,
) -> String {
    let token = uuid::Uuid::new_v4().to_string();
    insert_pending_overwrite_confirmation(
        pending,
        token.clone(),
        PendingOverwriteConfirmation {
            app: app.to_string(),
            profile_id: profile_id.to_string(),
            name: name.to_string(),
            data_fingerprint: canonical_import_data_fingerprint(data),
            created_at: now,
        },
        now,
    );
    token
}

fn register_overwrite_confirmation(
    app: &str,
    profile_id: &str,
    name: &str,
    data: &serde_json::Value,
) -> String {
    register_overwrite_confirmation_at(app, profile_id, name, data, Instant::now())
}

struct OverwriteConfirmationLease {
    store: Arc<Mutex<PendingOverwriteConfirmations>>,
    token: String,
    confirmation: Option<PendingOverwriteConfirmation>,
}

impl OverwriteConfirmationLease {
    fn matches_target(&self, profile_id: &str, name: &str) -> bool {
        self.confirmation
            .as_ref()
            .is_some_and(|confirmation| confirmation.matches_target(profile_id, name))
    }

    fn commit(mut self) {
        self.confirmation = None;
    }
}

impl Drop for OverwriteConfirmationLease {
    fn drop(&mut self) {
        let Some(confirmation) = self.confirmation.take() else {
            return;
        };
        let now = Instant::now();
        if now.saturating_duration_since(confirmation.created_at) > OVERWRITE_CONFIRMATION_TTL {
            return;
        }
        let mut pending = self
            .store
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        insert_pending_overwrite_confirmation(&mut pending, self.token.clone(), confirmation, now);
    }
}

fn acquire_overwrite_confirmation_from(
    store: Arc<Mutex<PendingOverwriteConfirmations>>,
    token: &str,
    app: &str,
    name: &str,
    data: &serde_json::Value,
    now: Instant,
) -> Result<OverwriteConfirmationLease, String> {
    let confirmation = {
        let mut pending = store
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        pending
            .remove(token)
            .ok_or("覆盖确认已失效，请重新发起导入")?
    };
    if now.saturating_duration_since(confirmation.created_at) > OVERWRITE_CONFIRMATION_TTL
        || confirmation.app != app
        || confirmation.name != name
        || confirmation.data_fingerprint != canonical_import_data_fingerprint(data)
    {
        return Err("覆盖确认已失效，请重新发起导入".into());
    }
    Ok(OverwriteConfirmationLease {
        store,
        token: token.to_string(),
        confirmation: Some(confirmation),
    })
}

fn acquire_overwrite_confirmation(
    token: &str,
    app: &str,
    name: &str,
    data: &serde_json::Value,
) -> Result<OverwriteConfirmationLease, String> {
    acquire_overwrite_confirmation_from(
        pending_overwrite_confirmations(),
        token,
        app,
        name,
        data,
        Instant::now(),
    )
}

/// 极简 percent 解码：只处理 %XX 十六进制序列，其余字节原样保留。
/// 深链 query 里只有 base64url 字符与少量安全字符，无需完整 URL 解码器。
pub(crate) fn percent_decode_component(input: &str) -> String {
    fn hex_val(b: u8) -> Option<u8> {
        match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(b - b'a' + 10),
            b'A'..=b'F' => Some(b - b'A' + 10),
            _ => None,
        }
    }
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                out.push(hi * 16 + lo);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// 解析 query 字符串为键值表（键值都做 percent 解码，重复键取最后一个）
pub(crate) fn parse_query_params(query: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        map.insert(
            percent_decode_component(key),
            percent_decode_component(value),
        );
    }
    map
}

/// base64url 解码（同时兼容带 = 填充与不带填充两种写法）
pub(crate) fn decode_base64url(payload: &str) -> Result<Vec<u8>, String> {
    let trimmed = payload.trim().trim_end_matches('=');
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(trimmed.as_bytes())
        .map_err(|e| format!("payload 不是合法的 base64url：{e}"))
}

/// 校验 profile 导入 payload：name / apiKey 必填，baseUrl 若提供必须是 http(s) 地址
pub(crate) fn validate_profile_payload(data: &serde_json::Value) -> Result<(), String> {
    let obj = data.as_object().ok_or("payload 必须是 JSON 对象")?;
    let name = obj
        .get("name")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .unwrap_or("");
    if name.is_empty() {
        return Err("payload 缺少 name 字段".into());
    }
    let api_key = obj
        .get("apiKey")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .unwrap_or("");
    if api_key.is_empty() {
        return Err("payload 缺少 apiKey 字段".into());
    }
    if let Some(base) = obj.get("baseUrl") {
        let base = base.as_str().ok_or("baseUrl 必须是字符串")?.trim();
        if !base.is_empty() && !base.starts_with("http://") && !base.starts_with("https://") {
            return Err("baseUrl 必须以 http:// 或 https:// 开头".into());
        }
    }
    Ok(())
}

/// 校验 mcp 导入 payload：name 必填，config 必须是对象，apps 可选但须是对象
pub(crate) fn validate_mcp_payload(data: &serde_json::Value) -> Result<(), String> {
    let obj = data.as_object().ok_or("payload 必须是 JSON 对象")?;
    let name = obj
        .get("name")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .unwrap_or("");
    if name.is_empty() {
        return Err("payload 缺少 name 字段".into());
    }
    if !obj.get("config").map(|v| v.is_object()).unwrap_or(false) {
        return Err("payload 缺少 config 对象".into());
    }
    if let Some(apps) = obj.get("apps") {
        if !apps.is_object() {
            return Err("apps 必须是对象".into());
        }
    }
    Ok(())
}

fn query_params(url: &reqwest::Url) -> HashMap<String, String> {
    url.query_pairs()
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect()
}

fn required_param(params: &HashMap<String, String>, key: &str) -> Result<String, String> {
    let value = params.get(key).map(|value| value.trim()).unwrap_or("");
    if value.is_empty() {
        Err(format!("缺少必填参数 {key}"))
    } else {
        Ok(value.to_string())
    }
}

fn optional_param(params: &HashMap<String, String>, key: &str) -> Option<String> {
    params
        .get(key)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn parse_http_endpoint(raw: &str) -> Result<String, String> {
    let has_http_scheme = raw
        .split_once("://")
        .map(|(scheme, _)| {
            scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https")
        })
        .unwrap_or(false);
    if !has_http_scheme {
        return Err("endpoint 必须是合法的绝对 HTTP(S) URL".into());
    }
    let endpoint = reqwest::Url::parse(raw)
        .map_err(|_| "endpoint 必须是合法的绝对 HTTP(S) URL".to_string())?;
    if !matches!(endpoint.scheme(), "http" | "https") {
        return Err("endpoint 必须使用 HTTP 或 HTTPS".into());
    }
    Ok(endpoint.to_string())
}

fn parse_cc_switch_v1_provider(url: &reqwest::Url) -> Result<DeepLinkImport, String> {
    let params = query_params(url);
    if required_param(&params, "resource")? != "provider" {
        return Err("resource 必须是 provider".into());
    }
    let app = required_param(&params, "app")?.to_ascii_lowercase();
    if !matches!(app.as_str(), "claude" | "codex" | "gemini") {
        return Err(format!("不支持的目标应用：{app}"));
    }
    let name = required_param(&params, "name")?;
    let endpoint = parse_http_endpoint(&required_param(&params, "endpoint")?)?;
    let api_key = required_param(&params, "apiKey")?;
    let model = required_param(&params, "model")?;
    let homepage = required_param(&params, "homepage")?;
    parse_http_endpoint(&homepage)?;
    if required_param(&params, "enabled")? != "true" {
        return Err("enabled 必须为 true".into());
    }
    Ok(DeepLinkImport {
        kind: "profile".into(),
        app,
        data: serde_json::json!({
            "name": name,
            "apiKey": api_key,
            "baseUrl": endpoint,
            "model": model,
            "haikuModel": optional_param(&params, "haikuModel"),
            "sonnetModel": optional_param(&params, "sonnetModel"),
            "opusModel": optional_param(&params, "opusModel"),
            "homepage": homepage,
            "enabled": true,
        }),
        source: "cc_switch_v1".into(),
        conflict: None,
    })
}

/// 返回固定白名单日志目标，绝不拼接或回显原始 URL 的任何部分。
pub(crate) fn safe_deep_link_log_target(url: &str) -> &'static str {
    let Ok(parsed) = reqwest::Url::parse(url.trim()) else {
        return "[invalid deep link]";
    };
    if !parsed.scheme().eq_ignore_ascii_case("varswitch")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return "[invalid deep link]";
    }
    let host = parsed.host_str().map(str::to_ascii_lowercase);
    let path = parsed.path().trim_end_matches('/').to_ascii_lowercase();
    match (host.as_deref(), path.as_str()) {
        (Some("v1"), "/import") => "varswitch://v1/import",
        (Some("import"), "/profile") | (None, "import/profile") => "varswitch://import/profile",
        (Some("import"), "/mcp") | (None, "import/mcp") => "varswitch://import/mcp",
        _ => "varswitch://[unsupported]",
    }
}

/// 解析并校验一条 varswitch:// 深链（纯函数，便于单测）。
/// 容忍大小写 scheme 与尾部斜杠；解析失败返回可读的中文错误。
pub(crate) fn parse_deep_link_url(url: &str) -> Result<DeepLinkImport, String> {
    let url = url.trim();
    let parsed =
        reqwest::Url::parse(url).map_err(|_| "不是合法的 varswitch:// 协议链接".to_string())?;
    if !parsed.scheme().eq_ignore_ascii_case("varswitch") {
        return Err("不是 varswitch:// 协议链接".into());
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("深链不能包含 userinfo".into());
    }
    let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
    if host == "v1" {
        if parsed.fragment().is_some() {
            return Err("v1 深链不能包含 fragment".into());
        }
        if parsed.path() != "/import" {
            return Err("不支持的 v1 深链路径或 action".into());
        }
        return parse_cc_switch_v1_provider(&parsed);
    }
    if host.starts_with('v') && host[1..].parse::<u32>().is_ok() {
        return Err("不支持的深链版本".into());
    }
    let location = match parsed.host_str() {
        Some(host) => format!("{host}{}", parsed.path()),
        None => parsed.path().to_string(),
    };
    let location = location.trim_matches('/').to_ascii_lowercase();
    let params = parse_query_params(parsed.query().unwrap_or_default());

    let payload_raw = params.get("payload").ok_or("缺少 payload 参数")?;
    let decoded = decode_base64url(payload_raw)?;
    let text =
        String::from_utf8(decoded).map_err(|_| "payload 不是合法的 UTF-8 文本".to_string())?;
    let data: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("payload JSON 解析失败：{e}"))?;

    match location.as_str() {
        "import/profile" => {
            let app = params
                .get("app")
                .map(|s| s.trim().to_ascii_lowercase())
                .unwrap_or_default();
            if app.is_empty() {
                return Err("缺少 app 参数".into());
            }
            if !matches!(app.as_str(), "claude" | "codex" | "gemini" | "grok") {
                return Err(format!("不支持的目标应用：{app}"));
            }
            validate_profile_payload(&data)?;
            Ok(DeepLinkImport {
                kind: "profile".into(),
                app,
                data,
                source: "legacy".into(),
                conflict: None,
            })
        }
        "import/mcp" => {
            validate_mcp_payload(&data)?;
            Ok(DeepLinkImport {
                kind: "mcp".into(),
                app: String::new(),
                data,
                source: "legacy".into(),
                conflict: None,
            })
        }
        other => Err(format!("不支持的深链路径：{other}")),
    }
}

/// 需要新增且重名时自动追加 " (2)"、" (3)" 后缀直到不冲突。
pub(crate) fn unique_import_name(existing: &[String], wanted: &str) -> String {
    let wanted = wanted.trim();
    let taken: HashSet<&str> = existing.iter().map(|s| s.as_str()).collect();
    if !taken.contains(wanted) {
        return wanted.to_string();
    }
    for n in 2..1000u32 {
        let candidate = format!("{wanted} ({n})");
        if !taken.contains(candidate.as_str()) {
            return candidate;
        }
    }
    // 理论上到不了这里，兜底用 uuid 保证唯一
    format!("{wanted} ({})", uuid::Uuid::new_v4())
}

#[derive(Debug, PartialEq)]
pub(crate) enum ProfileImportResolution {
    Add(String),
    Overwrite(String),
}

pub(crate) fn detect_profile_conflict(
    existing: &[String],
    wanted: &str,
    confirmation_token: String,
) -> Option<DeepLinkConflict> {
    let wanted = wanted.trim();
    existing
        .iter()
        .any(|name| name == wanted)
        .then(|| DeepLinkConflict {
            existing_name: wanted.to_string(),
            suggested_name: unique_import_name(existing, wanted),
            confirmation_token,
        })
}

pub(crate) fn resolve_profile_import(
    existing: &[String],
    wanted: &str,
    source: &str,
    action: Option<&str>,
) -> Result<ProfileImportResolution, String> {
    let wanted = wanted.trim();
    if !matches!(source, "legacy" | "cc_switch_v1") {
        return Err("不支持的导入来源".into());
    }
    match action {
        Some("overwrite") if source == "legacy" => Err("旧版深链不支持覆盖同名配置".into()),
        Some("overwrite") if source == "cc_switch_v1" => {
            if existing.iter().any(|name| name == wanted) {
                Ok(ProfileImportResolution::Overwrite(wanted.to_string()))
            } else {
                Err("同名配置已变化，请重新发起导入".into())
            }
        }
        Some("rename") | None => Ok(ProfileImportResolution::Add(unique_import_name(
            existing, wanted,
        ))),
        Some(_) => Err("不支持的重名处理方式".into()),
    }
}

fn required_profile_value(data: &serde_json::Value, key: &str) -> Result<String, String> {
    let value = data
        .get(key)
        .and_then(|value| value.as_str())
        .map(str::trim)
        .unwrap_or("");
    if value.is_empty() {
        Err(format!("payload 缺少 {key} 字段"))
    } else {
        Ok(value.to_string())
    }
}

fn optional_profile_value(data: &serde_json::Value, key: &str) -> Option<String> {
    data.get(key)
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(crate) fn merge_claude_v1_profile(
    mut existing: Profile,
    data: &serde_json::Value,
) -> Result<Profile, String> {
    existing.name = required_profile_value(data, "name")?;
    existing.api_key = required_profile_value(data, "apiKey")?;
    existing.base_url = resolve_base_url_or_default(
        &required_profile_value(data, "baseUrl")?,
        DEFAULT_ANTHROPIC_BASE_URL,
    );
    existing.model_id = required_profile_value(data, "model")?;
    if let Some(model) = optional_profile_value(data, "sonnetModel") {
        existing.sonnet_model = model;
    }
    if let Some(model) = optional_profile_value(data, "opusModel") {
        existing.opus_model = model;
    }
    if let Some(model) = optional_profile_value(data, "haikuModel") {
        existing.haiku_model = model;
    }
    Ok(existing)
}

pub(crate) fn merge_codex_v1_profile(
    mut existing: CodexProfile,
    data: &serde_json::Value,
) -> Result<CodexProfile, String> {
    existing.name = required_profile_value(data, "name")?;
    existing.api_key = required_profile_value(data, "apiKey")?;
    existing.base_url = resolve_base_url_or_default(
        &required_profile_value(data, "baseUrl")?,
        DEFAULT_OPENAI_BASE_URL,
    );
    existing.model = required_profile_value(data, "model")?;
    Ok(existing)
}

pub(crate) fn merge_gemini_v1_profile(
    mut existing: GeminiProfile,
    data: &serde_json::Value,
) -> Result<GeminiProfile, String> {
    existing.name = required_profile_value(data, "name")?;
    existing.api_key = required_profile_value(data, "apiKey")?;
    existing.base_url = required_profile_value(data, "baseUrl")?
        .trim_end_matches('/')
        .to_string();
    existing.model = required_profile_value(data, "model")?;
    Ok(existing)
}

fn existing_profile_identities(
    handle: &tauri::AppHandle,
    app: &str,
) -> Result<Vec<(String, String)>, String> {
    match app {
        "claude" => Ok(read_profiles(handle)
            .profiles
            .iter()
            .map(|profile| (profile.id.clone(), profile.name.clone()))
            .collect()),
        "codex" => Ok(read_codex_profiles(handle)
            .profiles
            .iter()
            .map(|profile| (profile.id.clone(), profile.name.clone()))
            .collect()),
        "gemini" => Ok(read_gemini_profiles(handle)
            .profiles
            .iter()
            .map(|profile| (profile.id.clone(), profile.name.clone()))
            .collect()),
        other => Err(format!("不支持的目标应用：{other}")),
    }
}

pub(crate) fn existing_profile_names(
    handle: &tauri::AppHandle,
    app: &str,
) -> Result<Vec<String>, String> {
    Ok(existing_profile_identities(handle, app)?
        .into_iter()
        .map(|(_, name)| name)
        .collect())
}

fn validate_cc_switch_v1_profile_payload(data: &serde_json::Value) -> Result<(), String> {
    required_profile_value(data, "name")?;
    required_profile_value(data, "apiKey")?;
    let endpoint = required_profile_value(data, "baseUrl")?;
    parse_http_endpoint(&endpoint)?;
    required_profile_value(data, "model")?;
    let homepage = required_profile_value(data, "homepage")?;
    parse_http_endpoint(&homepage)?;
    if data.get("enabled").and_then(|value| value.as_bool()) != Some(true) {
        return Err("enabled 必须为 true".into());
    }
    Ok(())
}

fn validate_import_source(kind: &str, source: &str) -> Result<(), String> {
    match (kind, source) {
        ("profile", "legacy" | "cc_switch_v1") => Ok(()),
        ("profile", _) => Err("不支持的导入来源".into()),
        ("mcp", "legacy") => Ok(()),
        ("mcp", _) => Err("MCP 导入仅支持 legacy 来源".into()),
        _ => Ok(()),
    }
}

/// 处理一条运行期收到的深链 URL：
/// 解析成功 → emit "deeplink-import" 事件交前端弹窗确认，并把主窗口拉到前台；
/// 解析失败 → 只写日志 + emit "deeplink-import-error" 给前端 toast，绝不崩溃。
pub(crate) fn handle_deep_link_url(app: &tauri::AppHandle, url: &str) {
    let visible = safe_deep_link_log_target(url);
    match parse_deep_link_url(url) {
        Ok(mut import) => {
            if import.source == "cc_switch_v1" && import.kind == "profile" {
                match existing_profile_identities(app, &import.app) {
                    Ok(existing) => {
                        let wanted = import.data["name"].as_str().unwrap_or_default();
                        if let Some((profile_id, _)) =
                            existing.iter().find(|(_, name)| name == wanted)
                        {
                            let token = register_overwrite_confirmation(
                                &import.app,
                                profile_id,
                                wanted,
                                &import.data,
                            );
                            let names: Vec<String> =
                                existing.iter().map(|(_, name)| name.clone()).collect();
                            import.conflict = detect_profile_conflict(&names, wanted, token);
                        }
                    }
                    Err(err) => log_error!("[deep-link] 读取现有配置失败：{err}"),
                }
            }
            log_info!(
                "[deep-link] 解析成功：{visible}（kind={}，app={}）",
                import.kind,
                import.app
            );
            if let Err(e) = app.emit("deeplink-import", &import) {
                log_error!("[deep-link] 事件发送失败：{e}");
            }
            focus_main_window(app);
        }
        Err(err) => {
            // err 可能包含来自 path/query 的无效值，只把固定白名单目标写入日志；
            // 具体错误仅通过前端事件展示，不落盘。
            log_error!("[deep-link] 解析失败（{visible}）");
            let _ = app.emit(
                "deeplink-import-error",
                serde_json::json!({ "message": err }),
            );
            focus_main_window(app);
        }
    }
}

/// 前端确认后真正执行深链导入。
/// 复用现有 add_* / save_unified_mcp_server 命令的内部逻辑：
/// 默认新增（不激活、不切换），v1 同名配置可在确认后安全覆盖；返回一句可读的结果描述。
#[tauri::command]
pub(crate) fn apply_deep_link_import(
    handle: tauri::AppHandle,
    kind: String,
    app: String,
    data: serde_json::Value,
    source: Option<String>,
    conflict_action: Option<String>,
    conflict_token: Option<String>,
) -> Result<String, String> {
    match kind.as_str() {
        "profile" => {
            let source = source.unwrap_or_else(|| "legacy".into());
            validate_import_source("profile", &source)?;
            // 与解析阶段相同的校验，防止绕过事件流程直接调用时写入脏数据。
            if source == "cc_switch_v1" {
                validate_cc_switch_v1_profile_payload(&data)?;
            } else {
                validate_profile_payload(&data)?;
            }
            let obj = data.as_object().ok_or("payload 必须是 JSON 对象")?;
            let get = |key: &str| -> String {
                obj.get(key)
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string()
            };
            let get_opt = |key: &str| -> Option<String> {
                let value = get(key);
                if value.is_empty() {
                    None
                } else {
                    Some(value)
                }
            };
            let api_key = get("apiKey");
            let base_url = get("baseUrl");
            let wanted = get("name");
            let mut overwrite_lease = if conflict_action.as_deref() == Some("overwrite") {
                if source != "cc_switch_v1" {
                    return Err("旧版深链不支持覆盖同名配置".into());
                }
                let token = conflict_token
                    .as_deref()
                    .map(str::trim)
                    .filter(|token| !token.is_empty())
                    .ok_or("缺少有效的覆盖确认，请重新发起导入")?;
                Some(acquire_overwrite_confirmation(token, &app, &wanted, &data)?)
            } else {
                if conflict_token.is_some() {
                    return Err("覆盖确认 token 仅可用于覆盖操作".into());
                }
                None
            };
            let result: Result<String, String> = (|| match app.as_str() {
                "claude" => {
                    let names = existing_profile_names(&handle, "claude")?;
                    match resolve_profile_import(
                        &names,
                        &wanted,
                        &source,
                        conflict_action.as_deref(),
                    )? {
                        ProfileImportResolution::Add(name) => {
                            let profile = add_profile(
                                handle.clone(),
                                name,
                                api_key,
                                base_url,
                                get_opt("model"),
                                get_opt("apiFormat"),
                                get_opt("sonnetModel"),
                                get_opt("opusModel"),
                                get_opt("haikuModel"),
                                obj.get("proxyFailover").and_then(|v| v.as_bool()),
                                obj.get("proxyTakeover").and_then(|v| v.as_bool()),
                            )?;
                            Ok(format!("已添加 Claude 配置「{}」", profile.name))
                        }
                        ProfileImportResolution::Overwrite(name) => {
                            let confirmation = overwrite_lease
                                .as_ref()
                                .ok_or("缺少有效的覆盖确认，请重新发起导入")?;
                            let mut profiles = read_profiles(&handle);
                            let profile = profiles
                                .profiles
                                .iter_mut()
                                .find(|profile| {
                                    confirmation.matches_target(&profile.id, &profile.name)
                                })
                                .ok_or("同名配置已变化，请重新发起导入")?;
                            *profile = merge_claude_v1_profile(profile.clone(), &data)?;
                            write_profiles(&handle, &profiles)?;
                            Ok(format!("已覆盖 Claude 配置「{name}」"))
                        }
                    }
                }
                "codex" => {
                    let names = existing_profile_names(&handle, "codex")?;
                    match resolve_profile_import(
                        &names,
                        &wanted,
                        &source,
                        conflict_action.as_deref(),
                    )? {
                        ProfileImportResolution::Add(name) => {
                            let profile = add_codex_profile(
                                handle.clone(),
                                name,
                                api_key,
                                base_url,
                                get_opt("authMode"),
                                get_opt("wireApi"),
                                get_opt("model"),
                                get_opt("providerName"),
                                get_opt("imageApiKey"),
                                get_opt("imageBaseUrl"),
                            )?;
                            Ok(format!("已添加 Codex 配置「{}」", profile.name))
                        }
                        ProfileImportResolution::Overwrite(name) => {
                            let confirmation = overwrite_lease
                                .as_ref()
                                .ok_or("缺少有效的覆盖确认，请重新发起导入")?;
                            let mut profiles = read_codex_profiles(&handle);
                            let profile = profiles
                                .profiles
                                .iter_mut()
                                .find(|profile| {
                                    confirmation.matches_target(&profile.id, &profile.name)
                                })
                                .ok_or("同名配置已变化，请重新发起导入")?;
                            *profile = merge_codex_v1_profile(profile.clone(), &data)?;
                            write_codex_profiles(&handle, &profiles)?;
                            Ok(format!("已覆盖 Codex 配置「{name}」"))
                        }
                    }
                }
                "gemini" => {
                    let names = existing_profile_names(&handle, "gemini")?;
                    match resolve_profile_import(
                        &names,
                        &wanted,
                        &source,
                        conflict_action.as_deref(),
                    )? {
                        ProfileImportResolution::Add(name) => {
                            let profile = add_gemini_profile(
                                handle.clone(),
                                name,
                                api_key,
                                base_url,
                                get_opt("model"),
                            )?;
                            Ok(format!("已添加 Gemini 配置「{}」", profile.name))
                        }
                        ProfileImportResolution::Overwrite(name) => {
                            let confirmation = overwrite_lease
                                .as_ref()
                                .ok_or("缺少有效的覆盖确认，请重新发起导入")?;
                            let mut profiles = read_gemini_profiles(&handle);
                            let profile = profiles
                                .profiles
                                .iter_mut()
                                .find(|profile| {
                                    confirmation.matches_target(&profile.id, &profile.name)
                                })
                                .ok_or("同名配置已变化，请重新发起导入")?;
                            *profile = merge_gemini_v1_profile(profile.clone(), &data)?;
                            write_gemini_profiles(&handle, &profiles)?;
                            Ok(format!("已覆盖 Gemini 配置「{name}」"))
                        }
                    }
                }
                "grok" => {
                    if source != "legacy" {
                        return Err("不支持的目标应用：grok".into());
                    }
                    let names: Vec<String> = read_grok_profiles(&handle)
                        .profiles
                        .iter()
                        .map(|p| p.name.clone())
                        .collect();
                    let name = match resolve_profile_import(
                        &names,
                        &wanted,
                        &source,
                        conflict_action.as_deref(),
                    )? {
                        ProfileImportResolution::Add(name) => name,
                        ProfileImportResolution::Overwrite(_) => {
                            unreachable!("legacy 覆盖已被拒绝")
                        }
                    };
                    let profile = add_grok_profile(
                        handle.clone(),
                        name,
                        api_key,
                        base_url,
                        get_opt("model"),
                        get_opt("apiBackend"),
                    )?;
                    Ok(format!("已添加 Grok 配置「{}」", profile.name))
                }
                other => Err(format!("不支持的目标应用：{other}")),
            })();
            if result.is_ok() {
                if let Some(lease) = overwrite_lease.take() {
                    lease.commit();
                }
            }
            result
        }
        "mcp" => {
            let source = source.unwrap_or_else(|| "legacy".into());
            validate_import_source("mcp", &source)?;
            validate_mcp_payload(&data)?;
            let obj = data.as_object().ok_or("payload 必须是 JSON 对象")?;
            let raw_name = obj
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let config = obj
                .get("config")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            // 用唯一名字新增：目标应用里已有同名条目时加后缀，
            // 这样 save_unified_mcp_server 的"停用即移除"逻辑不会误删既有配置
            let names: Vec<String> = get_unified_mcp_servers()?
                .get("servers")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|s| s.get("name").and_then(|n| n.as_str()).map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            let name = unique_import_name(&names, &raw_name);
            // 未指定 apps 时默认三个应用都写入（前端确认框里会明示目标应用）
            let apps = obj.get("apps").cloned().unwrap_or_else(
                || serde_json::json!({ "claude": true, "codex": true, "gemini": true }),
            );
            save_unified_mcp_server(name.clone(), config, apps.clone())?;
            let mut targets: Vec<&str> = Vec::new();
            let enabled = |key: &str| apps.get(key).and_then(|v| v.as_bool()).unwrap_or(false);
            if enabled("claude") {
                targets.push("Claude");
            }
            if enabled("codex") {
                targets.push("Codex");
            }
            if enabled("gemini") {
                targets.push("Gemini");
            }
            Ok(format!(
                "已添加 MCP 服务器「{name}」（{}）",
                if targets.is_empty() {
                    "未启用任何应用".to_string()
                } else {
                    targets.join("、")
                }
            ))
        }
        other => Err(format!("未知导入类型：{other}")),
    }
}

#[cfg(test)]
mod deep_link_tests {
    use super::*;
    use base64::Engine as _;

    /// 把 JSON 文本编码为 base64url（不带填充），模拟深链发起方
    fn b64(json: &str) -> String {
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json.as_bytes())
    }

    fn register_local_confirmation(
        store: &Arc<Mutex<PendingOverwriteConfirmations>>,
        app: &str,
        profile_id: &str,
        name: &str,
        data: &serde_json::Value,
        now: Instant,
    ) -> String {
        let mut pending = store.lock().unwrap();
        register_overwrite_confirmation_in(&mut pending, app, profile_id, name, data, now)
    }

    #[test]
    fn parse_valid_profile_url() {
        let payload = b64(
            r#"{"name":"公司中转","apiKey":"sk-test-123","baseUrl":"https://api.example.com","model":"claude-sonnet-4"}"#,
        );
        let url = format!("varswitch://import/profile?app=claude&payload={payload}");
        let import = parse_deep_link_url(&url).expect("合法 profile 深链应解析成功");
        assert_eq!(import.kind, "profile");
        assert_eq!(import.app, "claude");
        assert_eq!(import.data["name"], "公司中转");
        assert_eq!(import.data["apiKey"], "sk-test-123");
        assert_eq!(import.data["baseUrl"], "https://api.example.com");
    }

    #[test]
    fn parse_valid_mcp_url() {
        let payload = b64(
            r#"{"name":"context7","config":{"command":"npx","args":["-y","@upstash/context7-mcp"]},"apps":{"claude":true,"codex":false,"gemini":false}}"#,
        );
        let url = format!("varswitch://import/mcp?payload={payload}");
        let import = parse_deep_link_url(&url).expect("合法 mcp 深链应解析成功");
        assert_eq!(import.kind, "mcp");
        assert_eq!(import.app, "");
        assert_eq!(import.data["name"], "context7");
        assert_eq!(import.data["config"]["command"], "npx");
    }

    #[test]
    fn parse_tolerates_padding_case_and_legacy_fragment() {
        // 带 = 填充（percent 编码为 %3D）、大写 scheme 与 legacy fragment 应继续兼容
        let padded = base64::engine::general_purpose::URL_SAFE
            .encode(r#"{"name":"n","apiKey":"k"}"#.as_bytes())
            .replace('=', "%3D");
        let url = format!("VARSWITCH://import/profile?app=CODEX&payload={padded}#legacy-fragment");
        let import =
            parse_deep_link_url(&url).expect("带填充/大写 scheme/legacy fragment 应解析成功");
        assert_eq!(import.app, "codex");
    }

    #[test]
    fn parse_rejects_userinfo_for_all_deep_link_versions() {
        let payload = b64(r#"{"name":"n","apiKey":"k"}"#);
        for url in [
            format!("varswitch://user:password@import/profile?app=claude&payload={payload}"),
            "varswitch://user:password@v1/import?resource=provider&app=codex&name=n&endpoint=https%3A%2F%2Fapi.example.com&apiKey=sk-test&model=m&homepage=https%3A%2F%2Fexample.com&enabled=true".into(),
        ] {
            assert!(parse_deep_link_url(&url).is_err(), "应拒绝 userinfo：{url}");
        }
    }

    #[test]
    fn parse_rejects_v1_fragment() {
        let url = "varswitch://v1/import?resource=provider&app=codex&name=n&endpoint=https%3A%2F%2Fapi.example.com&apiKey=sk-test&model=m&homepage=https%3A%2F%2Fexample.com&enabled=true#V1_FRAGMENT_SECRET";
        assert!(parse_deep_link_url(url).is_err(), "v1 fragment 必须拒绝");
        assert!(!safe_deep_link_log_target(url).contains("V1_FRAGMENT_SECRET"));
    }

    #[test]
    fn legacy_profile_and_mcp_fragments_are_accepted_but_never_logged() {
        let profile = b64(r#"{"name":"n","apiKey":"k"}"#);
        let mcp = b64(r#"{"name":"srv","config":{"command":"npx"}}"#);
        for (url, expected_target) in [
            (
                format!("varswitch://import/profile?app=claude&payload={profile}#PROFILE_FRAGMENT_SECRET"),
                "varswitch://import/profile",
            ),
            (
                format!("varswitch://import/mcp?payload={mcp}#MCP_FRAGMENT_SECRET"),
                "varswitch://import/mcp",
            ),
        ] {
            assert!(parse_deep_link_url(&url).is_ok(), "legacy fragment 应兼容：{url}");
            let target = safe_deep_link_log_target(&url);
            assert_eq!(target, expected_target);
            assert!(!target.contains("FRAGMENT_SECRET"));
        }
    }

    #[test]
    fn safe_log_target_uses_only_fixed_allowlisted_strings() {
        let cases = [
            (
                "varswitch://v1/import?resource=provider&apiKey=QUERY_SECRET",
                "varswitch://v1/import",
            ),
            (
                "varswitch://import/profile?payload=QUERY_SECRET",
                "varswitch://import/profile",
            ),
            (
                "varswitch://import/mcp?payload=QUERY_SECRET",
                "varswitch://import/mcp",
            ),
            (
                "VARSWITCH://IMPORT/PROFILE?payload=QUERY_SECRET",
                "varswitch://import/profile",
            ),
            (
                "varswitch:import/profile?payload=QUERY_SECRET",
                "varswitch://import/profile",
            ),
            (
                "varswitch://USERINFO_SECRET:password@import/profile?payload=QUERY_SECRET",
                "[invalid deep link]",
            ),
            (
                "varswitch://import/PATH_SECRET?payload=QUERY_SECRET",
                "varswitch://[unsupported]",
            ),
            (
                "varswitch://import/profile?payload=QUERY_SECRET#FRAGMENT_SECRET",
                "varswitch://import/profile",
            ),
        ];

        for (url, expected) in cases {
            let target = safe_deep_link_log_target(url);
            assert_eq!(target, expected);
            for sentinel in [
                "USERINFO_SECRET",
                "password",
                "PATH_SECRET",
                "QUERY_SECRET",
                "FRAGMENT_SECRET",
            ] {
                assert!(
                    !target.contains(sentinel),
                    "日志目标泄漏 {sentinel}：{target}"
                );
            }
        }
        assert_eq!(
            safe_deep_link_log_target("not a deep link QUERY_SECRET"),
            "[invalid deep link]"
        );
    }

    #[test]
    fn reject_bad_base64() {
        let url = "varswitch://import/profile?app=claude&payload=%%%not-base64!!";
        let err = parse_deep_link_url(url).unwrap_err();
        assert!(
            err.contains("base64url"),
            "错误信息应指出 base64url 问题：{err}"
        );
    }

    #[test]
    fn reject_missing_fields() {
        // 缺 apiKey
        let payload = b64(r#"{"name":"只有名字"}"#);
        let url = format!("varswitch://import/profile?app=claude&payload={payload}");
        let err = parse_deep_link_url(&url).unwrap_err();
        assert!(err.contains("apiKey"), "应报缺少 apiKey：{err}");

        // mcp 缺 config
        let payload = b64(r#"{"name":"srv"}"#);
        let url = format!("varswitch://import/mcp?payload={payload}");
        let err = parse_deep_link_url(&url).unwrap_err();
        assert!(err.contains("config"), "应报缺少 config：{err}");

        // 缺 payload 参数
        let err = parse_deep_link_url("varswitch://import/profile?app=claude").unwrap_err();
        assert!(err.contains("payload"), "应报缺少 payload：{err}");
    }

    #[test]
    fn reject_wrong_scheme_path_and_app() {
        let payload = b64(r#"{"name":"n","apiKey":"k"}"#);
        // 错 scheme
        assert!(parse_deep_link_url(&format!(
            "https://import/profile?app=claude&payload={payload}"
        ))
        .is_err());
        // 错路径
        assert!(parse_deep_link_url(&format!(
            "varswitch://export/profile?app=claude&payload={payload}"
        ))
        .is_err());
        // 不支持的 app
        assert!(parse_deep_link_url(&format!(
            "varswitch://import/profile?app=cursor&payload={payload}"
        ))
        .is_err());
    }

    #[test]
    fn reject_non_http_base_url() {
        let payload = b64(r#"{"name":"n","apiKey":"k","baseUrl":"file:///C:/evil"}"#);
        let url = format!("varswitch://import/profile?app=claude&payload={payload}");
        let err = parse_deep_link_url(&url).unwrap_err();
        assert!(err.contains("http"), "应拒绝非 http(s) 的 baseUrl：{err}");
    }

    #[test]
    fn unique_name_appends_suffix() {
        let existing = vec!["默认".to_string(), "默认 (2)".to_string()];
        assert_eq!(unique_import_name(&existing, "新配置"), "新配置");
        assert_eq!(unique_import_name(&existing, "默认"), "默认 (3)");
        assert_eq!(unique_import_name(&existing, " 默认 "), "默认 (3)");
    }

    #[test]
    fn v1_conflict_defaults_to_unique_rename_and_requires_existing_overwrite_target() {
        let existing = vec!["Team".to_string(), "Team (2)".to_string()];
        assert_eq!(
            resolve_profile_import(&existing, "Team", "cc_switch_v1", Some("rename")).unwrap(),
            ProfileImportResolution::Add("Team (3)".into())
        );
        assert_eq!(
            resolve_profile_import(&existing, "Team", "cc_switch_v1", Some("overwrite")).unwrap(),
            ProfileImportResolution::Overwrite("Team".into())
        );
        assert!(resolve_profile_import(
            &["Other".into()],
            "Team",
            "cc_switch_v1",
            Some("overwrite")
        )
        .unwrap_err()
        .contains("已变化"));
        assert!(resolve_profile_import(&existing, "Team", "legacy", Some("overwrite")).is_err());
    }

    #[test]
    fn conflict_metadata_exposes_names_and_backend_confirmation_token() {
        assert_eq!(
            detect_profile_conflict(
                &["Team".into(), "Team (2)".into()],
                "Team",
                "server-token".into(),
            ),
            Some(DeepLinkConflict {
                existing_name: "Team".into(),
                suggested_name: "Team (3)".into(),
                confirmation_token: "server-token".into(),
            })
        );
    }

    #[test]
    fn overwrite_confirmation_token_binds_app_name_and_complete_import_data() {
        let data = serde_json::json!({
            "name": "Team", "apiKey": "secret", "baseUrl": "https://api.example.com/v1",
            "model": "model-1", "homepage": "https://example.com", "enabled": true,
        });
        let now = Instant::now();
        let store = Arc::new(Mutex::new(HashMap::new()));

        let wrong_app =
            register_local_confirmation(&store, "codex", "profile-id", "Team", &data, now);
        assert!(acquire_overwrite_confirmation_from(
            store.clone(),
            &wrong_app,
            "gemini",
            "Team",
            &data,
            now,
        )
        .is_err());
        assert!(acquire_overwrite_confirmation_from(
            store.clone(),
            &wrong_app,
            "codex",
            "Team",
            &data,
            now,
        )
        .is_err());

        let wrong_name =
            register_local_confirmation(&store, "codex", "profile-id", "Team", &data, now);
        assert!(acquire_overwrite_confirmation_from(
            store.clone(),
            &wrong_name,
            "codex",
            "Renamed",
            &data,
            now,
        )
        .is_err());

        let wrong_data =
            register_local_confirmation(&store, "codex", "profile-id", "Team", &data, now);
        let mut changed = data.clone();
        changed["apiKey"] = serde_json::json!("attacker-key");
        assert!(acquire_overwrite_confirmation_from(
            store,
            &wrong_data,
            "codex",
            "Team",
            &changed,
            now,
        )
        .is_err());
    }

    #[test]
    fn overwrite_confirmation_token_is_one_time_and_rejects_replacement_target() {
        let data = serde_json::json!({
            "name": "Team", "apiKey": "secret", "baseUrl": "https://api.example.com/v1",
            "model": "model-1", "homepage": "https://example.com", "enabled": true,
        });
        let now = Instant::now();
        let store = Arc::new(Mutex::new(HashMap::new()));
        let token = register_local_confirmation(&store, "codex", "original-id", "Team", &data, now);
        let confirmation =
            acquire_overwrite_confirmation_from(store.clone(), &token, "codex", "Team", &data, now)
                .unwrap();

        assert!(confirmation.matches_target("original-id", "Team"));
        assert!(!confirmation.matches_target("replacement-id", "Team"));
        assert!(!confirmation.matches_target("original-id", "Renamed"));
        confirmation.commit();
        assert!(
            acquire_overwrite_confirmation_from(store, &token, "codex", "Team", &data, now,)
                .is_err()
        );
    }

    #[test]
    fn overwrite_confirmation_lease_restores_after_failure_and_commits_after_success() {
        let data = serde_json::json!({
            "name": "Team", "apiKey": "secret", "baseUrl": "https://api.example.com/v1",
            "model": "model-1", "homepage": "https://example.com", "enabled": true,
        });
        let now = Instant::now();
        let store = Arc::new(Mutex::new(HashMap::new()));
        let token = register_local_confirmation(&store, "codex", "original-id", "Team", &data, now);

        let failed_attempt =
            acquire_overwrite_confirmation_from(store.clone(), &token, "codex", "Team", &data, now)
                .unwrap();
        assert!(acquire_overwrite_confirmation_from(
            store.clone(),
            &token,
            "codex",
            "Team",
            &data,
            now,
        )
        .is_err(), "lease in-flight 时并发调用不得重放");
        drop(failed_attempt);

        let successful_retry =
            acquire_overwrite_confirmation_from(store.clone(), &token, "codex", "Team", &data, now)
                .expect("失败写入释放 lease 后应允许重试");
        successful_retry.commit();
        assert!(
            acquire_overwrite_confirmation_from(store, &token, "codex", "Team", &data, now,)
                .is_err(),
            "成功 commit 后 token 不得重放"
        );
    }

    #[test]
    fn canonical_import_data_fingerprint_is_order_independent_and_secret_sensitive() {
        let first = serde_json::json!({
            "name": "Team", "apiKey": "secret", "nested": { "b": 2, "a": 1 }
        });
        let reordered = serde_json::json!({
            "nested": { "a": 1, "b": 2 }, "apiKey": "secret", "name": "Team"
        });
        let changed_secret = serde_json::json!({
            "name": "Team", "apiKey": "other", "nested": { "b": 2, "a": 1 }
        });

        assert_eq!(
            canonical_import_data_fingerprint(&first),
            canonical_import_data_fingerprint(&reordered)
        );
        assert_ne!(
            canonical_import_data_fingerprint(&first),
            canonical_import_data_fingerprint(&changed_secret)
        );
    }

    #[test]
    fn overwrite_confirmation_token_expires_after_ten_minutes() {
        let data = serde_json::json!({
            "name": "Team", "apiKey": "secret", "baseUrl": "https://api.example.com/v1",
            "model": "model-1", "homepage": "https://example.com", "enabled": true,
        });
        let now = Instant::now();
        let store = Arc::new(Mutex::new(HashMap::new()));
        let token = register_local_confirmation(
            &store,
            "codex",
            "profile-id",
            "Team",
            &data,
            now - OVERWRITE_CONFIRMATION_TTL - Duration::from_secs(1),
        );

        assert!(
            acquire_overwrite_confirmation_from(store, &token, "codex", "Team", &data, now,)
                .is_err()
        );
    }

    #[test]
    fn pending_overwrite_confirmation_store_is_bounded() {
        let data = serde_json::json!({ "name": "Team" });
        let start = Instant::now();
        let mut pending = HashMap::new();
        let mut first = String::new();

        for index in 0..=MAX_PENDING_OVERWRITE_CONFIRMATIONS {
            let token = register_overwrite_confirmation_in(
                &mut pending,
                "codex",
                &format!("profile-{index}"),
                "Team",
                &data,
                start + Duration::from_secs(index as u64),
            );
            if index == 0 {
                first = token;
            }
        }

        assert_eq!(pending.len(), MAX_PENDING_OVERWRITE_CONFIRMATIONS);
        assert!(
            !pending.contains_key(&first),
            "最早的确认应在达到上限时淘汰"
        );
    }

    #[test]
    fn v1_revalidation_rejects_non_http_homepage() {
        let data = serde_json::json!({
            "name": "Team", "apiKey": "test-key", "baseUrl": "https://api.example.com/v1",
            "model": "model-1", "homepage": "file:///C:/evil", "enabled": true,
        });
        let error = validate_cc_switch_v1_profile_payload(&data).unwrap_err();
        assert!(error.contains("HTTP"), "应拒绝非 HTTP(S) homepage：{error}");
    }

    #[test]
    fn v1_mcp_source_is_rejected_before_write() {
        assert!(validate_import_source("mcp", "legacy").is_ok());
        assert!(validate_import_source("mcp", "cc_switch_v1").is_err());
    }

    #[test]
    fn overwrite_codex_fields_preserves_identity_activation_and_local_options() {
        let existing = CodexProfile {
            id: "id-1".into(),
            name: "Team".into(),
            api_key: "old-key".into(),
            base_url: "https://old.example.com/v1".into(),
            auth_mode: "api_key".into(),
            wire_api: "responses".into(),
            model: "old-model".into(),
            provider_name: "local-provider".into(),
            image_api_key: "image-key".into(),
            image_base_url: "https://images.example.com".into(),
            is_active: true,
            created_at: "2026-08-19 10:00:00".into(),
        };
        let updated = merge_codex_v1_profile(
            existing,
            &serde_json::json!({
                "name": "Team", "apiKey": "new-key",
                "baseUrl": "https://new.example.com/v1", "model": "gpt-5-codex"
            }),
        )
        .unwrap();
        assert_eq!(updated.id, "id-1");
        assert_eq!(updated.created_at, "2026-08-19 10:00:00");
        assert!(updated.is_active);
        assert_eq!(updated.api_key, "new-key");
        assert_eq!(updated.base_url, "https://new.example.com/v1");
        assert_eq!(updated.model, "gpt-5-codex");
        assert_eq!(updated.provider_name, "local-provider");
        assert_eq!(updated.image_api_key, "image-key");
    }

    #[test]
    fn overwrite_claude_fields_preserves_identity_activation_and_proxy_options() {
        let existing = Profile {
            id: "claude-id".into(),
            name: "Team".into(),
            api_key: "old".into(),
            base_url: "https://old.example.com".into(),
            model_id: "old-model".into(),
            sonnet_model: "old-sonnet".into(),
            opus_model: "old-opus".into(),
            haiku_model: "old-haiku".into(),
            api_format: "openai_chat".into(),
            proxy_failover: true,
            proxy_takeover: true,
            is_active: true,
            created_at: "2026-08-19 10:00:00".into(),
        };
        let updated = merge_claude_v1_profile(
            existing,
            &serde_json::json!({
                "name": "Team", "apiKey": "new", "baseUrl": "https://new.example.com/",
                "model": "primary", "sonnetModel": "sonnet", "opusModel": "opus",
                "haikuModel": "haiku"
            }),
        )
        .unwrap();
        assert_eq!(updated.id, "claude-id");
        assert_eq!(updated.created_at, "2026-08-19 10:00:00");
        assert!(updated.is_active);
        assert_eq!(updated.api_format, "openai_chat");
        assert!(updated.proxy_failover);
        assert!(updated.proxy_takeover);
        assert_eq!(updated.model_id, "primary");
        assert_eq!(updated.sonnet_model, "sonnet");
        assert_eq!(updated.opus_model, "opus");
        assert_eq!(updated.haiku_model, "haiku");
    }

    #[test]
    fn overwrite_claude_preserves_role_models_when_import_values_are_not_non_empty_strings() {
        let existing = Profile {
            id: "claude-id".into(),
            name: "Team".into(),
            api_key: "old".into(),
            base_url: "https://old.example.com".into(),
            model_id: "old-model".into(),
            sonnet_model: "old-sonnet".into(),
            opus_model: "old-opus".into(),
            haiku_model: "old-haiku".into(),
            api_format: "anthropic".into(),
            proxy_failover: false,
            proxy_takeover: false,
            is_active: false,
            created_at: "2026-08-19 10:00:00".into(),
        };
        let updated = merge_claude_v1_profile(
            existing,
            &serde_json::json!({
                "name": "Team", "apiKey": "new", "baseUrl": "https://new.example.com/",
                "model": "primary", "sonnetModel": null, "opusModel": "   "
            }),
        )
        .unwrap();

        assert_eq!(updated.sonnet_model, "old-sonnet");
        assert_eq!(updated.opus_model, "old-opus");
        assert_eq!(updated.haiku_model, "old-haiku");
    }

    #[test]
    fn merged_claude_overwrite_is_encrypted_when_written_to_unique_temp_file() {
        struct ExactTempFile(Option<PathBuf>);
        impl Drop for ExactTempFile {
            fn drop(&mut self) {
                if let Some(path) = self.0.as_ref() {
                    let _ = fs::remove_file(path);
                }
            }
        }

        let secret = format!("sk-deeplink-test-{}", uuid::Uuid::new_v4());
        let profile = merge_claude_v1_profile(
            Profile {
                id: "claude-id".into(),
                name: "Team".into(),
                api_key: "old".into(),
                base_url: "https://old.example.com".into(),
                model_id: "old-model".into(),
                sonnet_model: "old-sonnet".into(),
                opus_model: "old-opus".into(),
                haiku_model: "old-haiku".into(),
                api_format: "anthropic".into(),
                proxy_failover: false,
                proxy_takeover: false,
                is_active: false,
                created_at: "2026-08-19 10:00:00".into(),
            },
            &serde_json::json!({
                "name": "Team", "apiKey": secret, "baseUrl": "https://new.example.com/",
                "model": "primary"
            }),
        )
        .unwrap();
        let path = std::env::temp_dir().join(format!(
            "varswitch-deeplink-encryption-{}.json",
            uuid::Uuid::new_v4()
        ));
        let mut temp_file = ExactTempFile(Some(path.clone()));
        let policy_probe = encrypt_secret(&secret);

        crate::claude::write_profiles_to_path(
            &path,
            &ProfilesData {
                profiles: vec![profile],
            },
        )
        .unwrap();
        let raw = fs::read_to_string(&path).unwrap();
        let stored: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let stored_key = stored["profiles"][0]["apiKey"].as_str().unwrap();
        if is_encrypted(&policy_probe) {
            assert!(!raw.contains(&secret), "临时配置文件不得包含明文 API Key");
            assert!(is_encrypted(stored_key), "apiKey 字段必须是加密值");
        } else {
            eprintln!("系统凭据库不可用，按 secret_store 策略跳过密文断言");
            assert_eq!(stored_key, secret, "writer 必须遵循当前明文 fallback 策略");
        }

        fs::remove_file(&path).unwrap();
        temp_file.0 = None;
        assert!(!path.exists(), "测试必须删除精确的临时文件");
    }

    #[test]
    fn overwrite_gemini_fields_preserves_identity_and_activation() {
        let existing = GeminiProfile {
            id: "gemini-id".into(),
            name: "Team".into(),
            api_key: "old".into(),
            base_url: "https://old.example.com".into(),
            model: "old-model".into(),
            is_active: true,
            created_at: "2026-08-19 10:00:00".into(),
        };
        let updated = merge_gemini_v1_profile(
            existing,
            &serde_json::json!({
                "name": "Team", "apiKey": "new", "baseUrl": "https://new.example.com/",
                "model": "gemini-2.5-pro"
            }),
        )
        .unwrap();
        assert_eq!(updated.id, "gemini-id");
        assert_eq!(updated.created_at, "2026-08-19 10:00:00");
        assert!(updated.is_active);
        assert_eq!(updated.api_key, "new");
        assert_eq!(updated.base_url, "https://new.example.com");
        assert_eq!(updated.model, "gemini-2.5-pro");
    }

    #[test]
    fn parse_cc_switch_v1_provider_urls() {
        let claude = parse_deep_link_url(
            "varswitch://v1/import?resource=provider&app=claude&name=Team+Claude&endpoint=https%3A%2F%2Fapi.example.com&apiKey=sk-test-claude&model=claude-sonnet-4&haikuModel=claude-haiku-4&sonnetModel=claude-sonnet-4&opusModel=claude-opus-4&homepage=https%3A%2F%2Fapi.example.com&enabled=true",
        ).expect("合法 Claude v1 深链应解析成功");
        assert_eq!(claude.source, "cc_switch_v1");
        assert_eq!(claude.kind, "profile");
        assert_eq!(claude.app, "claude");
        assert_eq!(claude.data["name"], "Team Claude");
        assert_eq!(claude.data["baseUrl"], "https://api.example.com/");
        assert_eq!(claude.data["apiKey"], "sk-test-claude");
        assert_eq!(claude.data["model"], "claude-sonnet-4");
        assert_eq!(claude.data["haikuModel"], "claude-haiku-4");
        assert_eq!(claude.data["sonnetModel"], "claude-sonnet-4");
        assert_eq!(claude.data["opusModel"], "claude-opus-4");
        assert_eq!(claude.data["homepage"], "https://api.example.com");
        assert_eq!(claude.data["enabled"], true);

        for app in ["codex", "gemini"] {
            let raw = format!(
                "varswitch://v1/import?resource=provider&app={app}&name=Provider&endpoint=https%3A%2F%2Fapi.example.com%2Fv1&apiKey=sk-test&model=model-1&homepage=https%3A%2F%2Fapi.example.com&enabled=true"
            );
            let import = parse_deep_link_url(&raw).expect("合法 v1 深链应解析成功");
            assert_eq!(import.source, "cc_switch_v1");
            assert_eq!(import.app, app);
            assert_eq!(import.data["baseUrl"], "https://api.example.com/v1");
        }
    }

    #[test]
    fn cc_switch_v1_decodes_form_query_once() {
        let import = parse_deep_link_url(
            "varswitch://v1/import?resource=provider&app=gemini&name=%E4%B8%AD%E6%96%87+%25+Provider&endpoint=https%3A%2F%2Fapi.example.com%2F%252Fkeep&apiKey=sk-test%2525key&model=gemini-2.5-pro&homepage=https%3A%2F%2Fapi.example.com&enabled=true",
        ).expect("form query 应正确解码");
        assert_eq!(import.data["name"], "中文 % Provider");
        assert_eq!(import.data["baseUrl"], "https://api.example.com/%2Fkeep");
        assert_eq!(import.data["apiKey"], "sk-test%25key");
    }

    #[test]
    fn cc_switch_v1_rejects_invalid_contract_values() {
        let valid = "name=n&endpoint=https%3A%2F%2Fapi.example.com&apiKey=sk-test&model=m&homepage=https%3A%2F%2Fapi.example.com&enabled=true";
        for prefix in [
            "varswitch://v2/import?resource=provider&app=claude",
            "varswitch://v1/export?resource=provider&app=claude",
            "varswitch://v1/import?resource=mcp&app=claude",
            "varswitch://v1/import?resource=provider&app=grok",
        ] {
            assert!(parse_deep_link_url(&format!("{prefix}&{valid}")).is_err());
        }
        assert!(parse_deep_link_url(
            "varswitch://v1/import?resource=provider&app=codex&name=n&endpoint=file%3A%2F%2F%2FC%3A%2Fevil&apiKey=sk-test&model=m&homepage=https%3A%2F%2Fapi.example.com&enabled=true"
        ).unwrap_err().contains("HTTP"));
        assert!(parse_deep_link_url(
            "varswitch://v1/import?resource=provider&app=codex&name=n&endpoint=https%3A%2F%2Fapi.example.com&apiKey=sk-test&model=m&homepage=https%3A%2F%2Fapi.example.com&enabled=false"
        ).unwrap_err().contains("enabled"));
        assert!(parse_deep_link_url(
            "varswitch://v1/import?resource=provider&app=codex&name=n&endpoint=https%3A%2F%2Fapi.example.com&apiKey=sk-test&model=m&homepage=docs.example.com&enabled=true"
        ).unwrap_err().contains("HTTP"));
        assert!(parse_deep_link_url(
            "varswitch://v1/import?resource=provider&app=codex&name=n&endpoint=https%3A%2F%2Fapi.example.com&apiKey=sk-test&model=m&homepage=file%3A%2F%2F%2FC%3A%2Fevil&enabled=true"
        ).unwrap_err().contains("HTTP"));
    }

    #[test]
    fn cc_switch_v1_rejects_http_endpoints_without_double_slash() {
        for endpoint in ["https%3Aapi.example.com", "https%3A%2Fapi.example.com"] {
            let raw = format!(
                "varswitch://v1/import?resource=provider&app=codex&name=n&endpoint={endpoint}&apiKey=sk-test&model=m&homepage=https%3A%2F%2Fapi.example.com&enabled=true"
            );
            assert!(
                parse_deep_link_url(&raw).is_err(),
                "应拒绝非绝对 URL：{endpoint}"
            );
        }
    }

    #[test]
    fn cc_switch_v1_accepts_case_insensitive_http_endpoint_scheme() {
        for endpoint in [
            "HTTPS%3A%2F%2Fapi.example.com",
            "HTTP%3A%2F%2Fapi.example.com",
        ] {
            let raw = format!(
                "varswitch://v1/import?resource=provider&app=codex&name=n&endpoint={endpoint}&apiKey=sk-test&model=m&homepage=https%3A%2F%2Fapi.example.com&enabled=true"
            );
            let import =
                parse_deep_link_url(&raw).expect("HTTP(S) scheme 大小写不应影响绝对 URL 合法性");
            assert!(import.data["baseUrl"].as_str().unwrap().starts_with("http"));
        }
    }

    #[test]
    fn cc_switch_v1_requires_every_contract_field_and_ignores_unknown_fields() {
        let cases = [
            ("name", "resource=provider&app=claude&endpoint=https%3A%2F%2Fapi.example.com&apiKey=sk-test&model=m&homepage=https%3A%2F%2Fapi.example.com&enabled=true"),
            ("endpoint", "resource=provider&app=claude&name=n&apiKey=sk-test&model=m&homepage=https%3A%2F%2Fapi.example.com&enabled=true"),
            ("apiKey", "resource=provider&app=claude&name=n&endpoint=https%3A%2F%2Fapi.example.com&model=m&homepage=https%3A%2F%2Fapi.example.com&enabled=true"),
            ("model", "resource=provider&app=claude&name=n&endpoint=https%3A%2F%2Fapi.example.com&apiKey=sk-test&homepage=https%3A%2F%2Fapi.example.com&enabled=true"),
            ("homepage", "resource=provider&app=claude&name=n&endpoint=https%3A%2F%2Fapi.example.com&apiKey=sk-test&model=m&enabled=true"),
            ("enabled", "resource=provider&app=claude&name=n&endpoint=https%3A%2F%2Fapi.example.com&apiKey=sk-test&model=m&homepage=https%3A%2F%2Fapi.example.com"),
        ];
        for (field, query) in cases {
            let error = parse_deep_link_url(&format!("varswitch://v1/import?{query}"))
                .expect_err("缺少必填参数必须拒绝");
            assert!(error.contains(field), "{field} 的错误应点明字段：{error}");
        }
        let accepted = parse_deep_link_url(
            "varswitch://v1/import?resource=provider&app=claude&name=n&endpoint=https%3A%2F%2Fapi.example.com&apiKey=sk-test&model=m&homepage=https%3A%2F%2Fapi.example.com&enabled=true&futureField=ignored",
        );
        assert!(accepted.is_ok(), "未知参数必须被忽略");
    }
}
