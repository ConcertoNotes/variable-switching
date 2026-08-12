//! Gemini CLI 配置域：档案存取、settings.json 与官方环境变量同步、前端命令（从 lib.rs 拆分，逻辑未改动）。

use crate::*;

pub(crate) const GEMINI_API_KEY_ENV: &str = "GEMINI_API_KEY";
pub(crate) const GOOGLE_GEMINI_BASE_URL_ENV: &str = "GOOGLE_GEMINI_BASE_URL";
pub(crate) const GEMINI_MODEL_ENV: &str = "GEMINI_MODEL";

/// Gemini CLI API Key 配置档案（对应 ~/.gemini/settings.json 与官方环境变量）。
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GeminiProfile {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) api_key: String,
    pub(crate) base_url: String,
    #[serde(default)]
    pub(crate) model: String,
    pub(crate) is_active: bool,
    pub(crate) created_at: String,
}

#[derive(Serialize, Deserialize, Default)]
pub(crate) struct GeminiProfilesData {
    pub(crate) profiles: Vec<GeminiProfile>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GeminiRuntimeStatus {
    pub(crate) api_key: String,
    pub(crate) base_url: String,
    pub(crate) model: String,
    pub(crate) auth_type: String,
    pub(crate) settings_path: String,
    pub(crate) settings_exists: bool,
    pub(crate) source: String,
}

pub(crate) fn gemini_profiles_path(app: &tauri::AppHandle) -> PathBuf {
    data_dir(app).join("gemini_profiles.json")
}

pub(crate) fn read_gemini_profiles(app: &tauri::AppHandle) -> GeminiProfilesData {
    let path = gemini_profiles_path(app);
    if !path.exists() {
        return GeminiProfilesData::default();
    }
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub(crate) fn write_gemini_profiles(app: &tauri::AppHandle, data: &GeminiProfilesData) -> Result<(), String> {
    let json = serde_json::to_string_pretty(data).map_err(|e| e.to_string())?;
    write_private_file(&gemini_profiles_path(app), &json)?;
    refresh_tray_menu(app);
    Ok(())
}

pub(crate) fn gemini_settings_path() -> PathBuf {
    home_dir().join(".gemini").join("settings.json")
}

pub(crate) fn read_gemini_runtime_status() -> GeminiRuntimeStatus {
    let path = gemini_settings_path();
    let settings_exists = path.exists();
    let settings = read_json_or_default(&path, serde_json::json!({}));
    let auth_type = settings
        .pointer("/security/auth/selectedType")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string();
    let settings_model = settings
        .pointer("/model/name")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string();
    let api_key = reg_get_env_opt(GEMINI_API_KEY_ENV).unwrap_or_default();
    let base_url = reg_get_env_opt(GOOGLE_GEMINI_BASE_URL_ENV).unwrap_or_default();
    let model = reg_get_env_opt(GEMINI_MODEL_ENV).unwrap_or(settings_model);
    let source = if !api_key.is_empty() {
        "env"
    } else if settings_exists {
        "settings.json"
    } else {
        "none"
    };

    GeminiRuntimeStatus {
        api_key,
        base_url,
        model,
        auth_type,
        settings_path: path.to_string_lossy().to_string(),
        settings_exists,
        source: source.to_string(),
    }
}

pub(crate) fn write_gemini_settings(profile: &GeminiProfile) -> Result<(), String> {
    let path = gemini_settings_path();
    let mut settings = read_json_or_default(&path, serde_json::json!({}));
    if !settings.is_object() {
        settings = serde_json::json!({});
    }

    if !settings
        .get("security")
        .is_some_and(|value| value.is_object())
    {
        settings["security"] = serde_json::json!({});
    }
    if !settings["security"]
        .get("auth")
        .is_some_and(|value| value.is_object())
    {
        settings["security"]["auth"] = serde_json::json!({});
    }
    settings["security"]["auth"]["selectedType"] = serde_json::json!("gemini-api-key");

    if !settings.get("model").is_some_and(|value| value.is_object()) {
        settings["model"] = serde_json::json!({});
    }
    if profile.model.trim().is_empty() {
        if let Some(model) = settings
            .get_mut("model")
            .and_then(|value| value.as_object_mut())
        {
            model.remove("name");
        }
    } else {
        settings["model"]["name"] = serde_json::json!(profile.model.trim());
    }

    write_json(&path, &settings)
        .map_err(|error| format!("写入 ~/.gemini/settings.json 失败: {error}"))
}

pub(crate) fn apply_gemini_to_system_env(profile: &GeminiProfile) -> Result<(), String> {
    reg_set_env(GEMINI_API_KEY_ENV, profile.api_key.trim())?;
    if profile.base_url.trim().is_empty() {
        reg_delete_env(GOOGLE_GEMINI_BASE_URL_ENV)?;
    } else {
        reg_set_env(
            GOOGLE_GEMINI_BASE_URL_ENV,
            profile.base_url.trim().trim_end_matches('/'),
        )?;
    }
    if profile.model.trim().is_empty() {
        reg_delete_env(GEMINI_MODEL_ENV)?;
    } else {
        reg_set_env(GEMINI_MODEL_ENV, profile.model.trim())?;
    }
    Ok(())
}

#[tauri::command]
pub(crate) fn reorder_gemini_profiles(
    app: tauri::AppHandle,
    ids: Vec<String>,
) -> Result<GeminiProfilesData, String> {
    let mut data = read_gemini_profiles(&app);
    data.profiles = reorder_by_ids(std::mem::take(&mut data.profiles), &ids, |p| &p.id);
    write_gemini_profiles(&app, &data)?;
    Ok(data)
}

// ── Gemini Profile Commands ─────────────────────────

#[tauri::command]
pub(crate) fn get_gemini_profiles(app: tauri::AppHandle) -> GeminiProfilesData {
    read_gemini_profiles(&app)
}

#[tauri::command]
pub(crate) fn add_gemini_profile(
    app: tauri::AppHandle,
    name: String,
    api_key: String,
    base_url: String,
    model: Option<String>,
) -> Result<GeminiProfile, String> {
    if name.trim().is_empty() || api_key.trim().is_empty() {
        return Err("配置名称和 API Key 都必须填写".into());
    }
    let profile = GeminiProfile {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.trim().to_string(),
        api_key: api_key.trim().to_string(),
        base_url: base_url.trim().trim_end_matches('/').to_string(),
        model: model.unwrap_or_default().trim().to_string(),
        is_active: false,
        created_at: chrono_now(),
    };
    let mut data = read_gemini_profiles(&app);
    data.profiles.push(profile.clone());
    write_gemini_profiles(&app, &data)?;
    Ok(profile)
}

#[tauri::command]
pub(crate) fn update_gemini_profile(
    app: tauri::AppHandle,
    id: String,
    name: String,
    api_key: String,
    base_url: String,
    model: Option<String>,
) -> Result<GeminiProfile, String> {
    let mut data = read_gemini_profiles(&app);
    let profile = data
        .profiles
        .iter_mut()
        .find(|profile| profile.id == id)
        .ok_or("配置未找到")?;
    if !name.trim().is_empty() {
        profile.name = name.trim().to_string();
    }
    if !api_key.trim().is_empty() {
        profile.api_key = api_key.trim().to_string();
    }
    profile.base_url = resolve_base_url_or_default(&base_url, DEFAULT_GEMINI_BASE_URL);
    profile.model = model.unwrap_or_default().trim().to_string();
    let updated = profile.clone();
    write_gemini_profiles(&app, &data)?;
    Ok(updated)
}

#[tauri::command]
pub(crate) fn delete_gemini_profile(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let mut data = read_gemini_profiles(&app);
    data.profiles.retain(|profile| profile.id != id);
    write_gemini_profiles(&app, &data)
}

#[tauri::command]
pub(crate) fn switch_gemini_profile(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let mut data = read_gemini_profiles(&app);
    let profile = data
        .profiles
        .iter()
        .find(|profile| profile.id == id)
        .ok_or("配置未找到")?
        .clone();

    auto_backup_configs(&app);
    write_gemini_settings(&profile)?;
    apply_gemini_to_system_env(&profile)?;
    broadcast_env_change();

    for item in data.profiles.iter_mut() {
        item.is_active = item.id == profile.id;
    }
    write_gemini_profiles(&app, &data)
}

#[tauri::command]
pub(crate) fn import_gemini_current(app: tauri::AppHandle, name: String) -> Result<GeminiProfile, String> {
    let status = read_gemini_runtime_status();
    if status.api_key.is_empty() {
        return Err("未检测到 GEMINI_API_KEY".into());
    }
    let mut data = read_gemini_profiles(&app);
    if data
        .profiles
        .iter()
        .any(|profile| profile.api_key == status.api_key && profile.base_url == status.base_url)
    {
        return Err("该配置已存在".into());
    }
    for profile in data.profiles.iter_mut() {
        profile.is_active = false;
    }
    let profile = GeminiProfile {
        id: uuid::Uuid::new_v4().to_string(),
        name: if name.trim().is_empty() {
            "导入的 Gemini 配置".into()
        } else {
            name.trim().to_string()
        },
        api_key: status.api_key,
        base_url: status.base_url,
        model: status.model,
        is_active: true,
        created_at: chrono_now(),
    };
    data.profiles.push(profile.clone());
    write_gemini_profiles(&app, &data)?;
    Ok(profile)
}

#[tauri::command]
pub(crate) fn get_gemini_status() -> Option<GeminiRuntimeStatus> {
    let status = read_gemini_runtime_status();
    if status.api_key.is_empty() && !status.settings_exists {
        None
    } else {
        Some(status)
    }
}
