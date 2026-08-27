//! Claude Desktop 独立供应商配置、3P Profile 投影与状态管理。

use crate::*;

pub(crate) const CLAUDE_DESKTOP_OFFICIAL_ID: &str = "claude-desktop-official";
pub(crate) const VARSWITCH_DESKTOP_PROFILE_ID: &str = "00000000-0000-4000-8000-000000257890";
pub(crate) const CLAUDE_DESKTOP_GATEWAY_URL: &str = "http://127.0.0.1:25789/claude-desktop";

const SONNET_ROUTE_ID: &str = "claude-sonnet-4-6";
const OPUS_ROUTE_ID: &str = "claude-opus-4-6";
const HAIKU_ROUTE_ID: &str = "claude-haiku-4-5-20251001";

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ClaudeDesktopConnectionMode {
    Gateway,
    Direct,
    Official,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeDesktopProfile {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) api_key: String,
    pub(crate) base_url: String,
    pub(crate) connection_mode: ClaudeDesktopConnectionMode,
    pub(crate) api_format: String,
    pub(crate) model_id: String,
    pub(crate) sonnet_model: String,
    pub(crate) opus_model: String,
    pub(crate) haiku_model: String,
    #[serde(default)]
    pub(crate) available_models: Vec<String>,
    pub(crate) proxy_failover: bool,
    pub(crate) is_active: bool,
    pub(crate) created_at: String,
}

#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq, Eq)]
pub(crate) struct ClaudeDesktopProfilesData {
    pub(crate) profiles: Vec<ClaudeDesktopProfile>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeDesktopProfileInput {
    pub(crate) name: String,
    pub(crate) api_key: String,
    pub(crate) base_url: String,
    pub(crate) connection_mode: ClaudeDesktopConnectionMode,
    pub(crate) api_format: String,
    #[serde(default)]
    pub(crate) model_id: String,
    #[serde(default)]
    pub(crate) sonnet_model: String,
    #[serde(default)]
    pub(crate) opus_model: String,
    #[serde(default)]
    pub(crate) haiku_model: String,
    #[serde(default)]
    pub(crate) available_models: Vec<String>,
    #[serde(default)]
    pub(crate) proxy_failover: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct ClaudeDesktopPaths {
    pub(crate) normal_config_path: PathBuf,
    pub(crate) threep_config_path: PathBuf,
    #[cfg(test)]
    pub(crate) config_library_path: PathBuf,
    pub(crate) profile_path: PathBuf,
    pub(crate) meta_path: PathBuf,
}

#[derive(Clone)]
struct FileSnapshot {
    path: PathBuf,
    content: Option<Vec<u8>>,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ClaudeDesktopGatewaySettings {
    token: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeDesktopSwitchResult {
    pub(crate) success: bool,
    pub(crate) profile_id: String,
    pub(crate) profile_name: String,
    pub(crate) mode: ClaudeDesktopConnectionMode,
    pub(crate) restart_required: bool,
    pub(crate) keep_varswitch_running: bool,
    pub(crate) message: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeDesktopProviderStatus {
    pub(crate) installed: bool,
    pub(crate) supported: bool,
    pub(crate) mode: String,
    pub(crate) active_profile_id: Option<String>,
    pub(crate) active_profile_name: Option<String>,
    pub(crate) profile_path: Option<String>,
    pub(crate) gateway_running: bool,
    pub(crate) gateway_url: String,
    pub(crate) warning: Option<String>,
}

pub(crate) fn claude_desktop_profiles_path(app: &tauri::AppHandle) -> PathBuf {
    data_dir(app).join("claude_desktop_profiles.json")
}

pub(crate) fn claude_desktop_gateway_settings_path(app: &tauri::AppHandle) -> PathBuf {
    data_dir(app).join("claude_desktop_gateway.json")
}

fn paths_from_dirs(normal_dir: PathBuf, threep_dir: PathBuf) -> ClaudeDesktopPaths {
    let config_library_path = threep_dir.join("configLibrary");
    ClaudeDesktopPaths {
        normal_config_path: normal_dir.join("claude_desktop_config.json"),
        threep_config_path: threep_dir.join("claude_desktop_config.json"),
        profile_path: config_library_path.join(format!("{VARSWITCH_DESKTOP_PROFILE_ID}.json")),
        meta_path: config_library_path.join("_meta.json"),
        #[cfg(test)]
        config_library_path,
    }
}

fn paths_from_base(base: &Path) -> ClaudeDesktopPaths {
    paths_from_dirs(base.join("Claude"), base.join("Claude-3p"))
}

fn current_platform_paths() -> Result<ClaudeDesktopPaths, String> {
    #[cfg(target_os = "windows")]
    {
        let local_app_data = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home_dir().join("AppData").join("Local"));
        return Ok(paths_from_base(&local_app_data));
    }
    #[cfg(target_os = "macos")]
    {
        let app_support = home_dir().join("Library").join("Application Support");
        return Ok(paths_from_base(&app_support));
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        Err("当前平台不支持启用 Claude Desktop 3P Profile；仅支持 Windows 和 macOS".into())
    }
}

fn official_profile(is_active: bool) -> ClaudeDesktopProfile {
    ClaudeDesktopProfile {
        id: CLAUDE_DESKTOP_OFFICIAL_ID.into(),
        name: "Claude Official".into(),
        api_key: String::new(),
        base_url: String::new(),
        connection_mode: ClaudeDesktopConnectionMode::Official,
        api_format: "anthropic".into(),
        model_id: String::new(),
        sonnet_model: String::new(),
        opus_model: String::new(),
        haiku_model: String::new(),
        available_models: Vec::new(),
        proxy_failover: false,
        is_active,
        created_at: "0".into(),
    }
}

fn with_official_profile(mut data: ClaudeDesktopProfilesData) -> ClaudeDesktopProfilesData {
    data.profiles
        .retain(|profile| profile.id != CLAUDE_DESKTOP_OFFICIAL_ID);
    let official_is_active = !data.profiles.iter().any(|profile| profile.is_active);
    data.profiles
        .insert(0, official_profile(official_is_active));
    data
}

fn validate_profile(profile: &ClaudeDesktopProfile) -> Result<(), String> {
    if profile.connection_mode == ClaudeDesktopConnectionMode::Official {
        return Ok(());
    }
    if profile.name.trim().is_empty() {
        return Err("配置名称不能为空".into());
    }
    if profile.api_key.trim().is_empty() {
        return Err("API Key 不能为空".into());
    }
    if profile.base_url.trim().is_empty() {
        return Err("Base URL 不能为空".into());
    }
    let parsed_url = reqwest::Url::parse(profile.base_url.trim())
        .map_err(|error| format!("Base URL 无效: {error}"))?;
    if !matches!(parsed_url.scheme(), "http" | "https") || parsed_url.host_str().is_none() {
        return Err("Base URL 必须是有效的 HTTP 或 HTTPS URL".into());
    }
    if !matches!(profile.api_format.as_str(), "anthropic" | "openai_chat") {
        return Err("API 格式仅支持 anthropic 或 openai_chat".into());
    }
    if profile.connection_mode == ClaudeDesktopConnectionMode::Direct
        && profile.api_format != "anthropic"
    {
        return Err("直连模式仅支持 Anthropic Messages 格式".into());
    }
    if [
        &profile.model_id,
        &profile.sonnet_model,
        &profile.opus_model,
        &profile.haiku_model,
    ]
    .iter()
    .all(|model| model.trim().is_empty())
    {
        return Err("至少填写一个可用模型".into());
    }
    if profile.connection_mode == ClaudeDesktopConnectionMode::Direct {
        direct_model_ids(profile)?;
    }
    ensure_secret_usable(&profile.api_key, &format!("配置「{}」", profile.name))
}

fn resolved_model_for_role(profile: &ClaudeDesktopProfile, role: &str) -> Option<String> {
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

fn gateway_route_ids(profile: &ClaudeDesktopProfile) -> Vec<String> {
    [
        ("sonnet", SONNET_ROUTE_ID),
        ("opus", OPUS_ROUTE_ID),
        ("haiku", HAIKU_ROUTE_ID),
    ]
    .into_iter()
    .filter(|(role, _)| resolved_model_for_role(profile, role).is_some())
    .map(|(_, route)| route.to_string())
    .collect()
}

fn is_safe_claude_model_id(model: &str) -> bool {
    let model = model.trim().to_ascii_lowercase();
    ["claude-sonnet-", "claude-opus-", "claude-haiku-"]
        .iter()
        .any(|prefix| {
            model
                .strip_prefix(prefix)
                .is_some_and(|tail| !tail.is_empty())
        })
}

fn direct_model_ids(profile: &ClaudeDesktopProfile) -> Result<Vec<String>, String> {
    let mut models = Vec::new();
    for role in ["sonnet", "opus", "haiku"] {
        let Some(model) = resolved_model_for_role(profile, role) else {
            continue;
        };
        if !is_safe_claude_model_id(&model) {
            return Err(format!(
                "直连模式的模型「{model}」不是 Claude Desktop 可识别的 Claude 模型；请改用 Gateway 模式"
            ));
        }
        if !models.contains(&model) {
            models.push(model);
        }
    }
    Ok(models)
}

fn build_profile_json(base_url: &str, api_key: &str, models: Vec<String>) -> serde_json::Value {
    serde_json::json!({
        "coworkEgressAllowedHosts": ["*"],
        "disableDeploymentModeChooser": true,
        "inferenceGatewayApiKey": api_key,
        "inferenceGatewayAuthScheme": "bearer",
        "inferenceGatewayBaseUrl": base_url,
        "inferenceProvider": "gateway",
        "inferenceModels": models,
    })
}

fn build_gateway_profile(
    profile: &ClaudeDesktopProfile,
    token: &str,
) -> Result<serde_json::Value, String> {
    validate_profile(profile)?;
    if profile.connection_mode != ClaudeDesktopConnectionMode::Gateway {
        return Err("当前配置不是 Gateway 模式".into());
    }
    if !token.starts_with("vsd-") || token == "PROXY_MANAGED" {
        return Err("Claude Desktop Gateway Token 无效".into());
    }
    Ok(build_profile_json(
        CLAUDE_DESKTOP_GATEWAY_URL,
        token,
        gateway_route_ids(profile),
    ))
}

fn build_direct_profile(profile: &ClaudeDesktopProfile) -> Result<serde_json::Value, String> {
    validate_profile(profile)?;
    if profile.connection_mode != ClaudeDesktopConnectionMode::Direct {
        return Err("当前配置不是直连模式".into());
    }
    Ok(build_profile_json(
        profile.base_url.trim_end_matches('/'),
        &profile.api_key,
        direct_model_ids(profile)?,
    ))
}

fn read_json_object_or_empty(path: &Path) -> Result<serde_json::Value, String> {
    if !path.exists() {
        return Ok(serde_json::json!({}));
    }
    if !path.is_file() {
        return Err(format!("目标路径不是文件: {}", path.display()));
    }
    let text = fs::read_to_string(path)
        .map_err(|error| format!("读取 {} 失败: {error}", path.display()))?;
    let value: serde_json::Value = serde_json::from_str(&text)
        .map_err(|error| format!("解析 {} 失败（未覆盖原文件）: {error}", path.display()))?;
    if !value.is_object() {
        return Err(format!("{} 的 JSON 顶层必须是对象", path.display()));
    }
    Ok(value)
}

fn write_json_value(path: &Path, value: &serde_json::Value) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("创建 {} 失败: {error}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(value).map_err(|error| error.to_string())?;
    write_file_atomic(path, &text).map_err(|error| format!("写入 {} 失败: {error}", path.display()))
}

fn write_deployment_mode(path: &Path, mode: &str) -> Result<(), String> {
    let mut value = read_json_object_or_empty(path)?;
    value
        .as_object_mut()
        .expect("validated object")
        .insert("deploymentMode".into(), mode.into());
    write_json_value(path, &value)
}

fn write_meta(path: &Path, applied_profile_id: Option<&str>) -> Result<(), String> {
    let mut value = read_json_object_or_empty(path)?;
    let object = value.as_object_mut().expect("validated object");
    let mut entries = object
        .get("entries")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    entries.retain(|entry| {
        entry.get("id").and_then(serde_json::Value::as_str) != Some(VARSWITCH_DESKTOP_PROFILE_ID)
    });
    match applied_profile_id {
        Some(id) => {
            entries.push(serde_json::json!({
                "id": VARSWITCH_DESKTOP_PROFILE_ID,
                "name": "VarSwitch",
            }));
            object.insert("appliedId".into(), id.into());
        }
        None => {
            if object.get("appliedId").and_then(serde_json::Value::as_str)
                == Some(VARSWITCH_DESKTOP_PROFILE_ID)
            {
                if let Some(next_id) = entries
                    .iter()
                    .find_map(|entry| entry.get("id").and_then(serde_json::Value::as_str))
                {
                    object.insert("appliedId".into(), next_id.into());
                } else {
                    object.remove("appliedId");
                }
            }
        }
    }
    object.insert("entries".into(), entries.into());
    write_json_value(path, &value)
}

fn snapshot_paths(paths: &ClaudeDesktopPaths) -> Result<Vec<FileSnapshot>, String> {
    [
        &paths.normal_config_path,
        &paths.threep_config_path,
        &paths.profile_path,
        &paths.meta_path,
    ]
    .into_iter()
    .map(|path| {
        let content = if path.exists() {
            if !path.is_file() {
                return Err(format!("目标路径不是文件: {}", path.display()));
            }
            Some(fs::read(path).map_err(|error| format!("读取 {} 失败: {error}", path.display()))?)
        } else {
            None
        };
        Ok(FileSnapshot {
            path: path.clone(),
            content,
        })
    })
    .collect()
}

fn restore_snapshots(snapshots: &[FileSnapshot]) -> Result<(), String> {
    for snapshot in snapshots {
        match &snapshot.content {
            Some(content) => {
                if let Some(parent) = snapshot.path.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|error| format!("创建 {} 失败: {error}", parent.display()))?;
                }
                fs::write(&snapshot.path, content)
                    .map_err(|error| format!("恢复 {} 失败: {error}", snapshot.path.display()))?;
            }
            None if snapshot.path.is_file() => {
                fs::remove_file(&snapshot.path)
                    .map_err(|error| format!("删除 {} 失败: {error}", snapshot.path.display()))?;
            }
            None => {}
        }
    }
    Ok(())
}

fn with_path_rollback(
    paths: &ClaudeDesktopPaths,
    operation: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    let snapshots = snapshot_paths(paths)?;
    match operation() {
        Ok(()) => Ok(()),
        Err(error) => match restore_snapshots(&snapshots) {
            Ok(()) => Err(error),
            Err(rollback_error) => Err(format!("{error}；回滚失败: {rollback_error}")),
        },
    }
}

fn apply_profile_at_paths(
    paths: &ClaudeDesktopPaths,
    profile: &ClaudeDesktopProfile,
    gateway_token: &str,
) -> Result<(), String> {
    let projected = match profile.connection_mode {
        ClaudeDesktopConnectionMode::Gateway => build_gateway_profile(profile, gateway_token)?,
        ClaudeDesktopConnectionMode::Direct => build_direct_profile(profile)?,
        ClaudeDesktopConnectionMode::Official => return restore_official_at_paths(paths),
    };
    with_path_rollback(paths, || {
        write_deployment_mode(&paths.normal_config_path, "3p")?;
        write_deployment_mode(&paths.threep_config_path, "3p")?;
        write_json_value(&paths.profile_path, &projected)?;
        write_meta(&paths.meta_path, Some(VARSWITCH_DESKTOP_PROFILE_ID))
    })
}

fn restore_official_at_paths(paths: &ClaudeDesktopPaths) -> Result<(), String> {
    with_path_rollback(paths, || {
        write_deployment_mode(&paths.normal_config_path, "1p")?;
        write_deployment_mode(&paths.threep_config_path, "1p")?;
        if paths.profile_path.is_file() {
            fs::remove_file(&paths.profile_path)
                .map_err(|error| format!("删除 {} 失败: {error}", paths.profile_path.display()))?;
        }
        write_meta(&paths.meta_path, None)
    })
}

fn get_or_create_gateway_token_at_path(path: &Path) -> Result<String, String> {
    if path.exists() {
        let text = fs::read_to_string(path)
            .map_err(|error| format!("读取 Gateway Token 失败: {error}"))?;
        let settings: ClaudeDesktopGatewaySettings = serde_json::from_str(&text)
            .map_err(|error| format!("解析 Gateway Token 配置失败（未覆盖原文件）: {error}"))?;
        let token = decrypt_secret_or_keep(&settings.token, "Claude Desktop Gateway Token");
        ensure_secret_usable(&token, "Claude Desktop Gateway Token")?;
        if token.starts_with("vsd-") && token != "PROXY_MANAGED" {
            return Ok(token);
        }
        return Err("已保存的 Claude Desktop Gateway Token 格式无效".into());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("创建配置目录失败: {error}"))?;
    }
    let token = format!("vsd-{}", uuid::Uuid::new_v4());
    let settings = ClaudeDesktopGatewaySettings {
        token: encrypt_secret(&token),
    };
    let text = serde_json::to_string_pretty(&settings).map_err(|error| error.to_string())?;
    write_private_file(path, &text)?;
    Ok(token)
}

pub(crate) fn get_or_create_claude_desktop_gateway_token(
    app: &tauri::AppHandle,
) -> Result<String, String> {
    get_or_create_gateway_token_at_path(&claude_desktop_gateway_settings_path(app))
}

fn read_profiles_from_path(path: &Path) -> ClaudeDesktopProfilesData {
    let mut data: ClaudeDesktopProfilesData = fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    for profile in &mut data.profiles {
        profile.api_key = decrypt_secret_or_keep(
            &profile.api_key,
            &format!("Claude Desktop 配置「{}」", profile.name),
        );
    }
    data
}

fn read_profiles_from_path_checked(path: &Path) -> Result<ClaudeDesktopProfilesData, String> {
    if !path.exists() {
        return Ok(ClaudeDesktopProfilesData::default());
    }
    if !path.is_file() {
        return Err(format!("配置路径不是文件: {}", path.display()));
    }
    let text = fs::read_to_string(path)
        .map_err(|error| format!("读取 {} 失败: {error}", path.display()))?;
    let mut data: ClaudeDesktopProfilesData = serde_json::from_str(&text)
        .map_err(|error| format!("解析 {} 失败（未覆盖原文件）: {error}", path.display()))?;
    for profile in &mut data.profiles {
        profile.api_key = decrypt_secret_or_keep(
            &profile.api_key,
            &format!("Claude Desktop 配置「{}」", profile.name),
        );
    }
    Ok(data)
}

fn write_profiles_to_path(path: &Path, data: &ClaudeDesktopProfilesData) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("创建配置目录失败: {error}"))?;
    }
    let mut encrypted = data.clone();
    for profile in &mut encrypted.profiles {
        profile.api_key = encrypt_secret(&profile.api_key);
    }
    let json = serde_json::to_string_pretty(&encrypted).map_err(|error| error.to_string())?;
    write_private_file(path, &json)
}

fn mutate_profiles_at_path<T>(
    path: &Path,
    mutation: impl FnOnce(&mut ClaudeDesktopProfilesData) -> Result<T, String>,
) -> Result<T, String> {
    let mut data = read_profiles_from_path_checked(path)?;
    let result = mutation(&mut data)?;
    write_profiles_to_path(path, &data)?;
    Ok(result)
}

pub(crate) fn read_stored_claude_desktop_profiles(
    app: &tauri::AppHandle,
) -> ClaudeDesktopProfilesData {
    read_profiles_from_path(&claude_desktop_profiles_path(app))
}

pub(crate) fn read_stored_claude_desktop_profiles_checked(
    app: &tauri::AppHandle,
) -> Result<ClaudeDesktopProfilesData, String> {
    read_profiles_from_path_checked(&claude_desktop_profiles_path(app))
}

pub(crate) fn write_claude_desktop_profiles(
    app: &tauri::AppHandle,
    data: &ClaudeDesktopProfilesData,
) -> Result<(), String> {
    let mut stored = data.clone();
    stored
        .profiles
        .retain(|profile| profile.id != CLAUDE_DESKTOP_OFFICIAL_ID);
    write_profiles_to_path(&claude_desktop_profiles_path(app), &stored)
}

fn import_from_claude_profiles(
    mut desktop: ClaudeDesktopProfilesData,
    source: &[crate::Profile],
) -> ClaudeDesktopProfilesData {
    desktop
        .profiles
        .retain(|profile| profile.id != CLAUDE_DESKTOP_OFFICIAL_ID);
    let mut names: Vec<String> = desktop
        .profiles
        .iter()
        .map(|profile| profile.name.clone())
        .collect();
    for profile in source {
        let name = unique_import_name(&names, &profile.name);
        names.push(name.clone());
        desktop.profiles.push(ClaudeDesktopProfile {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            api_key: profile.api_key.clone(),
            base_url: profile.base_url.clone(),
            connection_mode: if profile.proxy_takeover || profile.api_format == "openai_chat" {
                ClaudeDesktopConnectionMode::Gateway
            } else {
                ClaudeDesktopConnectionMode::Direct
            },
            api_format: normalize_claude_api_format(&profile.api_format),
            model_id: profile.model_id.clone(),
            sonnet_model: profile.sonnet_model.clone(),
            opus_model: profile.opus_model.clone(),
            haiku_model: profile.haiku_model.clone(),
            available_models: Vec::new(),
            proxy_failover: profile.proxy_failover,
            is_active: false,
            created_at: chrono_now(),
        });
    }
    desktop
}

fn reorder_custom_profiles(
    mut data: ClaudeDesktopProfilesData,
    ids: &[String],
) -> ClaudeDesktopProfilesData {
    data.profiles
        .retain(|profile| profile.id != CLAUDE_DESKTOP_OFFICIAL_ID);
    let custom_ids: Vec<String> = ids
        .iter()
        .filter(|id| id.as_str() != CLAUDE_DESKTOP_OFFICIAL_ID)
        .cloned()
        .collect();
    data.profiles = reorder_by_ids(data.profiles, &custom_ids, |profile| &profile.id);
    data
}

fn profile_from_input(input: ClaudeDesktopProfileInput) -> ClaudeDesktopProfile {
    ClaudeDesktopProfile {
        id: uuid::Uuid::new_v4().to_string(),
        name: input.name.trim().to_string(),
        api_key: input.api_key.trim().to_string(),
        base_url: input.base_url.trim().trim_end_matches('/').to_string(),
        connection_mode: input.connection_mode,
        api_format: input.api_format.trim().to_lowercase(),
        model_id: input.model_id.trim().to_string(),
        sonnet_model: input.sonnet_model.trim().to_string(),
        opus_model: input.opus_model.trim().to_string(),
        haiku_model: input.haiku_model.trim().to_string(),
        available_models: input.available_models,
        proxy_failover: input.proxy_failover,
        is_active: false,
        created_at: chrono_now(),
    }
}

fn ensure_unique_profile_name(
    data: &ClaudeDesktopProfilesData,
    name: &str,
    except_id: Option<&str>,
) -> Result<(), String> {
    if data.profiles.iter().any(|profile| {
        Some(profile.id.as_str()) != except_id && profile.name.eq_ignore_ascii_case(name.trim())
    }) {
        return Err("配置名称已存在".into());
    }
    Ok(())
}

#[tauri::command]
pub(crate) fn get_claude_desktop_profiles(app: tauri::AppHandle) -> ClaudeDesktopProfilesData {
    with_official_profile(read_stored_claude_desktop_profiles(&app))
}

#[tauri::command]
pub(crate) fn add_claude_desktop_profile(
    app: tauri::AppHandle,
    input: ClaudeDesktopProfileInput,
) -> Result<ClaudeDesktopProfile, String> {
    if input.connection_mode == ClaudeDesktopConnectionMode::Official {
        return Err("不能创建 Official 配置".into());
    }
    let profile = profile_from_input(input);
    validate_profile(&profile)?;
    mutate_profiles_at_path(&claude_desktop_profiles_path(&app), |data| {
        ensure_unique_profile_name(data, &profile.name, None)?;
        data.profiles.push(profile.clone());
        Ok(profile)
    })
}

#[tauri::command]
pub(crate) fn update_claude_desktop_profile(
    app: tauri::AppHandle,
    id: String,
    input: ClaudeDesktopProfileInput,
) -> Result<ClaudeDesktopProfile, String> {
    if id == CLAUDE_DESKTOP_OFFICIAL_ID
        || input.connection_mode == ClaudeDesktopConnectionMode::Official
    {
        return Err("Official 配置不能编辑".into());
    }
    mutate_profiles_at_path(&claude_desktop_profiles_path(&app), |data| {
        ensure_unique_profile_name(data, &input.name, Some(&id))?;
        let existing = data
            .profiles
            .iter()
            .find(|profile| profile.id == id)
            .cloned()
            .ok_or("配置未找到")?;
        let mut updated = profile_from_input(input);
        updated.id = existing.id.clone();
        updated.created_at = existing.created_at;
        updated.is_active = existing.is_active;
        if updated.api_key.is_empty() {
            updated.api_key = existing.api_key;
        }
        validate_profile(&updated)?;
        let slot = data
            .profiles
            .iter_mut()
            .find(|profile| profile.id == id)
            .ok_or("配置未找到")?;
        *slot = updated.clone();
        Ok(updated)
    })
}

#[tauri::command]
pub(crate) fn delete_claude_desktop_profile(
    app: tauri::AppHandle,
    id: String,
) -> Result<(), String> {
    if id == CLAUDE_DESKTOP_OFFICIAL_ID {
        return Err("Official 配置不能删除".into());
    }
    mutate_profiles_at_path(&claude_desktop_profiles_path(&app), |data| {
        let profile = data
            .profiles
            .iter()
            .find(|profile| profile.id == id)
            .ok_or("配置未找到")?;
        if profile.is_active {
            return Err("当前启用的配置不能删除，请先切换到其他配置".into());
        }
        data.profiles.retain(|profile| profile.id != id);
        Ok(())
    })
}

#[tauri::command]
pub(crate) fn reorder_claude_desktop_profiles(
    app: tauri::AppHandle,
    ids: Vec<String>,
) -> Result<ClaudeDesktopProfilesData, String> {
    mutate_profiles_at_path(&claude_desktop_profiles_path(&app), |data| {
        *data = reorder_custom_profiles(std::mem::take(data), &ids);
        Ok(with_official_profile(data.clone()))
    })
}

#[tauri::command]
pub(crate) fn import_claude_profiles_to_desktop(
    app: tauri::AppHandle,
) -> Result<ClaudeDesktopProfilesData, String> {
    let source = read_profiles(&app).profiles;
    mutate_profiles_at_path(&claude_desktop_profiles_path(&app), |data| {
        *data = import_from_claude_profiles(std::mem::take(data), &source);
        Ok(with_official_profile(data.clone()))
    })
}

fn activate_profile_at_paths(
    profiles_path: &Path,
    desktop_paths: &ClaudeDesktopPaths,
    id: &str,
    gateway_token: &str,
) -> Result<ClaudeDesktopProfile, String> {
    let mut data = read_profiles_from_path_checked(profiles_path)?;
    let selected = if id == CLAUDE_DESKTOP_OFFICIAL_ID {
        official_profile(true)
    } else {
        data.profiles
            .iter()
            .find(|profile| profile.id == id)
            .cloned()
            .ok_or("配置未找到")?
    };
    let desktop_snapshots = snapshot_paths(desktop_paths)?;
    match selected.connection_mode {
        ClaudeDesktopConnectionMode::Official => restore_official_at_paths(desktop_paths)?,
        _ => apply_profile_at_paths(desktop_paths, &selected, gateway_token)?,
    }

    for profile in &mut data.profiles {
        profile.is_active = profile.id == id;
    }
    if let Err(error) = write_profiles_to_path(profiles_path, &data) {
        return match restore_snapshots(&desktop_snapshots) {
            Ok(()) => Err(error),
            Err(rollback_error) => Err(format!(
                "{error}；Claude Desktop Profile 回滚失败: {rollback_error}"
            )),
        };
    }

    let mut activated = selected;
    activated.is_active = true;
    Ok(activated)
}

fn gateway_token_for_profile(app: &tauri::AppHandle, profile_id: &str) -> Result<String, String> {
    if profile_id == CLAUDE_DESKTOP_OFFICIAL_ID {
        return Ok(String::new());
    }
    let data = read_profiles_from_path_checked(&claude_desktop_profiles_path(app))?;
    let profile = data
        .profiles
        .iter()
        .find(|profile| profile.id == profile_id)
        .ok_or("配置未找到")?;
    if profile.connection_mode == ClaudeDesktopConnectionMode::Gateway {
        get_or_create_claude_desktop_gateway_token(app)
    } else {
        Ok(String::new())
    }
}

fn desktop_gateway_runtime(
    data: &ClaudeDesktopProfilesData,
    active: &ClaudeDesktopProfile,
    token: String,
) -> Result<crate::claude_desktop_gateway::ClaudeDesktopRuntime, String> {
    let pool = data
        .profiles
        .iter()
        .filter(|profile| {
            profile.id != active.id
                && profile.connection_mode == ClaudeDesktopConnectionMode::Gateway
                && profile.proxy_failover
                && !profile.base_url.trim().is_empty()
                && !profile.api_key.trim().is_empty()
        })
        .cloned()
        .collect();
    crate::claude_desktop_gateway::runtime_from_profile_pool(active.clone(), token, pool)
}

fn switch_result(profile: ClaudeDesktopProfile) -> ClaudeDesktopSwitchResult {
    let keep_running = profile.connection_mode == ClaudeDesktopConnectionMode::Gateway;
    let message = if keep_running {
        "配置已写入。请完全退出并重启 Claude Desktop，并保持 VarSwitch 运行。"
    } else {
        "配置已写入。请完全退出并重启 Claude Desktop。"
    };
    ClaudeDesktopSwitchResult {
        success: true,
        profile_id: profile.id,
        profile_name: profile.name,
        mode: profile.connection_mode,
        restart_required: true,
        keep_varswitch_running: keep_running,
        message: message.into(),
    }
}

#[tauri::command]
pub(crate) fn switch_claude_desktop_profile(
    app: tauri::AppHandle,
    id: String,
) -> Result<ClaudeDesktopSwitchResult, String> {
    let paths = current_platform_paths()?;
    let data = read_profiles_from_path_checked(&claude_desktop_profiles_path(&app))?;
    let token = gateway_token_for_profile(&app, &id)?;
    let prospective_runtime = data
        .profiles
        .iter()
        .find(|profile| profile.id == id)
        .filter(|profile| profile.connection_mode == ClaudeDesktopConnectionMode::Gateway)
        .map(|profile| desktop_gateway_runtime(&data, profile, token.clone()))
        .transpose()?;
    if prospective_runtime.is_some() {
        crate::claude_proxy::ensure_server()?;
    }
    let profile =
        activate_profile_at_paths(&claude_desktop_profiles_path(&app), &paths, &id, &token)?;
    if let Some(runtime) = prospective_runtime {
        crate::claude_desktop_gateway::set_desktop_runtime(Some(runtime));
    } else {
        crate::claude_desktop_gateway::clear_desktop_runtime();
    }
    Ok(switch_result(profile))
}

#[tauri::command]
pub(crate) fn sync_claude_desktop_profile(
    app: tauri::AppHandle,
) -> Result<ClaudeDesktopSwitchResult, String> {
    let data = read_stored_claude_desktop_profiles(&app);
    let id = data
        .profiles
        .iter()
        .find(|profile| profile.is_active)
        .map(|profile| profile.id.clone())
        .unwrap_or_else(|| CLAUDE_DESKTOP_OFFICIAL_ID.into());
    switch_claude_desktop_profile(app, id)
}

pub(crate) fn restore_claude_desktop_runtime(app: &tauri::AppHandle) -> Result<(), String> {
    let data = read_profiles_from_path_checked(&claude_desktop_profiles_path(app))?;
    let Some(active) = data.profiles.iter().find(|profile| profile.is_active) else {
        crate::claude_desktop_gateway::clear_desktop_runtime();
        return Ok(());
    };
    if active.connection_mode != ClaudeDesktopConnectionMode::Gateway {
        crate::claude_desktop_gateway::clear_desktop_runtime();
        return Ok(());
    }
    let token = get_or_create_claude_desktop_gateway_token(app)?;
    let runtime = desktop_gateway_runtime(&data, active, token)?;
    crate::claude_proxy::ensure_server()?;
    crate::claude_desktop_gateway::set_desktop_runtime(Some(runtime));
    Ok(())
}

fn provider_status_from_data(
    data: &ClaudeDesktopProfilesData,
    paths: &ClaudeDesktopPaths,
    installed: bool,
    supported: bool,
    warning: Option<String>,
) -> ClaudeDesktopProviderStatus {
    let active = data.profiles.iter().find(|profile| profile.is_active);
    let (mode, active_profile_id, active_profile_name) = match active {
        Some(profile) => (
            match profile.connection_mode {
                ClaudeDesktopConnectionMode::Gateway => "gateway",
                ClaudeDesktopConnectionMode::Direct => "direct",
                ClaudeDesktopConnectionMode::Official => "official",
            }
            .to_string(),
            Some(profile.id.clone()),
            Some(profile.name.clone()),
        ),
        None => (
            "official".into(),
            Some(CLAUDE_DESKTOP_OFFICIAL_ID.into()),
            Some("Claude Official".into()),
        ),
    };
    let warning = warning.or_else(|| {
        if !supported {
            Some("当前平台不支持启用 Claude Desktop 3P Profile".into())
        } else if !installed {
            Some("未检测到 Claude Desktop；请先安装并启动一次".into())
        } else if mode == "gateway"
            && crate::claude_desktop_gateway::current_desktop_runtime().is_none()
        {
            Some("Claude Desktop Gateway 运行态尚未恢复，请重新同步当前配置".into())
        } else {
            None
        }
    });
    ClaudeDesktopProviderStatus {
        installed,
        supported,
        mode: mode.clone(),
        active_profile_id,
        active_profile_name,
        profile_path: supported.then(|| paths.profile_path.to_string_lossy().to_string()),
        gateway_running: mode == "gateway"
            && crate::claude_proxy::is_running()
            && crate::claude_desktop_gateway::current_desktop_runtime().is_some(),
        gateway_url: CLAUDE_DESKTOP_GATEWAY_URL.into(),
        warning,
    }
}

#[tauri::command]
pub(crate) fn get_claude_desktop_provider_status(
    app: tauri::AppHandle,
) -> ClaudeDesktopProviderStatus {
    let data = read_stored_claude_desktop_profiles(&app);
    let supported = cfg!(any(target_os = "windows", target_os = "macos"));
    let paths = current_platform_paths().unwrap_or_else(|_| paths_from_base(Path::new("")));
    provider_status_from_data(
        &data,
        &paths,
        crate::claude_desktop::claude_desktop_installed(),
        supported,
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn temp_profiles_path(tag: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!(
                "varswitch-claude-desktop-provider-{tag}-{}",
                uuid::Uuid::new_v4()
            ))
            .join("claude_desktop_profiles.json")
    }

    fn fixture_gateway_profile() -> ClaudeDesktopProfile {
        ClaudeDesktopProfile {
            id: "desktop-provider-1".into(),
            name: "Desktop Gateway".into(),
            api_key: "sk-desktop-secret".into(),
            base_url: "https://api.example.com".into(),
            connection_mode: ClaudeDesktopConnectionMode::Gateway,
            api_format: "openai_chat".into(),
            model_id: "upstream-default".into(),
            sonnet_model: "upstream-sonnet".into(),
            opus_model: "upstream-opus".into(),
        haiku_model: String::new(),
        available_models: Vec::new(),
            proxy_failover: false,
            is_active: false,
            created_at: "2026-08-26T00:00:00Z".into(),
        }
    }

    #[test]
    fn desktop_profiles_round_trip_encrypts_api_keys() {
        let path = temp_profiles_path("round-trip");
        let data = ClaudeDesktopProfilesData {
            profiles: vec![fixture_gateway_profile()],
        };

        write_profiles_to_path(&path, &data).unwrap();

        let raw = fs::read_to_string(&path).unwrap();
        assert!(!raw.contains("sk-desktop-secret"));
        let loaded = read_profiles_from_path(&path);
        assert_eq!(loaded.profiles[0].api_key, "sk-desktop-secret");
    }

    #[test]
    fn damaged_profiles_are_not_overwritten_by_mutation() {
        let path = temp_profiles_path("damaged-mutation");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"{ damaged json").unwrap();
        let original = fs::read(&path).unwrap();

        let result = mutate_profiles_at_path(&path, |data| {
            data.profiles.push(fixture_gateway_profile());
            Ok(())
        });

        assert!(result.unwrap_err().contains("未覆盖原文件"));
        assert_eq!(fs::read(&path).unwrap(), original);
    }

    #[test]
    fn gateway_requires_one_resolvable_model() {
        let mut profile = fixture_gateway_profile();
        profile.model_id.clear();
        profile.sonnet_model.clear();
        profile.opus_model.clear();
        profile.haiku_model.clear();

        assert!(validate_profile(&profile).unwrap_err().contains("模型"));
    }

    #[test]
    fn direct_mode_rejects_openai_chat() {
        let mut profile = fixture_gateway_profile();
        profile.connection_mode = ClaudeDesktopConnectionMode::Direct;
        profile.api_format = "openai_chat".into();

        assert!(validate_profile(&profile).is_err());
    }

    #[test]
    fn profile_validation_rejects_invalid_urls() {
        let mut profile = fixture_gateway_profile();
        profile.base_url = "not a url".into();

        assert!(validate_profile(&profile).unwrap_err().contains("URL"));
    }

    #[test]
    fn direct_mode_requires_claude_safe_model_ids() {
        let mut profile = fixture_gateway_profile();
        profile.connection_mode = ClaudeDesktopConnectionMode::Direct;
        profile.api_format = "anthropic".into();

        assert!(validate_profile(&profile).unwrap_err().contains("Gateway"));
    }

    #[test]
    fn official_profile_is_synthesized_first_and_cannot_be_duplicated() {
        let mut persisted_official = fixture_gateway_profile();
        persisted_official.id = CLAUDE_DESKTOP_OFFICIAL_ID.into();
        let data = ClaudeDesktopProfilesData {
            profiles: vec![fixture_gateway_profile(), persisted_official],
        };

        let visible = with_official_profile(data);

        assert_eq!(visible.profiles[0].id, CLAUDE_DESKTOP_OFFICIAL_ID);
        assert_eq!(
            visible.profiles[0].connection_mode,
            ClaudeDesktopConnectionMode::Official
        );
        assert_eq!(
            visible
                .profiles
                .iter()
                .filter(|profile| profile.id == CLAUDE_DESKTOP_OFFICIAL_ID)
                .count(),
            1
        );
    }

    #[test]
    fn claude_import_maps_modes_and_generates_unique_names() {
        let existing = ClaudeDesktopProfilesData {
            profiles: vec![fixture_gateway_profile()],
        };
        let source = vec![
            crate::Profile {
                id: "claude-1".into(),
                name: "Desktop Gateway".into(),
                api_key: "sk-openai".into(),
                base_url: "https://openai.example.com".into(),
                model_id: "gpt-model".into(),
                sonnet_model: String::new(),
                opus_model: String::new(),
                haiku_model: String::new(),
                api_format: "openai_chat".into(),
                proxy_failover: true,
                proxy_takeover: false,
                is_active: true,
                created_at: "1".into(),
            },
            crate::Profile {
                id: "claude-2".into(),
                name: "Anthropic Direct".into(),
                api_key: "sk-anthropic".into(),
                base_url: "https://anthropic.example.com".into(),
                model_id: "claude-model".into(),
                sonnet_model: String::new(),
                opus_model: String::new(),
                haiku_model: String::new(),
                api_format: "anthropic".into(),
                proxy_failover: false,
                proxy_takeover: false,
                is_active: false,
                created_at: "2".into(),
            },
        ];

        let imported = import_from_claude_profiles(existing, &source);

        assert_eq!(imported.profiles.len(), 3);
        assert_eq!(imported.profiles[1].name, "Desktop Gateway (2)");
        assert_eq!(
            imported.profiles[1].connection_mode,
            ClaudeDesktopConnectionMode::Gateway
        );
        assert_eq!(
            imported.profiles[2].connection_mode,
            ClaudeDesktopConnectionMode::Direct
        );
        assert!(imported.profiles[1..]
            .iter()
            .all(|profile| !profile.is_active));
    }

    #[test]
    fn custom_reorder_ignores_official_and_keeps_unlisted_profiles() {
        let first = fixture_gateway_profile();
        let mut second = fixture_gateway_profile();
        second.id = "desktop-provider-2".into();
        let mut third = fixture_gateway_profile();
        third.id = "desktop-provider-3".into();
        let data = ClaudeDesktopProfilesData {
            profiles: vec![first, second, third],
        };

        let reordered = reorder_custom_profiles(
            data,
            &[
                CLAUDE_DESKTOP_OFFICIAL_ID.into(),
                "desktop-provider-3".into(),
                "desktop-provider-1".into(),
            ],
        );

        let ids: Vec<&str> = reordered
            .profiles
            .iter()
            .map(|profile| profile.id.as_str())
            .collect();
        assert_eq!(
            ids,
            vec![
                "desktop-provider-3",
                "desktop-provider-1",
                "desktop-provider-2"
            ]
        );
    }

    fn temp_desktop_paths(tag: &str) -> ClaudeDesktopPaths {
        paths_from_base(&std::env::temp_dir().join(format!(
            "varswitch-claude-desktop-paths-{tag}-{}",
            uuid::Uuid::new_v4()
        )))
    }

    fn read_json_value(path: &Path) -> serde_json::Value {
        serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
    }

    #[test]
    fn gateway_projection_writes_local_url_generated_token_and_safe_models() {
        let paths = temp_desktop_paths("gateway");
        let profile = fixture_gateway_profile();

        apply_profile_at_paths(&paths, &profile, "vsd-test-token").unwrap();

        let json = read_json_value(&paths.profile_path);
        assert_eq!(
            json["inferenceGatewayBaseUrl"],
            "http://127.0.0.1:25789/claude-desktop"
        );
        assert_eq!(json["inferenceGatewayApiKey"], "vsd-test-token");
        assert_eq!(
            json["inferenceModels"],
            serde_json::json!([
                "claude-sonnet-4-6",
                "claude-opus-4-6",
                "claude-haiku-4-5-20251001"
            ])
        );
        assert!(!json.to_string().contains("sk-desktop-secret"));
        assert!(!json.to_string().contains("upstream-opus"));
    }

    #[test]
    fn direct_projection_writes_upstream_credentials() {
        let paths = temp_desktop_paths("direct");
        let mut profile = fixture_gateway_profile();
        profile.connection_mode = ClaudeDesktopConnectionMode::Direct;
        profile.api_format = "anthropic".into();
        profile.model_id = "claude-sonnet-4-6".into();
        profile.sonnet_model = "claude-sonnet-4-6".into();
        profile.opus_model = "claude-opus-4-6".into();
        profile.haiku_model = "claude-haiku-4-5-20251001".into();

        apply_profile_at_paths(&paths, &profile, "unused").unwrap();

        let json = read_json_value(&paths.profile_path);
        assert_eq!(json["inferenceGatewayBaseUrl"], profile.base_url);
        assert_eq!(json["inferenceGatewayApiKey"], profile.api_key);
    }

    #[test]
    fn projection_preserves_unrelated_config_and_meta_entries() {
        let paths = temp_desktop_paths("preserve");
        fs::create_dir_all(paths.normal_config_path.parent().unwrap()).unwrap();
        fs::create_dir_all(&paths.config_library_path).unwrap();
        fs::write(
            &paths.normal_config_path,
            r#"{"mcpServers":{"keep":{}},"theme":"dark"}"#,
        )
        .unwrap();
        fs::write(
            &paths.meta_path,
            r#"{"entries":[{"id":"other","name":"Other"}],"custom":true}"#,
        )
        .unwrap();

        apply_profile_at_paths(&paths, &fixture_gateway_profile(), "vsd-test").unwrap();

        let normal = read_json_value(&paths.normal_config_path);
        let meta = read_json_value(&paths.meta_path);
        assert_eq!(normal["theme"], "dark");
        assert!(normal["mcpServers"]["keep"].is_object());
        assert_eq!(meta["custom"], true);
        assert!(meta["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["id"] == "other"));
    }

    #[test]
    fn failed_profile_write_rolls_back_earlier_files() {
        let paths = temp_desktop_paths("rollback");
        fs::create_dir_all(paths.normal_config_path.parent().unwrap()).unwrap();
        fs::create_dir_all(paths.threep_config_path.parent().unwrap()).unwrap();
        fs::write(
            &paths.normal_config_path,
            br#"{"deploymentMode":"1p","keep":1}"#,
        )
        .unwrap();
        fs::write(
            &paths.threep_config_path,
            br#"{"deploymentMode":"1p","keep":2}"#,
        )
        .unwrap();
        fs::write(&paths.config_library_path, b"blocks-directory-creation").unwrap();
        let normal_before = fs::read(&paths.normal_config_path).unwrap();
        let threep_before = fs::read(&paths.threep_config_path).unwrap();

        assert!(apply_profile_at_paths(&paths, &fixture_gateway_profile(), "vsd-test").is_err());

        assert_eq!(fs::read(&paths.normal_config_path).unwrap(), normal_before);
        assert_eq!(fs::read(&paths.threep_config_path).unwrap(), threep_before);
        assert!(!paths.profile_path.exists());
        assert!(!paths.meta_path.exists());
    }

    #[test]
    fn official_restore_removes_only_varswitch_profile_and_reference() {
        let paths = temp_desktop_paths("official");
        fs::create_dir_all(&paths.config_library_path).unwrap();
        fs::create_dir_all(paths.normal_config_path.parent().unwrap()).unwrap();
        fs::write(
            &paths.normal_config_path,
            r#"{"deploymentMode":"3p","keep":1}"#,
        )
        .unwrap();
        fs::write(
            &paths.threep_config_path,
            r#"{"deploymentMode":"3p","keep":2}"#,
        )
        .unwrap();
        fs::write(&paths.profile_path, "{}").unwrap();
        fs::write(
            &paths.meta_path,
            format!(
                r#"{{"appliedId":"{VARSWITCH_DESKTOP_PROFILE_ID}","entries":[{{"id":"other","name":"Other"}},{{"id":"{VARSWITCH_DESKTOP_PROFILE_ID}","name":"VarSwitch"}}],"keep":true}}"#
            ),
        )
        .unwrap();

        restore_official_at_paths(&paths).unwrap();

        let normal = read_json_value(&paths.normal_config_path);
        let threep = read_json_value(&paths.threep_config_path);
        let meta = read_json_value(&paths.meta_path);
        assert_eq!(normal["deploymentMode"], "1p");
        assert_eq!(threep["deploymentMode"], "1p");
        assert_eq!(normal["keep"], 1);
        assert_eq!(threep["keep"], 2);
        assert!(!paths.profile_path.exists());
        assert_eq!(meta["appliedId"], "other");
        assert_eq!(meta["keep"], true);
        assert_eq!(meta["entries"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn gateway_token_is_random_prefixed_and_stable() {
        let path = temp_profiles_path("gateway-token");

        let first = get_or_create_gateway_token_at_path(&path).unwrap();
        let second = get_or_create_gateway_token_at_path(&path).unwrap();

        assert!(first.starts_with("vsd-"));
        assert_ne!(first, "PROXY_MANAGED");
        assert_eq!(first, second);
    }

    #[test]
    fn projection_failure_keeps_previous_active_profile() {
        let profiles_path = temp_profiles_path("active-rollback");
        let paths = temp_desktop_paths("active-rollback");
        let mut first = fixture_gateway_profile();
        first.is_active = true;
        let mut second = fixture_gateway_profile();
        second.id = "desktop-provider-2".into();
        second.name = "Second".into();
        let data = ClaudeDesktopProfilesData {
            profiles: vec![first, second],
        };
        write_profiles_to_path(&profiles_path, &data).unwrap();
        fs::create_dir_all(paths.threep_config_path.parent().unwrap()).unwrap();
        fs::write(&paths.config_library_path, b"blocked").unwrap();

        assert!(activate_profile_at_paths(
            &profiles_path,
            &paths,
            "desktop-provider-2",
            "vsd-test"
        )
        .is_err());

        let stored = read_profiles_from_path(&profiles_path);
        assert!(stored.profiles[0].is_active);
        assert!(!stored.profiles[1].is_active);
    }

    #[test]
    fn successful_activation_commits_exactly_one_active_profile() {
        let profiles_path = temp_profiles_path("active-success");
        let paths = temp_desktop_paths("active-success");
        let mut first = fixture_gateway_profile();
        first.is_active = true;
        let mut second = fixture_gateway_profile();
        second.id = "desktop-provider-2".into();
        second.name = "Second".into();
        let data = ClaudeDesktopProfilesData {
            profiles: vec![first, second],
        };
        write_profiles_to_path(&profiles_path, &data).unwrap();

        let activated =
            activate_profile_at_paths(&profiles_path, &paths, "desktop-provider-2", "vsd-test")
                .unwrap();

        assert_eq!(activated.id, "desktop-provider-2");
        let stored = read_profiles_from_path(&profiles_path);
        assert_eq!(
            stored
                .profiles
                .iter()
                .filter(|profile| profile.is_active)
                .count(),
            1
        );
        assert!(stored.profiles[1].is_active);
    }

    #[test]
    fn provider_status_contract_is_camel_case_and_never_contains_secrets() {
        let mut profile = fixture_gateway_profile();
        profile.is_active = true;
        let data = ClaudeDesktopProfilesData {
            profiles: vec![profile],
        };
        let paths = temp_desktop_paths("status");

        let status = provider_status_from_data(&data, &paths, true, true, None);
        let json = serde_json::to_value(status).unwrap();

        assert_eq!(json["installed"], true);
        assert_eq!(json["supported"], true);
        assert_eq!(json["mode"], "gateway");
        assert_eq!(json["activeProfileId"], "desktop-provider-1");
        assert_eq!(json["gatewayUrl"], CLAUDE_DESKTOP_GATEWAY_URL);
        assert!(json.get("active_profile_id").is_none());
        assert!(!json.to_string().contains("sk-desktop-secret"));
    }
}
