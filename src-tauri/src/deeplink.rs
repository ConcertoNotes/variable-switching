//! 深链导入 varswitch://（从 lib.rs 拆分，逻辑未改动）。

use crate::*;

// ── Deep Link（varswitch:// 一键导入）─────────────────
// URL 契约：
//   varswitch://import/profile?app=<claude|codex|gemini|grok>&payload=<base64url(JSON)>
//     payload JSON：{ name, apiKey, baseUrl?, model?, 其他应用特有字段可选 }
//   varswitch://import/mcp?payload=<base64url(JSON)>
//     payload JSON：{ name, config, apps? }
// 安全流程：后端只解析校验，通过 deeplink-import 事件交前端弹窗确认，
// 用户点「确认导入」后才调用 apply_deep_link_import 真正写入（只新增，不激活不切换）。

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

/// 解析并校验一条 varswitch:// 深链（纯函数，便于单测）。
/// 容忍大小写 scheme、尾部斜杠与 #fragment；解析失败返回可读的中文错误。
pub(crate) fn parse_deep_link_url(url: &str) -> Result<DeepLinkImport, String> {
    let url = url.trim();
    let parsed =
        reqwest::Url::parse(url).map_err(|_| "不是合法的 varswitch:// 协议链接".to_string())?;
    if !parsed.scheme().eq_ignore_ascii_case("varswitch") {
        return Err("不是 varswitch:// 协议链接".into());
    }
    if parsed.host_str() == Some("v1") {
        if parsed.path() != "/import" {
            return Err("不支持的 v1 深链路径或 action".into());
        }
        return parse_cc_switch_v1_provider(&parsed);
    }
    if parsed
        .host_str()
        .is_some_and(|host| host.starts_with('v') && host[1..].parse::<u32>().is_ok())
    {
        return Err("不支持的深链版本".into());
    }
    // scheme 校验（大小写不敏感）
    let scheme_prefix = "varswitch:";
    if url.len() < scheme_prefix.len()
        || !url[..scheme_prefix.len()].eq_ignore_ascii_case(scheme_prefix)
    {
        return Err("不是 varswitch:// 协议链接".into());
    }
    let rest = &url[scheme_prefix.len()..];
    let rest = rest.strip_prefix("//").unwrap_or(rest);
    // 去掉 #fragment
    let rest = rest.split('#').next().unwrap_or(rest);
    // 分离路径与 query
    let (location, query) = match rest.split_once('?') {
        Some((l, q)) => (l, q),
        None => (rest, ""),
    };
    let location = location.trim_matches('/').to_ascii_lowercase();
    let params = parse_query_params(query);

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

/// 重名时自动追加 " (2)"、" (3)" 后缀直到不冲突（导入只新增、绝不覆盖已有配置）
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

/// 处理一条运行期收到的深链 URL：
/// 解析成功 → emit "deeplink-import" 事件交前端弹窗确认，并把主窗口拉到前台；
/// 解析失败 → 只写日志 + emit "deeplink-import-error" 给前端 toast，绝不崩溃。
pub(crate) fn handle_deep_link_url(app: &tauri::AppHandle, url: &str) {
    // 日志只记录 ? 之前的部分：query 里的 payload 含明文密钥，不能落盘
    let visible = url.split('?').next().unwrap_or(url);
    match parse_deep_link_url(url) {
        Ok(import) => {
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
            log_error!("[deep-link] 解析失败（{visible}）：{err}");
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
/// 只新增（不激活、不切换），重名自动加后缀；返回一句可读的结果描述。
#[tauri::command]
pub(crate) fn apply_deep_link_import(
    handle: tauri::AppHandle,
    kind: String,
    app: String,
    data: serde_json::Value,
) -> Result<String, String> {
    match kind.as_str() {
        "profile" => {
            // 与解析阶段相同的校验，防止绕过事件流程直接调用时写入脏数据
            validate_profile_payload(&data)?;
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
            match app.as_str() {
                "claude" => {
                    let names: Vec<String> = read_profiles(&handle)
                        .profiles
                        .iter()
                        .map(|p| p.name.clone())
                        .collect();
                    let name = unique_import_name(&names, &get("name"));
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
                "codex" => {
                    let names: Vec<String> = read_codex_profiles(&handle)
                        .profiles
                        .iter()
                        .map(|p| p.name.clone())
                        .collect();
                    let name = unique_import_name(&names, &get("name"));
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
                "gemini" => {
                    let names: Vec<String> = read_gemini_profiles(&handle)
                        .profiles
                        .iter()
                        .map(|p| p.name.clone())
                        .collect();
                    let name = unique_import_name(&names, &get("name"));
                    let profile = add_gemini_profile(
                        handle.clone(),
                        name,
                        api_key,
                        base_url,
                        get_opt("model"),
                    )?;
                    Ok(format!("已添加 Gemini 配置「{}」", profile.name))
                }
                "grok" => {
                    let names: Vec<String> = read_grok_profiles(&handle)
                        .profiles
                        .iter()
                        .map(|p| p.name.clone())
                        .collect();
                    let name = unique_import_name(&names, &get("name"));
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
            }
        }
        "mcp" => {
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
            let apps = obj
                .get("apps")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({ "claude": true, "codex": true, "gemini": true }));
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

    #[test]
    fn parse_valid_profile_url() {
        let payload = b64(r#"{"name":"公司中转","apiKey":"sk-test-123","baseUrl":"https://api.example.com","model":"claude-sonnet-4"}"#);
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
    fn parse_tolerates_padding_case_and_fragment() {
        // 带 = 填充（percent 编码为 %3D）、scheme 大写、尾部 fragment 都应容忍
        let padded = base64::engine::general_purpose::URL_SAFE
            .encode(r#"{"name":"n","apiKey":"k"}"#.as_bytes())
            .replace('=', "%3D");
        let url = format!("VARSWITCH://import/profile?app=CODEX&payload={padded}#frag");
        let import = parse_deep_link_url(&url).expect("带填充/大写 scheme 应解析成功");
        assert_eq!(import.app, "codex");
    }

    #[test]
    fn reject_bad_base64() {
        let url = "varswitch://import/profile?app=claude&payload=%%%not-base64!!";
        let err = parse_deep_link_url(url).unwrap_err();
        assert!(err.contains("base64url"), "错误信息应指出 base64url 问题：{err}");
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
