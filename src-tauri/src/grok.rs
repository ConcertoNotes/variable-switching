//! Grok / xAI 配置域：档案存取、~/.grok/config.toml 读写、诊断与前端命令（从 lib.rs 拆分，逻辑未改动）。

use crate::*;

/// xAI / Grok 官方环境变量
pub(crate) const XAI_API_KEY_ENV: &str = "XAI_API_KEY";
pub(crate) const XAI_BASE_URL_ENV: &str = "XAI_BASE_URL";
pub(crate) const XAI_MODEL_ENV: &str = "XAI_MODEL";
/// 部分工具使用的兼容别名
pub(crate) const GROK_API_KEY_ENV: &str = "GROK_API_KEY";
pub(crate) const GROK_BASE_URL_ENV: &str = "GROK_BASE_URL";
pub(crate) const DEFAULT_XAI_BASE_URL: &str = "https://api.x.ai/v1";

/// VarSwitch 在 ~/.grok/config.toml 中管理的模型段 ID
pub(crate) const GROK_MANAGED_MODEL_ID: &str = "varswitch";

/// Grok / xAI API 配置档案（对应 ~/.grok/config.toml 的 [model.*]）
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GrokProfile {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) api_key: String,
    pub(crate) base_url: String,
    #[serde(default)]
    pub(crate) model: String,
    /// chat_completions | responses | messages
    #[serde(default = "default_grok_api_backend")]
    pub(crate) api_backend: String,
    pub(crate) is_active: bool,
    pub(crate) created_at: String,
}

#[derive(Serialize, Deserialize, Default, Clone)]
pub(crate) struct GrokProfilesData {
    pub(crate) profiles: Vec<GrokProfile>,
}


pub(crate) fn default_grok_api_backend() -> String {
    "chat_completions".to_string()
}

/// 返回给前端的 Grok 运行时状态（比 LocationStatus 更完整）
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GrokRuntimeStatus {
    pub(crate) api_key: String,
    pub(crate) base_url: String,
    pub(crate) model: String,
    pub(crate) default_model_id: String,
    pub(crate) api_backend: String,
    pub(crate) config_path: String,
    pub(crate) config_exists: bool,
    pub(crate) source: String, // "config.toml" | "env" | "none"
}

/// Grok 诊断信息
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GrokConfigDiagnostics {
    pub(crate) config_path: String,
    pub(crate) config_exists: bool,
    pub(crate) has_default_model: bool,
    pub(crate) has_api_key: bool,
    pub(crate) has_base_url: bool,
    pub(crate) default_model_id: String,
    pub(crate) model: String,
    pub(crate) base_url: String,
    pub(crate) api_backend: String,
    pub(crate) active_profile_name: String,
    pub(crate) source: String,
    pub(crate) issues: Vec<String>,
    pub(crate) suggestions: Vec<String>,
    pub(crate) last_checked_at: String,
}

pub(crate) fn grok_profiles_path(app: &tauri::AppHandle) -> PathBuf {
    data_dir(app).join("grok_profiles.json")
}

pub(crate) fn read_grok_profiles(app: &tauri::AppHandle) -> GrokProfilesData {
    let path = grok_profiles_path(app);
    if !path.exists() {
        return GrokProfilesData::default();
    }
    let mut data: GrokProfilesData = fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    for p in data.profiles.iter_mut() {
        p.api_key = decrypt_secret_or_keep(&p.api_key, &format!("Grok 配置「{}」", p.name));
    }
    data
}

pub(crate) fn write_grok_profiles(app: &tauri::AppHandle, data: &GrokProfilesData) -> Result<(), String> {
    let path = grok_profiles_path(app);
    let mut encrypted = data.clone();
    for p in encrypted.profiles.iter_mut() {
        p.api_key = encrypt_secret(&p.api_key);
    }
    let json = serde_json::to_string_pretty(&encrypted).map_err(|e| e.to_string())?;
    write_private_file(&path, &json)?;
    refresh_tray_menu(app);
    Ok(())
}


pub(crate) fn default_xai_base_url() -> String {
    DEFAULT_XAI_BASE_URL.to_string()
}

pub(crate) fn grok_config_dir() -> PathBuf {
    home_dir().join(".grok")
}

pub(crate) fn grok_config_path() -> PathBuf {
    grok_config_dir().join("config.toml")
}


/// 删除 TOML 中指定 section（含表头与正文），保留其它内容。
pub(crate) fn remove_toml_section(config: &str, section: &str) -> String {
    let target = format!("[{section}]");
    let mut out = String::new();
    let mut skipping = false;
    for line in config.lines() {
        let trimmed = line.trim();
        if is_toml_section_header(trimmed) {
            skipping = trimmed == target;
            if skipping {
                continue;
            }
        }
        if skipping {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// 在指定 TOML 表内 upsert 字符串键；表不存在则追加。
pub(crate) fn upsert_toml_table_string_key(config: &str, table: &str, key: &str, value: &str) -> String {
    let target = format!("[{table}]");
    let key_prefix = format!("{key} =");
    let new_line = format!("{key} = \"{}\"", escape_toml_string_value(value));
    let mut out = String::new();
    let mut in_target = false;
    let mut key_written = false;
    let mut table_found = false;
    let lines: Vec<&str> = config.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();
        if is_toml_section_header(trimmed) {
            if in_target && !key_written {
                out.push_str(&new_line);
                out.push('\n');
                key_written = true;
            }
            in_target = trimmed == target;
            if in_target {
                table_found = true;
            }
            out.push_str(line);
            out.push('\n');
            i += 1;
            continue;
        }
        if in_target && trimmed.starts_with(&key_prefix) {
            if !key_written {
                out.push_str(&new_line);
                out.push('\n');
                key_written = true;
            }
            i += 1;
            continue;
        }
        out.push_str(line);
        out.push('\n');
        i += 1;
    }
    if in_target && !key_written {
        out.push_str(&new_line);
        out.push('\n');
        key_written = true;
    }
    if !table_found {
        if !out.ends_with('\n') && !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&target);
        out.push('\n');
        out.push_str(&new_line);
        out.push('\n');
        let _ = key_written;
    }
    out
}

/// 解析 ~/.grok/config.toml 中当前默认模型段的关键字段。
pub(crate) fn read_grok_runtime_status() -> GrokRuntimeStatus {
    let path = grok_config_path();
    let config_exists = path.exists();
    let config = fs::read_to_string(&path).unwrap_or_default();

    let mut default_model_id = String::new();
    let mut api_key = String::new();
    let mut base_url = String::new();
    let mut model = String::new();
    let mut api_backend = String::new();
    let mut source = "none".to_string();

    if !config.trim().is_empty() {
        default_model_id = toml_section_value(&config, "models", "default");
        if default_model_id.is_empty() {
            // 没有 default 时优先用托管段，否则取空
            default_model_id = GROK_MANAGED_MODEL_ID.to_string();
        }

        let section = format!("model.{default_model_id}");
        api_key = toml_section_value(&config, &section, "api_key");
        base_url = toml_section_value(&config, &section, "base_url");
        model = toml_section_value(&config, &section, "model");
        api_backend = toml_section_value(&config, &section, "api_backend");

        // 默认段没有 key 时，回退到托管段
        if api_key.is_empty() {
            let managed = format!("model.{GROK_MANAGED_MODEL_ID}");
            let managed_key = toml_section_value(&config, &managed, "api_key");
            if !managed_key.is_empty() {
                api_key = managed_key;
                if base_url.is_empty() {
                    base_url = toml_section_value(&config, &managed, "base_url");
                }
                if model.is_empty() {
                    model = toml_section_value(&config, &managed, "model");
                }
                if api_backend.is_empty() {
                    api_backend = toml_section_value(&config, &managed, "api_backend");
                }
                default_model_id = GROK_MANAGED_MODEL_ID.to_string();
            }
        }

        if !api_key.is_empty() || !base_url.is_empty() {
            source = "config.toml".to_string();
        }
    }

    if api_key.is_empty() {
        if let Some(env_key) =
            reg_get_env_opt(XAI_API_KEY_ENV).or_else(|| reg_get_env_opt(GROK_API_KEY_ENV))
        {
            api_key = env_key;
            if source == "none" {
                source = "env".to_string();
            }
        }
    }
    if base_url.is_empty() {
        base_url = reg_get_env_opt(XAI_BASE_URL_ENV)
            .or_else(|| reg_get_env_opt(GROK_BASE_URL_ENV))
            .unwrap_or_else(default_xai_base_url);
    }
    if model.is_empty() {
        model = reg_get_env_opt(XAI_MODEL_ENV).unwrap_or_default();
    }
    if api_backend.is_empty() {
        api_backend = default_grok_api_backend();
    }

    GrokRuntimeStatus {
        api_key,
        base_url,
        model,
        default_model_id,
        api_backend,
        config_path: path.to_string_lossy().to_string(),
        config_exists,
        source,
    }
}

/// 兼容旧 LocationStatus 接口。
pub(crate) fn read_grok_status() -> Option<LocationStatus> {
    let status = read_grok_runtime_status();
    if status.api_key.is_empty() && status.base_url.is_empty() {
        return None;
    }
    Some(LocationStatus {
        api_key: status.api_key,
        base_url: status.base_url,
        image_api_key: String::new(),
        image_base_url: String::new(),
        image_skill_installed: false,
    })
}

pub(crate) fn read_grok_current_model_id() -> String {
    let status = read_grok_runtime_status();
    if !status.model.is_empty() {
        return status.model;
    }
    reg_get_env_opt(XAI_MODEL_ENV).unwrap_or_default()
}

pub(crate) fn normalize_grok_api_backend(value: &str) -> String {
    match value.trim() {
        "responses" => "responses".to_string(),
        "messages" => "messages".to_string(),
        _ => default_grok_api_backend(),
    }
}

pub(crate) fn read_grok_config_diagnostics(app: &tauri::AppHandle) -> GrokConfigDiagnostics {
    let runtime = read_grok_runtime_status();
    let profiles = read_grok_profiles(app);
    let active_name = profiles
        .profiles
        .iter()
        .find(|p| p.is_active)
        .map(|p| p.name.clone())
        .unwrap_or_default();

    let mut issues = Vec::new();
    let mut suggestions = Vec::new();

    if !runtime.config_exists {
        issues.push("~/.grok/config.toml 不存在".into());
        suggestions.push("点击切换任意 Grok 配置，将自动创建并写入 config.toml".into());
    }
    if runtime.api_key.is_empty() {
        issues.push("未检测到 API Key".into());
        suggestions.push("在 Grok 页添加配置并切换，或手动填写 [model.*].api_key".into());
    }
    if runtime.base_url.is_empty() {
        issues.push("未检测到 Base URL".into());
        suggestions.push("切换配置时会写入 base_url，默认 https://api.x.ai/v1".into());
    }
    if runtime.default_model_id.is_empty() {
        issues.push("未设置 [models].default".into());
        suggestions.push("切换配置会把 default 设为 varswitch 托管段".into());
    }

    GrokConfigDiagnostics {
        config_path: runtime.config_path,
        config_exists: runtime.config_exists,
        has_default_model: !runtime.default_model_id.is_empty(),
        has_api_key: !runtime.api_key.is_empty(),
        has_base_url: !runtime.base_url.is_empty(),
        default_model_id: runtime.default_model_id,
        model: runtime.model,
        base_url: runtime.base_url,
        api_backend: runtime.api_backend,
        active_profile_name: active_name,
        source: runtime.source,
        issues,
        suggestions,
        last_checked_at: chrono_now(),
    }
}

/// 将 Grok 配置写入系统环境变量（XAI_* / GROK_*，作为兼容回退）。
pub(crate) fn apply_grok_to_system_env(api_key: &str, base_url: &str, model: &str) -> Result<(), String> {
    reg_set_env(XAI_API_KEY_ENV, api_key)?;
    reg_set_env(XAI_BASE_URL_ENV, base_url)?;
    reg_set_env(GROK_API_KEY_ENV, api_key)?;
    reg_set_env(GROK_BASE_URL_ENV, base_url)?;
    if model.trim().is_empty() {
        if reg_get_env_opt(XAI_MODEL_ENV).is_some() {
            reg_delete_env(XAI_MODEL_ENV)?;
        }
    } else {
        reg_set_env(XAI_MODEL_ENV, model.trim())?;
    }
    Ok(())
}

/// 写入 ~/.grok/config.toml：设置默认模型为 varswitch，并更新托管模型段。
/// 会保留用户其它 section（[ui]、其它 [model.*]、MCP 等）。
pub(crate) fn write_grok_config(profile: &GrokProfile) -> Result<(), String> {
    let dir = grok_config_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("创建 ~/.grok 目录失败: {e}"))?;

    let path = grok_config_path();
    let existing = fs::read_to_string(&path).unwrap_or_default();

    // 移除旧的托管段，再写回，避免残留字段
    let mut content = remove_toml_section(&existing, &format!("model.{GROK_MANAGED_MODEL_ID}"));
    content = upsert_toml_table_string_key(&content, "models", "default", GROK_MANAGED_MODEL_ID);

    let model_id = if profile.model.trim().is_empty() {
        "grok-4".to_string()
    } else {
        profile.model.trim().to_string()
    };
    let display_name = if profile.name.trim().is_empty() {
        model_id.clone()
    } else {
        profile.name.trim().to_string()
    };
    let base_url = if profile.base_url.trim().is_empty() {
        default_xai_base_url()
    } else {
        profile.base_url.trim().trim_end_matches('/').to_string()
    };
    let api_backend = normalize_grok_api_backend(&profile.api_backend);

    let managed_section = format!(
        r#"
[model.{managed}]
model = "{model}"
base_url = "{base_url}"
name = "{name}"
api_key = "{api_key}"
api_backend = "{api_backend}"
"#,
        managed = GROK_MANAGED_MODEL_ID,
        model = escape_toml_string_value(&model_id),
        base_url = escape_toml_string_value(&base_url),
        name = escape_toml_string_value(&display_name),
        api_key = escape_toml_string_value(profile.api_key.trim()),
        api_backend = escape_toml_string_value(&api_backend),
    );

    if !content.ends_with('\n') && !content.is_empty() {
        content.push('\n');
    }
    content.push_str(managed_section.trim_start());
    if !content.ends_with('\n') {
        content.push('\n');
    }

    write_file_atomic(&path, &content).map_err(|e| format!("写入 ~/.grok/config.toml 失败: {e}"))?;
    log_info!(
        "[grok] 已写入 ~/.grok/config.toml model={} base_url={} api_backend={}",
        model_id,
        base_url,
        api_backend
    );
    Ok(())
}

#[tauri::command]
pub(crate) fn reorder_grok_profiles(
    app: tauri::AppHandle,
    ids: Vec<String>,
) -> Result<GrokProfilesData, String> {
    let mut data = read_grok_profiles(&app);
    data.profiles = reorder_by_ids(std::mem::take(&mut data.profiles), &ids, |p| &p.id);
    write_grok_profiles(&app, &data)?;
    Ok(data)
}

// ── Grok Profile Commands ───────────────────────────

#[tauri::command]
pub(crate) fn get_grok_profiles(app: tauri::AppHandle) -> GrokProfilesData {
    read_grok_profiles(&app)
}

#[tauri::command]
pub(crate) fn add_grok_profile(
    app: tauri::AppHandle,
    name: String,
    api_key: String,
    base_url: String,
    model: Option<String>,
    api_backend: Option<String>,
) -> Result<GrokProfile, String> {
    if name.is_empty() || api_key.is_empty() {
        return Err("配置名称和 API Key 都必须填写".into());
    }
    let mut data = read_grok_profiles(&app);
    let resolved_base = {
        let trimmed = base_url.trim().trim_end_matches('/');
        if trimmed.is_empty() {
            default_xai_base_url()
        } else {
            trimmed.to_string()
        }
    };
    let profile = GrokProfile {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.trim().to_string(),
        api_key: api_key.trim().to_string(),
        base_url: resolved_base,
        model: model.unwrap_or_default().trim().to_string(),
        api_backend: normalize_grok_api_backend(&api_backend.unwrap_or_default()),
        is_active: false,
        created_at: chrono_now(),
    };
    data.profiles.push(profile.clone());
    write_grok_profiles(&app, &data)?;
    Ok(profile)
}

#[tauri::command]
pub(crate) fn update_grok_profile(
    app: tauri::AppHandle,
    id: String,
    name: String,
    api_key: String,
    base_url: String,
    model: Option<String>,
    api_backend: Option<String>,
) -> Result<GrokProfile, String> {
    let mut data = read_grok_profiles(&app);
    let p = data
        .profiles
        .iter_mut()
        .find(|x| x.id == id)
        .ok_or("配置未找到")?;
    if !name.is_empty() {
        p.name = name.trim().to_string();
    }
    if !api_key.is_empty() {
        p.api_key = api_key.trim().to_string();
    }
    if !base_url.trim().is_empty() {
        p.base_url = base_url.trim().trim_end_matches('/').to_string();
    }
    p.model = model.unwrap_or_default().trim().to_string();
    if let Some(backend) = api_backend {
        p.api_backend = normalize_grok_api_backend(&backend);
    }
    let updated = p.clone();
    write_grok_profiles(&app, &data)?;
    Ok(updated)
}

#[tauri::command]
pub(crate) fn delete_grok_profile(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let mut data = read_grok_profiles(&app);
    data.profiles.retain(|x| x.id != id);
    write_grok_profiles(&app, &data)
}

#[tauri::command]
pub(crate) fn switch_grok_profile(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let mut data = read_grok_profiles(&app);
    let profile = data
        .profiles
        .iter()
        .find(|x| x.id == id)
        .ok_or("配置未找到")?
        .clone();
    ensure_secret_usable(&profile.api_key, &format!("Grok 配置「{}」", profile.name))?;

    // 切换前自动备份当前配置（含 ~/.grok/config.toml）
    auto_backup_configs(&app);

    // 主路径：写入 ~/.grok/config.toml（Grok CLI 实际读取位置）
    write_grok_config(&profile)?;
    // 兼容路径：同步系统环境变量，供其它依赖 XAI_* 的工具使用
    apply_grok_to_system_env(&profile.api_key, &profile.base_url, &profile.model)?;
    broadcast_env_change();

    for p in data.profiles.iter_mut() {
        p.is_active = p.id == profile.id;
    }
    write_grok_profiles(&app, &data)?;
    Ok(())
}

#[tauri::command]
pub(crate) fn import_grok_current(app: tauri::AppHandle, name: String) -> Result<GrokProfile, String> {
    let status =
        read_grok_status().ok_or("未检测到当前 Grok 配置（~/.grok/config.toml 或环境变量）")?;
    if status.api_key.is_empty() {
        return Err("未检测到 API Key（请检查 ~/.grok/config.toml 或 XAI_API_KEY）".into());
    }

    let mut data = read_grok_profiles(&app);
    if data
        .profiles
        .iter()
        .any(|x| x.api_key == status.api_key && x.base_url == status.base_url)
    {
        return Err("该配置已存在".into());
    }

    let runtime = read_grok_runtime_status();
    let profile = GrokProfile {
        id: uuid::Uuid::new_v4().to_string(),
        name: if name.is_empty() {
            "导入的 Grok 配置".into()
        } else {
            name
        },
        api_key: status.api_key,
        base_url: if status.base_url.trim().is_empty() {
            default_xai_base_url()
        } else {
            status.base_url
        },
        model: if runtime.model.is_empty() {
            read_grok_current_model_id()
        } else {
            runtime.model
        },
        api_backend: normalize_grok_api_backend(&runtime.api_backend),
        is_active: true,
        created_at: chrono_now(),
    };

    for p in data.profiles.iter_mut() {
        p.is_active = false;
    }
    data.profiles.push(profile.clone());
    write_grok_profiles(&app, &data)?;
    Ok(profile)
}

#[tauri::command]
pub(crate) fn get_grok_status() -> Option<GrokRuntimeStatus> {
    let status = read_grok_runtime_status();
    if status.api_key.is_empty() && !status.config_exists {
        return None;
    }
    Some(status)
}

#[tauri::command]
pub(crate) fn get_grok_diagnostics(app: tauri::AppHandle) -> GrokConfigDiagnostics {
    read_grok_config_diagnostics(&app)
}

#[tauri::command]
pub(crate) fn backup_grok_runtime(app: tauri::AppHandle) -> Result<String, String> {
    let dir = backups_dir(&app).join("grok-runtime");
    fs::create_dir_all(&dir).map_err(|e| format!("创建备份目录失败: {e}"))?;
    let stamp = format_compact_time(chrono_timestamp_millis());
    match backup_one_file_with_ext(&dir, &grok_config_path(), "config", &stamp, "toml") {
        Some(path) => Ok(path),
        None => Err("没有可备份的 ~/.grok/config.toml".into()),
    }
}

#[tauri::command]
pub(crate) fn open_grok_config_folder() -> Result<(), String> {
    let dir = grok_config_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("创建 ~/.grok 失败: {e}"))?;
    open_folder(dir.to_string_lossy().to_string())
}
