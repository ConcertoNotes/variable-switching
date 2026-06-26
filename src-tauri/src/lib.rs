use serde::{Deserialize, Serialize};
use std::cmp::Ordering as CmpOrdering;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpStream, UdpSocket};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use qrcode::{QrCode, EcLevel};
use image::{DynamicImage, Luma};
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, State,
};

#[cfg(target_os = "windows")]
use winreg::enums::HKEY_CURRENT_USER;
#[cfg(target_os = "windows")]
use winreg::RegKey;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

// Windows 常量：CREATE_NO_WINDOW 标志，用于隐藏子进程窗口
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

const AUTH_TOKEN_ENV: &str = "ANTHROPIC_AUTH_TOKEN";
const AUTH_KEY_ENV: &str = "ANTHROPIC_AUTH_KEY";
const LEGACY_AUTH_ENV: &str = "ANTHROPIC_API_KEY";
const BASE_URL_ENV: &str = "ANTHROPIC_BASE_URL";
const SWITCH_TOTAL_STEPS: u32 = 6;
const GITHUB_REPO_URL: &str = "https://github.com/ConcertoNotes/variable-switching";
const GITHUB_LATEST_RELEASE_API: &str =
    "https://api.github.com/repos/ConcertoNotes/variable-switching/releases/latest";
const ENDPOINT_TEST_DEFAULT_TIMEOUT_SECS: u64 = 8;
const ENDPOINT_TEST_MIN_TIMEOUT_SECS: u64 = 2;
const ENDPOINT_TEST_MAX_TIMEOUT_SECS: u64 = 30;
static QQ_QR_ACTIVE: AtomicBool = AtomicBool::new(false);
static WECHAT_QR_ACTIVE: AtomicBool = AtomicBool::new(false);
static LARK_REGISTRATION_ACTIVE: AtomicBool = AtomicBool::new(false);
static MOBILE_REMOTE_START_ACTIVE: AtomicBool = AtomicBool::new(false);
static MOBILE_REMOTE_CANCEL_REQUESTED: AtomicBool = AtomicBool::new(false);
static LARK_BRIDGE_ACTIVE: AtomicBool = AtomicBool::new(false);
static LARK_BRIDGE_CHILD: Mutex<Option<std::process::Child>> = Mutex::new(None);
static QQ_GATEWAY_ACTIVE: AtomicBool = AtomicBool::new(false);
static QQ_GATEWAY_CHILD: Mutex<Option<std::process::Child>> = Mutex::new(None);
static WECHAT_LISTENER_ACTIVE: AtomicBool = AtomicBool::new(false);

// ── Data Structures ─────────────────────────────────

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Profile {
    id: String,
    name: String,
    api_key: String,
    base_url: String,
    #[serde(default)]
    model_id: String,
    is_active: bool,
    created_at: String,
}

#[derive(Serialize, Deserialize, Default)]
struct ProfilesData {
    profiles: Vec<Profile>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CodexProfile {
    id: String,
    name: String,
    api_key: String,
    base_url: String,
    #[serde(default = "default_codex_auth_mode")]
    auth_mode: String,
    #[serde(default)]
    model: String,
    #[serde(default)]
    provider_name: String,
    is_active: bool,
    created_at: String,
}

#[derive(Serialize, Deserialize, Default)]
struct CodexProfilesData {
    profiles: Vec<CodexProfile>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
struct CodexSessionBinding {
    channel: String,
    thread_id: String,
    thread_name: String,
    session_file: String,
    updated_at: String,
    cwd: String,
    sync_enabled: bool,
    last_synced_at: String,
    note: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
struct CodexThreadRecord {
    id: String,
    thread_name: String,
    updated_at: String,
    session_file: String,
    cwd: String,
    last_user_message: String,
    last_assistant_message: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
struct CodexSessionSyncState {
    last_synced_at: String,
    total: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct PluginMarketplaceItem {
    name: String,
    source: String,
    source_type: String,
    is_official: bool,
    is_current: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct RemoteDeviceState {
    enabled: bool,
    listen_addr: String,
    port: u16,
    discovery_token: String,
    last_error: String,
    last_started_at: String,
    last_seen_at: String,
    device_name: String,
    local_ip: String,
    #[serde(default)]
    mode: String,
    #[serde(default)]
    active_thread_id: String,
    #[serde(default)]
    active_thread_name: String,
}

impl Default for RemoteDeviceState {
    fn default() -> Self {
        Self {
            enabled: false,
            listen_addr: "0.0.0.0".into(),
            port: 38527,
            discovery_token: String::new(),
            last_error: String::new(),
            last_started_at: String::new(),
            last_seen_at: String::new(),
            device_name: String::new(),
            local_ip: String::new(),
            mode: "platform_bot".into(),
            active_thread_id: String::new(),
            active_thread_name: String::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
struct MobileChannelBinding {
    channel: String,
    thread_id: String,
    thread_name: String,
    session_file: String,
    app_id: String,
    app_secret: String,
    bot_token: String,
    account_id: String,
    base_url: String,
    user_id: String,
    bot_open_id: String,
    gateway_url: String,
    qr_url: String,
    qr_data_url: String,
    qr_status: String,
    qr_device_code: String,
    qr_started_at: String,
    launcher_url: String,
    enabled: bool,
    listening: bool,
    status: String,
    last_error: String,
    credential_status: String,
    last_checked_at: String,
    updated_at: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
struct ToolboxState {
    plugin_marketplace_input: String,
    mobile_remote: RemoteDeviceState,
    session_bindings: Vec<CodexSessionBinding>,
    synced_codex_threads: Vec<CodexThreadRecord>,
    session_sync: CodexSessionSyncState,
    mobile_channels: Vec<MobileChannelBinding>,
    selected_mobile_thread_id: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ToolboxSnapshot {
    plugin_marketplace_input: String,
    plugin_marketplaces: Vec<PluginMarketplaceItem>,
    session_bindings: Vec<CodexSessionBinding>,
    codex_threads: Vec<CodexThreadRecord>,
    synced_codex_threads: Vec<CodexThreadRecord>,
    session_sync: CodexSessionSyncState,
    mobile_channels: Vec<MobileChannelBinding>,
    selected_mobile_thread_id: String,
    mobile_remote: RemoteDeviceState,
    codex_home: String,
    codex_config_path: String,
}

fn default_codex_auth_mode() -> String {
    "auth_json".to_string()
}

fn is_codex_official_account_api_quota(auth_mode: &str) -> bool {
    auth_mode == "official_account_api_quota"
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SwitchResult {
    success: bool,
    results: SwitchDetails,
    errors: Vec<String>,
    profile_name: String,
    cancelled: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SwitchDetails {
    env_vars: bool,
    /// 动态编辑器结果: key = 编辑器 id (如 "vscode", "cursor"), value = 是否成功
    editors: HashMap<String, bool>,
    claude: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LocationStatus {
    api_key: String,
    base_url: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StatusResult {
    env_vars: Option<LocationStatus>,
    /// 动态编辑器状态: key = 编辑器 id, value = 状态
    editors: HashMap<String, LocationStatus>,
    claude: Option<LocationStatus>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ConfigSnapshot {
    env_auth_token: Option<String>,
    env_auth_key: Option<String>,
    env_api_key: Option<String>,
    env_base_url: Option<String>,
    /// 动态编辑器快照: key = 编辑器 id, value = 文件内容
    editor_contents: HashMap<String, String>,
    claude_content: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct SkillInfo {
    name: String,
    content: String,
    /// "command" = ~/.claude/commands/, "skill" = ~/.claude/skills/
    source_type: String,
    /// 从 SKILL.md frontmatter 中解析的描述
    description: String,
}

// ── 应用设置数据结构 ──

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase", default)]
struct AppSettings {
    /// 语言: "zh" | "en"
    language: String,
    /// 主题: "light" | "dark"
    theme: String,
    /// 开机自启
    auto_start: bool,
    /// 静默启动（启动时最小化到托盘）
    silent_startup: bool,
    /// 关闭窗口时最小化到托盘
    minimize_to_tray: bool,
    never_show_usage_guide: bool,
    editor_paths: HashMap<String, String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            language: "zh".into(),
            theme: "light".into(),
            auto_start: false,
            silent_startup: false,
            minimize_to_tray: true,
            never_show_usage_guide: false,
            editor_paths: HashMap::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
}

#[derive(Deserialize, Clone, Debug)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    published_at: String,
    #[serde(default)]
    assets: Vec<ReleaseAsset>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateCheckResult {
    current_version: String,
    latest_version: String,
    has_update: bool,
    release_url: String,
    release_notes: String,
    published_at: String,
    asset_name: Option<String>,
    can_auto_update: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateDownloadResult {
    latest_version: String,
    file_name: String,
    file_path: String,
    release_url: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct EndpointLatency {
    url: String,
    latency: Option<u128>,
    status: Option<u16>,
    error: Option<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct EditorPathInfo {
    id: String,
    display_name: String,
    settings_path: String,
    default_path: String,
    customized: bool,
    detected: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppPaths {
    config_dir: String,
    profiles_path: String,
    claude_settings: String,
    /// 动态编辑器路径: key = 编辑器 id, value = settings.json 路径
    editor_settings: Vec<EditorPathInfo>,
    claude_md: String,
    claude_mcp: String,
}

#[derive(Serialize, Clone)]
struct ProgressEvent {
    step: u32,
    total: u32,
    label: String,
}

struct AppState {
    cancel_flag: AtomicBool,
}

// ── Helpers ─────────────────────────────────────────

fn data_dir(app: &tauri::AppHandle) -> PathBuf {
    // app_data_dir 理论上不会失败，但若系统异常返回错误，
    // 退回到 用户主目录/.varswitch，避免直接 panic 崩溃整个应用。
    let dir = app.path().app_data_dir().unwrap_or_else(|_| {
        let fallback = home_dir().join(".varswitch");
        eprintln!("[data_dir] app_data_dir 不可用，退回到 {fallback:?}");
        fallback
    });
    fs::create_dir_all(&dir).ok();
    dir
}

// ── 日志模块 ─────────────────────────────────────────
// 统一日志：同时输出到控制台和文件 app_data_dir/logs/varswitch.log。
// 打包后 stdout/stderr 用户看不到，日志文件方便用户截图反馈或自行排查。

/// 全局日志文件路径，在 app 启动时 init_logging 设置一次。
static LOG_FILE_PATH: OnceLock<PathBuf> = OnceLock::new();
/// 日志文件滚动阈值：超过 5MB 转存为 .old。
const LOG_MAX_BYTES: u64 = 5 * 1024 * 1024;

/// 在应用启动时初始化日志文件路径（setup 阶段调用一次）。
fn init_logging(app: &tauri::AppHandle) {
    let dir = data_dir(app).join("logs");
    if fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join("varswitch.log");
    let _ = LOG_FILE_PATH.set(path);
    app_log("INFO", "VarSwitch 启动，日志系统已就绪");
}

/// 把 Unix 毫秒时间戳格式化为本地时间字符串（UTC+8，面向中国用户）。
fn format_log_time(millis: u128) -> String {
    let total_secs = (millis / 1000) as i64 + 8 * 3600; // 东八区偏移
    let ms = (millis % 1000) as u64;
    let days = total_secs.div_euclid(86400);
    let secs_of_day = total_secs.rem_euclid(86400);
    let hour = secs_of_day / 3600;
    let minute = (secs_of_day % 3600) / 60;
    let second = secs_of_day % 60;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}.{ms:03}"
    )
}

/// 把"从 1970-01-01 起的天数"转成 (年, 月, 日)。
/// 采用 Howard Hinnant 的 civil_from_days 算法，正确处理闰年。
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as i64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// 把 Unix 毫秒时间戳格式化为紧凑的本地时间字符串（用于备份文件名，无非法字符）。
/// 例如 "20260624-143025"（UTC+8）。
fn format_compact_time(millis: u128) -> String {
    let total_secs = (millis / 1000) as i64 + 8 * 3600;
    let secs_of_day = total_secs.rem_euclid(86400);
    let hour = secs_of_day / 3600;
    let minute = (secs_of_day % 3600) / 60;
    let second = secs_of_day % 60;
    let (year, month, day) = civil_from_days(total_secs.div_euclid(86400));
    format!("{year:04}{month:02}{day:02}-{hour:02}{minute:02}{second:02}")
}

/// 写一条日志：同时输出到控制台（开发期可见）和日志文件（打包后可查）。
fn app_log(level: &str, msg: &str) {
    let line = format!("[{}] [{level}] {msg}", format_log_time(chrono_timestamp_millis()));
    // 控制台输出（dev 模式可见）
    eprintln!("{line}");
    // 文件输出
    if let Some(path) = LOG_FILE_PATH.get() {
        // 简单滚动：超过阈值就把当前日志转存为 .old
        if let Ok(meta) = fs::metadata(path) {
            if meta.len() > LOG_MAX_BYTES {
                let old = path.with_file_name("varswitch.log.old");
                let _ = fs::rename(path, &old);
            }
        }
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(file, "{line}");
        }
    }
}

/// 便捷日志宏，用法同 println!：log_info!("xxx {}", v)。
macro_rules! log_info {
    ($($arg:tt)*) => { app_log("INFO", &format!($($arg)*)) };
}
macro_rules! log_warn {
    ($($arg:tt)*) => { app_log("WARN", &format!($($arg)*)) };
}
macro_rules! log_error {
    ($($arg:tt)*) => { app_log("ERROR", &format!($($arg)*)) };
}


fn profiles_path(app: &tauri::AppHandle) -> PathBuf {
    data_dir(app).join("profiles.json")
}

fn read_profiles(app: &tauri::AppHandle) -> ProfilesData {
    let path = profiles_path(app);
    if !path.exists() {
        return ProfilesData::default();
    }
    let mut data: ProfilesData = fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    // 修复空 id/createdAt 的历史数据
    let mut fixed = false;
    for p in data.profiles.iter_mut() {
        if p.id.is_empty() {
            p.id = uuid::Uuid::new_v4().to_string();
            fixed = true;
        }
        if p.created_at.is_empty() {
            p.created_at = chrono_now();
            fixed = true;
        }
    }
    if fixed {
        let _ = write_profiles_to_path(&path, &data);
    }
    data
}

fn write_profiles_to_path(path: &PathBuf, data: &ProfilesData) -> Result<(), String> {
    let json = serde_json::to_string_pretty(data).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())
}

fn write_profiles(app: &tauri::AppHandle, data: &ProfilesData) -> Result<(), String> {
    let path = profiles_path(app);
    write_profiles_to_path(&path, data)
}

fn codex_profiles_path(app: &tauri::AppHandle) -> PathBuf {
    data_dir(app).join("codex_profiles.json")
}

fn read_codex_profiles(app: &tauri::AppHandle) -> CodexProfilesData {
    let path = codex_profiles_path(app);
    if !path.exists() {
        return CodexProfilesData::default();
    }
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn write_codex_profiles(app: &tauri::AppHandle, data: &CodexProfilesData) -> Result<(), String> {
    let path = codex_profiles_path(app);
    let json = serde_json::to_string_pretty(data).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())
}

fn home_dir() -> PathBuf {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_default();
    PathBuf::from(home)
}

fn claude_settings_path() -> PathBuf {
    home_dir().join(".claude").join("settings.json")
}

fn codex_config_dir() -> PathBuf {
    home_dir().join(".codex")
}

fn codex_auth_path() -> PathBuf {
    codex_config_dir().join("auth.json")
}

fn codex_config_path() -> PathBuf {
    codex_config_dir().join("config.toml")
}

fn codex_sessions_root() -> PathBuf {
    codex_config_dir().join("sessions")
}

fn codex_session_index_path() -> PathBuf {
    codex_config_dir().join("session_index.jsonl")
}

fn toolbox_state_path(app: &tauri::AppHandle) -> PathBuf {
    data_dir(app).join("codex_toolbox_state.json")
}

/// 写入 Codex 配置文件。
/// 默认写入 ~/.codex/auth.json 和 ~/.codex/config.toml；
/// 官方账号登录/API 额度模式只写 ~/.codex/config.toml，不改动 auth.json。
fn write_codex_config(profile: &CodexProfile) -> Result<(), String> {
    write_codex_config_with_base_url(profile, &profile.base_url)
}

fn write_codex_config_with_base_url(profile: &CodexProfile, base_url: &str) -> Result<(), String> {
    let dir = codex_config_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("创建 ~/.codex 目录失败: {}", e))?;
    let existing_config = fs::read_to_string(codex_config_path()).unwrap_or_default();
    let marketplace_sections = extract_marketplace_sections(&existing_config);

    // 写入 config.toml
    let official_account_mode = is_codex_official_account_api_quota(&profile.auth_mode);
    let provider = if official_account_mode {
        "customer".to_string()
    } else if profile.provider_name.is_empty() {
        "custom".to_string()
    } else {
        profile.provider_name.clone()
    };
    let model = if official_account_mode {
        "gpt-5.5".to_string()
    } else if profile.model.is_empty() {
        "default".to_string()
    } else {
        profile.model.clone()
    };
    let toml_content = if official_account_mode {
        format!(
            r#"model_provider = "customer"
model = "gpt-5.5"
review_model = "gpt-5.5"
model_reasoning_effort = "xhigh"
disable_response_storage = true
preferred_auth_method = "apikey"

[model_providers.customer]
name = "customer"
wire_api = "responses"
requires_openai_auth = true
base_url = "{base_url}"
experimental_bearer_token = "{api_key}"
"#,
            base_url = base_url,
            api_key = profile.api_key,
        )
    } else {
        let auth_path = codex_auth_path();
        let mut auth = if auth_path.exists() {
            fs::read_to_string(&auth_path)
                .ok()
                .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                .filter(|value| value.is_object())
                .unwrap_or_else(|| serde_json::json!({}))
        } else {
            serde_json::json!({
                "OPENAI_API_KEY": profile.api_key
            })
        };
        if let Some(obj) = auth.as_object_mut() {
            obj.insert(
                "OPENAI_API_KEY".to_string(),
                serde_json::Value::String(profile.api_key.clone()),
            );
        }
        let auth_str = serde_json::to_string_pretty(&auth).map_err(|e| e.to_string())?;
        fs::write(&auth_path, auth_str).map_err(|e| format!("写入 codex auth.json 失败: {}", e))?;

        format!(
            r#"model_provider = "{provider}"
model = "{model}"

[model_providers.{provider}]
name = "{provider}"
base_url = "{base_url}"
wire_api = "responses"
requires_openai_auth = true
"#,
            provider = provider,
            model = model,
            base_url = base_url,
        )
    };
    let final_toml = if marketplace_sections.trim().is_empty() {
        toml_content
    } else {
        format!("{}\n{}", toml_content.trim_end(), marketplace_sections)
    };
    fs::write(codex_config_path(), final_toml)
        .map_err(|e| format!("写入 codex config.toml 失败: {}", e))?;

    Ok(())
}

/// 读取当前 Codex 配置状态
fn read_codex_status() -> Option<LocationStatus> {
    let config_str = fs::read_to_string(codex_config_path()).unwrap_or_default();
    let auth_api_key = fs::read_to_string(codex_auth_path())
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|auth| {
            auth.get("OPENAI_API_KEY")
                .and_then(|v| v.as_str())
                .map(|value| value.to_string())
        })
        .unwrap_or_default();
    let bearer_token = config_str
        .lines()
        .find(|l| l.trim().starts_with("experimental_bearer_token"))
        .and_then(|l| l.split('=').nth(1))
        .map(|v| v.trim().trim_matches('"').to_string())
        .unwrap_or_default();
    let api_key = if auth_api_key.is_empty() {
        bearer_token
    } else {
        auth_api_key
    };

    let base_url = config_str
        .lines()
        .find(|l| l.trim().starts_with("base_url"))
        .and_then(|l| l.split('=').nth(1))
        .map(|v| v.trim().trim_matches('"').to_string())
        .unwrap_or_default();

    Some(LocationStatus { api_key, base_url })
}

fn read_toolbox_state(app: &tauri::AppHandle) -> ToolboxState {
    let path = toolbox_state_path(app);
    if !path.exists() {
        let mut state = ToolboxState::default();
        state.plugin_marketplace_input = default_plugin_marketplace_url().to_string();
        state.mobile_remote.discovery_token = uuid::Uuid::new_v4().to_string();
        state.mobile_remote.device_name = local_device_name();
        state.mobile_remote.local_ip = detect_local_ip();
        return normalize_toolbox_state(state);
    }
    let mut state: ToolboxState = fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    if state.plugin_marketplace_input.trim().is_empty() {
        state.plugin_marketplace_input = default_plugin_marketplace_url().to_string();
    }
    if state.mobile_remote.discovery_token.trim().is_empty() {
        state.mobile_remote.discovery_token = uuid::Uuid::new_v4().to_string();
    }
    if state.mobile_remote.device_name.trim().is_empty() {
        state.mobile_remote.device_name = local_device_name();
    }
    state.mobile_remote.local_ip = detect_local_ip();
    normalize_toolbox_state(state)
}

fn normalize_toolbox_state(mut state: ToolboxState) -> ToolboxState {
    state.plugin_marketplace_input = default_plugin_marketplace_url().to_string();
    if state.mobile_remote.discovery_token.trim().is_empty() {
        state.mobile_remote.discovery_token = uuid::Uuid::new_v4().to_string();
    }
    if state.mobile_remote.device_name.trim().is_empty() {
        state.mobile_remote.device_name = local_device_name();
    }
    if state.mobile_remote.mode.trim().is_empty() {
        state.mobile_remote.mode = "platform_bot".into();
    }
    if state.mobile_remote.listen_addr.trim().is_empty() {
        state.mobile_remote.listen_addr = "platform".into();
    }
    state.mobile_remote.local_ip = detect_local_ip();

    for channel in ["lark", "wechat", "qq"] {
        if !state
            .mobile_channels
            .iter()
            .any(|binding| binding.channel == channel)
        {
            state.mobile_channels.push(default_mobile_channel(channel));
        }
    }
    for binding in state.mobile_channels.iter_mut() {
        binding.channel = normalize_mobile_channel(&binding.channel);
        if binding.status.trim().is_empty() {
            binding.status = if binding.thread_id.trim().is_empty() {
                "未绑定".into()
            } else {
                "已绑定，等待开启平台连接".into()
            };
        }
        if binding.base_url.trim().is_empty() {
            binding.base_url = default_mobile_channel_base_url(&binding.channel).to_string();
        }
        normalize_mobile_channel_qr_cache(binding);
    }

    if state.selected_mobile_thread_id.trim().is_empty() {
        if let Some(first) = state.synced_codex_threads.first() {
            state.selected_mobile_thread_id = first.id.clone();
        }
    }

    let active = state
        .synced_codex_threads
        .iter()
        .find(|thread| thread.id == state.selected_mobile_thread_id)
        .cloned();
    if let Some(thread) = active {
        state.mobile_remote.active_thread_id = thread.id;
        state.mobile_remote.active_thread_name = thread.thread_name;
    }

    state
}

fn normalize_mobile_channel(channel: &str) -> String {
    match channel.trim().to_ascii_lowercase().as_str() {
        "feishu" | "lark" => "lark".into(),
        "wechat" | "weixin" | "wx" => "wechat".into(),
        "qq" => "qq".into(),
        other => other.to_string(),
    }
}

const MOBILE_QR_CACHE_TTL_MS: u128 = 10 * 60 * 1000;

fn mobile_qr_cache_is_fresh(started_at: &str) -> bool {
    let Ok(started_at) = started_at.trim().parse::<u128>() else {
        return false;
    };
    chrono_timestamp_millis().saturating_sub(started_at) <= MOBILE_QR_CACHE_TTL_MS
}

fn is_qq_authorization_target(value: &str) -> bool {
    let lower = value.trim().to_ascii_lowercase();
    lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("mqqapi://")
        || lower.starts_with("qqbot://")
}

fn clear_stale_mobile_qr(binding: &mut MobileChannelBinding, message: &str) {
    binding.qr_url.clear();
    binding.qr_data_url.clear();
    binding.qr_device_code.clear();
    binding.qr_status = message.into();
}

fn normalize_mobile_channel_qr_cache(binding: &mut MobileChannelBinding) {
    let has_qr = !binding.qr_url.trim().is_empty() || !binding.qr_data_url.trim().is_empty();
    if !has_qr {
        return;
    }
    match binding.channel.as_str() {
        "qq" => {
            if !is_qq_authorization_target(&binding.qr_url)
                || !mobile_qr_cache_is_fresh(&binding.qr_started_at)
            {
                clear_stale_mobile_qr(binding, "QQ 二维码已失效，请重新绑定");
            }
        }
        "wechat" => {
            let qr_data = binding.qr_data_url.trim().to_ascii_lowercase();
            if binding.qr_device_code.trim().is_empty()
                || !mobile_qr_cache_is_fresh(&binding.qr_started_at)
                || qr_data.starts_with("data:image/svg+xml")
            {
                clear_stale_mobile_qr(binding, "微信二维码已失效，请重新生成");
            }
        }
        _ => {}
    }
}

fn default_mobile_channel_base_url(channel: &str) -> &'static str {
    match normalize_mobile_channel(channel).as_str() {
        "lark" => "https://open.feishu.cn",
        "wechat" => "https://ilinkai.weixin.qq.com",
        _ => "",
    }
}

fn default_mobile_channel(channel: &str) -> MobileChannelBinding {
    let normalized = normalize_mobile_channel(channel);
    MobileChannelBinding {
        channel: normalized.clone(),
        status: "未绑定".into(),
        base_url: default_mobile_channel_base_url(&normalized).to_string(),
        ..MobileChannelBinding::default()
    }
}

fn write_toolbox_state(app: &tauri::AppHandle, state: &ToolboxState) -> Result<(), String> {
    let path = toolbox_state_path(app);
    let json = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())
}

fn default_plugin_marketplace_url() -> &'static str {
    "https://gitcode.com/2301_79703673/codex-plugins.git"
}

fn varswitch_github_plugin_marketplace_url() -> &'static str {
    "https://github.com/ConcertoNotes/codex-plugins.git"
}

fn upstream_gitcode_plugin_marketplace_url() -> &'static str {
    "https://gitcode.com/weixin_65003717/codex-plugin.git"
}

fn awesome_plugin_marketplace_url() -> &'static str {
    "https://github.com/hashgraph-online/awesome-codex-plugins.git"
}

fn default_plugin_marketplace_name() -> &'static str {
    "VarSwitch-Plugin"
}

fn supported_plugin_marketplace_source(source: &str) -> &'static str {
    let trimmed = source.trim();
    if trimmed == varswitch_github_plugin_marketplace_url()
        || trimmed == "git@github.com:ConcertoNotes/codex-plugins.git"
    {
        varswitch_github_plugin_marketplace_url()
    } else if trimmed == upstream_gitcode_plugin_marketplace_url() {
        upstream_gitcode_plugin_marketplace_url()
    } else if trimmed == awesome_plugin_marketplace_url()
        || trimmed == "git@github.com:hashgraph-online/awesome-codex-plugins.git"
    {
        awesome_plugin_marketplace_url()
    } else {
        default_plugin_marketplace_url()
    }
}

fn lark_create_bot_launcher_url() -> &'static str {
    "https://open.feishu.cn/page/launcher?from=sdk&source=node-sdk%2Fvarswitch&tp=sdk&addons=H4sIAAAAAAAC_2XLMQqAMAwF0LtkloJrriJSav1Ih6RiQpfSuyvi5v5eJ8v1hBF3cmhSJ16oCAvM0gE26B6Txa06rWMiNKi_vDjk98L3woWM0hDb_LQxbv9jmpZoAAAA&createOnly=true&name=VarSwitch+%E6%99%BA%E8%83%BD%E4%BD%93&desc=%E6%8A%8A%E9%A3%9E%E4%B9%A6%E6%B6%88%E6%81%AF%E8%BD%AC%E5%8F%91%E7%BB%99+Codex%EF%BC%8C%E5%B9%B6%E6%8A%8A+Codex+%E5%9B%9E%E5%A4%8D%E5%90%8C%E6%AD%A5%E5%9B%9E%E9%A3%9E%E4%B9%A6%E3%80%82"
}

fn lark_registration_base_url() -> &'static str {
    "https://accounts.feishu.cn"
}

fn lark_registration_endpoint() -> String {
    format!("{}/oauth/v1/app/registration", lark_registration_base_url())
}

fn lark_registration_addons() -> &'static str {
    "H4sIAAAAAAAC_2XLMQqAMAwF0LtkloJrriJSav1Ih6RiQpfSuyvi5v5eJ8v1hBF3cmhSJ16oCAvM0gE26B6Txa06rWMiNKi_vDjk98L3woWM0hDb_LQxbv9jmpZoAAAA"
}

fn percent_encode_query_value(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.as_bytes() {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(*byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn append_query_params(url: &str, params: &[(&str, &str)]) -> String {
    let mut next = url.trim().to_string();
    if next.is_empty() {
        return String::new();
    }
    let mut first = !next.contains('?');
    for (key, value) in params {
        if key.trim().is_empty() {
            continue;
        }
        if first {
            next.push('?');
            first = false;
        } else if !next.ends_with('?') && !next.ends_with('&') {
            next.push('&');
        }
        next.push_str(&percent_encode_query_value(key));
        next.push('=');
        next.push_str(&percent_encode_query_value(value));
    }
    next
}

fn base64_encode_bytes(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(((bytes.len() + 2) / 3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = *chunk.get(1).unwrap_or(&0);
        let b2 = *chunk.get(2).unwrap_or(&0);
        out.push(TABLE[(b0 >> 2) as usize] as char);
        out.push(TABLE[(((b0 & 0b0000_0011) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[(((b1 & 0b0000_1111) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(b2 & 0b0011_1111) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// 将文本(通常是 URL)生成二维码图片的 PNG data URL。用于微信/QQ 等需本地生成二维码的场景。
fn generate_qr_code_data_url(content: &str) -> Result<String, String> {
    let code = QrCode::with_error_correction_level(content, EcLevel::M)
        .map_err(|e| format!("生成二维码失败: {e}"))?;
    let image = code.render::<Luma<u8>>()
        .min_dimensions(240, 240)
        .build();
    let dyn_img = DynamicImage::ImageLuma8(image);
    let mut png_bytes = Vec::new();
    dyn_img.write_to(&mut std::io::Cursor::new(&mut png_bytes), image::ImageFormat::Png)
        .map_err(|e| format!("PNG 编码失败: {e}"))?;
    let b64 = base64_encode_bytes(&png_bytes);
    Ok(format!("data:image/png;base64,{b64}"))
}

fn official_plugin_marketplace_names() -> &'static [&'static str] {
    &["openai-bundled", "openai-primary-runtime"]
}

fn local_device_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "VarSwitch".to_string())
}

fn detect_local_ip() -> String {
    if let Ok(socket) = UdpSocket::bind("0.0.0.0:0") {
        let _ = socket.connect("8.8.8.8:80");
        if let Ok(addr) = socket.local_addr() {
            return addr.ip().to_string();
        }
    }
    "127.0.0.1".to_string()
}

fn read_lines_if_exists(path: &PathBuf) -> Vec<String> {
    fs::read_to_string(path)
        .ok()
        .map(|text| {
            text.lines()
                .map(|line| line.trim().to_string())
                .filter(|line| !line.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn codex_session_relative_file(thread_id: &str) -> Option<String> {
    let root = codex_sessions_root();
    if !root.exists() {
        return None;
    }
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
                    continue;
                };
                if !name.ends_with(".jsonl") {
                    continue;
                }
                if name.contains(thread_id) {
                    if let Ok(relative) = path.strip_prefix(codex_config_dir()) {
                        return Some(relative.to_string_lossy().replace('\\', "/"));
                    }
                }
            }
        }
    }
    None
}

fn codex_session_path_from_relative(relative: &str) -> PathBuf {
    let text = relative.replace('/', "\\");
    codex_config_dir().join(text)
}

fn extract_text_from_content_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Array(items) => items
            .iter()
            .map(extract_text_from_content_value)
            .filter(|text| !text.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        serde_json::Value::Object(map) => {
            if let Some(text) = map.get("text").and_then(|value| value.as_str()) {
                return text.to_string();
            }
            if let Some(text) = map.get("content").map(extract_text_from_content_value) {
                if !text.trim().is_empty() {
                    return text;
                }
            }
            if let Some(text) = map.get("message").map(extract_text_from_content_value) {
                if !text.trim().is_empty() {
                    return text;
                }
            }
            String::new()
        }
        _ => String::new(),
    }
}

fn shorten_preview(text: &str, max_chars: usize) -> String {
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = compact.chars();
    let preview: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{preview}...")
    } else {
        preview
    }
}

fn read_codex_thread_preview(session_file: &str) -> (String, String, String) {
    let path = codex_session_path_from_relative(session_file);
    let text = fs::read_to_string(path).unwrap_or_default();
    let mut cwd = String::new();
    let mut last_user_message = String::new();
    let mut last_assistant_message = String::new();

    for line in text.lines() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if cwd.is_empty() {
            cwd = value
                .pointer("/payload/cwd")
                .and_then(|value| value.as_str())
                .or_else(|| {
                    value
                        .pointer("/payload/origin/cwd")
                        .and_then(|value| value.as_str())
                })
                .unwrap_or("")
                .to_string();
        }
        let role = value
            .pointer("/payload/role")
            .and_then(|value| value.as_str())
            .or_else(|| {
                value
                    .pointer("/payload/message/role")
                    .and_then(|value| value.as_str())
            })
            .unwrap_or("");

        let text = value
            .pointer("/payload/content")
            .map(extract_text_from_content_value)
            .or_else(|| {
                value
                    .pointer("/payload/message/content")
                    .map(extract_text_from_content_value)
            })
            .or_else(|| {
                value
                    .pointer("/payload/text")
                    .map(extract_text_from_content_value)
            })
            .unwrap_or_default();

        if text.trim().is_empty() {
            continue;
        }

        match role {
            "user" => last_user_message = shorten_preview(&text, 140),
            "assistant" => last_assistant_message = shorten_preview(&text, 180),
            _ => {}
        }
    }

    (cwd, last_user_message, last_assistant_message)
}

fn read_codex_threads(limit: usize) -> Vec<CodexThreadRecord> {
    let mut items = Vec::new();
    for line in read_lines_if_exists(&codex_session_index_path())
        .into_iter()
        .rev()
    {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        let id = value
            .get("id")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if id.is_empty() || items.iter().any(|item: &CodexThreadRecord| item.id == id) {
            continue;
        }
        let thread_name = value
            .get("thread_name")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let updated_at = value
            .get("updated_at")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let Some(session_file) = codex_session_relative_file(&id) else {
            continue;
        };
        let (cwd, last_user_message, last_assistant_message) =
            read_codex_thread_preview(&session_file);
        items.push(CodexThreadRecord {
            id,
            thread_name,
            updated_at,
            session_file,
            cwd,
            last_user_message,
            last_assistant_message,
        });
        if items.len() >= limit {
            break;
        }
    }
    items
}

fn parse_toml_string_value(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.len() >= 2 {
        let bytes = trimmed.as_bytes();
        if (bytes[0] == b'"' && bytes[trimmed.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[trimmed.len() - 1] == b'\'')
        {
            return trimmed[1..trimmed.len() - 1].to_string();
        }
    }
    trimmed.to_string()
}

fn list_plugin_marketplaces(config_text: &str, current_source: &str) -> Vec<PluginMarketplaceItem> {
    let mut result = Vec::new();
    let lines: Vec<&str> = config_text.lines().collect();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index].trim();
        if !(line.starts_with("[marketplaces.") && line.ends_with(']')) {
            index += 1;
            continue;
        }
        let name = line
            .trim_start_matches("[marketplaces.")
            .trim_end_matches(']')
            .trim_matches('"')
            .trim_matches('\'')
            .to_string();
        let mut source = String::new();
        let mut source_type = String::new();
        index += 1;
        while index < lines.len() {
            let current = lines[index].trim();
            if current.starts_with('[') && current.ends_with(']') {
                break;
            }
            if let Some(value) = current.strip_prefix("source =") {
                source = parse_toml_string_value(value);
            } else if let Some(value) = current.strip_prefix("source_type =") {
                source_type = parse_toml_string_value(value);
            }
            index += 1;
        }
        result.push(PluginMarketplaceItem {
            name: name.clone(),
            source: source.clone(),
            source_type: source_type.clone(),
            is_official: official_plugin_marketplace_names().contains(&name.as_str()),
            is_current: !current_source.trim().is_empty() && current_source.trim() == source.trim(),
        });
    }
    result
}

fn extract_marketplace_sections(config_text: &str) -> String {
    let lines: Vec<&str> = config_text.lines().collect();
    let mut index = 0;
    let mut sections = Vec::new();

    while index < lines.len() {
        let line = lines[index].trim();
        if !(line.starts_with("[marketplaces.") && line.ends_with(']')) {
            index += 1;
            continue;
        }
        let mut block = vec![lines[index].to_string()];
        index += 1;
        while index < lines.len() {
            let current = lines[index].trim();
            if current.starts_with('[') && current.ends_with(']') {
                break;
            }
            block.push(lines[index].to_string());
            index += 1;
        }
        sections.push(block.join("\n").trim().to_string());
    }

    if sections.is_empty() {
        String::new()
    } else {
        format!("{}\n", sections.join("\n\n"))
    }
}

fn ensure_plugin_marketplace_section(
    existing: &str,
    name: &str,
    source: &str,
    source_type: &str,
) -> String {
    let header = format!("[marketplaces.{name}]");
    let section = format!(
        "{header}\nlast_updated = \"{}\"\nsource_type = \"{}\"\nsource = \"{}\"\n",
        chrono_now(),
        source_type,
        source.replace('\\', "\\\\").replace('"', "\\\"")
    );

    let lines: Vec<&str> = existing.lines().collect();
    let mut output = Vec::new();
    let mut index = 0;
    let mut replaced = false;

    while index < lines.len() {
        let line = lines[index].trim();
        if line == header {
            replaced = true;
            output.push(section.trim_end().to_string());
            index += 1;
            while index < lines.len() {
                let current = lines[index].trim();
                if current.starts_with('[') && current.ends_with(']') {
                    break;
                }
                index += 1;
            }
            continue;
        }
        output.push(lines[index].to_string());
        index += 1;
    }

    let mut text = output.join("\n").trim().to_string();
    if text.is_empty() {
        return section;
    }
    if !replaced {
        text.push_str("\n\n");
        text.push_str(&section);
        return text;
    }
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

fn remove_all_plugin_marketplace_sections(existing: &str) -> String {
    let lines: Vec<&str> = existing.lines().collect();
    let mut output = Vec::new();
    let mut index = 0;
    let mut skipping = false;

    while index < lines.len() {
        let trimmed = lines[index].trim();
        if trimmed.starts_with("[marketplaces.") && trimmed.ends_with(']') {
            skipping = true;
            index += 1;
            continue;
        }
        if skipping && trimmed.starts_with('[') && trimmed.ends_with(']') {
            skipping = false;
        }
        if !skipping {
            output.push(lines[index].to_string());
        }
        index += 1;
    }

    let text = output.join("\n");
    let mut compact = Vec::new();
    let mut last_blank = false;
    for line in text.lines() {
        let blank = line.trim().is_empty();
        if blank && last_blank {
            continue;
        }
        compact.push(line.to_string());
        last_blank = blank;
    }
    compact.join("\n").trim().to_string()
}

fn plugin_marketplace_source_type(source: &str) -> &'static str {
    let lowered = source.to_ascii_lowercase();
    if lowered.starts_with("http://")
        || lowered.starts_with("https://")
        || lowered.starts_with("ssh://")
        || lowered.starts_with("git@")
    {
        "git"
    } else {
        "local"
    }
}

fn run_codex_plugin_marketplace_add(source: &str) -> Result<(), String> {
    let executable = resolve_codex_command()?;
    let mut command = codex_command(&executable);
    command
        .args(["plugin", "marketplace", "add", source, "--json"])
        .env("CODEX_HOME", codex_config_dir())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = command
        .output()
        .map_err(|e| format!("启动 Codex CLI 安装插件市场失败({}): {e}", executable.display()))?;
    if output.status.success() {
        return Ok(());
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let detail = [stdout, stderr]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    Err(format!(
        "Codex CLI 安装插件市场失败(exit {})：{}",
        output.status.code().unwrap_or(-1),
        if detail.is_empty() {
            "无错误输出".to_string()
        } else {
            detail
        }
    ))
}

fn config_has_plugin_marketplace_source(config_text: &str, source: &str) -> bool {
    list_plugin_marketplaces(config_text, source)
        .iter()
        .any(|item| item.source.trim() == source.trim())
}

fn normalize_mobile_base_url(value: &str, channel: &str) -> String {
    let fallback = default_mobile_channel_base_url(channel);
    let mut raw = value.trim().trim_end_matches('/').to_string();
    if raw.is_empty() {
        raw = fallback.to_string();
    }
    if !raw.is_empty() && !raw.starts_with("http://") && !raw.starts_with("https://") {
        raw = format!("https://{raw}");
    }
    raw.trim_end_matches('/').to_string()
}

fn mobile_channel_credential_hint(channel: &str) -> &'static str {
    match normalize_mobile_channel(channel).as_str() {
        "lark" => "请填写飞书 App ID 和 App Secret",
        "wechat" => "请填写微信 iLink Bot Token",
        "qq" => "请填写 QQ Bot AppID 和 AppSecret",
        _ => "请填写平台凭据",
    }
}

fn mobile_channel_has_credentials(binding: &MobileChannelBinding) -> bool {
    match normalize_mobile_channel(&binding.channel).as_str() {
        "lark" | "qq" => !binding.app_id.trim().is_empty() && !binding.app_secret.trim().is_empty(),
        "wechat" => !binding.bot_token.trim().is_empty(),
        _ => false,
    }
}

fn is_platform_code_ok(value: &serde_json::Value) -> bool {
    match value.get("code").or_else(|| value.get("errcode")) {
        None | Some(serde_json::Value::Null) => true,
        Some(serde_json::Value::Number(number)) => number.as_i64().unwrap_or(0) == 0,
        Some(serde_json::Value::String(text)) => text.trim().is_empty() || text.trim() == "0",
        _ => false,
    }
}

fn platform_error_message(value: &serde_json::Value, fallback: &str) -> String {
    value
        .get("msg")
        .or_else(|| value.get("message"))
        .or_else(|| value.get("errmsg"))
        .and_then(|item| item.as_str())
        .filter(|text| !text.trim().is_empty())
        .unwrap_or(fallback)
        .to_string()
}

fn gateway_url_label(url: &str) -> String {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|parsed| {
            parsed
                .host_str()
                .map(|host| format!("{}://{}", parsed.scheme(), host))
        })
        .unwrap_or_else(|| "平台网关".into())
}

fn post_json_value(
    client: &reqwest::blocking::Client,
    url: String,
    payload: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let response = client
        .post(&url)
        .json(&payload)
        .send()
        .map_err(|e| format!("平台接口请求失败: {e}"))?;
    let status = response.status();
    let body = response
        .text()
        .map_err(|e| format!("平台接口响应读取失败: {e}"))?;
    if !status.is_success() {
        return Err(format!("平台接口返回 HTTP {}：{}", status.as_u16(), body));
    }
    if body.trim().is_empty() {
        return Ok(serde_json::json!({}));
    }
    serde_json::from_str(&body).map_err(|e| format!("平台接口返回不是 JSON: {e}; {body}"))
}

fn probe_lark_channel(
    client: &reqwest::blocking::Client,
    binding: &MobileChannelBinding,
) -> Result<(String, String), String> {
    if !mobile_channel_has_credentials(binding) {
        return Err(mobile_channel_credential_hint("lark").into());
    }
    let base_url = normalize_mobile_base_url(&binding.base_url, "lark");
    let result = post_json_value(
        client,
        format!("{base_url}/callback/ws/endpoint"),
        serde_json::json!({
            "AppID": binding.app_id.trim(),
            "AppSecret": binding.app_secret.trim(),
        }),
    )?;
    if !is_platform_code_ok(&result) {
        return Err(format!(
            "飞书 WebSocket 网关获取失败：{}",
            platform_error_message(&result, "接口返回异常")
        ));
    }
    let data = result.get("data").and_then(|item| item.as_object());
    let gateway_url = data
        .and_then(|item| item.get("URL").or_else(|| item.get("url")))
        .and_then(|item| item.as_str())
        .unwrap_or_default()
        .trim()
        .to_string();
    if gateway_url.is_empty() {
        return Err("飞书 WebSocket 网关地址为空".into());
    }
    Ok((
        gateway_url.clone(),
        format!(
            "飞书 WebSocket 网关已验证：{}",
            gateway_url_label(&gateway_url)
        ),
    ))
}

fn probe_qq_channel(
    client: &reqwest::blocking::Client,
    binding: &MobileChannelBinding,
) -> Result<(String, String), String> {
    if !mobile_channel_has_credentials(binding) {
        return Err(mobile_channel_credential_hint("qq").into());
    }
    let token_result = post_json_value(
        client,
        "https://bots.qq.com/app/getAppAccessToken".into(),
        serde_json::json!({
            "appId": binding.app_id.trim(),
            "clientSecret": binding.app_secret.trim(),
        }),
    )?;
    let token = token_result
        .get("access_token")
        .and_then(|item| item.as_str())
        .unwrap_or_default()
        .trim()
        .to_string();
    if token.is_empty() {
        return Err(format!(
            "QQ Bot 鉴权失败：{}",
            platform_error_message(&token_result, "未返回 access_token")
        ));
    }
    let response = client
        .get("https://api.sgroup.qq.com/gateway")
        .header("Authorization", format!("QQBot {token}"))
        .send()
        .map_err(|e| format!("QQ 网关请求失败: {e}"))?;
    let status = response.status();
    let body = response
        .text()
        .map_err(|e| format!("QQ 网关响应读取失败: {e}"))?;
    if !status.is_success() {
        return Err(format!("QQ 网关返回 HTTP {}：{}", status.as_u16(), body));
    }
    let result: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("QQ 网关返回不是 JSON: {e}; {body}"))?;
    let gateway_url = result
        .get("url")
        .and_then(|item| item.as_str())
        .unwrap_or_default()
        .trim()
        .to_string();
    if gateway_url.is_empty() {
        return Err("QQ 网关地址为空".into());
    }
    Ok((
        gateway_url.clone(),
        format!(
            "QQ WebSocket 网关已验证：{}",
            gateway_url_label(&gateway_url)
        ),
    ))
}

fn probe_wechat_channel(
    client: &reqwest::blocking::Client,
    binding: &MobileChannelBinding,
) -> Result<(String, String), String> {
    if !mobile_channel_has_credentials(binding) {
        return Err(mobile_channel_credential_hint("wechat").into());
    }
    let base_url = normalize_mobile_base_url(&binding.base_url, "wechat");
    let response = client
        .post(format!("{base_url}/ilink/bot/msg/notifystart"))
        .header("AuthorizationType", "ilink_bot_token")
        .header(
            "Authorization",
            format!("Bearer {}", binding.bot_token.trim()),
        )
        .header("X-WECHAT-UIN", uuid::Uuid::new_v4().to_string())
        .header("iLink-App-Id", "bot")
        .header("iLink-App-ClientVersion", "132100")
        .json(&serde_json::json!({
            "base_info": {
                "channel_version": "2.4.4",
                "bot_agent": "VarSwitch/1.0"
            }
        }))
        .send()
        .map_err(|e| format!("微信 iLink 请求失败: {e}"))?;
    let status = response.status();
    let body = response
        .text()
        .map_err(|e| format!("微信 iLink 响应读取失败: {e}"))?;
    if !status.is_success() {
        return Err(format!(
            "微信 iLink 返回 HTTP {}：{}",
            status.as_u16(),
            body
        ));
    }
    if !body.trim().is_empty() {
        let result: serde_json::Value = serde_json::from_str(&body)
            .map_err(|e| format!("微信 iLink 返回不是 JSON: {e}; {body}"))?;
        if !is_platform_code_ok(&result) {
            let err_msg = platform_error_message(&result, "接口返回异常");
            // 微信 iLink bot_token 有时效性，session timeout 提示用户重新绑定
            if err_msg.to_lowercase().contains("session timeout") || err_msg.to_lowercase().contains("expired") {
                return Err(format!(
                    "微信 iLink 会话已过期（bot_token 失效），请清除微信绑定后重新扫码，并在扫码成功后立即开启连接。错误详情：{}",
                    err_msg
                ));
            }
            return Err(format!(
                "微信 iLink 启动失败：{}",
                err_msg
            ));
        }
    }
    Ok((
        base_url.clone(),
        format!("微信 iLink 已验证：{}", gateway_url_label(&base_url)),
    ))
}

fn probe_mobile_channel(binding: &MobileChannelBinding) -> Result<(String, String), String> {
    let client = build_http_client(15)?;
    match normalize_mobile_channel(&binding.channel).as_str() {
        "lark" => probe_lark_channel(&client, binding),
        "wechat" => probe_wechat_channel(&client, binding),
        "qq" => probe_qq_channel(&client, binding),
        _ => Err("不支持的手机通道".into()),
    }
}

fn command_available(name: &str) -> bool {
    let mut cmd = Command::new(name);
    cmd.arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd.status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn node_command_name() -> Result<&'static str, String> {
    for candidate in ["node", "node.exe"] {
        if command_available(candidate) {
            return Ok(candidate);
        }
    }
    Err("没有找到 node，请先安装 Node.js，或手动填写 QQ AppID/AppSecret".into())
}

/// 在 PATH 各目录下按文件名查找可执行文件，返回首个存在的完整路径。
/// 用于解决 Windows 下 Rust 的 Command 只识别 .exe、找不到 npm 包装脚本(.cmd)的问题。
fn which_in_path(file_names: &[&str]) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        for name in file_names {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// 解析 Codex CLI 的可执行文件完整路径。
///
/// 优先级:
/// 1. PATH 中的原生二进制 `codex.exe`(Rust 可直接 spawn，最稳)
/// 2. PATH 中 npm 安装的包装脚本 `codex.cmd`(Windows 下 Rust 可经 cmd 执行)
/// 3. PATH 中的无扩展名 `codex`(类 Unix 平台)
/// 4. 若 PATH 仅有包装脚本，尝试解析其内部的原生 codex.exe
fn resolve_codex_command() -> Result<PathBuf, String> {
    // Windows 优先原生 exe，其次 .cmd；非 Windows 直接找 codex。
    let candidates: &[&str] = if cfg!(windows) {
        &["codex.exe", "codex.cmd", "codex.bat", "codex"]
    } else {
        &["codex", "codex.exe"]
    };

    if let Some(found) = which_in_path(candidates) {
        // 找到的是 npm 包装脚本时，尝试定位其内部的原生 codex.exe 以获得更稳定的执行。
        if let Some(native) = resolve_bundled_codex_exe(&found) {
            return Ok(native);
        }
        return Ok(found);
    }

    Err(
        "没有找到 Codex CLI。请先安装 Codex（npm i -g @openai/codex），或将 codex 加入 PATH 后重试。"
            .into(),
    )
}

/// 如果传入的是 npm 包装脚本(codex / codex.cmd)，尝试在其同级 node_modules 中
/// 找到平台原生的 codex.exe，避免每次都经过 node 启动包装层。找不到则返回 None。
fn resolve_bundled_codex_exe(wrapper: &std::path::Path) -> Option<PathBuf> {
    if !cfg!(windows) {
        return None;
    }
    let ext = wrapper
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());
    // 只有包装脚本(无扩展名或 .cmd/.bat)才需要进一步解析；本身就是 .exe 直接用。
    if matches!(ext.as_deref(), Some("exe")) {
        return None;
    }
    let base = wrapper.parent()?;
    let native = base
        .join("node_modules")
        .join("@openai")
        .join("codex")
        .join("node_modules")
        .join("@openai")
        .join("codex-win32-x64")
        .join("vendor")
        .join("x86_64-pc-windows-msvc")
        .join("bin")
        .join("codex.exe");
    if native.is_file() {
        Some(native)
    } else {
        None
    }
}

fn npm_command_name() -> Result<&'static str, String> {
    for candidate in ["npm.cmd", "npm", "npm.exe"] {
        if command_available(candidate) {
            return Ok(candidate);
        }
    }
    Err("没有找到 npm，请先安装 Node.js/npm，或手动填写 QQ AppID/AppSecret".into())
}

fn qq_qr_runner_text() -> &'static str {
    r#"import { startQrConnect } from '@tencent-connect/qqbot-connector';
import QRCode from 'qrcode';

const emit = (payload) => process.stdout.write(`${JSON.stringify(payload)}\n`);

let stop = null;
try {
  stop = startQrConnect({
    async onSuccess(credentials) {
      emit({ type: 'success', credentials });
      setTimeout(() => process.exit(0), 80);
    },
    onFailure(error) {
      emit({ type: 'failure', message: error?.message || String(error || '扫码绑定失败') });
      setTimeout(() => process.exit(1), 80);
    },
    async onQrDisplayed(url) {
      let dataUrl = '';
      const value = String(url || '').trim();
      if (!/^(https?:\/\/|mqqapi:\/\/|qqbot:\/\/)/i.test(value)) {
        emit({ type: 'failure', message: `QQ connector 返回的不是可扫码授权链接，而是内部字符串：${value.slice(0, 24)}...` });
        setTimeout(() => process.exit(1), 80);
        return;
      }
      try {
        dataUrl = await QRCode.toDataURL(value, { width: 240, margin: 1 });
      } catch (error) {
        emit({ type: 'warning', message: error?.message || String(error || '二维码生成失败') });
      }
      emit({ type: 'qr', url: value, dataUrl });
    },
    onQrExpired() {
      emit({ type: 'expired' });
    },
  }, { displayQrCodeToConsole: false });
} catch (error) {
  emit({ type: 'failure', message: error?.message || String(error || '扫码服务启动失败') });
  process.exit(1);
}

const shutdown = () => {
  try {
    if (typeof stop === 'function') stop();
  } catch {}
  process.exit(0);
};

process.on('SIGTERM', shutdown);
process.on('SIGINT', shutdown);
"#
}

fn qq_gateway_runner_text() -> &'static str {
    r#"import WebSocket from 'ws';
import https from 'node:https';

const emit = (payload) => process.stdout.write(`${JSON.stringify(payload)}\n`);
const appId = process.env.QQ_APP_ID || '';
const appSecret = process.env.QQ_APP_SECRET || '';
const intents = Number(process.env.QQ_INTENTS || '33554432');
let accessToken = '';
let expiresAt = 0;
let stopping = false;
let ws = null;
let heartbeatTimer = null;

function requestJson(method, url, payload, headers = {}) {
  return new Promise((resolve, reject) => {
    const body = payload == null ? '' : JSON.stringify(payload);
    const req = https.request(url, {
      method,
      headers: {
        'content-type': 'application/json; charset=utf-8',
        ...(body ? { 'content-length': Buffer.byteLength(body) } : {}),
        ...headers,
      },
    }, (res) => {
      const chunks = [];
      res.on('data', (chunk) => chunks.push(chunk));
      res.on('end', () => {
        const text = Buffer.concat(chunks).toString('utf8');
        if (res.statusCode < 200 || res.statusCode >= 300) {
          reject(new Error(`HTTP ${res.statusCode}: ${text}`));
          return;
        }
        try {
          resolve(text.trim() ? JSON.parse(text) : {});
        } catch {
          reject(new Error(`接口返回不是 JSON: ${text.slice(0, 200)}`));
        }
      });
    });
    req.on('error', reject);
    if (body) req.write(body);
    req.end();
  });
}

async function token() {
  if (accessToken && Date.now() < expiresAt - 60000) return accessToken;
  const result = await requestJson('POST', 'https://bots.qq.com/app/getAppAccessToken', {
    appId,
    clientSecret: appSecret,
  });
  accessToken = String(result.access_token || '').trim();
  if (!accessToken) throw new Error(`QQ 鉴权未返回 access_token: ${JSON.stringify(result)}`);
  expiresAt = Date.now() + Math.max(0, Number(result.expires_in || 0)) * 1000;
  return accessToken;
}

async function api(method, path, payload) {
  const t = await token();
  return requestJson(method, `https://api.sgroup.qq.com${path}`, payload, {
    authorization: `QQBot ${t}`,
  });
}

function normalizeContent(data) {
  for (const key of ['content', 'text', 'message', 'msg']) {
    let text = String(data?.[key] || '').trim();
    if (!text) continue;
    text = text.replace(/<@!?\d+>/g, '').replace(/\s+/g, ' ').trim();
    if (text) return text;
  }
  const attachments = Array.isArray(data?.attachments) ? data.attachments : [];
  return attachments.map((item, index) => {
    const url = String(item?.url || item?.file_url || item?.image_url || '').trim();
    const name = String(item?.filename || item?.name || item?.file_name || '').trim();
    return `[QQ 附件 ${index + 1}: ${[name, url].filter(Boolean).join(' ') || '未提供链接'}]`;
  }).join('\n').trim();
}

function parseMessage(payload) {
  const eventType = String(payload?.t || '');
  if (!['C2C_MESSAGE_CREATE', 'GROUP_MSG_RECEIVE', 'GROUP_AT_MESSAGE_CREATE', 'C2C_MSG_RECEIVE'].includes(eventType)) return null;
  const data = payload?.d || {};
  const content = normalizeContent(data);
  if (!content) return null;
  const author = data.author || {};
  const groupOpenid = String(data.group_openid || data.group_id || '').trim();
  const openid = String(data.openid || data.user_openid || author.user_openid || author.member_openid || author.id || '').trim();
  return {
    type: 'message',
    eventType,
    eventId: String(payload.id || data.event_id || '').trim(),
    messageId: String(data.id || data.msg_id || '').trim(),
    content,
    scene: groupOpenid || eventType.startsWith('GROUP_') ? 'group' : 'c2c',
    openid,
    groupOpenid,
  };
}

function clearHeartbeat() {
  if (heartbeatTimer) clearInterval(heartbeatTimer);
  heartbeatTimer = null;
}

async function connectOnce() {
  const gateway = await api('GET', '/gateway');
  const url = String(gateway.url || '').trim();
  if (!url) throw new Error(`QQ 网关地址为空: ${JSON.stringify(gateway)}`);
  emit({ type: 'status', message: '正在连接 QQ 网关...' });
  await new Promise((resolve, reject) => {
    ws = new WebSocket(url);
    let settled = false;
    const fail = (error) => {
      clearHeartbeat();
      if (!settled) {
        settled = true;
        reject(error instanceof Error ? error : new Error(String(error || 'QQ 网关断开')));
      }
    };
    ws.on('open', () => emit({ type: 'status', message: 'QQ 网关已连接' }));
    ws.on('message', async (data) => {
      let payload = {};
      try {
        payload = JSON.parse(Buffer.isBuffer(data) ? data.toString('utf8') : String(data));
      } catch {
        return;
      }
      if (payload.s != null) ws.nextSeq = payload.s;
      if (payload.op === 10) {
        const heartbeatMs = Number(payload?.d?.heartbeat_interval || payload?.d?.heartbeatInterval || 30000);
        const t = await token();
        ws.send(JSON.stringify({
          op: 2,
          d: {
            token: `QQBot ${t}`,
            intents,
            shard: [0, 1],
            properties: { '$os': process.platform, '$browser': 'VarSwitch', '$device': 'VarSwitch' },
          },
        }));
        clearHeartbeat();
        heartbeatTimer = setInterval(() => {
          try {
            if (ws?.readyState === WebSocket.OPEN) ws.send(JSON.stringify({ op: 1, d: ws.nextSeq ?? null }));
          } catch {}
        }, Math.max(1000, heartbeatMs));
        return;
      }
      if (payload.op === 0 && payload.t === 'READY') {
        emit({ type: 'ready' });
        return;
      }
      if (payload.op === 7 || payload.op === 9) {
        fail(new Error('QQ 网关要求重新连接'));
        return;
      }
      const message = parseMessage(payload);
      if (message) emit(message);
    });
    ws.on('error', fail);
    ws.on('close', () => fail(new Error('QQ 网关连接已关闭')));
  });
}

async function main() {
  if (!appId || !appSecret) throw new Error('缺少 QQ AppID/AppSecret');
  while (!stopping) {
    try {
      await connectOnce();
    } catch (error) {
      if (stopping) break;
      emit({ type: 'status', message: `QQ 网关断开，准备重连：${error?.message || String(error)}` });
      await new Promise((resolve) => setTimeout(resolve, 5000));
    } finally {
      clearHeartbeat();
      try { ws?.close(); } catch {}
      ws = null;
    }
  }
}

const shutdown = () => {
  stopping = true;
  clearHeartbeat();
  try { ws?.close(); } catch {}
  setTimeout(() => process.exit(0), 50);
};
process.on('SIGTERM', shutdown);
process.on('SIGINT', shutdown);

main().catch((error) => {
  emit({ type: 'failure', message: error?.message || String(error || 'QQ 网关启动失败') });
  process.exit(1);
});
"#
}

fn lark_bridge_runner_text() -> &'static str {
    r#"import WebSocket from 'ws';
import https from 'node:https';

const emit = (payload) => process.stdout.write(`${JSON.stringify(payload)}\n`);

const appId = String(process.env.LARK_APP_ID || '').trim();
const appSecret = String(process.env.LARK_APP_SECRET || '').trim();
const baseUrl = String(process.env.LARK_BASE_URL || 'https://open.feishu.cn').replace(/\/+$/, '');
const botOpenId = String(process.env.LARK_BOT_OPEN_ID || '').trim();
const WS_METHOD_CONTROL = 0;
const WS_METHOD_DATA = 1;
const WS_EVENT_TYPE = 'event';
const WS_CARD_TYPE = 'card';
const WS_PING_TYPE = 'ping';
const WS_PONG_TYPE = 'pong';
let stopping = false;
let ws = null;
let pingTimer = null;
let pingIntervalMs = 30000;
let serviceId = 0;
const chunkCache = new Map();

if (!appId || !appSecret) {
  emit({ type: 'failure', message: '缺少飞书 AppID/AppSecret' });
  process.exit(1);
}

function requestJson(method, url, payload, headers = {}) {
  return new Promise((resolve, reject) => {
    const body = payload == null ? '' : JSON.stringify(payload);
    const req = https.request(url, {
      method,
      headers: {
        'content-type': 'application/json; charset=utf-8',
        'user-agent': 'VarSwitch hi-codex-compatible',
        ...(body ? { 'content-length': Buffer.byteLength(body) } : {}),
        ...headers,
      },
    }, (res) => {
      const chunks = [];
      res.on('data', (chunk) => chunks.push(chunk));
      res.on('end', () => {
        const text = Buffer.concat(chunks).toString('utf8');
        if (res.statusCode < 200 || res.statusCode >= 300) {
          reject(new Error(`HTTP ${res.statusCode}: ${text}`));
          return;
        }
        try {
          resolve(text.trim() ? JSON.parse(text) : {});
        } catch {
          reject(new Error(`接口返回不是 JSON: ${text.slice(0, 200)}`));
        }
      });
    });
    req.on('error', reject);
    if (body) req.write(body);
    req.end();
  });
}

function resultOk(result) {
  return result?.code == null || result.code === 0 || result.code === '0';
}

function apiError(result, fallback = '接口返回异常') {
  const message = String(result?.msg || result?.message || fallback).trim();
  return result?.code == null || result.code === 0 || result.code === '0'
    ? message
    : `code=${result.code} ${message}`.trim();
}

async function pullWsConfig() {
  const result = await requestJson('POST', `${baseUrl}/callback/ws/endpoint`, {
    AppID: appId,
    AppSecret: appSecret,
  });
  if (!resultOk(result)) throw new Error(`飞书 WebSocket 网关获取失败：${apiError(result)}`);
  const data = result?.data || {};
  const url = String(data.URL || data.url || '').trim();
  if (!url) throw new Error(`飞书 WebSocket 网关地址为空：${JSON.stringify(result)}`);
  const parsed = new URL(url);
  serviceId = Number(parsed.searchParams.get('service_id') || parsed.searchParams.get('ServiceID') || 0);
  const clientConfig = data.ClientConfig || data.clientConfig || {};
  const seconds = Number(clientConfig.PingInterval || clientConfig.pingInterval || 30);
  pingIntervalMs = Math.max(1000, seconds * 1000);
  return url;
}

function readVarint(buffer, start) {
  let value = 0;
  let shift = 0;
  let pos = start;
  while (pos < buffer.length) {
    const byte = buffer[pos++];
    value += (byte & 0x7f) * Math.pow(2, shift);
    if ((byte & 0x80) === 0) return [value, pos];
    shift += 7;
    if (shift > 70) break;
  }
  throw new Error('invalid protobuf varint');
}

function writeVarint(value) {
  let next = Number(value || 0);
  const out = [];
  while (true) {
    let byte = next & 0x7f;
    next = Math.floor(next / 128);
    if (next) out.push(byte | 0x80);
    else {
      out.push(byte);
      break;
    }
  }
  return Buffer.from(out);
}

function readBytes(buffer, pos) {
  const [length, afterLength] = readVarint(buffer, pos);
  const end = afterLength + length;
  if (end > buffer.length) throw new Error('invalid protobuf length');
  return [buffer.subarray(afterLength, end), end];
}

function skipValue(buffer, pos, wireType) {
  if (wireType === 0) return readVarint(buffer, pos)[1];
  if (wireType === 1) return Math.min(buffer.length, pos + 8);
  if (wireType === 2) return readBytes(buffer, pos)[1];
  if (wireType === 5) return Math.min(buffer.length, pos + 4);
  throw new Error(`unsupported protobuf wire type: ${wireType}`);
}

function protoBytes(fieldId, value) {
  const raw = Buffer.isBuffer(value) ? value : Buffer.from(value || '');
  return Buffer.concat([writeVarint((fieldId << 3) | 2), writeVarint(raw.length), raw]);
}

function protoString(fieldId, value) {
  return protoBytes(fieldId, Buffer.from(String(value || ''), 'utf8'));
}

function protoVarint(fieldId, value) {
  return Buffer.concat([writeVarint((fieldId << 3) | 0), writeVarint(value)]);
}

function encodeHeader(key, value) {
  return Buffer.concat([protoString(1, key), protoString(2, value)]);
}

function decodeHeader(buffer) {
  let pos = 0;
  let key = '';
  let value = '';
  while (pos < buffer.length) {
    const [tag, afterTag] = readVarint(buffer, pos);
    pos = afterTag;
    const fieldId = tag >> 3;
    const wireType = tag & 7;
    if (fieldId === 1 && wireType === 2) {
      const [raw, next] = readBytes(buffer, pos);
      key = raw.toString('utf8');
      pos = next;
    } else if (fieldId === 2 && wireType === 2) {
      const [raw, next] = readBytes(buffer, pos);
      value = raw.toString('utf8');
      pos = next;
    } else {
      pos = skipValue(buffer, pos, wireType);
    }
  }
  return [key, value];
}

function encodeFrame(frame) {
  const parts = [
    protoVarint(1, frame.seqId || 0),
    protoVarint(2, frame.logId || 0),
    protoVarint(3, frame.service || 0),
    protoVarint(4, frame.method || 0),
  ];
  for (const [key, value] of frame.headers || []) {
    parts.push(protoBytes(5, encodeHeader(key, value)));
  }
  if (frame.payloadEncoding) parts.push(protoString(6, frame.payloadEncoding));
  if (frame.payloadType) parts.push(protoString(7, frame.payloadType));
  if (frame.payload?.length) parts.push(protoBytes(8, frame.payload));
  if (frame.logIdNew) parts.push(protoString(9, frame.logIdNew));
  return Buffer.concat(parts);
}

function decodeFrame(input) {
  const buffer = Buffer.isBuffer(input) ? input : Buffer.from(input);
  const frame = { seqId: 0, logId: 0, service: 0, method: 0, headers: [], payloadEncoding: '', payloadType: '', payload: Buffer.alloc(0), logIdNew: '' };
  let pos = 0;
  while (pos < buffer.length) {
    const [tag, afterTag] = readVarint(buffer, pos);
    pos = afterTag;
    const fieldId = tag >> 3;
    const wireType = tag & 7;
    if (fieldId === 1 && wireType === 0) [frame.seqId, pos] = readVarint(buffer, pos);
    else if (fieldId === 2 && wireType === 0) [frame.logId, pos] = readVarint(buffer, pos);
    else if (fieldId === 3 && wireType === 0) [frame.service, pos] = readVarint(buffer, pos);
    else if (fieldId === 4 && wireType === 0) [frame.method, pos] = readVarint(buffer, pos);
    else if (fieldId === 5 && wireType === 2) {
      const [raw, next] = readBytes(buffer, pos);
      frame.headers.push(decodeHeader(raw));
      pos = next;
    } else if (fieldId === 6 && wireType === 2) {
      const [raw, next] = readBytes(buffer, pos);
      frame.payloadEncoding = raw.toString('utf8');
      pos = next;
    } else if (fieldId === 7 && wireType === 2) {
      const [raw, next] = readBytes(buffer, pos);
      frame.payloadType = raw.toString('utf8');
      pos = next;
    } else if (fieldId === 8 && wireType === 2) {
      const [raw, next] = readBytes(buffer, pos);
      frame.payload = raw;
      pos = next;
    } else if (fieldId === 9 && wireType === 2) {
      const [raw, next] = readBytes(buffer, pos);
      frame.logIdNew = raw.toString('utf8');
      pos = next;
    } else {
      pos = skipValue(buffer, pos, wireType);
    }
  }
  return frame;
}

function headersObject(frame) {
  return Object.fromEntries((frame.headers || []).map(([key, value]) => [String(key), String(value)]));
}

function safeJson(value) {
  if (value && typeof value === 'object') return value;
  const text = String(value || '').trim();
  if (!text) return {};
  try {
    return JSON.parse(text);
  } catch {
    return {};
  }
}

function stripAtTags(text) {
  return String(text || '')
    .replace(/<at\s+[^>]*>(.*?)<\/at>/g, (_, name) => `@${String(name || '').trim()}`)
    .replace(/\s+/g, ' ')
    .trim();
}

function collectCardText(value, out = []) {
  if (Array.isArray(value)) {
    for (const item of value) collectCardText(item, out);
  } else if (value && typeof value === 'object') {
    for (const key of ['text', 'content', 'title', 'subtitle', 'alt', 'value']) {
      if (typeof value[key] === 'string' && value[key].trim()) out.push(value[key].trim());
    }
    for (const item of Object.values(value)) collectCardText(item, out);
  }
  return out;
}

function normalizeContent(message) {
  const type = String(message?.message_type || message?.msg_type || '').toLowerCase();
  const parsed = safeJson(message?.content || message?.body || {});
  if (type === 'text') {
    const text = typeof parsed.text === 'string' ? parsed.text : String(message?.text || message?.content || '');
    return stripAtTags(text);
  }
  if (type === 'post') {
    const body = parsed.zh_cn || parsed.en_us || parsed.ja_jp || parsed;
    const parts = [];
    if (body?.title) parts.push(String(body.title).trim());
    for (const paragraph of body?.content || []) {
      if (!Array.isArray(paragraph)) continue;
      const line = paragraph.map((item) => String(item?.text || item?.href || '')).join('').trim();
      if (line) parts.push(line);
    }
    return parts.join('\n').trim();
  }
  if (type === 'interactive') return collectCardText(parsed).slice(0, 8).join('\n').trim() || '[飞书 卡片消息]';
  if (['img', 'image'].includes(type)) return `[飞书 图片: ${parsed.image_key || parsed.file_key || '未提供 file_key'}]`;
  if (['media', 'video', 'audio', 'file'].includes(type)) return `[飞书 文件: ${parsed.file_name || parsed.name || parsed.file_key || parsed.image_key || '未提供 file_key'}]`;
  if (typeof parsed.text === 'string') return stripAtTags(parsed.text);
  return stripAtTags(message?.content || '');
}

function parseEventPayload(data) {
  if (data?.schema) {
    const header = data.header || {};
    return [String(header.event_type || data.event_type || '').trim(), { ...(data.event || {}), app_id: header.app_id, tenant_key: header.tenant_key, event_id: header.event_id }];
  }
  if (data?.event && typeof data.event === 'object') {
    return [String(data.event.type || data.event_type || data.type || '').trim(), { ...data.event, app_id: data.app_id ?? data.event.app_id, tenant_key: data.tenant_key ?? data.event.tenant_key, event_id: data.event_id ?? data.event.event_id }];
  }
  return [String(data?.event_type || data?.type || '').trim(), data || {}];
}

function parseIncomingMessage(payload) {
  const [eventType, event] = parseEventPayload(payload);
  if (eventType && eventType !== 'im.message.receive_v1') return null;
  if (event?.app_id && String(event.app_id).trim() !== appId) return null;
  const senderId = event?.sender?.sender_id || {};
  const senderOpenId = String(senderId.open_id || senderId.user_id || '').trim();
  if (botOpenId && senderOpenId && senderOpenId === botOpenId) return null;
  const message = event?.message || {};
  const text = normalizeContent(message);
  const messageId = String(message.message_id || '').trim();
  const chatId = String(message.chat_id || '').trim();
  if (!messageId || !text) return null;
  return {
    type: 'message',
    eventId: String(event.event_id || messageId).trim(),
    messageId,
    chatId,
    senderId: senderOpenId,
    chatType: String(message.chat_type || '').trim(),
    threadId: String(message.thread_id || message.root_id || '').trim(),
    rootId: String(message.root_id || '').trim(),
    text,
  };
}

function mergeFramePayload(frame) {
  const headers = headersObject(frame);
  const messageId = headers.message_id || headers.trace_id || String(frame.seqId || '');
  const total = Math.max(1, Number(headers.sum || 1));
  const seq = Number(headers.seq || 0);
  if (total <= 1) return safeJson(frame.payload.toString('utf8'));
  const index = seq >= 0 && seq < total ? seq : Math.max(0, Math.min(total - 1, seq - 1));
  const cache = chunkCache.get(messageId) || { chunks: Array(total).fill(null), created: Date.now() };
  cache.chunks[index] = frame.payload;
  chunkCache.set(messageId, cache);
  if (cache.chunks.every(Boolean)) {
    chunkCache.delete(messageId);
    return safeJson(Buffer.concat(cache.chunks).toString('utf8'));
  }
  for (const [key, value] of chunkCache.entries()) {
    if (Date.now() - value.created > 20000) chunkCache.delete(key);
  }
  return null;
}

function sendFrame(frame) {
  if (ws?.readyState === WebSocket.OPEN) ws.send(encodeFrame(frame));
}

function sendPing() {
  sendFrame({ seqId: 0, logId: 0, service: serviceId, method: WS_METHOD_CONTROL, headers: [['type', WS_PING_TYPE]] });
}

function sendAck(frame, code = 200) {
  sendFrame({
    seqId: frame.seqId,
    logId: frame.logId,
    service: frame.service,
    method: frame.method,
    headers: [...(frame.headers || []), ['biz_rt', '0']],
    payloadEncoding: frame.payloadEncoding,
    payloadType: frame.payloadType,
    payload: Buffer.from(JSON.stringify({ code }), 'utf8'),
    logIdNew: frame.logIdNew,
  });
}

function resetPingTimer() {
  if (pingTimer) clearInterval(pingTimer);
  pingTimer = setInterval(sendPing, Math.max(1000, pingIntervalMs));
}

function handleFrame(frame) {
  const headers = headersObject(frame);
  const frameType = headers.type || '';
  if (frame.method === WS_METHOD_CONTROL) {
    if (frameType === WS_PONG_TYPE && frame.payload?.length) {
      const data = safeJson(frame.payload.toString('utf8'));
      if (data.PingInterval) {
        pingIntervalMs = Math.max(1000, Number(data.PingInterval) * 1000);
        resetPingTimer();
      }
    }
    return;
  }
  if (frame.method !== WS_METHOD_DATA || ![WS_EVENT_TYPE, WS_CARD_TYPE].includes(frameType)) return;
  const payload = mergeFramePayload(frame);
  if (!payload) return;
  let code = 200;
  try {
    const message = parseIncomingMessage(payload);
    if (message) emit(message);
  } catch (error) {
    code = 500;
    emit({ type: 'status', message: `飞书消息解析失败：${error?.message || String(error)}` });
  } finally {
    sendAck(frame, code);
  }
}

async function connectOnce() {
  const url = await pullWsConfig();
  emit({ type: 'status', message: '正在连接飞书 WebSocket...' });
  await new Promise((resolve, reject) => {
    let settled = false;
    ws = new WebSocket(url);
    const fail = (error) => {
      if (pingTimer) clearInterval(pingTimer);
      pingTimer = null;
      if (settled) return;
      settled = true;
      if (stopping) resolve();
      else reject(error instanceof Error ? error : new Error(String(error || '飞书 WebSocket 已断开')));
    };
    ws.on('open', () => {
      emit({ type: 'ready' });
      sendPing();
      resetPingTimer();
    });
    ws.on('message', (data) => {
      try {
        handleFrame(decodeFrame(Buffer.isBuffer(data) ? data : Buffer.from(data)));
      } catch (error) {
        emit({ type: 'status', message: `飞书 WebSocket 帧处理失败：${error?.message || String(error)}` });
      }
    });
    ws.on('error', fail);
    ws.on('close', () => fail(new Error('飞书 WebSocket 已关闭')));
  });
}

async function main() {
  while (!stopping) {
    try {
      await connectOnce();
    } catch (error) {
      if (stopping) break;
      emit({ type: 'status', message: `飞书 WebSocket 异常，准备重连：${error?.message || String(error)}` });
      await new Promise((resolve) => setTimeout(resolve, 5000));
    } finally {
      if (pingTimer) clearInterval(pingTimer);
      pingTimer = null;
      try { ws?.close(); } catch {}
      ws = null;
    }
  }
}

const shutdown = () => {
  stopping = true;
  if (pingTimer) clearInterval(pingTimer);
  try { ws?.close(); } catch {}
  setTimeout(() => process.exit(0), 50);
};

process.on('SIGTERM', shutdown);
process.on('SIGINT', shutdown);

main().catch((error) => {
  emit({ type: 'failure', message: error?.message || String(error || '飞书消息桥启动失败') });
  process.exit(1);
});
"#
}

fn ensure_lark_bridge_connector(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let connector_dir = data_dir(app).join("larkbot-connector");
    fs::create_dir_all(&connector_dir).map_err(|e| e.to_string())?;
    let package_path = connector_dir.join("package.json");
    let runner_path = connector_dir.join("lark-bridge-runner.mjs");
    let package_text = serde_json::to_string_pretty(&serde_json::json!({
        "private": true,
        "type": "module",
        "dependencies": {
            "ws": "latest"
        }
    }))
    .map_err(|e| e.to_string())?;
    fs::write(&package_path, package_text).map_err(|e| e.to_string())?;
    fs::write(&runner_path, lark_bridge_runner_text()).map_err(|e| e.to_string())?;

    let ws_module = connector_dir.join("node_modules").join("ws");
    if !ws_module.exists() {
        let npm = npm_command_name()?;
        let mut cmd = Command::new(npm);
        cmd.args([
                "install",
                "--omit=dev",
                "--no-audit",
                "--no-fund",
                "--registry=https://registry.npmmirror.com",
            ])
            .current_dir(&connector_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(target_os = "windows")]
        cmd.creation_flags(CREATE_NO_WINDOW);
        let output = cmd.output()
            .map_err(|e| format!("启动 npm 安装飞书 WebSocket 依赖失败: {e}"))?;
        if !output.status.success() {
            let mut detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
            if detail.is_empty() {
                detail = String::from_utf8_lossy(&output.stdout).trim().to_string();
            }
            return Err(format!("安装飞书 WebSocket 依赖失败: {detail}"));
        }
    }
    Ok(runner_path)
}

fn ensure_qq_qr_connector(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let connector_dir = data_dir(app).join("qqbot-connector");
    fs::create_dir_all(&connector_dir).map_err(|e| e.to_string())?;
    let package_path = connector_dir.join("package.json");
    let runner_path = connector_dir.join("qqbot-qr-runner.mjs");
    let package_text = serde_json::to_string_pretty(&serde_json::json!({
        "private": true,
        "type": "module",
        "dependencies": {
            "@tencent-connect/qqbot-connector": "latest",
            "qrcode": "latest",
            "ws": "latest"
        }
    }))
    .map_err(|e| e.to_string())?;
    fs::write(&package_path, package_text).map_err(|e| e.to_string())?;
    fs::write(&runner_path, qq_qr_runner_text()).map_err(|e| e.to_string())?;

    let connector_module = connector_dir
        .join("node_modules")
        .join("@tencent-connect")
        .join("qqbot-connector");
    let qrcode_module = connector_dir.join("node_modules").join("qrcode");
    let ws_module = connector_dir.join("node_modules").join("ws");
    if !connector_module.exists() || !qrcode_module.exists() || !ws_module.exists() {
        let npm = npm_command_name()?;
        let mut cmd = Command::new(npm);
        cmd.args([
                "install",
                "--omit=dev",
                "--no-audit",
                "--no-fund",
                "--registry=https://registry.npmmirror.com",
            ])
            .current_dir(&connector_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(target_os = "windows")]
        cmd.creation_flags(CREATE_NO_WINDOW);
        let output = cmd.output()
            .map_err(|e| format!("启动 npm 安装 QQ 扫码依赖失败: {e}"))?;
        if !output.status.success() {
            let mut detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
            if detail.is_empty() {
                detail = String::from_utf8_lossy(&output.stdout).trim().to_string();
            }
            return Err(format!("安装 QQ 扫码依赖失败: {detail}"));
        }
    }
    Ok(runner_path)
}

fn ensure_qq_gateway_connector(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let connector_dir = data_dir(app).join("qqbot-connector");
    let _ = ensure_qq_qr_connector(app)?;
    let runner_path = connector_dir.join("qqbot-gateway-runner.mjs");
    fs::write(&runner_path, qq_gateway_runner_text()).map_err(|e| e.to_string())?;
    Ok(runner_path)
}

fn write_mobile_channel_qr_state(
    app: &tauri::AppHandle,
    channel: &str,
    status: &str,
    qr_url: &str,
    qr_data_url: &str,
    error: &str,
) -> Result<ToolboxSnapshot, String> {
    let normalized = normalize_mobile_channel(channel);
    let mut state = read_toolbox_state(app);
    if !state
        .mobile_channels
        .iter()
        .any(|binding| binding.channel == normalized)
    {
        state
            .mobile_channels
            .push(default_mobile_channel(&normalized));
    }
    if let Some(binding) = state
        .mobile_channels
        .iter_mut()
        .find(|binding| binding.channel == normalized)
    {
        let now = chrono_now();
        binding.qr_status = status.to_string();
        binding.qr_url = qr_url.to_string();
        binding.qr_data_url = qr_data_url.to_string();
        if qr_url.trim().is_empty() && qr_data_url.trim().is_empty() {
            binding.qr_device_code.clear();
        }
        binding.last_error = error.to_string();
        if !status.trim().is_empty()
            && (!qr_url.trim().is_empty()
                || !qr_data_url.trim().is_empty()
                || binding.qr_started_at.trim().is_empty())
        {
            binding.qr_started_at = now.clone();
        }
        binding.updated_at = now;
    }
    write_toolbox_state(app, &state)?;
    Ok(build_toolbox_snapshot(app))
}

fn selected_thread_for_mobile_state(state: &ToolboxState) -> Option<CodexThreadRecord> {
    if state.synced_codex_threads.is_empty() {
        return None;
    }
    let selected = state.selected_mobile_thread_id.trim();
    if !selected.is_empty() {
        if let Some(thread) = state
            .synced_codex_threads
            .iter()
            .find(|thread| thread.id == selected)
        {
            return Some(thread.clone());
        }
    }
    state.synced_codex_threads.first().cloned()
}

fn attach_selected_thread_to_mobile_channel(state: &mut ToolboxState, channel: &str) {
    let normalized = normalize_mobile_channel(channel);
    let Some(thread) = selected_thread_for_mobile_state(state) else {
        return;
    };

    state
        .session_bindings
        .retain(|binding| normalize_mobile_channel(&binding.channel) != normalized);
    state.session_bindings.push(CodexSessionBinding {
        channel: normalized.clone(),
        thread_id: thread.id.clone(),
        thread_name: thread.thread_name.clone(),
        session_file: thread.session_file.clone(),
        updated_at: thread.updated_at.clone(),
        cwd: thread.cwd.clone(),
        sync_enabled: true,
        last_synced_at: String::new(),
        note: "mobile-control".into(),
    });
    state.selected_mobile_thread_id = thread.id.clone();
    state.mobile_remote.active_thread_id = thread.id.clone();
    state.mobile_remote.active_thread_name = thread.thread_name.clone();

    if let Some(binding) = state
        .mobile_channels
        .iter_mut()
        .find(|binding| binding.channel == normalized)
    {
        binding.thread_id = thread.id;
        binding.thread_name = thread.thread_name;
        binding.session_file = thread.session_file;
        binding.enabled = true;
        binding.status = "已绑定，等待开启平台连接".into();
        binding.updated_at = chrono_now();
    }
}

#[derive(Clone, Debug)]
struct LarkRegistrationPollResult {
    status: String,
    client_id: String,
    client_secret: String,
    tenant_brand: String,
    message: String,
    interval_secs: u64,
}

fn post_lark_registration_form(
    client: &reqwest::blocking::Client,
    params: &[(&str, &str)],
) -> Result<serde_json::Value, String> {
    let response = client
        .post(lark_registration_endpoint())
        .header("locale", "zh")
        .form(params)
        .send()
        .map_err(|e| format!("飞书机器人注册请求失败: {e}"))?;
    let status = response.status();
    let body = response
        .text()
        .map_err(|e| format!("飞书机器人注册响应读取失败: {e}"))?;
    if !status.is_success() && body.trim().is_empty() {
        return Err(format!("飞书机器人注册返回 HTTP {}", status.as_u16()));
    }
    serde_json::from_str(&body).map_err(|e| format!("飞书机器人注册返回不是 JSON: {e}; {body}"))
}

fn request_lark_registration(
    client: &reqwest::blocking::Client,
    create_only: bool,
    app_id: &str,
) -> Result<(String, String, u64), String> {
    let result = post_lark_registration_form(
        client,
        &[
            ("action", "begin"),
            ("archetype", "PersonalAgent"),
            ("auth_method", "client_secret"),
            ("request_user_info", "open_id"),
        ],
    )?;
    let device_code = json_string(&result, &["device_code"]);
    let verification_url = json_string(
        &result,
        &[
            "verification_uri_complete",
            "verification_uri",
            "qrcode_url",
        ],
    );
    if device_code.is_empty() || verification_url.is_empty() {
        return Err(format!("飞书注册接口未返回授权信息：{result}"));
    }
    let mut params = vec![
        ("from", "sdk"),
        ("source", "node-sdk/varswitch"),
        ("tp", "sdk"),
        ("addons", lark_registration_addons()),
        ("name", "VarSwitch 智能体"),
        (
            "desc",
            "把飞书消息转发给 Codex，并把 Codex 回复同步回飞书。",
        ),
    ];
    if create_only {
        params.push(("createOnly", "true"));
    }
    if !app_id.trim().is_empty() {
        params.push(("clientID", app_id.trim()));
    }
    let launcher_url = append_query_params(&verification_url, &params);
    let interval = result
        .get("interval")
        .and_then(|value| value.as_u64())
        .unwrap_or(5)
        .clamp(2, 10);
    Ok((device_code, launcher_url, interval))
}

fn poll_lark_registration_device(
    client: &reqwest::blocking::Client,
    device_code: &str,
) -> Result<LarkRegistrationPollResult, String> {
    let result = post_lark_registration_form(
        client,
        &[("action", "poll"), ("device_code", device_code.trim())],
    )?;
    let user_info = result
        .get("user_info")
        .and_then(|value| value.as_object())
        .cloned()
        .unwrap_or_default();
    let tenant_brand = user_info
        .get("tenant_brand")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let client_id = json_string(&result, &["client_id"]);
    let client_secret = json_string(&result, &["client_secret"]);
    if !client_id.is_empty() && !client_secret.is_empty() {
        return Ok(LarkRegistrationPollResult {
            status: "confirmed".into(),
            client_id,
            client_secret,
            tenant_brand,
            message: "飞书机器人创建完成，AppID/AppSecret 已自动填充".into(),
            interval_secs: 0,
        });
    }

    let error = json_string(&result, &["error"]);
    let description = json_string(&result, &["error_description", "message", "msg"]);
    let (status, message, interval_secs) = match error.as_str() {
        "" | "authorization_pending" => ("pending", "等待飞书创建/授权确认", 5),
        "slow_down" => ("slow_down", "飞书要求降低轮询频率，稍后继续检查", 8),
        "access_denied" => ("access_denied", "飞书授权被取消", 0),
        "expired_token" => ("expired_token", "飞书创建授权已过期，请重新创建", 0),
        _ => ("error", description.as_str(), 0),
    };
    Ok(LarkRegistrationPollResult {
        status: status.into(),
        client_id: String::new(),
        client_secret: String::new(),
        tenant_brand,
        message: if message.is_empty() {
            "等待飞书创建/授权确认".into()
        } else {
            message.into()
        },
        interval_secs,
    })
}

fn apply_lark_registration_poll(
    app: &tauri::AppHandle,
    poll: LarkRegistrationPollResult,
) -> Result<ToolboxSnapshot, String> {
    let mut state = read_toolbox_state(app);
    if let Some(binding) = state
        .mobile_channels
        .iter_mut()
        .find(|binding| binding.channel == "lark")
    {
        binding.qr_status = poll.message.clone();
        binding.last_error.clear();
        binding.updated_at = chrono_now();
        if poll.status == "confirmed" {
            binding.app_id = poll.client_id;
            binding.app_secret = poll.client_secret;
            binding.base_url = if poll.tenant_brand == "lark" {
                "https://open.larksuite.com".into()
            } else {
                "https://open.feishu.cn".into()
            };
            binding.qr_device_code.clear();
            binding.qr_url.clear();
            binding.qr_data_url.clear();
            binding.credential_status = "飞书 AppID/AppSecret 已自动保存".into();
        } else if matches!(
            poll.status.as_str(),
            "access_denied" | "expired_token" | "error"
        ) {
            binding.last_error = poll.message;
        }
    }
    if poll.status == "confirmed" {
        attach_selected_thread_to_mobile_channel(&mut state, "lark");
    }
    write_toolbox_state(app, &state)?;
    Ok(build_toolbox_snapshot(app))
}

fn start_lark_registration_poll_worker(
    app: tauri::AppHandle,
    device_code: String,
    initial_interval_secs: u64,
) {
    std::thread::spawn(move || {
        let client = match build_http_client(15) {
            Ok(client) => client,
            Err(error) => {
                update_channel_status(&app, "lark", "飞书注册轮询失败", &error);
                LARK_REGISTRATION_ACTIVE.store(false, Ordering::SeqCst);
                return;
            }
        };
        let mut interval = initial_interval_secs.clamp(2, 10);
        for _ in 0..120 {
            std::thread::sleep(Duration::from_secs(interval));
            let poll = match poll_lark_registration_device(&client, &device_code) {
                Ok(poll) => poll,
                Err(error) => {
                    update_channel_status(&app, "lark", "飞书注册轮询失败", &error);
                    LARK_REGISTRATION_ACTIVE.store(false, Ordering::SeqCst);
                    return;
                }
            };
            interval = poll.interval_secs.clamp(2, 10);
            let final_status = matches!(
                poll.status.as_str(),
                "confirmed" | "access_denied" | "expired_token" | "error"
            );
            let _ = apply_lark_registration_poll(&app, poll);
            if final_status {
                LARK_REGISTRATION_ACTIVE.store(false, Ordering::SeqCst);
                return;
            }
        }
        update_channel_status(
            &app,
            "lark",
            "飞书注册轮询超时",
            "长时间没有拿到 AppID/AppSecret，请重新创建飞书机器人",
        );
        LARK_REGISTRATION_ACTIVE.store(false, Ordering::SeqCst);
    });
}

fn json_string<'a>(value: &'a serde_json::Value, keys: &[&str]) -> String {
    for key in keys {
        if let Some(text) = value.get(*key).and_then(|item| item.as_str()) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
    }
    String::new()
}

fn split_platform_reply_text(content: &str, chunk_size: usize, max_parts: usize) -> Vec<String> {
    let mut text = content.trim().to_string();
    if text.is_empty() {
        text = "Codex 没有返回可发送的文本。".into();
    }
    let mut parts = Vec::new();
    while !text.is_empty() && parts.len() < max_parts {
        let char_count = text.chars().count();
        if char_count <= chunk_size {
            parts.push(text.trim().to_string());
            text.clear();
            break;
        }
        let mut split_at = 0usize;
        for (idx, (byte_index, ch)) in text.char_indices().enumerate() {
            if idx >= chunk_size {
                break;
            }
            if ch == '\n' || ch == '。' || ch == '.' {
                split_at = byte_index + ch.len_utf8();
            }
        }
        if split_at < chunk_size / 2 {
            split_at = text
                .char_indices()
                .nth(chunk_size)
                .map(|(idx, _)| idx)
                .unwrap_or_else(|| text.len());
        }
        let head = text[..split_at].trim().to_string();
        if !head.is_empty() {
            parts.push(head);
        }
        text = text[split_at..].trim().to_string();
    }
    if !text.is_empty() && !parts.is_empty() {
        let suffix = "\n\n[回复过长，已截断]";
        let mut last = parts.pop().unwrap_or_default();
        while last.chars().count() + suffix.chars().count() > chunk_size {
            last.pop();
        }
        parts.push(format!("{}{}", last.trim_end(), suffix));
    }
    if parts.is_empty() {
        vec!["Codex 没有返回可发送的文本。".into()]
    } else {
        parts
    }
}

fn update_mobile_channel_credentials_from_qr(
    app: &tauri::AppHandle,
    channel: &str,
    app_id: &str,
    app_secret: &str,
    bot_token: &str,
    account_id: &str,
    base_url: &str,
    user_id: &str,
) -> Result<ToolboxSnapshot, String> {
    let normalized = normalize_mobile_channel(channel);
    let mut state = read_toolbox_state(app);
    {
        let Some(binding) = state
            .mobile_channels
            .iter_mut()
            .find(|binding| binding.channel == normalized)
        else {
            write_toolbox_state(app, &state)?;
            return Ok(build_toolbox_snapshot(app));
        };
        if !app_id.trim().is_empty() {
            binding.app_id = app_id.trim().to_string();
        }
        if !app_secret.trim().is_empty() {
            binding.app_secret = app_secret.trim().to_string();
        }
        if !bot_token.trim().is_empty() {
            binding.bot_token = bot_token.trim().to_string();
        }
        if !account_id.trim().is_empty() {
            binding.account_id = account_id.trim().to_string();
        }
        if !base_url.trim().is_empty() {
            binding.base_url = normalize_mobile_base_url(base_url, &normalized);
        }
        if !user_id.trim().is_empty() {
            binding.user_id = user_id.trim().to_string();
        }
        binding.qr_status = "绑定成功".into();
        binding.credential_status = "平台凭据已保存".into();
        binding.last_error.clear();
        binding.updated_at = chrono_now();
    }
    attach_selected_thread_to_mobile_channel(&mut state, &normalized);
    write_toolbox_state(app, &state)?;
    Ok(build_toolbox_snapshot(app))
}

fn run_codex_cli_reply(prompt: &str, cwd: &str, thread_id: &str) -> Result<String, String> {
    let text = prompt.trim();
    if text.is_empty() {
        return Err("消息内容为空".into());
    }
    let executable = resolve_codex_command()?;
    let output_path = std::env::temp_dir().join(format!(
        "varswitch-codex-reply-{}-{}.txt",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    let workdir = if cwd.trim().is_empty() {
        std::env::current_dir().unwrap_or_else(|_| home_dir())
    } else {
        PathBuf::from(cwd)
    };
    let mut command = codex_command(&executable);
    command.args([
        "--ask-for-approval",
        "never",
        "exec",
        "--sandbox",
        "workspace-write",
        "--color",
        "never",
        "--output-last-message",
    ]);
    command.arg(&output_path);
    // 选中了已同步的会话时，用 `resume <SESSION_ID>` 续接该会话，
    // 这样手机消息会接到用户在「控制对话」里选择的那个 Codex 线程中，
    // 并携带其完整历史上下文；没有选中线程才退回到全新临时会话。
    let thread = thread_id.trim();
    if thread.is_empty() {
        command.arg("--ephemeral");
    } else {
        command.arg("resume").arg(thread);
    }
    command.arg("-");
    let mut child = command
        .current_dir(workdir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("启动 Codex CLI 失败({}): {e}", executable.display()))?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(text.as_bytes())
            .map_err(|e| format!("写入 Codex CLI prompt 失败: {e}"))?;
    }
    // 关闭 stdin，通知 codex 输入结束，否则 `exec -` 会一直等待。
    drop(child.stdin.take());
    let output = child
        .wait_with_output()
        .map_err(|e| format!("等待 Codex CLI 回复失败: {e}"))?;
    let reply = fs::read_to_string(&output_path)
        .unwrap_or_else(|_| String::from_utf8_lossy(&output.stdout).to_string())
        .trim()
        .to_string();
    let _ = fs::remove_file(&output_path);
    if !output.status.success() {
        let mut detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if detail.is_empty() {
            detail = String::from_utf8_lossy(&output.stdout).trim().to_string();
        }
        return Err(format!("Codex CLI 执行失败: {detail}"));
    }
    if reply.is_empty() {
        // 退出码成功但没有 last-message 文件时，退回用 stdout 内容。
        let stdout_text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !stdout_text.is_empty() {
            return Ok(stdout_text);
        }
        Err("Codex CLI 没有返回回复".into())
    } else {
        Ok(reply)
    }
}

/// 构造执行 Codex 的 Command。对于 .cmd/.bat 包装脚本，Windows 下需通过 `cmd /C` 调用，
/// 否则 Rust 会因为不是合法的 PE 可执行文件而报 "program not found" / "%1 不是有效的应用程序"。
fn codex_command(executable: &std::path::Path) -> Command {
    let ext = executable
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());
    let mut cmd = if cfg!(windows) && matches!(ext.as_deref(), Some("cmd") | Some("bat")) {
        let mut c = Command::new("cmd");
        c.arg("/C").arg(executable);
        c
    } else {
        Command::new(executable)
    };
    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

// ===================== Codex 桌面 App CDP 注入 =====================
//
// 把手机消息注入 Codex 桌面 App(Electron)里用户选中的对话，让 App 自己收发回复。
// 机制:Codex App 带 `--remote-debugging-port` 启动后暴露 Chrome DevTools Protocol，
// 通过 HTTP 拿到页面 target 的 webSocketDebuggerUrl，再用裸 WebSocket 发
// `Runtime.evaluate` 在页面里执行 JS，定位输入框→填消息→点发送→轮询回复。

/// Codex App 常见的调试端口候选。watcher 启动 Codex 时通常用 9229。
const CODEX_CDP_PORTS: &[u16] = &[9229, 9222, 9223, 9230];

/// 探测 Codex App 的调试端口。先扫候选端口(命中 /json/version 即可)，
/// 再退回读取 Codex.exe 进程命令行里的 `--remote-debugging-port=`。
fn codex_debug_port() -> Option<u16> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(800))
        .build()
        .ok()?;
    for &port in CODEX_CDP_PORTS {
        let url = format!("http://127.0.0.1:{port}/json/version");
        if let Ok(resp) = client.get(&url).send() {
            if resp.status().is_success() {
                return Some(port);
            }
        }
    }
    codex_debug_port_from_process()
}

/// 从正在运行的 Codex.exe 命令行里解析 `--remote-debugging-port=<port>`。
#[cfg(windows)]
fn codex_debug_port_from_process() -> Option<u16> {
    let mut cmd = Command::new("powershell");
    cmd.args([
            "-NoProfile",
            "-Command",
            "Get-CimInstance Win32_Process -Filter \"name='Codex.exe'\" | Select-Object -ExpandProperty CommandLine",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let output = cmd.output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let marker = "--remote-debugging-port=";
    for line in text.lines() {
        if let Some(idx) = line.find(marker) {
            let rest = &line[idx + marker.len()..];
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(port) = digits.parse::<u16>() {
                if port > 0 {
                    return Some(port);
                }
            }
        }
    }
    None
}

#[cfg(not(windows))]
fn codex_debug_port_from_process() -> Option<u16> {
    None
}

/// 用于重启的目标调试端口。
const CODEX_PREFERRED_DEBUG_PORT: u16 = 9229;

/// 探测调试端口；若没有，则自动用调试端口重启 Codex App 后再探测。
/// 这样即使用户的 Codex 以普通模式启动(没带 --remote-debugging-port)，也能用手机控制。
fn codex_debug_port_or_relaunch() -> Result<u16, String> {
    if let Some(port) = codex_debug_port() {
        return Ok(port);
    }
    // 没有调试端口 —— 尝试带调试端口重启 Codex App。
    log_info!("[mobile-control][codex-app] 未检测到调试端口，尝试用调试端口重启 Codex App");
    relaunch_codex_with_debug_port(CODEX_PREFERRED_DEBUG_PORT)?;
    // 重启后轮询等待端口起来(最多约 20 秒)。
    for _ in 0..40 {
        std::thread::sleep(Duration::from_millis(500));
        if let Some(port) = codex_debug_port() {
            log_info!("[mobile-control][codex-app] Codex 已带调试端口 {port} 重启成功");
            return Ok(port);
        }
    }
    Err("已尝试重启 Codex App，但调试端口仍未就绪，请手动重启 Codex 后重试".into())
}

/// 从正在运行的 Codex 主进程拿到其可执行文件完整路径(动态获取，不写死安装位置)。
#[cfg(windows)]
fn running_codex_exe_path() -> Option<String> {
    let mut cmd = Command::new("powershell");
    cmd.args([
            "-NoProfile",
            "-Command",
            // 主进程:名为 Codex.exe、命令行不含 --type= (排除 GPU/渲染子进程) 且不在 resources 下。
            "Get-CimInstance Win32_Process -Filter \"name='Codex.exe'\" | Where-Object { $_.CommandLine -notlike '*--type=*' -and $_.CommandLine -notlike '*resources*' } | Select-Object -First 1 -ExpandProperty ExecutablePath",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let output = cmd.output().ok()?;
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        None
    } else {
        Some(path)
    }
}

/// 关闭所有 Codex.exe 进程，再用 `--remote-debugging-port` 重启同一个可执行文件。
#[cfg(windows)]
fn relaunch_codex_with_debug_port(port: u16) -> Result<(), String> {
    // 1) 先拿到当前运行的 Codex.exe 路径(重启后要用同一个版本)。
    let exe_path = running_codex_exe_path()
        .ok_or("未找到正在运行的 Codex App，请先手动打开 Codex 桌面应用")?;
    // 2) 关闭所有 Codex.exe(taskkill /F /T 连子进程一并结束)。
    let mut kill_cmd = Command::new("taskkill");
    kill_cmd.args(["/F", "/T", "/IM", "Codex.exe"])
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(target_os = "windows")]
    kill_cmd.creation_flags(CREATE_NO_WINDOW);
    let _ = kill_cmd.status();
    // 等待进程退出，端口释放。
    std::thread::sleep(Duration::from_millis(1500));
    // 3) 用调试端口重启。--remote-allow-origins 允许本地 CDP 连接。
    let allow_origin = format!("http://127.0.0.1:{port}");
    let mut launch_cmd = Command::new(&exe_path);
    launch_cmd.arg(format!("--remote-debugging-port={port}"))
        .arg(format!("--remote-allow-origins={allow_origin}"))
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(target_os = "windows")]
    launch_cmd.creation_flags(CREATE_NO_WINDOW);
    launch_cmd.spawn()
        .map_err(|e| format!("重启 Codex App 失败({exe_path}): {e}"))?;
    Ok(())
}

#[cfg(not(windows))]
fn relaunch_codex_with_debug_port(_port: u16) -> Result<(), String> {
    Err("当前平台暂不支持自动重启 Codex App".into())
}

/// 在调试端口上找到 Codex 页面 target，返回其 webSocketDebuggerUrl。
fn codex_find_page_target(port: u16) -> Result<String, String> {
    // App 刚重启时端口已就绪但页面可能还没初始化，target 列表会暂时为空，故重试。
    let mut last_err = String::from("没有找到可注入的 Codex 页面");
    for attempt in 0..20 {
        match codex_find_page_target_once(port) {
            Ok(ws) => return Ok(ws),
            Err(e) => {
                last_err = e;
                if attempt < 19 {
                    std::thread::sleep(Duration::from_millis(500));
                }
            }
        }
    }
    Err(format!("{last_err}（请确认 Codex App 已打开且可见）"))
}

fn codex_find_page_target_once(port: u16) -> Result<String, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .map_err(|e| format!("创建 CDP HTTP 客户端失败: {e}"))?;
    let url = format!("http://127.0.0.1:{port}/json");
    let resp = client
        .get(&url)
        .send()
        .map_err(|e| format!("连接 Codex 调试端口失败: {e}"))?;
    let targets: serde_json::Value = resp
        .json()
        .map_err(|e| format!("解析 Codex target 列表失败: {e}"))?;
    let list = targets
        .as_array()
        .ok_or("Codex 调试端口未返回 target 列表")?;
    // 优先选 type==page 的可见页面;Codex 主窗口 url 形如 app://-/index.html。
    let mut fallback: Option<String> = None;
    for target in list {
        let target_type = json_string(target, &["type"]);
        let ws = json_string(target, &["webSocketDebuggerUrl"]);
        if ws.is_empty() {
            continue;
        }
        if target_type == "page" {
            let url = json_string(target, &["url"]);
            let title = json_string(target, &["title"]);
            // 跳过 devtools 自身页面。
            if url.starts_with("devtools://") {
                continue;
            }
            if url.contains("index.html")
                || url.starts_with("app://")
                || title.to_lowercase().contains("codex")
            {
                return Ok(ws);
            }
            if fallback.is_none() {
                fallback = Some(ws);
            }
        }
    }
    fallback.ok_or_else(|| "Codex 页面尚未就绪".into())
}

/// 极简 CDP WebSocket 客户端(手写裸 WebSocket，仅满足注入需求)。
struct CdpClient {
    stream: TcpStream,
    next_id: i64,
}

impl CdpClient {
    /// 连接 webSocketDebuggerUrl(形如 ws://127.0.0.1:9229/devtools/page/<id>)并完成握手。
    fn connect(ws_url: &str) -> Result<CdpClient, String> {
        let rest = ws_url
            .strip_prefix("ws://")
            .ok_or("仅支持 ws:// 的 CDP 地址")?;
        let slash = rest.find('/').unwrap_or(rest.len());
        let host_port = &rest[..slash];
        let path = if slash < rest.len() { &rest[slash..] } else { "/" };
        let (host, port) = match host_port.rsplit_once(':') {
            Some((h, p)) => (h.to_string(), p.parse::<u16>().unwrap_or(9229)),
            None => (host_port.to_string(), 9229),
        };
        let mut stream = TcpStream::connect((host.as_str(), port))
            .map_err(|e| format!("连接 Codex 调试端口失败: {e}"))?;
        stream
            .set_read_timeout(Some(Duration::from_secs(180)))
            .ok();
        stream
            .set_write_timeout(Some(Duration::from_secs(10)))
            .ok();
        // 随机 16 字节做握手 key。
        let key_bytes = uuid::Uuid::new_v4().into_bytes();
        let key = base64_encode_bytes(&key_bytes);
        let request = format!(
            "GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
        );
        stream
            .write_all(request.as_bytes())
            .map_err(|e| format!("发送 WebSocket 握手失败: {e}"))?;
        // 读取握手响应直到空行。
        let mut header = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            let n = stream
                .read(&mut byte)
                .map_err(|e| format!("读取 WebSocket 握手响应失败: {e}"))?;
            if n == 0 {
                return Err("WebSocket 握手连接被关闭".into());
            }
            header.push(byte[0]);
            if header.ends_with(b"\r\n\r\n") {
                break;
            }
            if header.len() > 8192 {
                return Err("WebSocket 握手响应过长".into());
            }
        }
        let header_text = String::from_utf8_lossy(&header);
        if !header_text.contains("101") {
            return Err(format!(
                "WebSocket 握手失败: {}",
                header_text.lines().next().unwrap_or("")
            ));
        }
        Ok(CdpClient {
            stream,
            next_id: 0,
        })
    }

    /// 发送一个带掩码的文本帧(客户端→服务端必须带掩码)。
    fn send_text(&mut self, text: &str) -> Result<(), String> {
        let data = text.as_bytes();
        let len = data.len();
        let mut frame = Vec::with_capacity(len + 14);
        frame.push(0x81); // FIN + 文本帧
        let mask_bit = 0x80u8;
        if len < 126 {
            frame.push(mask_bit | len as u8);
        } else if len < 65536 {
            frame.push(mask_bit | 126);
            frame.extend_from_slice(&(len as u16).to_be_bytes());
        } else {
            frame.push(mask_bit | 127);
            frame.extend_from_slice(&(len as u64).to_be_bytes());
        }
        let mask = uuid::Uuid::new_v4().into_bytes();
        frame.extend_from_slice(&mask[..4]);
        for (i, b) in data.iter().enumerate() {
            frame.push(b ^ mask[i % 4]);
        }
        self.stream
            .write_all(&frame)
            .map_err(|e| format!("发送 CDP 帧失败: {e}"))
    }

    /// 精确读取 n 字节。
    fn read_exact(&mut self, n: usize) -> Result<Vec<u8>, String> {
        let mut buf = vec![0u8; n];
        self.stream
            .read_exact(&mut buf)
            .map_err(|e| format!("读取 CDP 数据失败: {e}"))?;
        Ok(buf)
    }

    /// 读取一个完整帧的 payload(自动应答 ping，跳过非文本控制帧)。
    fn recv_text(&mut self) -> Result<String, String> {
        loop {
            let head = self.read_exact(2)?;
            let opcode = head[0] & 0x0f;
            let masked = (head[1] & 0x80) != 0;
            let mut len = (head[1] & 0x7f) as usize;
            if len == 126 {
                let ext = self.read_exact(2)?;
                len = u16::from_be_bytes([ext[0], ext[1]]) as usize;
            } else if len == 127 {
                let ext = self.read_exact(8)?;
                len = u64::from_be_bytes([
                    ext[0], ext[1], ext[2], ext[3], ext[4], ext[5], ext[6], ext[7],
                ]) as usize;
            }
            let mask = if masked {
                Some(self.read_exact(4)?)
            } else {
                None
            };
            let mut payload = self.read_exact(len)?;
            if let Some(mask) = mask {
                for (i, b) in payload.iter_mut().enumerate() {
                    *b ^= mask[i % 4];
                }
            }
            match opcode {
                0x1 => return Ok(String::from_utf8_lossy(&payload).to_string()),
                0x8 => return Err("CDP WebSocket 已关闭".into()),
                0x9 => {
                    // ping → 回 pong(带掩码空帧即可，简化处理)
                    let _ = self.send_pong(&payload);
                    continue;
                }
                _ => continue, // pong / 续帧等，忽略
            }
        }
    }

    fn send_pong(&mut self, payload: &[u8]) -> Result<(), String> {
        let len = payload.len().min(125);
        let mut frame = Vec::with_capacity(len + 6);
        frame.push(0x8a); // FIN + pong
        frame.push(0x80 | len as u8);
        let mask = uuid::Uuid::new_v4().into_bytes();
        frame.extend_from_slice(&mask[..4]);
        for (i, b) in payload.iter().take(len).enumerate() {
            frame.push(b ^ mask[i % 4]);
        }
        self.stream
            .write_all(&frame)
            .map_err(|e| format!("发送 pong 失败: {e}"))
    }

    /// 发送一条 CDP 命令并等待匹配 id 的响应。
    fn command(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        self.next_id += 1;
        let id = self.next_id;
        let request = serde_json::json!({ "id": id, "method": method, "params": params });
        self.send_text(&request.to_string())?;
        // 循环读取直到拿到本次命令的响应(跳过事件通知)。
        for _ in 0..10000 {
            let text = self.recv_text()?;
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
                continue;
            };
            if value.get("id").and_then(|v| v.as_i64()) == Some(id) {
                if let Some(err) = value.get("error") {
                    return Err(format!("CDP 命令 {method} 失败: {err}"));
                }
                return Ok(value.get("result").cloned().unwrap_or(serde_json::Value::Null));
            }
            // 否则是事件(method 字段)，继续等。
        }
        Err(format!("CDP 命令 {method} 等待响应超时"))
    }
}

/// 在 Codex 页面执行一段 JS 表达式，返回其(已 returnByValue 的)结果值。
fn cdp_evaluate(
    client: &mut CdpClient,
    expression: &str,
) -> Result<serde_json::Value, String> {
    client.command("Runtime.enable", serde_json::json!({}))?;
    let result = client.command(
        "Runtime.evaluate",
        serde_json::json!({
            "expression": expression,
            "returnByValue": true,
            "awaitPromise": true,
            "allowUnsafeEvalBlockedByCSP": true,
            "userGesture": true,
        }),
    )?;
    if let Some(exception) = result.get("exceptionDetails") {
        return Err(format!("Codex 页面脚本异常: {exception}"));
    }
    Ok(result
        .get("result")
        .and_then(|r| r.get("value"))
        .cloned()
        .unwrap_or(serde_json::Value::Null))
}

/// 注入到 Codex 页面的发送脚本(移植自参考实现 send_prompt_to_codex_page_no_wait)。
/// `{PROMPT_JSON}` 处会被替换为 JSON 安全转义后的消息字符串字面量。
/// 逻辑:找输入框 → 写入消息 → 点发送(兜底回车) → 轮询助手回复直到稳定 → 返回 {ok,text}。
fn codex_inject_send_script(prompt_json: &str) -> String {
    const TEMPLATE: &str = r#"
(async () => {
  const prompt = __PROMPT_JSON__;
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const textOf = (n) => (n?.innerText || n?.textContent || "").trim();
  function visible(n) {
    if (!n) return false;
    const s = getComputedStyle(n);
    const r = n.getBoundingClientRect();
    return s.visibility !== "hidden" && s.display !== "none" && r.width > 0 && r.height > 0;
  }
  function assistantTexts() {
    // Codex 桌面 App 的回复渲染在 markdown 内容块里，且不在用户气泡(items-end / bg-token-foreground)内。
    // 用户消息气泡含 items-end 或 bg-token-foreground 类，据此排除。
    function inUserBubble(node) {
      let c = node;
      for (let i = 0; c && i < 12; i++, c = c.parentElement) {
        const cl = (c.className || "").toString();
        if (cl.includes("items-end") || cl.includes("bg-token-foreground")) return true;
      }
      return false;
    }
    const values = [];
    // 优先用 markdown 内容块(每条回复一个 _markdownContent_ 容器)。
    let blocks = Array.from(document.querySelectorAll("[class*='markdownContent'], [class*='markdown-content'], [class*='_markdown']"))
      .filter(visible)
      .filter(n => !inUserBubble(n));
    // 退路:旧版属性选择器。
    if (!blocks.length) {
      blocks = Array.from(document.querySelectorAll("[data-message-author-role='assistant'], [data-role='assistant'], [class*='assistant'], article"))
        .filter(visible);
    }
    for (const n of blocks) {
      const t = textOf(n);
      if (t && !values.includes(t)) values.push(t);
    }
    return values;
  }
  function findInput() {
    const selectors = ["textarea:not([disabled])", "[contenteditable='true']", "[role='textbox']"];
    for (const sel of selectors) {
      const ns = Array.from(document.querySelectorAll(sel)).filter(visible);
      const n = ns[ns.length - 1];
      if (n) return n;
    }
    return null;
  }
  function setInputValue(input, value) {
    input.focus();
    if ("value" in input) {
      // textarea/input: 逐字符模拟打字,确保 React 受控组件每次都触发 onChange。
      const setter = Object.getOwnPropertyDescriptor(input.constructor.prototype, "value")?.set
        || Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")?.set
        || Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set;
      if (setter) {
        setter.call(input, "");
        for (const char of value) {
          const current = input.value;
          setter.call(input, current + char);
          input.dispatchEvent(new InputEvent("input", { bubbles: true, inputType: "insertText", data: char }));
        }
      } else {
        input.value = value;
        input.dispatchEvent(new InputEvent("input", { bubbles: true, inputType: "insertText", data: value }));
      }
      input.dispatchEvent(new Event("change", { bubbles: true }));
    } else {
      // contenteditable: execCommand 逐字符插入。
      input.textContent = "";
      if (document.execCommand) {
        for (const char of value) {
          document.execCommand("insertText", false, char);
        }
      } else {
        input.textContent = value;
      }
      input.dispatchEvent(new InputEvent("input", { bubbles: true, inputType: "insertText", data: value }));
      input.dispatchEvent(new Event("change", { bubbles: true }));
    }
  }
  function buttonLabel(b) {
    return [b.getAttribute("aria-label"), b.getAttribute("title"), b.getAttribute("data-testid"), b.textContent]
      .filter(Boolean).join(" ").toLowerCase();
  }
  function centerDistance(b, rect) {
    const r = b.getBoundingClientRect();
    const dx = Math.max(0, rect.left - r.right, r.left - rect.right);
    const dy = Math.max(0, rect.top - r.bottom, r.top - rect.bottom);
    return dx + dy;
  }
  function findSendButton(input) {
    const buttons = Array.from(document.querySelectorAll("button")).filter(visible);
    const inputRect = input.getBoundingClientRect();
    const labeled = buttons.filter(b => /send|submit|arrow|up|发送|提交/.test(buttonLabel(b)) && !b.disabled);
    if (labeled.length) return labeled.sort((a, b) => centerDistance(a, inputRect) - centerDistance(b, inputRect))[0];
    const containers = [];
    let node = input;
    for (let i = 0; node && i < 6; i++, node = node.parentElement) containers.push(node);
    for (const container of containers) {
      const local = Array.from(container.querySelectorAll("button")).filter(b => visible(b) && !b.disabled);
      const rightSide = local.filter(b => {
        const r = b.getBoundingClientRect();
        return r.left >= inputRect.left && r.top < inputRect.bottom + 80 && r.bottom > inputRect.top - 80;
      });
      if (rightSide.length) return rightSide.sort((a, b) => b.getBoundingClientRect().right - a.getBoundingClientRect().right)[0];
    }
    return buttons.filter(b => !b.disabled).sort((a, b) => centerDistance(a, inputRect) - centerDistance(b, inputRect))[0] || null;
  }
  function activateButton(button) {
    if (!button) return false;
    button.scrollIntoView?.({ block: "center", inline: "center" });
    // 直接调 click(),不触发冗余的鼠标事件链(可能导致重复提交)。
    button.click();
    return true;
  }
  function submitByKeyboard(input) {
    const opts = { key: "Enter", code: "Enter", which: 13, keyCode: 13, bubbles: true, cancelable: true };
    input.dispatchEvent(new KeyboardEvent("keydown", opts));
    input.dispatchEvent(new KeyboardEvent("keypress", opts));
    input.dispatchEvent(new KeyboardEvent("keyup", opts));
  }
  const before = assistantTexts();
  const input = findInput();
  if (!input) return { ok: false, error: "未找到 Codex 输入框" };
  setInputValue(input, prompt);
  let sent = false;
  for (let i = 0; i < 20; i++) {
    await sleep(150);
    const button = findSendButton(input);
    if (button && !button.disabled) { sent = activateButton(button); break; }
  }
  if (!sent) submitByKeyboard(input);
  // 给输入框清空留足时间(逐字符输入+发送后 Codex 清空输入框需要时间)。
  await sleep(1500);
  const startedAt = Date.now();
  const beforeCount = before.length;
  let latest = "";
  let lastChangeAt = 0;
  let everChanged = false;
  while (Date.now() - startedAt < 180000) {
    await sleep(800);
    const after = assistantTexts();
    // 获取新增的所有助手回复块（跳过发送前已存在的块）
    const newBlocks = after.slice(beforeCount);
    const current = newBlocks.join("\n\n").trim();
    // 出现了新回复
    if (current && after.length > beforeCount) {
      everChanged = true;
      if (current !== latest) {
        latest = current;
        lastChangeAt = Date.now();
      }
    }
    if (everChanged && latest) {
      // 以"回复文本连续稳定"为主判据；不再死等 Stop 按钮消失，
      // 因为 Codex 界面可能常驻停止类按钮，会导致永远判不了稳定。
      const busy = Boolean(document.querySelector("[data-testid*='stop-button'], button[aria-label='Stop generating'], button[aria-label='停止生成']"));
      const stableFor = Date.now() - lastChangeAt;
      if (!busy && stableFor > 2500) return { ok: true, text: latest };
      if (busy && stableFor > 8000) return { ok: true, text: latest };
    }
  }
  return { ok: !!latest, text: latest, error: latest ? "" : "Codex 没有产生回复" };
})()
"#;
    TEMPLATE.replace("__PROMPT_JSON__", prompt_json)
}

/// 用 codex://threads/<id> deep link 让 Codex App 切到指定对话，并等待切换完成。
fn activate_codex_thread(thread_id: &str) -> Result<(), String> {
    let id = thread_id.trim();
    if id.is_empty() {
        return Ok(());
    }
    let deep_link = format!("codex://threads/{id}");
    #[cfg(windows)]
    {
        // 用 cmd 的 start 触发系统协议处理(start 的首个引号参数是窗口标题，故留空)。
        let mut cmd = Command::new("cmd");
        cmd.args(["/C", "start", "", &deep_link])
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(target_os = "windows")]
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd.spawn()
            .map_err(|e| format!("打开 Codex 对话失败: {e}"))?;
    }
    #[cfg(not(windows))]
    {
        let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
        Command::new(opener)
            .arg(&deep_link)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("打开 Codex 对话失败: {e}"))?;
    }
    // 等待 App 切换对话并渲染(参考实现的 CODEX_DEEPLINK_SETTLE_SECONDS≈2.5s)。
    std::thread::sleep(Duration::from_millis(2500));
    Ok(())
}

/// 把消息注入 Codex 桌面 App 选中的对话，等待并返回助手回复。
/// 流程:切对话 → 探测调试端口 → 找页面 target → CDP 注入发送脚本 → 解析回复。
fn send_prompt_to_codex_app(thread_id: &str, prompt: &str) -> Result<String, String> {
    let text = prompt.trim();
    if text.is_empty() {
        return Err("消息内容为空".into());
    }
    // 1) 探测调试端口；没有则自动用调试端口重启 Codex App(放在切对话之前，
    //    因为重启会重置页面，切对话必须在重启之后做)。
    let port = codex_debug_port_or_relaunch()?;
    // 2) 切到选中的对话(thread_id 为空则注入当前打开的对话)。
    if let Err(error) = activate_codex_thread(thread_id) {
        log_warn!("[mobile-control][codex-app] 切换对话失败(继续尝试注入当前对话): {error}");
    }
    // 3) 找页面 target。
    let ws_url = codex_find_page_target(port)?;
    // 4) 连接 CDP 并注入。
    let mut client = CdpClient::connect(&ws_url)?;
    let prompt_json = serde_json::to_string(text)
        .map_err(|e| format!("序列化消息失败: {e}"))?;
    let script = codex_inject_send_script(&prompt_json);
    let value = cdp_evaluate(&mut client, &script)?;
    // 4) 解析脚本返回 {ok, text, error}。
    let ok = value.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
    if ok {
        let reply = json_string(&value, &["text"]);
        if reply.trim().is_empty() {
            return Err("Codex App 已发送但未捕获到回复内容".into());
        }
        Ok(reply.trim().to_string())
    } else {
        let error = json_string(&value, &["error"]);
        Err(if error.is_empty() {
            "Codex App 注入失败".into()
        } else {
            error
        })
    }
}

/// 统一的回复分发:优先注入 Codex 桌面 App，失败时降级到后台 CLI(resume 续接)。
fn dispatch_codex_reply(thread_id: &str, text: &str, cwd: &str) -> Result<String, String> {
    match send_prompt_to_codex_app(thread_id, text) {
        Ok(reply) => Ok(reply),
        Err(app_err) => {
            log_warn!("[mobile-control][codex-app] 注入失败，降级到 CLI: {app_err}");
            run_codex_cli_reply(text, cwd, thread_id).map_err(|cli_err| {
                format!("Codex App 注入失败({app_err})；CLI 兜底也失败：{cli_err}")
            })
        }
    }
}



fn lark_tenant_access_token(
    client: &reqwest::blocking::Client,
    binding: &MobileChannelBinding,
) -> Result<String, String> {
    let base_url = normalize_mobile_base_url(&binding.base_url, "lark");
    let result = post_json_value(
        client,
        format!("{base_url}/open-apis/auth/v3/tenant_access_token/internal"),
        serde_json::json!({
            "app_id": binding.app_id.trim(),
            "app_secret": binding.app_secret.trim(),
        }),
    )?;
    let token = json_string(&result, &["tenant_access_token"]);
    if token.is_empty() {
        Err(format!(
            "获取飞书 tenant_access_token 失败: {}",
            platform_error_message(&result, "未返回 token")
        ))
    } else {
        Ok(token)
    }
}

fn send_lark_text_reply(
    binding: &MobileChannelBinding,
    message_id: &str,
    chat_id: &str,
    content: &str,
) -> Result<(), String> {
    let client = build_http_client(20)?;
    let token = lark_tenant_access_token(&client, binding)?;
    let base_url = normalize_mobile_base_url(&binding.base_url, "lark");
    for chunk in split_platform_reply_text(content, 3500, 5) {
        let reply_error: String;
        let payload = serde_json::json!({
            "content": serde_json::json!({ "text": chunk }).to_string(),
            "msg_type": "text"
        });
        let reply_result = client
            .post(format!(
                "{base_url}/open-apis/im/v1/messages/{}/reply",
                percent_encode_query_value(message_id)
            ))
            .bearer_auth(&token)
            .json(&payload)
            .send();
        match reply_result {
            Ok(response) => {
                let status = response.status();
                let body = response
                    .text()
                    .map_err(|e| format!("飞书回复响应读取失败: {e}"))?;
                let result: serde_json::Value =
                    serde_json::from_str(&body).unwrap_or_else(|_| serde_json::json!({}));
                if status.is_success() && is_platform_code_ok(&result) {
                    continue;
                }
                reply_error = if !status.is_success() {
                    format!("飞书 reply 返回 HTTP {}：{}", status.as_u16(), body)
                } else {
                    format!(
                        "飞书 reply 返回错误：{}",
                        platform_error_message(&result, "接口返回异常")
                    )
                };
            }
            Err(error) => {
                reply_error = format!("飞书 reply 请求失败: {error}");
            }
        }
        if chat_id.trim().is_empty() {
            return Err(if reply_error.is_empty() {
                "飞书 reply 失败，且缺少 chat_id，无法改用主动发送".into()
            } else {
                reply_error
            });
        }
        let response = client
            .post(format!(
                "{base_url}/open-apis/im/v1/messages?receive_id_type=chat_id"
            ))
            .bearer_auth(&token)
            .json(&serde_json::json!({
                "receive_id": chat_id.trim(),
                "content": serde_json::json!({ "text": chunk }).to_string(),
                "msg_type": "text"
            }))
            .send()
            .map_err(|e| format!("飞书主动发送失败: {e}; reply_error={reply_error}"))?;
        let status = response.status();
        let body = response
            .text()
            .map_err(|e| format!("飞书主动发送响应读取失败: {e}"))?;
        if !status.is_success() {
            return Err(format!(
                "飞书主动发送返回 HTTP {}：{}；reply_error={}",
                status.as_u16(),
                body,
                reply_error
            ));
        }
        let result: serde_json::Value =
            serde_json::from_str(&body).unwrap_or_else(|_| serde_json::json!({}));
        if !is_platform_code_ok(&result) {
            return Err(format!(
                "飞书主动发送失败: {}；reply_error={}",
                platform_error_message(&result, "接口返回异常"),
                reply_error
            ));
        }
    }
    Ok(())
}

fn qq_access_token(
    client: &reqwest::blocking::Client,
    binding: &MobileChannelBinding,
) -> Result<String, String> {
    let result = post_json_value(
        client,
        "https://bots.qq.com/app/getAppAccessToken".into(),
        serde_json::json!({
            "appId": binding.app_id.trim(),
            "clientSecret": binding.app_secret.trim(),
        }),
    )?;
    let token = json_string(&result, &["access_token"]);
    if token.is_empty() {
        Err(format!(
            "QQ Bot 鉴权失败：{}",
            platform_error_message(&result, "未返回 access_token")
        ))
    } else {
        Ok(token)
    }
}

fn send_qq_text_reply(
    binding: &MobileChannelBinding,
    message: &serde_json::Value,
    content: &str,
) -> Result<(), String> {
    let client = build_http_client(20)?;
    let token = qq_access_token(&client, binding)?;
    let scene = json_string(message, &["scene"]);
    let group_openid = json_string(message, &["groupOpenid", "group_openid"]);
    let openid = json_string(message, &["openid", "openId", "userOpenid"]);
    let message_id = json_string(message, &["messageId", "message_id"]);
    let event_id = json_string(message, &["eventId", "event_id"]);
    let path = if scene == "group" || !group_openid.is_empty() {
        if group_openid.is_empty() {
            return Err("缺少 QQ 群聊 group_openid，无法回复".into());
        }
        format!(
            "https://api.sgroup.qq.com/v2/groups/{}/messages",
            percent_encode_query_value(&group_openid)
        )
    } else {
        if openid.is_empty() {
            return Err("缺少 QQ 用户 openid，无法回复".into());
        }
        format!(
            "https://api.sgroup.qq.com/v2/users/{}/messages",
            percent_encode_query_value(&openid)
        )
    };
    for (index, chunk) in split_platform_reply_text(content, 1800, 5)
        .into_iter()
        .enumerate()
    {
        let mut payload = serde_json::json!({
            "content": chunk,
            "msg_type": 0,
            "msg_seq": ((chrono_timestamp_millis() % 90_000_000) as usize + index + 1),
        });
        if let Some(object) = payload.as_object_mut() {
            if !message_id.is_empty() {
                object.insert("msg_id".into(), serde_json::json!(message_id.clone()));
            }
            if !event_id.is_empty() {
                object.insert("event_id".into(), serde_json::json!(event_id.clone()));
            }
        }
        let response = client
            .post(&path)
            .header("Authorization", format!("QQBot {token}"))
            .json(&payload)
            .send()
            .map_err(|e| format!("QQ 回复发送失败: {e}"))?;
        let status = response.status();
        let body = response
            .text()
            .map_err(|e| format!("QQ 回复响应读取失败: {e}"))?;
        if !status.is_success() {
            return Err(format!("QQ 回复返回 HTTP {}：{}", status.as_u16(), body));
        }
    }
    Ok(())
}

fn wechat_bot_base_info() -> serde_json::Value {
    serde_json::json!({
        "channel_version": "2.4.4",
        "bot_agent": "VarSwitch/1.0"
    })
}

fn wechat_request_json(
    client: &reqwest::blocking::Client,
    method: &str,
    base_url: &str,
    endpoint: &str,
    token: &str,
    payload: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let url = format!(
        "{}/{}",
        normalize_mobile_base_url(base_url, "wechat"),
        endpoint.trim_start_matches('/')
    );
    let request = match method {
        "GET" => client.get(&url),
        _ => client.post(&url).json(&payload),
    }
    .header("AuthorizationType", "ilink_bot_token")
    .header("Authorization", format!("Bearer {}", token.trim()))
    .header("X-WECHAT-UIN", uuid::Uuid::new_v4().to_string())
    .header("iLink-App-Id", "bot")
    .header("iLink-App-ClientVersion", "132100");
    let response = request
        .send()
        .map_err(|e| format!("微信 iLink 请求失败: {e}"))?;
    let status = response.status();
    let body = response
        .text()
        .map_err(|e| format!("微信 iLink 响应读取失败: {e}"))?;
    if !status.is_success() {
        return Err(format!(
            "微信 iLink 返回 HTTP {}：{}",
            status.as_u16(),
            body
        ));
    }
    if body.trim().is_empty() {
        Ok(serde_json::json!({}))
    } else {
        serde_json::from_str(&body).map_err(|e| format!("微信 iLink 返回不是 JSON: {e}; {body}"))
    }
}

fn wechat_json_array<'a>(
    value: &'a serde_json::Value,
    keys: &[&str],
) -> Vec<&'a serde_json::Value> {
    for key in keys {
        if let Some(array) = value.get(*key).and_then(|item| item.as_array()) {
            return array.iter().collect();
        }
    }
    Vec::new()
}

fn normalize_wechat_message_content(data: &serde_json::Value) -> String {
    let source = data
        .get("msg")
        .or_else(|| data.get("message"))
        .filter(|item| item.get("item_list").is_some())
        .unwrap_or(data);
    let mut parts = Vec::new();
    if let Some(items) = source.get("item_list").and_then(|item| item.as_array()) {
        for item in items {
            let item_type = item
                .get("type")
                .and_then(|value| value.as_i64())
                .unwrap_or(0);
            match item_type {
                1 => {
                    let text = item
                        .get("text_item")
                        .and_then(|value| value.get("text"))
                        .and_then(|value| value.as_str())
                        .unwrap_or_default()
                        .trim();
                    if !text.is_empty() {
                        parts.push(text.to_string());
                    }
                }
                2 => {
                    let url = item
                        .get("image_item")
                        .and_then(|value| value.get("media"))
                        .and_then(|value| value.get("full_url"))
                        .and_then(|value| value.as_str())
                        .unwrap_or_default();
                    parts.push(if url.is_empty() {
                        "[微信 图片]".into()
                    } else {
                        format!("[微信 图片: {url}]")
                    });
                }
                3 => parts.push("[微信 语音]".into()),
                4 => {
                    let filename = item
                        .get("file_item")
                        .and_then(|value| value.get("file_name"))
                        .and_then(|value| value.as_str())
                        .unwrap_or_default();
                    parts.push(if filename.is_empty() {
                        "[微信 文件]".into()
                    } else {
                        format!("[微信 文件: {filename}]")
                    });
                }
                5 => parts.push("[微信 视频]".into()),
                _ => {}
            }
        }
    }
    parts.join("\n").trim().to_string()
}

fn parse_wechat_incoming_message(
    data: &serde_json::Value,
    account_id: &str,
) -> Option<serde_json::Value> {
    let source = data
        .get("msg")
        .or_else(|| data.get("message"))
        .filter(|item| item.get("item_list").is_some())
        .unwrap_or(data);
    let content = normalize_wechat_message_content(source);
    let sender_id = json_string(source, &["from_user_id"]);
    if content.is_empty() || sender_id.is_empty() {
        return None;
    }
    Some(serde_json::json!({
        "eventId": json_string(source, &["client_id", "message_id", "seq"]),
        "messageId": json_string(source, &["message_id", "client_id", "seq"]),
        "content": content,
        "senderId": sender_id,
        "contextToken": json_string(source, &["context_token"]),
        "accountId": account_id,
    }))
}

fn send_wechat_text_reply(
    binding: &MobileChannelBinding,
    message: &serde_json::Value,
    content: &str,
) -> Result<(), String> {
    let client = build_http_client(20)?;
    let sender_id = json_string(message, &["senderId", "sender_id"]);
    if sender_id.is_empty() {
        return Err("缺少微信发送者 ID，无法回复".into());
    }
    let context_token = json_string(message, &["contextToken", "context_token"]);
    for chunk in split_platform_reply_text(content, 1800, 5) {
        let result = wechat_request_json(
            &client,
            "POST",
            &binding.base_url,
            "ilink/bot/sendmessage",
            &binding.bot_token,
            serde_json::json!({
                    "msg": {
                        "from_user_id": "",
                    "to_user_id": sender_id.clone(),
                    "client_id": format!("varswitch-wechat:{}-{}", chrono_timestamp_millis(), uuid::Uuid::new_v4()),
                    "message_type": 2,
                    "message_state": 2,
                    "item_list": [{
                        "type": 1,
                        "text_item": { "text": chunk }
                    }],
                    "context_token": if context_token.is_empty() { serde_json::Value::Null } else { serde_json::json!(context_token.clone()) }
                },
                "base_info": wechat_bot_base_info(),
            }),
        )?;
        if !is_platform_code_ok(&result) {
            return Err(format!(
                "微信回复发送失败：{}",
                platform_error_message(&result, "接口返回异常")
            ));
        }
    }
    Ok(())
}

fn update_channel_status(app: &tauri::AppHandle, channel: &str, status: &str, error: &str) {
    if error.trim().is_empty() {
        log_info!("[mobile-control][status][{}] {}", channel, status);
    } else {
        log_info!(
            "[mobile-control][status][{}] {} | error={}",
            channel, status, error
        );
    }
    let normalized = normalize_mobile_channel(channel);
    let mut state = read_toolbox_state(app);
    if let Some(binding) = state
        .mobile_channels
        .iter_mut()
        .find(|binding| binding.channel == normalized)
    {
        binding.status = status.to_string();
        binding.last_error = error.to_string();
        binding.updated_at = chrono_now();
    }
    let _ = write_toolbox_state(app, &state);
}

fn handle_lark_bridge_message(
    app: &tauri::AppHandle,
    payload: &serde_json::Value,
) -> Result<(), String> {
    let message_id = json_string(payload, &["messageId", "message_id"]);
    let chat_id = json_string(payload, &["chatId", "chat_id"]);
    let text = json_string(payload, &["text", "content"]);
    if message_id.is_empty() || text.is_empty() {
        log_info!(
            "[mobile-control][lark] ignored empty message: message_id_empty={}, text_empty={}",
            message_id.is_empty(),
            text.is_empty()
        );
        return Ok(());
    }
    log_info!(
        "[mobile-control][lark] received message: message_id={}, text_len={}",
        message_id,
        text.chars().count()
    );
    let state = read_toolbox_state(app);
    let binding = state
        .mobile_channels
        .iter()
        .find(|binding| binding.channel == "lark")
        .cloned()
        .ok_or("飞书通道不存在")?;
    let selected_thread = state
        .synced_codex_threads
        .iter()
        .find(|thread| {
            thread.id == binding.thread_id || thread.id == state.selected_mobile_thread_id
        })
        .cloned();
    let cwd = selected_thread
        .as_ref()
        .map(|thread| thread.cwd.as_str())
        .unwrap_or("");
    log_info!(
        "[mobile-control][lark] dispatching to codex: thread_id={}, thread_name={}, cwd={}",
        selected_thread
            .as_ref()
            .map(|thread| thread.id.as_str())
            .unwrap_or(""),
        selected_thread
            .as_ref()
            .map(|thread| thread.thread_name.as_str())
            .unwrap_or(""),
        cwd
    );
    update_channel_status(app, "lark", "收到飞书消息，正在发送给 Codex", "");
    let thread_id = selected_thread
        .as_ref()
        .map(|thread| thread.id.as_str())
        .unwrap_or("");
    let reply = dispatch_codex_reply(thread_id, &text, cwd)
        .map_err(|error| format!("Codex 执行失败：{error}"))?;
    log_info!(
        "[mobile-control][lark] codex replied: message_id={}, reply_len={}",
        message_id,
        reply.chars().count()
    );
    update_channel_status(app, "lark", "Codex 已回复，正在同步回飞书", "");
    send_lark_text_reply(&binding, &message_id, &chat_id, &reply)
        .map_err(|error| format!("飞书回发失败：{error}"))?;
    update_channel_status(app, "lark", "Codex 回复已同步回飞书", "");
    Ok(())
}

fn start_lark_bridge(app: tauri::AppHandle, binding: MobileChannelBinding) -> Result<(), String> {
    if LARK_BRIDGE_ACTIVE.swap(true, Ordering::SeqCst) {
        log_info!("[mobile-control][lark] bridge already active, skip start");
        return Ok(());
    }
    log_info!(
        "[mobile-control][lark] starting bridge: thread_id={}, thread_name={}, app_id={}",
        binding.thread_id, binding.thread_name, binding.app_id
    );
    let runner = ensure_lark_bridge_connector(&app)?;
    let node = node_command_name()?;
    let connector_dir = runner
        .parent()
        .ok_or("飞书 connector 运行目录不存在")?
        .to_path_buf();
    let mut cmd = Command::new(node);
    cmd.arg(&runner)
        .current_dir(connector_dir)
        .env("LARK_APP_ID", binding.app_id.trim())
        .env("LARK_APP_SECRET", binding.app_secret.trim())
        .env(
            "LARK_BASE_URL",
            normalize_mobile_base_url(&binding.base_url, "lark"),
        )
        .env("LARK_BOT_OPEN_ID", binding.bot_open_id.trim())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let mut child = cmd.spawn()
        .map_err(|e| {
            LARK_BRIDGE_ACTIVE.store(false, Ordering::SeqCst);
            format!("启动飞书消息桥失败: {e}")
        })?;
    let stdout = child.stdout.take().ok_or("飞书消息桥没有输出通道")?;
    let stderr = child.stderr.take();
    if let Ok(mut guard) = LARK_BRIDGE_CHILD.lock() {
        *guard = Some(child);
    }
    if let Some(stderr) = stderr {
        std::thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().flatten() {
                log_info!("[mobile-control][lark][stderr] {}", line);
            }
        });
    }
    std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().flatten() {
            if !LARK_BRIDGE_ACTIVE.load(Ordering::SeqCst) {
                break;
            }
            let Ok(payload) = serde_json::from_str::<serde_json::Value>(&line) else {
                log_info!("[mobile-control][lark][stdout] {}", line);
                continue;
            };
            log_info!("[mobile-control][lark][event] {}", payload);
            match json_string(&payload, &["type"]).as_str() {
                "ready" => {
                    update_channel_status(&app, "lark", "飞书智能体已在线，等待手机消息", "")
                }
                "message" => {
                    if let Err(error) = handle_lark_bridge_message(&app, &payload) {
                        update_channel_status(&app, "lark", "飞书消息处理失败", &error);
                    }
                }
                "status" => {
                    let message = json_string(&payload, &["message"]);
                    update_channel_status(&app, "lark", &message, "");
                }
                "failure" => {
                    let error = json_string(&payload, &["message"]);
                    update_channel_status(&app, "lark", "飞书消息桥启动失败", &error);
                }
                _ => {}
            }
        }
        LARK_BRIDGE_ACTIVE.store(false, Ordering::SeqCst);
        if let Ok(mut guard) = LARK_BRIDGE_CHILD.lock() {
            let _ = guard.take().map(|mut child| child.wait());
        }
        log_info!("[mobile-control][lark] bridge worker exited");
    });
    Ok(())
}

fn stop_lark_bridge() {
    log_info!("[mobile-control][lark] stopping bridge");
    LARK_BRIDGE_ACTIVE.store(false, Ordering::SeqCst);
    if let Ok(mut guard) = LARK_BRIDGE_CHILD.lock() {
        if let Some(mut child) = guard.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn selected_thread_for_message(
    state: &ToolboxState,
    binding: &MobileChannelBinding,
) -> Option<CodexThreadRecord> {
    state
        .synced_codex_threads
        .iter()
        .find(|thread| {
            thread.id == binding.thread_id || thread.id == state.selected_mobile_thread_id
        })
        .cloned()
}

fn handle_qq_gateway_message(
    app: &tauri::AppHandle,
    payload: &serde_json::Value,
) -> Result<(), String> {
    let text = json_string(payload, &["content", "text"]);
    if text.is_empty() {
        return Ok(());
    }
    let state = read_toolbox_state(app);
    let binding = state
        .mobile_channels
        .iter()
        .find(|binding| binding.channel == "qq")
        .cloned()
        .ok_or("QQ 通道不存在")?;
    let selected_thread = selected_thread_for_message(&state, &binding);
    let cwd = selected_thread
        .as_ref()
        .map(|thread| thread.cwd.clone())
        .unwrap_or_default();
    let thread_id = selected_thread
        .as_ref()
        .map(|thread| thread.id.as_str())
        .unwrap_or("");
    update_channel_status(app, "qq", "收到 QQ 消息，正在发送给 Codex", "");
    let reply = dispatch_codex_reply(thread_id, &text, &cwd)?;
    send_qq_text_reply(&binding, payload, &reply)?;
    update_channel_status(app, "qq", "Codex 回复已同步回 QQ", "");
    Ok(())
}

fn start_qq_gateway(app: tauri::AppHandle, binding: MobileChannelBinding) -> Result<(), String> {
    if QQ_GATEWAY_ACTIVE.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    let runner = ensure_qq_gateway_connector(&app).map_err(|error| {
        QQ_GATEWAY_ACTIVE.store(false, Ordering::SeqCst);
        error
    })?;
    let node = node_command_name().map_err(|error| {
        QQ_GATEWAY_ACTIVE.store(false, Ordering::SeqCst);
        error
    })?;
    let connector_dir = runner
        .parent()
        .ok_or("QQ 网关运行目录不存在")?
        .to_path_buf();
    let mut cmd = Command::new(node);
    cmd.arg(&runner)
        .current_dir(connector_dir)
        .env("QQ_APP_ID", binding.app_id.trim())
        .env("QQ_APP_SECRET", binding.app_secret.trim())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let mut child = cmd.spawn()
        .map_err(|e| {
            QQ_GATEWAY_ACTIVE.store(false, Ordering::SeqCst);
            format!("启动 QQ 网关失败: {e}")
        })?;
    let stdout = child.stdout.take().ok_or("QQ 网关没有输出通道")?;
    if let Ok(mut guard) = QQ_GATEWAY_CHILD.lock() {
        *guard = Some(child);
    }
    std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().flatten() {
            if !QQ_GATEWAY_ACTIVE.load(Ordering::SeqCst) {
                break;
            }
            let Ok(payload) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            match json_string(&payload, &["type"]).as_str() {
                "ready" => update_channel_status(&app, "qq", "QQ 机器人已在线，等待手机消息", ""),
                "status" => {
                    let message = json_string(&payload, &["message"]);
                    update_channel_status(&app, "qq", &message, "");
                }
                "message" => {
                    if let Err(error) = handle_qq_gateway_message(&app, &payload) {
                        update_channel_status(&app, "qq", "QQ 消息处理失败", &error);
                    }
                }
                "failure" => {
                    let error = json_string(&payload, &["message"]);
                    update_channel_status(&app, "qq", "QQ 网关启动失败", &error);
                }
                _ => {}
            }
        }
        QQ_GATEWAY_ACTIVE.store(false, Ordering::SeqCst);
        if let Ok(mut guard) = QQ_GATEWAY_CHILD.lock() {
            let _ = guard.take().map(|mut child| child.wait());
        }
    });
    Ok(())
}

fn stop_qq_gateway() {
    QQ_GATEWAY_ACTIVE.store(false, Ordering::SeqCst);
    if let Ok(mut guard) = QQ_GATEWAY_CHILD.lock() {
        if let Some(mut child) = guard.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn wechat_next_update_cursor(result: &serde_json::Value, previous: &str) -> String {
    for source in [
        Some(result),
        result.get("data"),
        result.get("result"),
        result.get("body"),
    ]
    .into_iter()
    .flatten()
    {
        let value = json_string(
            source,
            &[
                "get_updates_buf",
                "getUpdatesBuf",
                "next_get_updates_buf",
                "nextGetUpdatesBuf",
                "next_buf",
                "nextBuf",
                "buf",
            ],
        );
        if !value.is_empty() {
            return value;
        }
    }
    previous.to_string()
}

fn wechat_update_items(result: &serde_json::Value) -> Vec<serde_json::Value> {
    let mut items = Vec::new();
    for source in [
        Some(result),
        result.get("data"),
        result.get("result"),
        result.get("body"),
    ]
    .into_iter()
    .flatten()
    {
        for item in wechat_json_array(
            source,
            &[
                "msgs",
                "msg_list",
                "msgList",
                "message_list",
                "messageList",
                "messages",
                "updates",
                "update_list",
                "updateList",
            ],
        ) {
            items.push(item.clone());
        }
        if source.get("item_list").is_some() || source.get("msg").is_some() {
            items.push(source.clone());
        }
    }
    items
}

fn handle_wechat_bot_message(
    app: &tauri::AppHandle,
    payload: &serde_json::Value,
) -> Result<(), String> {
    let text = json_string(payload, &["content", "text"]);
    if text.is_empty() {
        return Ok(());
    }
    let state = read_toolbox_state(app);
    let binding = state
        .mobile_channels
        .iter()
        .find(|binding| binding.channel == "wechat")
        .cloned()
        .ok_or("微信通道不存在")?;
    let selected_thread = selected_thread_for_message(&state, &binding);
    let cwd = selected_thread
        .as_ref()
        .map(|thread| thread.cwd.clone())
        .unwrap_or_default();
    let thread_id = selected_thread
        .as_ref()
        .map(|thread| thread.id.as_str())
        .unwrap_or("");
    update_channel_status(app, "wechat", "收到微信消息，正在发送给 Codex", "");
    let reply = dispatch_codex_reply(thread_id, &text, &cwd)?;
    send_wechat_text_reply(&binding, payload, &reply)?;
    update_channel_status(app, "wechat", "Codex 回复已同步回微信", "");
    Ok(())
}

fn start_wechat_listener(
    app: tauri::AppHandle,
    binding: MobileChannelBinding,
) -> Result<(), String> {
    if WECHAT_LISTENER_ACTIVE.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    std::thread::spawn(move || {
        let client = match build_http_client(45) {
            Ok(client) => client,
            Err(error) => {
                update_channel_status(&app, "wechat", "微信监听启动失败", &error);
                WECHAT_LISTENER_ACTIVE.store(false, Ordering::SeqCst);
                return;
            }
        };
        match wechat_request_json(
            &client,
            "POST",
            &binding.base_url,
            "ilink/bot/msg/notifystart",
            &binding.bot_token,
            serde_json::json!({ "base_info": wechat_bot_base_info() }),
        ) {
            Ok(result) if is_platform_code_ok(&result) => {
                update_channel_status(&app, "wechat", "微信 iLink 已在线，等待手机消息", "")
            }
            Ok(result) => {
                let err_msg = platform_error_message(&result, "接口返回异常");
                // 检测 token 过期
                if err_msg.to_lowercase().contains("session timeout")
                    || err_msg.to_lowercase().contains("expired")
                    || err_msg.to_lowercase().contains("token")
                {
                    update_channel_status(
                        &app,
                        "wechat",
                        "微信 token 已过期，请清除绑定后重新扫码",
                        &format!("错误详情：{}", err_msg),
                    );
                } else {
                    update_channel_status(
                        &app,
                        "wechat",
                        "微信监听启动失败",
                        &err_msg,
                    );
                }
                WECHAT_LISTENER_ACTIVE.store(false, Ordering::SeqCst);
                return;
            }
            Err(error) => {
                update_channel_status(&app, "wechat", "微信监听启动失败", &error);
                WECHAT_LISTENER_ACTIVE.store(false, Ordering::SeqCst);
                return;
            }
        }
        let mut cursor = String::new();
        let mut seen = HashSet::new();
        while WECHAT_LISTENER_ACTIVE.load(Ordering::SeqCst) {
            let result = match wechat_request_json(
                &client,
                "POST",
                &binding.base_url,
                "ilink/bot/getupdates",
                &binding.bot_token,
                serde_json::json!({
                    "get_updates_buf": cursor,
                    "base_info": wechat_bot_base_info(),
                }),
            ) {
                Ok(result) => result,
                Err(error) => {
                    update_channel_status(&app, "wechat", "微信消息拉取失败", &error);
                    std::thread::sleep(Duration::from_secs(3));
                    continue;
                }
            };
            cursor = wechat_next_update_cursor(&result, &cursor);
            for item in wechat_update_items(&result) {
                let Some(message) = parse_wechat_incoming_message(&item, &binding.account_id)
                else {
                    continue;
                };
                let key = json_string(&message, &["messageId", "eventId"]);
                if !key.is_empty() && !seen.insert(key.clone()) {
                    continue;
                }
                if seen.len() > 300 {
                    seen.clear();
                }
                if let Err(error) = handle_wechat_bot_message(&app, &message) {
                    update_channel_status(&app, "wechat", "微信消息处理失败", &error);
                }
            }
        }
        let _ = wechat_request_json(
            &client,
            "POST",
            &binding.base_url,
            "ilink/bot/msg/notifystop",
            &binding.bot_token,
            serde_json::json!({ "base_info": wechat_bot_base_info() }),
        );
        update_channel_status(&app, "wechat", "微信监听已停止", "");
        WECHAT_LISTENER_ACTIVE.store(false, Ordering::SeqCst);
    });
    Ok(())
}

fn stop_wechat_listener(app: &tauri::AppHandle) {
    WECHAT_LISTENER_ACTIVE.store(false, Ordering::SeqCst);
    let state = read_toolbox_state(app);
    if let Some(binding) = state
        .mobile_channels
        .iter()
        .find(|binding| binding.channel == "wechat" && !binding.bot_token.trim().is_empty())
    {
        if let Ok(client) = build_http_client(10) {
            let _ = wechat_request_json(
                &client,
                "POST",
                &binding.base_url,
                "ilink/bot/msg/notifystop",
                &binding.bot_token,
                serde_json::json!({ "base_info": wechat_bot_base_info() }),
            );
        }
    }
}

fn build_toolbox_snapshot(app: &tauri::AppHandle) -> ToolboxSnapshot {
    let state = read_toolbox_state(app);
    let config_path = codex_config_path();
    let config_text = fs::read_to_string(&config_path).unwrap_or_default();
    let current_source = state.plugin_marketplace_input.clone();
    ToolboxSnapshot {
        plugin_marketplace_input: current_source.clone(),
        plugin_marketplaces: list_plugin_marketplaces(&config_text, &current_source),
        session_bindings: state.session_bindings.clone(),
        codex_threads: Vec::new(),
        synced_codex_threads: state.synced_codex_threads.clone(),
        session_sync: state.session_sync.clone(),
        mobile_channels: state.mobile_channels.clone(),
        selected_mobile_thread_id: state.selected_mobile_thread_id.clone(),
        mobile_remote: state.mobile_remote.clone(),
        codex_home: codex_config_dir().to_string_lossy().to_string(),
        codex_config_path: config_path.to_string_lossy().to_string(),
    }
}

/// 编辑器信息
struct EditorDef {
    /// 唯一标识 (如 "vscode", "cursor")
    id: &'static str,
    /// 显示名称
    display_name: &'static str,
    /// Windows 下 %APPDATA% 内的子目录名
    #[cfg(target_os = "windows")]
    win_appdata_dir: &'static str,
    #[cfg(target_os = "windows")]
    win_program_dirs: &'static [&'static str],
    /// macOS 下 ~/Library/Application Support/ 内的子目录名
    #[cfg(target_os = "macos")]
    mac_app_support_dir: &'static str,
    /// Linux 下 ~/.config/ 内的子目录名
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    linux_config_dir: &'static str,
}

/// 所有支持的编辑器定义
#[cfg(target_os = "windows")]
const KNOWN_EDITORS: &[EditorDef] = &[
    EditorDef {
        id: "vscode",
        display_name: "VS Code",
        win_appdata_dir: "Code",
        win_program_dirs: &["Microsoft VS Code"],
    },
    EditorDef {
        id: "vscode-insiders",
        display_name: "VS Code Insiders",
        win_appdata_dir: "Code - Insiders",
        win_program_dirs: &["Microsoft VS Code Insiders"],
    },
    EditorDef {
        id: "cursor",
        display_name: "Cursor",
        win_appdata_dir: "Cursor",
        win_program_dirs: &["Cursor"],
    },
    EditorDef {
        id: "windsurf",
        display_name: "Windsurf",
        win_appdata_dir: "Windsurf",
        win_program_dirs: &["Windsurf"],
    },
    EditorDef {
        id: "trae",
        display_name: "Trae",
        win_appdata_dir: "Trae",
        win_program_dirs: &["Trae"],
    },
    EditorDef {
        id: "vscodium",
        display_name: "VSCodium",
        win_appdata_dir: "VSCodium",
        win_program_dirs: &["VSCodium"],
    },
];

#[cfg(target_os = "macos")]
const KNOWN_EDITORS: &[EditorDef] = &[
    EditorDef {
        id: "vscode",
        display_name: "VS Code",
        mac_app_support_dir: "Code",
    },
    EditorDef {
        id: "vscode-insiders",
        display_name: "VS Code Insiders",
        mac_app_support_dir: "Code - Insiders",
    },
    EditorDef {
        id: "cursor",
        display_name: "Cursor",
        mac_app_support_dir: "Cursor",
    },
    EditorDef {
        id: "windsurf",
        display_name: "Windsurf",
        mac_app_support_dir: "Windsurf",
    },
    EditorDef {
        id: "trae",
        display_name: "Trae",
        mac_app_support_dir: "Trae",
    },
    EditorDef {
        id: "vscodium",
        display_name: "VSCodium",
        mac_app_support_dir: "VSCodium",
    },
];

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
const KNOWN_EDITORS: &[EditorDef] = &[
    EditorDef {
        id: "vscode",
        display_name: "VS Code",
        linux_config_dir: "Code",
    },
    EditorDef {
        id: "vscode-insiders",
        display_name: "VS Code Insiders",
        linux_config_dir: "Code - Insiders",
    },
    EditorDef {
        id: "cursor",
        display_name: "Cursor",
        linux_config_dir: "Cursor",
    },
    EditorDef {
        id: "windsurf",
        display_name: "Windsurf",
        linux_config_dir: "Windsurf",
    },
    EditorDef {
        id: "trae",
        display_name: "Trae",
        linux_config_dir: "Trae",
    },
    EditorDef {
        id: "vscodium",
        display_name: "VSCodium",
        linux_config_dir: "VSCodium",
    },
];

/// 获取编辑器 settings.json 的路径
fn default_editor_settings_path(editor: &EditorDef) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        let appdata = std::env::var("APPDATA").unwrap_or_default();
        PathBuf::from(appdata)
            .join(editor.win_appdata_dir)
            .join("User")
            .join("settings.json")
    }
    #[cfg(target_os = "macos")]
    {
        home_dir()
            .join("Library")
            .join("Application Support")
            .join(editor.mac_app_support_dir)
            .join("User")
            .join("settings.json")
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        home_dir()
            .join(".config")
            .join(editor.linux_config_dir)
            .join("User")
            .join("settings.json")
    }
}

fn normalize_editor_path_value(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let path = PathBuf::from(trimmed);
    let normalized = if path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("json"))
        .unwrap_or(false)
    {
        path
    } else {
        path.join("settings.json")
    };

    Some(normalized.to_string_lossy().to_string())
}

fn normalize_app_settings(mut settings: AppSettings) -> AppSettings {
    let mut normalized_paths = HashMap::new();
    for (editor_id, raw_path) in settings.editor_paths {
        if let Some(path) = normalize_editor_path_value(&raw_path) {
            normalized_paths.insert(editor_id, path);
        }
    }
    settings.editor_paths = normalized_paths;
    settings
}

fn editor_override_path(settings: &AppSettings, editor_id: &str) -> Option<PathBuf> {
    settings
        .editor_paths
        .get(editor_id)
        .and_then(|raw| normalize_editor_path_value(raw))
        .map(PathBuf::from)
}

fn resolved_editor_settings_path(editor: &EditorDef, settings: &AppSettings) -> PathBuf {
    editor_override_path(settings, editor.id)
        .unwrap_or_else(|| default_editor_settings_path(editor))
}

fn editor_has_custom_path(settings: &AppSettings, editor_id: &str) -> bool {
    editor_override_path(settings, editor_id).is_some()
}

fn editor_install_markers(editor: &EditorDef) -> Vec<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let mut markers = Vec::new();
        let appdata = std::env::var("APPDATA").unwrap_or_default();
        if !appdata.is_empty() {
            markers.push(PathBuf::from(&appdata).join(editor.win_appdata_dir));
        }
        let local_appdata = std::env::var("LOCALAPPDATA").unwrap_or_default();
        if !local_appdata.is_empty() {
            for dir in editor.win_program_dirs {
                markers.push(PathBuf::from(&local_appdata).join("Programs").join(dir));
            }
        }
        markers
    }
    #[cfg(target_os = "macos")]
    {
        vec![home_dir()
            .join("Library")
            .join("Application Support")
            .join(editor.mac_app_support_dir)]
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        vec![home_dir().join(".config").join(editor.linux_config_dir)]
    }
}

fn editor_is_detected(editor: &EditorDef, settings: &AppSettings) -> bool {
    if editor_has_custom_path(settings, editor.id) {
        return true;
    }

    let default_path = default_editor_settings_path(editor);
    if default_path.exists() {
        return true;
    }

    if default_path
        .parent()
        .map(|parent| parent.exists())
        .unwrap_or(false)
    {
        return true;
    }

    editor_install_markers(editor)
        .into_iter()
        .any(|path| path.exists())
}

fn detect_installed_editors(settings: &AppSettings) -> Vec<&'static EditorDef> {
    KNOWN_EDITORS
        .iter()
        .filter(|editor| editor_is_detected(editor, settings))
        .collect()
}

fn collect_editor_path_infos(settings: &AppSettings) -> Vec<EditorPathInfo> {
    KNOWN_EDITORS
        .iter()
        .map(|editor| EditorPathInfo {
            id: editor.id.to_string(),
            display_name: editor.display_name.to_string(),
            settings_path: resolved_editor_settings_path(editor, settings)
                .to_string_lossy()
                .to_string(),
            default_path: default_editor_settings_path(editor)
                .to_string_lossy()
                .to_string(),
            customized: editor_has_custom_path(settings, editor.id),
            detected: editor_is_detected(editor, settings),
        })
        .collect()
}

fn claude_commands_dir() -> PathBuf {
    home_dir().join(".claude").join("commands")
}

fn claude_skills_dir() -> PathBuf {
    home_dir().join(".claude").join("skills")
}

/// 从 SKILL.md 的 YAML frontmatter 中解析 description
fn parse_skill_description(content: &str) -> String {
    if !content.starts_with("---") {
        return String::new();
    }
    // 找到第二个 "---"
    if let Some(end) = content[3..].find("---") {
        let frontmatter = &content[3..3 + end];
        for line in frontmatter.lines() {
            let line = line.trim();
            if line.starts_with("description:") {
                return line["description:".len()..].trim().to_string();
            }
        }
    }
    String::new()
}

/// 收集 ~/.claude/skills/ 下的 SKILL.md 文件
fn collect_skills_from_skills_dir(skills: &mut Vec<SkillInfo>) {
    let dir = claude_skills_dir();
    if !dir.exists() {
        return;
    }
    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let skill_md = path.join("SKILL.md");
        if !skill_md.exists() {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        if name.is_empty() || name == "README.md" {
            continue;
        }
        let content = fs::read_to_string(&skill_md).unwrap_or_default();
        let description = parse_skill_description(&content);
        skills.push(SkillInfo {
            name,
            content,
            source_type: "skill".into(),
            description,
        });
    }
}

fn claude_md_path() -> PathBuf {
    home_dir().join(".claude").join("CLAUDE.md")
}

fn claude_mcp_path() -> PathBuf {
    home_dir().join(".claude.json")
}

fn read_json(path: &PathBuf) -> Result<serde_json::Value, String> {
    let s = fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&s).map_err(|e| e.to_string())
}

/// 读取 JSON 文件，如果不存在则返回默认值
fn read_json_or_default(path: &PathBuf, default: serde_json::Value) -> serde_json::Value {
    read_json(path).unwrap_or(default)
}

fn write_json(path: &PathBuf, val: &serde_json::Value) -> Result<(), String> {
    // 自动创建父目录
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let s = serde_json::to_string_pretty(val).map_err(|e| e.to_string())?;
    fs::write(path, s).map_err(|e| e.to_string())
}

// ── Registry-based env var operations (fast, no PowerShell) ──

#[cfg(target_os = "windows")]
fn env_reg_key() -> Result<RegKey, String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    hkcu.create_subkey("Environment")
        .map(|(key, _)| key)
        .map_err(|e| e.to_string())
}

#[cfg(target_os = "windows")]
fn reg_set_env(name: &str, value: &str) -> Result<(), String> {
    let key = env_reg_key()?;
    key.set_value(name, &value).map_err(|e| e.to_string())
}

#[cfg(not(target_os = "windows"))]
fn shell_rc_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join(".zshrc")
}

/// 从 shell 配置文件中读取 VarSwitch 管理的环境变量值
#[cfg(not(target_os = "windows"))]
fn shell_rc_get_env(name: &str) -> Option<String> {
    let rc = shell_rc_path();
    let content = fs::read_to_string(&rc).ok()?;
    // 查找格式: export NAME="value" # VarSwitch-managed
    let prefix = format!("export {}=\"", name);
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with(&prefix) && trimmed.contains("# VarSwitch-managed") {
            // 提取引号内的值
            let after_prefix = &trimmed[prefix.len()..];
            if let Some(end_quote) = after_prefix.find('"') {
                return Some(after_prefix[..end_quote].to_string());
            }
        }
    }
    None
}

/// 在 shell 配置文件中设置环境变量（带 VarSwitch-managed 标记）
#[cfg(not(target_os = "windows"))]
fn shell_rc_set_env(name: &str, value: &str) -> Result<(), String> {
    let rc = shell_rc_path();
    let content = fs::read_to_string(&rc).unwrap_or_default();
    let marker = format!("export {}=\"", name);
    // 过滤掉旧的同名行（仅删除 VarSwitch 管理的行）
    let mut lines: Vec<&str> = content
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !(trimmed.starts_with(&marker) && trimmed.contains("# VarSwitch-managed"))
        })
        .collect();
    // 添加新行
    let new_line = format!("export {}=\"{}\" # VarSwitch-managed", name, value);
    lines.push(&new_line);
    // 确保文件末尾有换行
    let mut result = lines.join("\n");
    if !result.ends_with('\n') {
        result.push('\n');
    }
    fs::write(&rc, result).map_err(|e| e.to_string())
}

/// 从 shell 配置文件中删除 VarSwitch 管理的环境变量
#[cfg(not(target_os = "windows"))]
fn shell_rc_delete_env(name: &str) -> Result<(), String> {
    let rc = shell_rc_path();
    let content = match fs::read_to_string(&rc) {
        Ok(c) => c,
        Err(_) => return Ok(()), // 文件不存在则无需删除
    };
    let marker = format!("export {}=\"", name);
    let lines: Vec<&str> = content
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !(trimmed.starts_with(&marker) && trimmed.contains("# VarSwitch-managed"))
        })
        .collect();
    let mut result = lines.join("\n");
    if !result.ends_with('\n') && !result.is_empty() {
        result.push('\n');
    }
    fs::write(&rc, result).map_err(|e| e.to_string())
}

#[cfg(not(target_os = "windows"))]
fn reg_set_env(name: &str, value: &str) -> Result<(), String> {
    // 同时设置进程内环境变量和持久化到 shell 配置文件
    std::env::set_var(name, value);
    shell_rc_set_env(name, value)
}

#[cfg(target_os = "windows")]
fn reg_get_env_opt(name: &str) -> Option<String> {
    let key = env_reg_key().ok()?;
    key.get_value::<String, _>(name).ok()
}

#[cfg(not(target_os = "windows"))]
fn reg_get_env_opt(name: &str) -> Option<String> {
    // 优先从 shell 配置文件读取持久化的值，回退到进程环境变量
    shell_rc_get_env(name).or_else(|| std::env::var(name).ok())
}

fn reg_get_env(name: &str) -> String {
    reg_get_env_opt(name).unwrap_or_default()
}

#[cfg(target_os = "windows")]
fn reg_delete_env(name: &str) -> Result<(), String> {
    let key = env_reg_key()?;
    match key.delete_value(name) {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(not(target_os = "windows"))]
fn reg_delete_env(name: &str) -> Result<(), String> {
    std::env::remove_var(name);
    shell_rc_delete_env(name)
}

/// Broadcast WM_SETTINGCHANGE so other apps pick up new env vars immediately
#[cfg(target_os = "windows")]
fn broadcast_env_change() {
    #[link(name = "user32")]
    extern "system" {
        fn SendMessageTimeoutW(
            hwnd: isize,
            msg: u32,
            wparam: usize,
            lparam: *const u16,
            flags: u32,
            timeout: u32,
            result: *mut usize,
        ) -> isize;
    }

    const HWND_BROADCAST: isize = 0xFFFF;
    const WM_SETTINGCHANGE: u32 = 0x001A;
    const SMTO_ABORTIFHUNG: u32 = 0x0002;
    const BROADCAST_TIMEOUT_MS: u32 = 400;

    let env: Vec<u16> = "Environment\0".encode_utf16().collect();
    let mut result: usize = 0;

    unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            0,
            env.as_ptr(),
            SMTO_ABORTIFHUNG,
            BROADCAST_TIMEOUT_MS,
            &mut result,
        );
    }
}

#[cfg(not(target_os = "windows"))]
fn broadcast_env_change() {}

fn upsert_env_array(arr: &mut Vec<serde_json::Value>, name: &str, value: &str) {
    arr.retain(|v| v.get("name").and_then(|n| n.as_str()) != Some(name));
    arr.push(serde_json::json!({ "name": name, "value": value }));
}

fn remove_env_array_key(arr: &mut Vec<serde_json::Value>, name: &str) {
    arr.retain(|v| v.get("name").and_then(|n| n.as_str()) != Some(name));
}

fn get_env_array_value(arr: &[serde_json::Value], name: &str) -> Option<String> {
    arr.iter()
        .find(|v| v.get("name").and_then(|n| n.as_str()) == Some(name))
        .and_then(|v| v.get("value"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn has_env_array_key(arr: &[serde_json::Value], name: &str) -> bool {
    arr.iter()
        .any(|v| v.get("name").and_then(|n| n.as_str()) == Some(name))
}

fn pick_auth_name(_has_token: bool, _has_key: bool) -> &'static str {
    AUTH_TOKEN_ENV
}

fn read_auth_from_env_array(arr: &[serde_json::Value]) -> String {
    get_env_array_value(arr, AUTH_TOKEN_ENV)
        .or_else(|| get_env_array_value(arr, AUTH_KEY_ENV))
        .or_else(|| get_env_array_value(arr, LEGACY_AUTH_ENV))
        .unwrap_or_default()
}

fn apply_auth_to_env_array(
    arr: &mut Vec<serde_json::Value>,
    api_key: &str,
    base_url: &str,
) -> &'static str {
    let auth_name = pick_auth_name(
        has_env_array_key(arr, AUTH_TOKEN_ENV),
        has_env_array_key(arr, AUTH_KEY_ENV),
    );
    upsert_env_array(arr, auth_name, api_key);
    upsert_env_array(arr, BASE_URL_ENV, base_url);
    remove_env_array_key(
        arr,
        if auth_name == AUTH_TOKEN_ENV {
            AUTH_KEY_ENV
        } else {
            AUTH_TOKEN_ENV
        },
    );
    remove_env_array_key(arr, LEGACY_AUTH_ENV);
    auth_name
}

fn read_auth_from_env_object(env: &serde_json::Map<String, serde_json::Value>) -> String {
    env.get(AUTH_TOKEN_ENV)
        .and_then(|v| v.as_str())
        .or_else(|| env.get(AUTH_KEY_ENV).and_then(|v| v.as_str()))
        .or_else(|| env.get(LEGACY_AUTH_ENV).and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_string()
}

fn apply_auth_to_env_object(
    env: &mut serde_json::Map<String, serde_json::Value>,
    api_key: &str,
    base_url: &str,
) -> &'static str {
    let auth_name = pick_auth_name(
        env.contains_key(AUTH_TOKEN_ENV),
        env.contains_key(AUTH_KEY_ENV),
    );
    env.insert(
        auth_name.to_string(),
        serde_json::Value::String(api_key.to_string()),
    );
    env.insert(
        BASE_URL_ENV.to_string(),
        serde_json::Value::String(base_url.to_string()),
    );
    env.remove(if auth_name == AUTH_TOKEN_ENV {
        AUTH_KEY_ENV
    } else {
        AUTH_TOKEN_ENV
    });
    env.remove(LEGACY_AUTH_ENV);
    auth_name
}

fn read_auth_from_system_env() -> String {
    reg_get_env_opt(AUTH_TOKEN_ENV)
        .or_else(|| reg_get_env_opt(AUTH_KEY_ENV))
        .or_else(|| reg_get_env_opt(LEGACY_AUTH_ENV))
        .unwrap_or_default()
}

fn apply_auth_to_system_env(api_key: &str, base_url: &str) -> Result<&'static str, String> {
    let auth_name = pick_auth_name(
        reg_get_env_opt(AUTH_TOKEN_ENV).is_some(),
        reg_get_env_opt(AUTH_KEY_ENV).is_some(),
    );
    reg_set_env(auth_name, api_key)?;
    reg_set_env(BASE_URL_ENV, base_url)?;

    let other = if auth_name == AUTH_TOKEN_ENV {
        AUTH_KEY_ENV
    } else {
        AUTH_TOKEN_ENV
    };
    if reg_get_env_opt(other).is_some() {
        reg_delete_env(other)?;
    }
    if reg_get_env_opt(LEGACY_AUTH_ENV).is_some() {
        reg_delete_env(LEGACY_AUTH_ENV)?;
    }

    Ok(auth_name)
}

fn restore_system_env_var(name: &str, value: &Option<String>) -> Result<(), String> {
    match value {
        Some(v) => reg_set_env(name, v),
        None => {
            if reg_get_env_opt(name).is_some() {
                reg_delete_env(name)?;
            }
            Ok(())
        }
    }
}

fn emit_switch_progress(app: &tauri::AppHandle, step: u32, label: &str) {
    let _ = app.emit(
        "switch-progress",
        ProgressEvent {
            step,
            total: SWITCH_TOTAL_STEPS,
            label: label.to_string(),
        },
    );
}

fn emit_plugin_marketplace_progress(app: &tauri::AppHandle, step: u32, label: &str) {
    let _ = app.emit(
        "plugin-marketplace-progress",
        ProgressEvent {
            step,
            total: 4,
            label: label.to_string(),
        },
    );
}

fn sanitize_endpoint_timeout(timeout_secs: Option<u64>) -> u64 {
    timeout_secs
        .unwrap_or(ENDPOINT_TEST_DEFAULT_TIMEOUT_SECS)
        .clamp(
            ENDPOINT_TEST_MIN_TIMEOUT_SECS,
            ENDPOINT_TEST_MAX_TIMEOUT_SECS,
        )
}

fn normalize_endpoint_url(raw_url: &str) -> Result<String, String> {
    let trimmed = raw_url.trim().trim_end_matches('/').to_string();
    if trimmed.is_empty() {
        return Err("URL 不能为空".to_string());
    }

    let parsed = reqwest::Url::parse(&trimmed).map_err(|e| format!("URL 无效: {}", e))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("仅支持 http 或 https URL".to_string());
    }

    Ok(trimmed)
}

fn measure_endpoint_latency(
    client: reqwest::blocking::Client,
    url: String,
    timeout: Duration,
) -> EndpointLatency {
    let parsed = match reqwest::Url::parse(&url) {
        Ok(parsed) => parsed,
        Err(e) => {
            return EndpointLatency {
                url,
                latency: None,
                status: None,
                error: Some(format!("URL 无效: {}", e)),
            };
        }
    };

    let _ = client.get(parsed.clone()).timeout(timeout).send().ok();

    let start = Instant::now();
    match client.get(parsed).timeout(timeout).send() {
        Ok(resp) => EndpointLatency {
            url,
            latency: Some(start.elapsed().as_millis()),
            status: Some(resp.status().as_u16()),
            error: None,
        },
        Err(e) => EndpointLatency {
            url,
            latency: None,
            status: e.status().map(|status| status.as_u16()),
            error: Some(if e.is_timeout() {
                "请求超时".to_string()
            } else if e.is_connect() {
                "连接失败".to_string()
            } else {
                e.to_string()
            }),
        },
    }
}

// ── Tauri Commands ──────────────────────────────────

#[tauri::command]
fn get_profiles(app: tauri::AppHandle) -> ProfilesData {
    read_profiles(&app)
}

#[tauri::command]
fn test_api_endpoints(
    urls: Vec<String>,
    timeout_secs: Option<u64>,
) -> Result<Vec<EndpointLatency>, String> {
    if urls.is_empty() {
        return Ok(Vec::new());
    }

    let timeout = Duration::from_secs(sanitize_endpoint_timeout(timeout_secs));
    let client = reqwest::blocking::Client::builder()
        .user_agent("VarSwitch endpoint speed test")
        .timeout(timeout)
        .build()
        .map_err(|e| e.to_string())?;

    let mut results: Vec<Option<EndpointLatency>> = vec![None; urls.len()];
    let mut handles = Vec::new();

    for (idx, raw_url) in urls.into_iter().enumerate() {
        match normalize_endpoint_url(&raw_url) {
            Ok(url) => {
                let client = client.clone();
                handles.push((
                    idx,
                    std::thread::spawn(move || measure_endpoint_latency(client, url, timeout)),
                ));
            }
            Err(error) => {
                results[idx] = Some(EndpointLatency {
                    url: raw_url,
                    latency: None,
                    status: None,
                    error: Some(error),
                });
            }
        }
    }

    for (idx, handle) in handles {
        results[idx] = Some(handle.join().map_err(|_| "测速线程异常退出".to_string())?);
    }

    Ok(results.into_iter().flatten().collect())
}

#[tauri::command]
fn add_profile(
    app: tauri::AppHandle,
    name: String,
    api_key: String,
    base_url: String,
    model_id: Option<String>,
) -> Result<Profile, String> {
    if name.is_empty() || api_key.is_empty() || base_url.is_empty() {
        return Err("所有字段都必须填写".into());
    }
    let mut data = read_profiles(&app);
    let profile = Profile {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.trim().to_string(),
        api_key: api_key.trim().to_string(),
        base_url: base_url.trim().trim_end_matches('/').to_string(),
        model_id: model_id.unwrap_or_default().trim().to_string(),
        is_active: false,
        created_at: chrono_now(),
    };
    data.profiles.push(profile.clone());
    write_profiles(&app, &data)?;
    Ok(profile)
}

#[tauri::command]
fn update_profile(
    app: tauri::AppHandle,
    id: String,
    name: String,
    api_key: String,
    base_url: String,
    model_id: Option<String>,
) -> Result<Profile, String> {
    let mut data = read_profiles(&app);
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
    if !base_url.is_empty() {
        p.base_url = base_url.trim().trim_end_matches('/').to_string();
    }
    p.model_id = model_id.map(|m| m.trim().to_string()).unwrap_or_default();
    let updated = p.clone();
    write_profiles(&app, &data)?;
    Ok(updated)
}

#[tauri::command]
fn delete_profile(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let mut data = read_profiles(&app);
    data.profiles.retain(|x| x.id != id);
    write_profiles(&app, &data)
}

#[tauri::command]
fn snapshot_config(app: tauri::AppHandle) -> ConfigSnapshot {
    let settings = read_app_settings(&app);
    let mut editor_contents = HashMap::new();
    for editor in detect_installed_editors(&settings) {
        if let Ok(content) = fs::read_to_string(resolved_editor_settings_path(editor, &settings)) {
            editor_contents.insert(editor.id.to_string(), content);
        }
    }
    ConfigSnapshot {
        env_auth_token: reg_get_env_opt(AUTH_TOKEN_ENV),
        env_auth_key: reg_get_env_opt(AUTH_KEY_ENV),
        env_api_key: reg_get_env_opt(LEGACY_AUTH_ENV),
        env_base_url: reg_get_env_opt(BASE_URL_ENV),
        editor_contents,
        claude_content: fs::read_to_string(claude_settings_path()).ok(),
    }
}

#[tauri::command]
fn restore_config(app: tauri::AppHandle, snapshot: ConfigSnapshot) -> Result<(), String> {
    let settings = read_app_settings(&app);
    restore_system_env_var(AUTH_TOKEN_ENV, &snapshot.env_auth_token)?;
    restore_system_env_var(AUTH_KEY_ENV, &snapshot.env_auth_key)?;
    restore_system_env_var(LEGACY_AUTH_ENV, &snapshot.env_api_key)?;
    restore_system_env_var(BASE_URL_ENV, &snapshot.env_base_url)?;
    broadcast_env_change();

    // 恢复所有编辑器配置
    for (editor_id, content) in &snapshot.editor_contents {
        if let Some(editor) = KNOWN_EDITORS.iter().find(|e| e.id == editor_id.as_str()) {
            let path = resolved_editor_settings_path(editor, &settings);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::write(&path, content).map_err(|e| e.to_string())?;
        }
    }

    if let Some(content) = &snapshot.claude_content {
        let path = claude_settings_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::write(&path, content).map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[tauri::command]
fn cancel_switch(state: State<'_, AppState>) {
    state.cancel_flag.store(true, Ordering::SeqCst);
}

// ── 配置自动备份 ─────────────────────────────────────
// 切换配置前自动快照 profiles.json / codex_profiles.json，误操作可回滚。

/// 返回给前端的备份信息。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConfigBackupInfo {
    name: String,  // 备份文件名
    kind: String,  // "claude" | "codex"
    stamp: String, // 紧凑时间戳，如 20260624-143025（前端格式化展示）
}

fn backups_dir(app: &tauri::AppHandle) -> PathBuf {
    let dir = data_dir(app).join("backups");
    fs::create_dir_all(&dir).ok();
    dir
}

/// 把单个配置文件复制到备份目录（带时间戳）。
fn backup_one_config(dir: &PathBuf, src: &PathBuf, prefix: &str, stamp: &str) {
    if !src.exists() {
        return;
    }
    let dst = dir.join(format!("{prefix}-{stamp}.json"));
    let _ = fs::copy(src, &dst);
}

/// 清理同类备份，只保留最近 keep 个（文件名时间戳字典序==时间序）。
fn prune_backups(dir: &PathBuf, prefix: &str, keep: usize) {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with(&format!("{prefix}-")) && n.ends_with(".json"))
                .unwrap_or(false)
        })
        .collect();
    if files.len() <= keep {
        return;
    }
    files.sort();
    let remove_count = files.len() - keep;
    for old in files.iter().take(remove_count) {
        let _ = fs::remove_file(old);
    }
}

/// 切换配置前自动备份。失败不阻断切换，仅记录日志。
fn auto_backup_configs(app: &tauri::AppHandle) {
    let dir = backups_dir(app);
    let stamp = format_compact_time(chrono_timestamp_millis());
    backup_one_config(&dir, &profiles_path(app), "profiles", &stamp);
    backup_one_config(&dir, &codex_profiles_path(app), "codex", &stamp);
    prune_backups(&dir, "profiles", 20);
    prune_backups(&dir, "codex", 20);
    log_info!("[backup] 已自动备份配置 stamp={stamp}");
}

/// 列出所有配置备份，最新的在前。
#[tauri::command]
fn list_config_backups(app: tauri::AppHandle) -> Vec<ConfigBackupInfo> {
    let dir = backups_dir(&app);
    let mut out: Vec<ConfigBackupInfo> = Vec::new();
    for entry in fs::read_dir(&dir).into_iter().flatten().flatten() {
        let path = entry.path();
        let Some(fname) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !fname.ends_with(".json") {
            continue;
        }
        let (kind, stamp) = if let Some(rest) = fname.strip_prefix("profiles-") {
            ("claude", rest.trim_end_matches(".json"))
        } else if let Some(rest) = fname.strip_prefix("codex-") {
            ("codex", rest.trim_end_matches(".json"))
        } else {
            continue;
        };
        out.push(ConfigBackupInfo {
            name: fname.to_string(),
            kind: kind.to_string(),
            stamp: stamp.to_string(),
        });
    }
    out.sort_by(|a, b| b.stamp.cmp(&a.stamp));
    out
}

/// 从指定备份恢复配置。恢复前会先备份当前状态，避免回滚错了无法挽回。
#[tauri::command]
fn restore_config_backup(app: tauri::AppHandle, name: String) -> Result<(), String> {
    // 安全校验：防止目录穿越
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err("非法的备份名".into());
    }
    let src = backups_dir(&app).join(&name);
    if !src.exists() {
        return Err("备份文件不存在".into());
    }
    let dst = if name.starts_with("profiles-") {
        profiles_path(&app)
    } else if name.starts_with("codex-") {
        codex_profiles_path(&app)
    } else {
        return Err("无法识别的备份类型".into());
    };
    // 恢复前先备份当前，留一条后悔路
    auto_backup_configs(&app);
    fs::copy(&src, &dst).map_err(|e| format!("恢复失败: {e}"))?;
    log_info!("[backup] 已从备份恢复配置: {name}");
    Ok(())
}

/// 打开备份文件夹。
#[tauri::command]
fn open_backups_folder(app: tauri::AppHandle) -> Result<(), String> {
    let dir = backups_dir(&app);
    open_folder(dir.to_string_lossy().to_string())
}

#[tauri::command]
fn switch_profile(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<SwitchResult, String> {
    let settings = read_app_settings(&app);
    let mut data = read_profiles(&app);
    let profile = data
        .profiles
        .iter()
        .find(|x| x.id == id)
        .ok_or("配置未找到")?
        .clone();

    state.cancel_flag.store(false, Ordering::SeqCst);

    // 切换前自动备份当前配置，误操作可在设置里回滚
    auto_backup_configs(&app);

    let mut errors: Vec<String> = Vec::new();
    let mut details = SwitchDetails {
        env_vars: false,
        editors: HashMap::new(),
        claude: false,
    };

    emit_switch_progress(&app, 1, "prepare");

    if state.cancel_flag.load(Ordering::SeqCst) {
        return Ok(SwitchResult {
            success: false,
            results: details,
            errors: vec!["已取消".into()],
            profile_name: profile.name,
            cancelled: true,
        });
    }

    emit_switch_progress(&app, 2, "system");
    match apply_auth_to_system_env(&profile.api_key, &profile.base_url) {
        Ok(_) => {
            broadcast_env_change();
            details.env_vars = true;
        }
        Err(e) => errors.push(format!("系统环境变量: {}", e)),
    }

    if state.cancel_flag.load(Ordering::SeqCst) {
        return Ok(SwitchResult {
            success: false,
            results: details,
            errors: vec!["已取消".into()],
            profile_name: profile.name,
            cancelled: true,
        });
    }

    emit_switch_progress(&app, 3, "editors");
    // 自动检测已安装的编辑器并逐一写入配置
    let editors = detect_installed_editors(&settings);
    for editor in &editors {
        let path = resolved_editor_settings_path(editor, &settings);
        // 读取现有配置，文件不存在则使用空 JSON 对象
        let mut settings = read_json_or_default(&path, serde_json::json!({}));
        if !settings
            .get("claudeCode.environmentVariables")
            .map(|v| v.is_array())
            .unwrap_or(false)
        {
            settings["claudeCode.environmentVariables"] = serde_json::json!([]);
        }
        if let Some(arr) = settings
            .get_mut("claudeCode.environmentVariables")
            .and_then(|v| v.as_array_mut())
        {
            apply_auth_to_env_array(arr, &profile.api_key, &profile.base_url);
        }
        // 处理 claudeCode.selectedModel: 仅当 profile.model_id 非空时才写入
        if !profile.model_id.is_empty() {
            settings["claudeCode.selectedModel"] = serde_json::json!(profile.model_id);
        }
        match write_json(&path, &settings) {
            Ok(_) => {
                details.editors.insert(editor.id.to_string(), true);
            }
            Err(e) => {
                details.editors.insert(editor.id.to_string(), false);
                errors.push(format!("{}: {}", editor.display_name, e));
            }
        }
    }

    if state.cancel_flag.load(Ordering::SeqCst) {
        return Ok(SwitchResult {
            success: false,
            results: details,
            errors: vec!["已取消".into()],
            profile_name: profile.name,
            cancelled: true,
        });
    }

    emit_switch_progress(&app, 4, "claude");
    let cp = claude_settings_path();
    // 文件不存在时自动创建默认配置
    let mut settings = read_json_or_default(
        &cp,
        serde_json::json!({
            "permissions": {
                "allow": [],
                "deny": []
            },
            "env": {}
        }),
    );
    if !settings.is_object() {
        settings = serde_json::json!({});
    }
    if !settings.get("env").map(|v| v.is_object()).unwrap_or(false) {
        settings["env"] = serde_json::json!({});
    }
    if let Some(env) = settings.get_mut("env").and_then(|v| v.as_object_mut()) {
        apply_auth_to_env_object(env, &profile.api_key, &profile.base_url);
    }
    // 处理 model: 仅当 profile.model_id 非空时才写入，逻辑与编辑器一致
    if !profile.model_id.is_empty() {
        settings["model"] = serde_json::json!(profile.model_id);
    }
    match write_json(&cp, &settings) {
        Ok(_) => details.claude = true,
        Err(e) => errors.push(format!("Claude: {}", e)),
    }

    if state.cancel_flag.load(Ordering::SeqCst) {
        return Ok(SwitchResult {
            success: false,
            results: details,
            errors: vec!["已取消".into()],
            profile_name: profile.name,
            cancelled: true,
        });
    }

    emit_switch_progress(&app, 5, "finalize");
    // Mark active
    for p in data.profiles.iter_mut() {
        p.is_active = p.id == profile.id;
    }
    write_profiles(&app, &data)?;

    emit_switch_progress(&app, 6, "done");

    Ok(SwitchResult {
        success: errors.is_empty(),
        results: details,
        errors,
        profile_name: profile.name,
        cancelled: false,
    })
}

#[tauri::command]
fn get_status(app: tauri::AppHandle) -> StatusResult {
    let settings = read_app_settings(&app);
    let env_vars = Some(LocationStatus {
        api_key: read_auth_from_system_env(),
        base_url: reg_get_env(BASE_URL_ENV),
    });

    // 动态检测已安装的编辑器并读取状态
    let mut editors = HashMap::new();
    for editor in detect_installed_editors(&settings) {
        if let Some(status) = (|| -> Option<LocationStatus> {
            let s = read_json(&resolved_editor_settings_path(editor, &settings)).ok()?;
            let arr = s.get("claudeCode.environmentVariables")?.as_array()?;
            Some(LocationStatus {
                api_key: read_auth_from_env_array(arr),
                base_url: get_env_array_value(arr, BASE_URL_ENV).unwrap_or_default(),
            })
        })() {
            editors.insert(editor.id.to_string(), status);
        }
    }

    let claude = (|| -> Option<LocationStatus> {
        let s = read_json(&claude_settings_path()).ok()?;
        let env = s.get("env").and_then(|v| v.as_object());
        Some(LocationStatus {
            api_key: env.map(read_auth_from_env_object).unwrap_or_default(),
            base_url: env
                .and_then(|e| e.get(BASE_URL_ENV))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        })
    })();

    StatusResult {
        env_vars,
        editors,
        claude,
    }
}

/// 返回检测到的已安装编辑器列表 (id -> displayName)
#[tauri::command]
fn get_detected_editors(app: tauri::AppHandle) -> HashMap<String, String> {
    let settings = read_app_settings(&app);
    detect_installed_editors(&settings)
        .into_iter()
        .map(|ed| (ed.id.to_string(), ed.display_name.to_string()))
        .collect()
}

#[tauri::command]
fn import_current(app: tauri::AppHandle, name: String) -> Result<Profile, String> {
    let settings = read_app_settings(&app);
    let mut api_key = String::new();
    let mut base_url = String::new();

    // 先尝试 Claude settings
    if let Ok(s) = read_json(&claude_settings_path()) {
        if let Some(env) = s.get("env").and_then(|v| v.as_object()) {
            api_key = read_auth_from_env_object(env);
            base_url = env
                .get(BASE_URL_ENV)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
        }
    }

    // 回退到已安装的编辑器配置
    if api_key.is_empty() || base_url.is_empty() {
        for editor in detect_installed_editors(&settings) {
            if let Ok(s) = read_json(&resolved_editor_settings_path(editor, &settings)) {
                if let Some(arr) = s
                    .get("claudeCode.environmentVariables")
                    .and_then(|v| v.as_array())
                {
                    if api_key.is_empty() {
                        api_key = read_auth_from_env_array(arr);
                    }
                    if base_url.is_empty() {
                        base_url = get_env_array_value(arr, BASE_URL_ENV).unwrap_or_default();
                    }
                }
            }
            if !api_key.is_empty() && !base_url.is_empty() {
                break;
            }
        }
    }

    // Fallback to system env vars for any missing field
    if api_key.is_empty() || base_url.is_empty() {
        let env_api_key = read_auth_from_system_env();
        let env_base_url = reg_get_env(BASE_URL_ENV);
        if api_key.is_empty() {
            api_key = env_api_key;
        }
        if base_url.is_empty() {
            base_url = env_base_url;
        }
    }

    if api_key.is_empty() || base_url.is_empty() {
        return Err("未检测到当前配置".into());
    }

    let mut data = read_profiles(&app);
    if data
        .profiles
        .iter()
        .any(|x| x.api_key == api_key && x.base_url == base_url)
    {
        return Err("该配置已存在".into());
    }

    let profile = Profile {
        id: uuid::Uuid::new_v4().to_string(),
        name: if name.is_empty() {
            "导入的配置".into()
        } else {
            name
        },
        api_key,
        base_url,
        model_id: String::new(),
        is_active: true,
        created_at: chrono_now(),
    };

    for p in data.profiles.iter_mut() {
        p.is_active = false;
    }
    data.profiles.push(profile.clone());
    write_profiles(&app, &data)?;
    Ok(profile)
}

// ── Codex Profile Commands ──────────────────────────

#[tauri::command]
fn get_codex_profiles(app: tauri::AppHandle) -> CodexProfilesData {
    read_codex_profiles(&app)
}

#[tauri::command]
fn add_codex_profile(
    app: tauri::AppHandle,
    name: String,
    api_key: String,
    base_url: String,
    auth_mode: Option<String>,
    model: Option<String>,
    provider_name: Option<String>,
) -> Result<CodexProfile, String> {
    if name.is_empty() || api_key.is_empty() || base_url.is_empty() {
        return Err("所有字段都必须填写".into());
    }
    let mut data = read_codex_profiles(&app);
    let profile = CodexProfile {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.trim().to_string(),
        api_key: api_key.trim().to_string(),
        base_url: base_url.trim().trim_end_matches('/').to_string(),
        auth_mode: auth_mode.unwrap_or_else(default_codex_auth_mode),
        model: model.unwrap_or_default().trim().to_string(),
        provider_name: provider_name.unwrap_or_default().trim().to_string(),
        is_active: false,
        created_at: chrono_now(),
    };
    data.profiles.push(profile.clone());
    write_codex_profiles(&app, &data)?;
    Ok(profile)
}

#[tauri::command]
fn update_codex_profile(
    app: tauri::AppHandle,
    id: String,
    name: String,
    api_key: String,
    base_url: String,
    auth_mode: Option<String>,
    model: Option<String>,
    provider_name: Option<String>,
) -> Result<CodexProfile, String> {
    let mut data = read_codex_profiles(&app);
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
    if !base_url.is_empty() {
        p.base_url = base_url.trim().trim_end_matches('/').to_string();
    }
    if let Some(mode) = auth_mode {
        if !mode.trim().is_empty() {
            p.auth_mode = mode.trim().to_string();
        }
    }
    p.model = model.unwrap_or_default().trim().to_string();
    p.provider_name = provider_name.unwrap_or_default().trim().to_string();
    let updated = p.clone();
    write_codex_profiles(&app, &data)?;
    Ok(updated)
}

#[tauri::command]
fn delete_codex_profile(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let mut data = read_codex_profiles(&app);
    data.profiles.retain(|x| x.id != id);
    write_codex_profiles(&app, &data)
}

#[tauri::command]
fn switch_codex_profile(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let mut data = read_codex_profiles(&app);
    let profile = data
        .profiles
        .iter()
        .find(|x| x.id == id)
        .ok_or("配置未找到")?
        .clone();

    // 切换前自动备份当前配置
    auto_backup_configs(&app);

    write_codex_config(&profile)?;

    for p in data.profiles.iter_mut() {
        p.is_active = p.id == profile.id;
    }
    write_codex_profiles(&app, &data)?;
    Ok(())
}

#[tauri::command]
fn import_codex_current(app: tauri::AppHandle, name: String) -> Result<CodexProfile, String> {
    let status = read_codex_status().ok_or("未检测到当前 Codex 配置")?;
    if status.api_key.is_empty() {
        return Err("未检测到当前 Codex 配置".into());
    }
    let config_str = fs::read_to_string(codex_config_path()).unwrap_or_default();
    let auth_mode = if config_str
        .lines()
        .any(|l| l.trim().starts_with("experimental_bearer_token"))
    {
        "official_account_api_quota".to_string()
    } else {
        default_codex_auth_mode()
    };

    let mut data = read_codex_profiles(&app);
    if data
        .profiles
        .iter()
        .any(|x| x.api_key == status.api_key && x.base_url == status.base_url)
    {
        return Err("该配置已存在".into());
    }

    let profile = CodexProfile {
        id: uuid::Uuid::new_v4().to_string(),
        name: if name.is_empty() {
            "导入的 Codex 配置".into()
        } else {
            name
        },
        api_key: status.api_key,
        base_url: status.base_url,
        auth_mode,
        model: String::new(),
        provider_name: String::new(),
        is_active: true,
        created_at: chrono_now(),
    };

    for p in data.profiles.iter_mut() {
        p.is_active = false;
    }
    data.profiles.push(profile.clone());
    write_codex_profiles(&app, &data)?;
    Ok(profile)
}

#[tauri::command]
fn get_codex_status() -> Option<LocationStatus> {
    read_codex_status()
}

#[tauri::command]
fn get_codex_toolbox(app: tauri::AppHandle) -> Result<ToolboxSnapshot, String> {
    Ok(build_toolbox_snapshot(&app))
}

#[tauri::command]
fn apply_plugin_marketplace(
    app: tauri::AppHandle,
    source: String,
) -> Result<ToolboxSnapshot, String> {
    let trimmed = supported_plugin_marketplace_source(&source);
    emit_plugin_marketplace_progress(&app, 1, "prepare");
    let config_path = codex_config_path();
    let existing = fs::read_to_string(&config_path).unwrap_or_default();
    let cleaned = remove_all_plugin_marketplace_sections(&existing);
    let parent = config_path
        .parent()
        .ok_or("Codex 配置目录不存在")?
        .to_path_buf();
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    fs::write(&config_path, cleaned).map_err(|e| e.to_string())?;

    emit_plugin_marketplace_progress(&app, 2, "install");
    if let Err(error) = run_codex_plugin_marketplace_add(trimmed) {
        let fallback_existing = fs::read_to_string(&config_path).unwrap_or_default();
        let fallback_next = ensure_plugin_marketplace_section(
            &fallback_existing,
            default_plugin_marketplace_name(),
            trimmed,
            plugin_marketplace_source_type(trimmed),
        );
        let _ = fs::write(&config_path, fallback_next);
        return Err(format!(
            "{error}\n已尝试把 VarSwitch-Plugin 写入 config.toml 作为兜底；请确认 Codex CLI 和 Git 可用后重试。"
        ));
    }

    emit_plugin_marketplace_progress(&app, 3, "verify");
    let updated = fs::read_to_string(&config_path).unwrap_or_default();
    if !config_has_plugin_marketplace_source(&updated, trimmed) {
        let fallback_next = ensure_plugin_marketplace_section(
            &updated,
            default_plugin_marketplace_name(),
            trimmed,
            plugin_marketplace_source_type(trimmed),
        );
        fs::write(&config_path, fallback_next).map_err(|e| e.to_string())?;
    }

    let mut state = read_toolbox_state(&app);
    state.plugin_marketplace_input = trimmed.to_string();
    write_toolbox_state(&app, &state)?;
    emit_plugin_marketplace_progress(&app, 4, "done");
    Ok(build_toolbox_snapshot(&app))
}

#[tauri::command]
fn sync_codex_sessions(app: tauri::AppHandle) -> Result<ToolboxSnapshot, String> {
    let threads = read_codex_threads(200);
    let mut state = read_toolbox_state(&app);
    state.synced_codex_threads = threads;
    state.session_sync = CodexSessionSyncState {
        last_synced_at: chrono_now(),
        total: state.synced_codex_threads.len(),
    };
    if state.selected_mobile_thread_id.trim().is_empty() {
        if let Some(first) = state.synced_codex_threads.first() {
            state.selected_mobile_thread_id = first.id.clone();
        }
    }
    write_toolbox_state(&app, &state)?;
    Ok(build_toolbox_snapshot(&app))
}

#[tauri::command]
fn select_mobile_thread(
    app: tauri::AppHandle,
    thread_id: String,
) -> Result<ToolboxSnapshot, String> {
    let normalized = thread_id.trim().to_string();
    if normalized.is_empty() {
        return Err("请选择要控制的会话".into());
    }
    let mut state = read_toolbox_state(&app);
    let threads = if state.synced_codex_threads.is_empty() {
        read_codex_threads(200)
    } else {
        state.synced_codex_threads.clone()
    };
    let thread = threads
        .into_iter()
        .find(|item| item.id == normalized)
        .ok_or("会话未找到，请先同步本地历史")?;
    state.selected_mobile_thread_id = thread.id.clone();
    state.mobile_remote.active_thread_id = thread.id;
    state.mobile_remote.active_thread_name = thread.thread_name;
    write_toolbox_state(&app, &state)?;
    Ok(build_toolbox_snapshot(&app))
}

#[tauri::command]
fn configure_mobile_channel(
    app: tauri::AppHandle,
    channel: String,
    app_id: String,
    app_secret: String,
    bot_token: String,
    account_id: String,
    base_url: String,
    user_id: String,
    bot_open_id: String,
) -> Result<ToolboxSnapshot, String> {
    let normalized_channel = normalize_mobile_channel(&channel);
    if !["lark", "wechat", "qq"].contains(&normalized_channel.as_str()) {
        return Err("不支持的手机通道".into());
    }
    let mut state = read_toolbox_state(&app);
    if !state
        .mobile_channels
        .iter()
        .any(|binding| binding.channel == normalized_channel)
    {
        state
            .mobile_channels
            .push(default_mobile_channel(&normalized_channel));
    }
    if let Some(binding) = state
        .mobile_channels
        .iter_mut()
        .find(|binding| binding.channel == normalized_channel)
    {
        binding.app_id = app_id.trim().to_string();
        binding.app_secret = app_secret.trim().to_string();
        binding.bot_token = bot_token.trim().to_string();
        binding.account_id = account_id.trim().to_string();
        binding.base_url = normalize_mobile_base_url(&base_url, &normalized_channel);
        binding.user_id = user_id.trim().to_string();
        binding.bot_open_id = bot_open_id.trim().to_string();
        binding.credential_status = if mobile_channel_has_credentials(binding) {
            "已保存平台凭据，等待开启连接".into()
        } else {
            mobile_channel_credential_hint(&normalized_channel).into()
        };
        binding.last_error.clear();
        binding.updated_at = chrono_now();
    }
    if state
        .mobile_channels
        .iter()
        .find(|binding| binding.channel == normalized_channel)
        .map(mobile_channel_has_credentials)
        .unwrap_or(false)
    {
        attach_selected_thread_to_mobile_channel(&mut state, &normalized_channel);
    }
    write_toolbox_state(&app, &state)?;
    Ok(build_toolbox_snapshot(&app))
}

#[tauri::command]
fn start_lark_bot_registration(
    app: tauri::AppHandle,
    create_only: Option<bool>,
) -> Result<ToolboxSnapshot, String> {
    if LARK_REGISTRATION_ACTIVE.swap(true, Ordering::SeqCst) {
        return Ok(build_toolbox_snapshot(&app));
    }
    let create_only = create_only.unwrap_or(true);
    let existing_app_id = read_toolbox_state(&app)
        .mobile_channels
        .iter()
        .find(|binding| binding.channel == "lark")
        .map(|binding| binding.app_id.clone())
        .unwrap_or_default();

    let client = match build_http_client(15) {
        Ok(client) => client,
        Err(error) => {
            LARK_REGISTRATION_ACTIVE.store(false, Ordering::SeqCst);
            return Err(error);
        }
    };
    let (device_code, launcher_url, interval) =
        match request_lark_registration(&client, create_only, &existing_app_id) {
            Ok(result) => result,
            Err(error) => {
                LARK_REGISTRATION_ACTIVE.store(false, Ordering::SeqCst);
                return Err(error);
            }
        };
    if let Err(error) = open_with_system(&launcher_url) {
        LARK_REGISTRATION_ACTIVE.store(false, Ordering::SeqCst);
        return Err(error);
    }
    let mut state = read_toolbox_state(&app);
    if let Some(binding) = state
        .mobile_channels
        .iter_mut()
        .find(|binding| binding.channel == "lark")
    {
        binding.base_url = default_mobile_channel_base_url("lark").into();
        binding.launcher_url = launcher_url;
        binding.qr_url = binding.launcher_url.clone();
        binding.qr_device_code = device_code.clone();
        binding.qr_status = if create_only {
            "已打开飞书创建页面，建议命名为 VarSwitch 智能体，创建完成后会自动填充 AppID/AppSecret".into()
        } else {
            "已打开飞书换绑页面，完成授权后会自动读取 AppID/AppSecret".into()
        };
        binding.credential_status = "等待飞书创建/授权确认".into();
        binding.status = if create_only {
            "等待飞书机器人创建完成".into()
        } else {
            "等待飞书已有机器人授权完成".into()
        };
        binding.last_error.clear();
        binding.qr_started_at = chrono_now();
        binding.updated_at = chrono_now();
    }
    write_toolbox_state(&app, &state)?;
    start_lark_registration_poll_worker(app.clone(), device_code, interval);
    Ok(build_toolbox_snapshot(&app))
}

#[tauri::command]
fn poll_lark_bot_registration(app: tauri::AppHandle) -> Result<ToolboxSnapshot, String> {
    let state = read_toolbox_state(&app);
    let binding = state
        .mobile_channels
        .iter()
        .find(|binding| binding.channel == "lark")
        .cloned()
        .ok_or("飞书通道不存在")?;
    if binding.qr_device_code.trim().is_empty() {
        return Ok(build_toolbox_snapshot(&app));
    }
    let client = build_http_client(15)?;
    let poll = match poll_lark_registration_device(&client, &binding.qr_device_code) {
        Ok(poll) => poll,
        Err(error) => {
            update_channel_status(&app, "lark", "飞书自动读取 AppID/AppSecret 失败", &error);
            return Ok(build_toolbox_snapshot(&app));
        }
    };
    apply_lark_registration_poll(&app, poll)
}

#[tauri::command]
fn open_lark_bot_launcher(app: tauri::AppHandle) -> Result<ToolboxSnapshot, String> {
    match start_lark_bot_registration(app.clone(), Some(true)) {
        Ok(snapshot) => Ok(snapshot),
        Err(_) => {
            let url = lark_create_bot_launcher_url();
            open_with_system(url)?;
            let mut state = read_toolbox_state(&app);
            if let Some(binding) = state
                .mobile_channels
                .iter_mut()
                .find(|binding| binding.channel == "lark")
            {
                binding.launcher_url = url.into();
                binding.qr_url = url.into();
                binding.qr_status =
                    "已打开飞书创建页面；当前网络未能启动自动填充，请稍后重试自动创建".into();
                binding.updated_at = chrono_now();
            }
            write_toolbox_state(&app, &state)?;
            Ok(build_toolbox_snapshot(&app))
        }
    }
}

#[tauri::command]
fn clear_mobile_channel_binding(
    app: tauri::AppHandle,
    channel: String,
) -> Result<ToolboxSnapshot, String> {
    let normalized_channel = normalize_mobile_channel(&channel);
    match normalized_channel.as_str() {
        "lark" => stop_lark_bridge(),
        "qq" => stop_qq_gateway(),
        "wechat" => stop_wechat_listener(&app),
        _ => {}
    }
    let mut state = read_toolbox_state(&app);
    state
        .session_bindings
        .retain(|binding| normalize_mobile_channel(&binding.channel) != normalized_channel);
    if let Some(binding) = state
        .mobile_channels
        .iter_mut()
        .find(|binding| binding.channel == normalized_channel)
    {
        let base_url = default_mobile_channel_base_url(&normalized_channel).to_string();
        *binding = MobileChannelBinding {
            channel: normalized_channel.clone(),
            status: "未绑定".into(),
            base_url,
            updated_at: chrono_now(),
            ..MobileChannelBinding::default()
        };
    }
    write_toolbox_state(&app, &state)?;
    Ok(build_toolbox_snapshot(&app))
}

#[tauri::command]
fn start_qq_qr_binding(app: tauri::AppHandle) -> Result<ToolboxSnapshot, String> {
    if QQ_QR_ACTIVE.swap(true, Ordering::SeqCst) {
        let _ = write_mobile_channel_qr_state(
            &app,
            "qq",
            "QQ 扫码绑定正在进行中，请稍等二维码刷新",
            "",
            "",
            "",
        );
        return Ok(build_toolbox_snapshot(&app));
    }

    let _ = write_mobile_channel_qr_state(&app, "qq", "正在准备 QQ 扫码绑定服务...", "", "", "");
    let app_for_thread = app.clone();
    std::thread::spawn(move || {
        let runner = match ensure_qq_qr_connector(&app_for_thread) {
            Ok(path) => path,
            Err(error) => {
                let _ = write_mobile_channel_qr_state(
                    &app_for_thread,
                    "qq",
                    "QQ 扫码绑定失败",
                    "",
                    "",
                    &error,
                );
                QQ_QR_ACTIVE.store(false, Ordering::SeqCst);
                return;
            }
        };
        let node = match node_command_name() {
            Ok(command) => command,
            Err(error) => {
                let _ = write_mobile_channel_qr_state(
                    &app_for_thread,
                    "qq",
                    "QQ 扫码绑定失败",
                    "",
                    "",
                    &error,
                );
                QQ_QR_ACTIVE.store(false, Ordering::SeqCst);
                return;
            }
        };
        let connector_dir = match runner.parent() {
            Some(parent) => parent.to_path_buf(),
            None => {
                let _ = write_mobile_channel_qr_state(
                    &app_for_thread,
                    "qq",
                    "QQ 扫码绑定失败",
                    "",
                    "",
                    "QQ 扫码运行目录不存在",
                );
                QQ_QR_ACTIVE.store(false, Ordering::SeqCst);
                return;
            }
        };
        let mut cmd = Command::new(node);
        cmd.arg(&runner)
            .current_dir(connector_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(target_os = "windows")]
        cmd.creation_flags(CREATE_NO_WINDOW);
        let mut child = match cmd.spawn()
        {
            Ok(child) => child,
            Err(error) => {
                let _ = write_mobile_channel_qr_state(
                    &app_for_thread,
                    "qq",
                    "QQ 扫码绑定失败",
                    "",
                    "",
                    &format!("启动 QQ 扫码服务失败: {error}"),
                );
                QQ_QR_ACTIVE.store(false, Ordering::SeqCst);
                return;
            }
        };
        let stdout = match child.stdout.take() {
            Some(stdout) => stdout,
            None => {
                let _ = write_mobile_channel_qr_state(
                    &app_for_thread,
                    "qq",
                    "QQ 扫码绑定失败",
                    "",
                    "",
                    "QQ 扫码服务没有输出通道",
                );
                let _ = child.kill();
                let _ = child.wait();
                QQ_QR_ACTIVE.store(false, Ordering::SeqCst);
                return;
            }
        };
        let reader = BufReader::new(stdout);
        for line in reader.lines().flatten() {
            let Ok(payload) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            let event_type = json_string(&payload, &["type"]);
            match event_type.as_str() {
                "qr" => {
                    let url = json_string(&payload, &["url"]);
                    let mut data_url = json_string(&payload, &["dataUrl", "data_url"]);
                    // QQ connector 应返回 data:image/ 格式二维码，但若失败或返回 URL，用本地生成兜底。
                    if !data_url.starts_with("data:image/") && !url.is_empty() {
                        data_url = generate_qr_code_data_url(&url).unwrap_or_default();
                    }
                    if data_url.is_empty() || !data_url.starts_with("data:image/") {
                        let _ = write_mobile_channel_qr_state(
                            &app_for_thread,
                            "qq",
                            "QQ 扫码绑定失败",
                            "",
                            "",
                            "QQ connector 未返回可用二维码且本地生成失败",
                        );
                        break;
                    }
                    let _ = write_mobile_channel_qr_state(
                        &app_for_thread,
                        "qq",
                        "请用 QQ 扫描二维码完成机器人绑定",
                        &url,
                        &data_url,
                        "",
                    );
                }
                "success" => {
                    let credentials_raw = payload
                        .get("credentials")
                        .cloned()
                        .unwrap_or_else(|| serde_json::json!({}));

                    // QQ connector 返回的 credentials 是数组 [{"appId":"...","appSecret":"..."}]，取第一个元素
                    let credentials = if credentials_raw.is_array() {
                        credentials_raw.get(0).cloned().unwrap_or_else(|| serde_json::json!({}))
                    } else {
                        credentials_raw
                    };

                    let app_id = json_string(&credentials, &["appId", "appid", "app_id", "id"]);
                    let app_secret = json_string(
                        &credentials,
                        &[
                            "appSecret",
                            "appsecret",
                            "app_secret",
                            "clientSecret",
                            "client_secret",
                        ],
                    );
                    let _ = update_mobile_channel_credentials_from_qr(
                        &app_for_thread,
                        "qq",
                        &app_id,
                        &app_secret,
                        "",
                        "",
                        "",
                        "",
                    );
                    break;
                }
                "expired" => {
                    let _ = write_mobile_channel_qr_state(
                        &app_for_thread,
                        "qq",
                        "二维码已过期，请重新绑定",
                        "",
                        "",
                        "QQ 扫码二维码已过期",
                    );
                    break;
                }
                "failure" => {
                    let message = json_string(&payload, &["message"]);
                    let _ = write_mobile_channel_qr_state(
                        &app_for_thread,
                        "qq",
                        "QQ 扫码绑定失败",
                        "",
                        "",
                        &message,
                    );
                    break;
                }
                _ => {}
            }
        }
        let _ = child.wait();
        QQ_QR_ACTIVE.store(false, Ordering::SeqCst);
    });

    Ok(build_toolbox_snapshot(&app))
}

fn wechat_qr_data_url_from_content(value: &str) -> String {
    let raw = value.trim();
    if raw.starts_with("data:image/") {
        raw.to_string()
    } else {
        let compact = raw.replace(['\r', '\n', ' '], "");
        let image_mime = if compact.starts_with("iVBORw0KGgo") {
            Some("image/png")
        } else if compact.starts_with("/9j/") {
            Some("image/jpeg")
        } else if compact.starts_with("R0lGOD") {
            Some("image/gif")
        } else if compact.starts_with("UklGR") {
            Some("image/webp")
        } else {
            None
        };
        image_mime
            .map(|mime| format!("data:{mime};base64,{compact}"))
            .unwrap_or_default()
    }
}

fn request_wechat_qr_binding(app: &tauri::AppHandle) -> Result<(), String> {
    let client = build_http_client(15)?;
    let base_url = default_mobile_channel_base_url("wechat");
    let url = format!("{base_url}/ilink/bot/get_bot_qrcode?bot_type=3");
    let response = client
        .post(&url)
        .json(&serde_json::json!({ "local_token_list": [] }))
        .send()
        .map_err(|e| format!("微信二维码请求失败: {e}"))?;
    let status = response.status();
    let body = response
        .text()
        .map_err(|e| format!("微信二维码响应读取失败: {e}"))?;
    if !status.is_success() {
        return Err(format!(
            "微信二维码接口返回 HTTP {}：{}",
            status.as_u16(),
            body
        ));
    }
    let result: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("微信二维码返回不是 JSON: {e}; {body}"))?;
    if !is_platform_code_ok(&result) {
        return Err(format!(
            "微信二维码生成失败：{}",
            platform_error_message(&result, "接口返回异常")
        ));
    }
    let qrcode = json_string(&result, &["qrcode"]);
    let image_content = json_string(&result, &["qrcode_img_content", "qrcodeImageContent"]);
    let qr_url = json_string(&result, &["qrcode_url", "qrcodeUrl", "url"]);
    if qrcode.is_empty() {
        return Err(format!("微信二维码接口未返回绑定码：{result}"));
    }

    let mut qr_data_url = wechat_qr_data_url_from_content(&image_content);
    if qr_data_url.is_empty()
        && (image_content.starts_with("http://") || image_content.starts_with("https://"))
    {
        // 微信 iLink 返回的 qrcode_img_content 是个 liteapp 短链(HTML页面),不是真图片。
        // 用本地 qrcode crate 把这个 URL 编码成二维码图片,用户扫短链即可绑定。
        qr_data_url = generate_qr_code_data_url(&image_content).unwrap_or_default();
    }
    if qr_data_url.is_empty() && (qr_url.starts_with("http://") || qr_url.starts_with("https://")) {
        qr_data_url = generate_qr_code_data_url(&qr_url).unwrap_or_default();
    }
    if qr_data_url
        .trim()
        .to_ascii_lowercase()
        .starts_with("data:image/svg+xml")
    {
        qr_data_url.clear();
    }
    if qr_data_url.is_empty() {
        return Err("微信接口没有返回真实可扫码二维码图片，已拒绝展示文本二维码；请重新生成或检查 iLink 接口返回".into());
    }

    let mut state = read_toolbox_state(app);
    if let Some(binding) = state
        .mobile_channels
        .iter_mut()
        .find(|binding| binding.channel == "wechat")
    {
        binding.qr_url.clear();
        binding.qr_data_url = qr_data_url.clone();
        binding.qr_device_code = qrcode;
        binding.qr_status = "请用手机微信扫描二维码，并在手机上确认绑定".into();
        binding.base_url = base_url.into();
        binding.qr_started_at = chrono_now();
        binding.updated_at = chrono_now();
        binding.last_error.clear();
    }
    write_toolbox_state(app, &state)
}

#[tauri::command]
fn start_wechat_qr_binding(app: tauri::AppHandle) -> Result<ToolboxSnapshot, String> {
    if WECHAT_QR_ACTIVE.swap(true, Ordering::SeqCst) {
        let _ = write_mobile_channel_qr_state(
            &app,
            "wechat",
            "微信二维码正在生成中，请稍等",
            "",
            "",
            "",
        );
        return Ok(build_toolbox_snapshot(&app));
    }

    let _ = write_mobile_channel_qr_state(&app, "wechat", "正在请求微信绑定二维码...", "", "", "");
    let app_for_thread = app.clone();
    std::thread::spawn(move || {
        if let Err(error) = request_wechat_qr_binding(&app_for_thread) {
            let _ = write_mobile_channel_qr_state(
                &app_for_thread,
                "wechat",
                "微信二维码生成失败",
                "",
                "",
                &error,
            );
        }
        WECHAT_QR_ACTIVE.store(false, Ordering::SeqCst);
    });
    Ok(build_toolbox_snapshot(&app))
}

#[tauri::command]
fn poll_wechat_qr_binding(
    app: tauri::AppHandle,
    verify_code: Option<String>,
) -> Result<ToolboxSnapshot, String> {
    let state = read_toolbox_state(&app);
    let binding = state
        .mobile_channels
        .iter()
        .find(|binding| binding.channel == "wechat")
        .cloned()
        .ok_or("微信通道不存在")?;
    if binding.qr_device_code.trim().is_empty() {
        return Err("请先生成微信绑定二维码".into());
    }
    let client = build_http_client(35)?;
    let mut url = format!(
        "{}/ilink/bot/get_qrcode_status?qrcode={}",
        normalize_mobile_base_url(&binding.base_url, "wechat"),
        percent_encode_query_value(&binding.qr_device_code)
    );
    if let Some(code) = verify_code {
        if !code.trim().is_empty() {
            url.push_str("&verify_code=");
            url.push_str(&percent_encode_query_value(code.trim()));
        }
    }
    let response = client
        .get(&url)
        .send()
        .map_err(|e| format!("微信扫码状态查询失败: {e}"))?;
    let status = response.status();
    let body = response
        .text()
        .map_err(|e| format!("微信扫码状态响应读取失败: {e}"))?;
    if !status.is_success() {
        return Err(format!(
            "微信扫码状态返回 HTTP {}：{}",
            status.as_u16(),
            body
        ));
    }
    let result: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| format!("微信扫码状态返回不是 JSON: {e}; {body}"))?;
    let qr_status = json_string(&result, &["status"]);
    if qr_status == "confirmed" {
        let bot_token = json_string(&result, &["bot_token"]);
        let account_id = json_string(&result, &["ilink_bot_id", "account_id"]);
        let base_url = json_string(&result, &["baseurl", "base_url"]);
        let user_id = json_string(&result, &["ilink_user_id", "user_id"]);
        return update_mobile_channel_credentials_from_qr(
            &app,
            "wechat",
            "",
            "",
            &bot_token,
            &account_id,
            &base_url,
            &user_id,
        );
    }
    let message = match qr_status.as_str() {
        "scaned" => "微信已扫码，请在手机上确认",
        "need_verifycode" => "微信需要验证码，请按手机提示输入验证码",
        "expired" => "微信二维码已过期，请重新生成",
        "binded_redirect" => "该微信机器人已绑定过，请重新生成或换绑",
        _ => "等待微信扫码确认",
    };
    write_mobile_channel_qr_state(
        &app,
        "wechat",
        message,
        &binding.qr_url,
        &binding.qr_data_url,
        "",
    )
}

#[tauri::command]
fn bind_codex_thread(
    app: tauri::AppHandle,
    channel: String,
    thread_id: String,
    sync_enabled: bool,
    note: Option<String>,
) -> Result<ToolboxSnapshot, String> {
    let normalized_channel = normalize_mobile_channel(&channel);
    log_info!(
        "[mobile-control][bind] request: channel={}, normalized={}, thread_id={}, sync_enabled={}, note={}",
        channel,
        normalized_channel,
        thread_id,
        sync_enabled,
        note.as_deref().unwrap_or("")
    );
    if normalized_channel.is_empty() {
        log_error!("[mobile-control][bind] failed: empty channel");
        return Err("绑定通道不能为空".into());
    }
    let state = read_toolbox_state(&app);
    let thread_pool = if state.synced_codex_threads.is_empty() {
        read_codex_threads(200)
    } else {
        state.synced_codex_threads.clone()
    };
    let thread = thread_pool
        .into_iter()
        .find(|item| item.id == thread_id)
        .ok_or("会话未找到")?;
    log_info!(
        "[mobile-control][bind] found thread: channel={}, thread_id={}, thread_name={}, session_file={}, cwd={}",
        normalized_channel,
        thread.id,
        thread.thread_name,
        thread.session_file,
        thread.cwd
    );
    let mut state = state;
    let thread_id_value = thread.id.clone();
    let thread_name_value = thread.thread_name.clone();
    let session_file_value = thread.session_file.clone();
    let updated_at_value = thread.updated_at.clone();
    let cwd_value = thread.cwd.clone();
    state
        .session_bindings
        .retain(|binding| normalize_mobile_channel(&binding.channel) != normalized_channel);
    state.session_bindings.push(CodexSessionBinding {
        channel: normalized_channel.clone(),
        thread_id: thread_id_value.clone(),
        thread_name: thread_name_value.clone(),
        session_file: session_file_value.clone(),
        updated_at: updated_at_value,
        cwd: cwd_value,
        sync_enabled,
        last_synced_at: String::new(),
        note: note.unwrap_or_default().trim().to_string(),
    });
    if let Some(binding) = state
        .mobile_channels
        .iter_mut()
        .find(|binding| binding.channel == normalized_channel)
    {
        binding.thread_id = thread_id_value.clone();
        binding.thread_name = thread_name_value.clone();
        binding.session_file = session_file_value.clone();
        binding.enabled = true;
        binding.status = "已绑定，等待开启平台连接".into();
        binding.last_error.clear();
        binding.updated_at = chrono_now();
    } else {
        let mut mobile_binding = default_mobile_channel(&normalized_channel);
        mobile_binding.thread_id = thread_id_value.clone();
        mobile_binding.thread_name = thread_name_value.clone();
        mobile_binding.session_file = session_file_value;
        mobile_binding.enabled = true;
        mobile_binding.status = "已绑定，等待开启平台连接".into();
        mobile_binding.updated_at = chrono_now();
        state.mobile_channels.push(mobile_binding);
    }
    state.selected_mobile_thread_id = thread_id_value.clone();
    state.mobile_remote.active_thread_id = thread_id_value;
    state.mobile_remote.active_thread_name = thread_name_value;
    write_toolbox_state(&app, &state)?;
    log_info!(
        "[mobile-control][bind] saved: channel={}, thread_id={}",
        normalized_channel, state.mobile_remote.active_thread_id
    );
    Ok(build_toolbox_snapshot(&app))
}

#[tauri::command]
fn unbind_codex_thread(app: tauri::AppHandle, channel: String) -> Result<ToolboxSnapshot, String> {
    let normalized_channel = normalize_mobile_channel(&channel);
    log_info!(
        "[mobile-control][unbind] request: channel={}, normalized={}",
        channel, normalized_channel
    );
    let mut state = read_toolbox_state(&app);
    state
        .session_bindings
        .retain(|binding| normalize_mobile_channel(&binding.channel) != normalized_channel);
    if let Some(binding) = state
        .mobile_channels
        .iter_mut()
        .find(|binding| binding.channel == normalized_channel)
    {
        binding.thread_id.clear();
        binding.thread_name.clear();
        binding.session_file.clear();
        binding.enabled = false;
        binding.listening = false;
        binding.status = "未绑定".into();
        binding.last_error.clear();
        binding.updated_at = chrono_now();
    }
    write_toolbox_state(&app, &state)?;
    log_info!(
        "[mobile-control][unbind] saved: channel={}",
        normalized_channel
    );
    Ok(build_toolbox_snapshot(&app))
}

#[tauri::command]
fn start_mobile_remote(
    app: tauri::AppHandle,
    state: State<AppState>,
    port: Option<u16>,
) -> Result<ToolboxSnapshot, String> {
    let _ = state;
    let _ = port;
    log_info!("[mobile-control][start] start_mobile_remote requested");
    MOBILE_REMOTE_CANCEL_REQUESTED.store(false, Ordering::SeqCst);
    let already_starting = MOBILE_REMOTE_START_ACTIVE.swap(true, Ordering::SeqCst);
    let mut toolbox_state = read_toolbox_state(&app);
    toolbox_state.mobile_remote.enabled = true;
    toolbox_state.mobile_remote.mode = "platform_bot".into();
    toolbox_state.mobile_remote.listen_addr = "platform".into();
    toolbox_state.mobile_remote.last_started_at = chrono_now();
    toolbox_state.mobile_remote.last_error.clear();
    toolbox_state.mobile_remote.device_name = local_device_name();
    toolbox_state.mobile_remote.local_ip = detect_local_ip();
    for channel in toolbox_state.mobile_channels.iter_mut() {
        if channel.enabled && !channel.thread_id.trim().is_empty() {
            channel.listening = false;
            channel.status = "正在连接平台...".into();
            channel.last_error.clear();
            channel.updated_at = chrono_now();
        }
    }
    write_toolbox_state(&app, &toolbox_state)?;
    if !already_starting {
        log_info!("[mobile-control][start] spawning platform connection worker");
        let app_for_thread = app.clone();
        std::thread::spawn(move || run_mobile_remote_start(app_for_thread));
    } else {
        log_info!("[mobile-control][start] worker already starting, skip spawn");
    }
    Ok(build_toolbox_snapshot(&app))
}

fn run_mobile_remote_start(app: tauri::AppHandle) {
    log_info!("[mobile-control][start-worker] begin");
    let mut toolbox_state = read_toolbox_state(&app);
    let mut connected_count = 0usize;
    let mut last_error = String::new();
    let mut lark_bridge_binding: Option<MobileChannelBinding> = None;
    let mut qq_gateway_binding: Option<MobileChannelBinding> = None;
    let mut wechat_listener_binding: Option<MobileChannelBinding> = None;
    for channel in toolbox_state.mobile_channels.iter_mut() {
        if MOBILE_REMOTE_CANCEL_REQUESTED.load(Ordering::SeqCst) {
            log_info!("[mobile-control][start-worker] cancelled before channel probe");
            MOBILE_REMOTE_START_ACTIVE.store(false, Ordering::SeqCst);
            return;
        }
        if channel.enabled && !channel.thread_id.trim().is_empty() {
            log_info!(
                "[mobile-control][start-worker] probing channel: channel={}, thread_id={}, thread_name={}",
                channel.channel, channel.thread_id, channel.thread_name
            );
            if !mobile_channel_has_credentials(channel) {
                channel.listening = false;
                channel.credential_status = mobile_channel_credential_hint(&channel.channel).into();
                channel.last_error = channel.credential_status.clone();
                channel.status = "缺少平台凭据，无法开启连接".into();
                channel.updated_at = chrono_now();
                last_error = channel.last_error.clone();
                log_info!(
                    "[mobile-control][start-worker] missing credentials: channel={}, error={}",
                    channel.channel, channel.last_error
                );
                continue;
            }
            match probe_mobile_channel(channel) {
                Ok((gateway_url, status)) => {
                    log_info!(
                        "[mobile-control][start-worker] probe ok: channel={}, gateway={}, status={}",
                        channel.channel, gateway_url, status
                    );
                    connected_count += 1;
                    channel.gateway_url = gateway_url;
                    channel.listening = true;
                    channel.status = status;
                    channel.credential_status = "平台凭据已验证".into();
                    channel.last_error.clear();
                    channel.last_checked_at = chrono_now();
                    channel.updated_at = chrono_now();
                    match channel.channel.as_str() {
                        "lark" => lark_bridge_binding = Some(channel.clone()),
                        "qq" => qq_gateway_binding = Some(channel.clone()),
                        "wechat" => wechat_listener_binding = Some(channel.clone()),
                        _ => {}
                    }
                }
                Err(error) => {
                    log_info!(
                        "[mobile-control][start-worker] probe failed: channel={}, error={}",
                        channel.channel, error
                    );
                    channel.listening = false;
                    channel.status = "平台连接失败".into();
                    channel.last_error = error.clone();
                    channel.last_checked_at = chrono_now();
                    channel.updated_at = chrono_now();
                    last_error = error;
                }
            }
        }
    }
    if MOBILE_REMOTE_CANCEL_REQUESTED.load(Ordering::SeqCst) {
        log_info!("[mobile-control][start-worker] cancelled after channel probe");
        MOBILE_REMOTE_START_ACTIVE.store(false, Ordering::SeqCst);
        return;
    }
    if connected_count == 0 {
        log_info!(
            "[mobile-control][start-worker] no channels connected: last_error={}",
            last_error
        );
        toolbox_state.mobile_remote.enabled = false;
        toolbox_state.mobile_remote.last_error = if last_error.is_empty() {
            "没有可连接的手机通道，请先绑定对话并填写平台凭据".into()
        } else {
            last_error
        };
    }
    let _ = write_toolbox_state(&app, &toolbox_state);
    log_info!(
        "[mobile-control][start-worker] state saved: enabled={}, connected_count={}",
        toolbox_state.mobile_remote.enabled, connected_count
    );
    if toolbox_state.mobile_remote.enabled {
        if let Some(binding) = lark_bridge_binding {
            log_info!("[mobile-control][start-worker] starting lark bridge");
            if let Err(error) = start_lark_bridge(app.clone(), binding) {
                log_info!(
                    "[mobile-control][start-worker] lark bridge failed: {}",
                    error
                );
                update_channel_status(&app, "lark", "飞书消息桥启动失败", &error);
            }
        }
        if let Some(binding) = qq_gateway_binding {
            log_info!("[mobile-control][start-worker] starting qq gateway");
            if let Err(error) = start_qq_gateway(app.clone(), binding) {
                log_info!(
                    "[mobile-control][start-worker] qq gateway failed: {}",
                    error
                );
                update_channel_status(&app, "qq", "QQ 网关启动失败", &error);
            }
        }
        if let Some(binding) = wechat_listener_binding {
            log_info!("[mobile-control][start-worker] starting wechat listener");
            if let Err(error) = start_wechat_listener(app.clone(), binding) {
                log_info!(
                    "[mobile-control][start-worker] wechat listener failed: {}",
                    error
                );
                update_channel_status(&app, "wechat", "微信监听启动失败", &error);
            }
        }
    }
    MOBILE_REMOTE_START_ACTIVE.store(false, Ordering::SeqCst);
    log_info!("[mobile-control][start-worker] finished");

    // 看门狗：后台持续监控各平台连接，断线后自动重启（最多等 5 分钟才重试）
    if toolbox_state.mobile_remote.enabled {
        let watchdog_app = app.clone();
        std::thread::spawn(move || {
            let mut fail_counts: std::collections::HashMap<&str, u32> = std::collections::HashMap::new();
            loop {
                // 每 30 秒轮询一次
                std::thread::sleep(Duration::from_secs(30));
                if MOBILE_REMOTE_CANCEL_REQUESTED.load(Ordering::SeqCst) {
                    log_info!("[mobile-control][watchdog] 已停止（用户取消）");
                    break;
                }
                let state = read_toolbox_state(&watchdog_app);
                if !state.mobile_remote.enabled {
                    log_info!("[mobile-control][watchdog] remote 已禁用，看门狗退出");
                    break;
                }
                // 检查飞书
                if !LARK_BRIDGE_ACTIVE.load(Ordering::SeqCst) {
                    if let Some(binding) = state.mobile_channels.iter().find(|b| b.channel == "lark" && mobile_channel_has_credentials(b)).cloned() {
                        let cnt = fail_counts.entry("lark").or_insert(0);
                        let delay = (30u64 * (1 << (*cnt).min(4))).min(300);
                        log_info!("[mobile-control][watchdog] 飞书断线，{}s 后重启（第{}次）", delay, *cnt + 1);
                        std::thread::sleep(Duration::from_secs(delay));
                        match start_lark_bridge(watchdog_app.clone(), binding) {
                            Ok(_) => { *cnt = 0; update_channel_status(&watchdog_app, "lark", "飞书已自动重连", ""); }
                            Err(e) => { *cnt += 1; update_channel_status(&watchdog_app, "lark", "飞书自动重连失败", &e); }
                        }
                    }
                } else {
                    fail_counts.remove("lark");
                }
                // 检查 QQ
                if !QQ_GATEWAY_ACTIVE.load(Ordering::SeqCst) {
                    if let Some(binding) = state.mobile_channels.iter().find(|b| b.channel == "qq" && mobile_channel_has_credentials(b)).cloned() {
                        let cnt = fail_counts.entry("qq").or_insert(0);
                        let delay = (30u64 * (1 << (*cnt).min(4))).min(300);
                        log_info!("[mobile-control][watchdog] QQ 断线，{}s 后重启（第{}次）", delay, *cnt + 1);
                        std::thread::sleep(Duration::from_secs(delay));
                        match start_qq_gateway(watchdog_app.clone(), binding) {
                            Ok(_) => { *cnt = 0; update_channel_status(&watchdog_app, "qq", "QQ 已自动重连", ""); }
                            Err(e) => { *cnt += 1; update_channel_status(&watchdog_app, "qq", "QQ 自动重连失败", &e); }
                        }
                    }
                } else {
                    fail_counts.remove("qq");
                }
                // 检查微信
                if !WECHAT_LISTENER_ACTIVE.load(Ordering::SeqCst) {
                    if let Some(binding) = state.mobile_channels.iter().find(|b| b.channel == "wechat" && mobile_channel_has_credentials(b)).cloned() {
                        let cnt = fail_counts.entry("wechat").or_insert(0);
                        let delay = (30u64 * (1 << (*cnt).min(4))).min(300);
                        log_info!("[mobile-control][watchdog] 微信断线，{}s 后重启（第{}次）", delay, *cnt + 1);
                        std::thread::sleep(Duration::from_secs(delay));
                        match start_wechat_listener(watchdog_app.clone(), binding) {
                            Ok(_) => { *cnt = 0; update_channel_status(&watchdog_app, "wechat", "微信已自动重连", ""); }
                            Err(e) => { *cnt += 1; update_channel_status(&watchdog_app, "wechat", "微信自动重连失败", &e); }
                        }
                    }
                } else {
                    fail_counts.remove("wechat");
                }
            }
        });
    }
}

#[tauri::command]
fn stop_mobile_remote(
    app: tauri::AppHandle,
    state: State<AppState>,
) -> Result<ToolboxSnapshot, String> {
    let _ = state;
    log_info!("[mobile-control][stop] stop_mobile_remote requested");
    MOBILE_REMOTE_CANCEL_REQUESTED.store(true, Ordering::SeqCst);
    MOBILE_REMOTE_START_ACTIVE.store(false, Ordering::SeqCst);
    stop_lark_bridge();
    stop_qq_gateway();
    stop_wechat_listener(&app);
    let mut toolbox_state = read_toolbox_state(&app);
    toolbox_state.mobile_remote.enabled = false;
    toolbox_state.mobile_remote.last_error.clear();
    toolbox_state.mobile_remote.local_ip = detect_local_ip();
    for channel in toolbox_state.mobile_channels.iter_mut() {
        channel.listening = false;
        if channel.enabled {
            channel.status = "已绑定，平台连接已停止".into();
            channel.updated_at = chrono_now();
        }
    }
    write_toolbox_state(&app, &toolbox_state)?;
    log_info!("[mobile-control][stop] stopped and state saved");
    Ok(build_toolbox_snapshot(&app))
}

// ── Skills Commands ──────────────────────────────────

// ── Settings Helpers ─────────────────────────────────

fn settings_path(app: &tauri::AppHandle) -> PathBuf {
    data_dir(app).join("settings.json")
}

fn read_app_settings(app: &tauri::AppHandle) -> AppSettings {
    let path = settings_path(app);
    if !path.exists() {
        return AppSettings::default();
    }
    normalize_app_settings(
        fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default(),
    )
}

fn write_app_settings(app: &tauri::AppHandle, settings: &AppSettings) -> Result<(), String> {
    let path = settings_path(app);
    let normalized = normalize_app_settings(settings.clone());
    let json = serde_json::to_string_pretty(&normalized).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())
}

/// Windows 开机自启：写入/删除注册表 Run 键
#[cfg(target_os = "windows")]
fn set_auto_start(enable: bool) -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let run_key = hkcu
        .open_subkey_with_flags(
            "Software\\Microsoft\\Windows\\CurrentVersion\\Run",
            winreg::enums::KEY_SET_VALUE,
        )
        .map_err(|e| e.to_string())?;

    const APP_NAME: &str = "VarSwitch";

    if enable {
        // 获取当前可执行文件路径
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let exe_str = exe.to_string_lossy().to_string();
        run_key
            .set_value(APP_NAME, &exe_str)
            .map_err(|e| e.to_string())
    } else {
        match run_key.delete_value(APP_NAME) {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

/// macOS 开机自启：通过 LaunchAgent plist 实现
#[cfg(target_os = "macos")]
fn set_auto_start(enable: bool) -> Result<(), String> {
    let home = std::env::var("HOME").map_err(|_| "无法获取 HOME 目录".to_string())?;
    let launch_agents_dir = PathBuf::from(&home).join("Library").join("LaunchAgents");
    let plist_path = launch_agents_dir.join("com.varswitch.app.plist");

    if enable {
        fs::create_dir_all(&launch_agents_dir).map_err(|e| e.to_string())?;
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let exe_str = exe.to_string_lossy();
        let plist_content = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.varswitch.app</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
</dict>
</plist>"#,
            exe_str
        );
        fs::write(&plist_path, plist_content).map_err(|e| e.to_string())
    } else {
        match fs::remove_file(&plist_path) {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn set_auto_start(_enable: bool) -> Result<(), String> {
    Ok(())
}

// ── Settings Commands ────────────────────────────────

#[tauri::command]
fn get_app_settings(app: tauri::AppHandle) -> AppSettings {
    read_app_settings(&app)
}

#[tauri::command]
fn save_app_settings(app: tauri::AppHandle, settings: AppSettings) -> Result<(), String> {
    let settings = normalize_app_settings(settings);
    // 处理开机自启
    set_auto_start(settings.auto_start)?;
    write_app_settings(&app, &settings)
}

#[tauri::command]
fn get_app_paths(app: tauri::AppHandle) -> AppPaths {
    let settings = read_app_settings(&app);
    AppPaths {
        config_dir: data_dir(&app).to_string_lossy().to_string(),
        profiles_path: profiles_path(&app).to_string_lossy().to_string(),
        claude_settings: claude_settings_path().to_string_lossy().to_string(),
        editor_settings: collect_editor_path_infos(&settings),
        claude_md: claude_md_path().to_string_lossy().to_string(),
        claude_mcp: claude_mcp_path().to_string_lossy().to_string(),
    }
}

#[tauri::command]
fn open_folder(path: String) -> Result<(), String> {
    let p = PathBuf::from(&path);
    // 如果是文件，打开其所在目录
    let dir = if p.is_file() || p.extension().is_some() {
        p.parent().unwrap_or(&p).to_path_buf()
    } else {
        p
    };
    #[cfg(target_os = "windows")]
    {
        let mut cmd = std::process::Command::new("explorer");
        cmd.arg(dir.to_string_lossy().to_string());
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd.spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(not(target_os = "windows"))]
    {
        #[cfg(target_os = "macos")]
        let cmd = "open";
        #[cfg(not(target_os = "macos"))]
        let cmd = "xdg-open";
        std::process::Command::new(cmd)
            .arg(dir.to_string_lossy().to_string())
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn open_external_target(target: String) -> Result<(), String> {
    let trimmed = target.trim();
    if trimmed.is_empty() {
        return Err("Target is required".into());
    }
    open_with_system(trimmed)
}

/// 打开日志文件夹，方便用户查看/反馈日志。
#[tauri::command]
fn open_logs_folder(app: tauri::AppHandle) -> Result<(), String> {
    let dir = data_dir(&app).join("logs");
    fs::create_dir_all(&dir).map_err(|e| format!("创建日志目录失败: {e}"))?;
    open_folder(dir.to_string_lossy().to_string())
}

#[tauri::command]
async fn check_app_update(app: tauri::AppHandle) -> Result<UpdateCheckResult, String> {
    let current_version = app.package_info().version.to_string();

    tauri::async_runtime::spawn_blocking(move || {
        let release = fetch_latest_release()?;
        let asset = select_release_asset(
            &release.assets,
            std::env::consts::OS,
            std::env::consts::ARCH,
        );

        Ok(UpdateCheckResult {
            current_version: current_version.clone(),
            latest_version: release.tag_name.clone(),
            has_update: is_remote_version_newer(&release.tag_name, &current_version),
            release_url: if release.html_url.is_empty() {
                format!("{}/releases", GITHUB_REPO_URL)
            } else {
                release.html_url
            },
            release_notes: release.body,
            published_at: release.published_at,
            asset_name: asset.as_ref().map(|item| item.name.clone()),
            can_auto_update: asset.is_some(),
        })
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?
}

#[tauri::command]
async fn download_and_open_update(app: tauri::AppHandle) -> Result<UpdateDownloadResult, String> {
    let current_version = app.package_info().version.to_string();
    let app_handle = app.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let release = fetch_latest_release()?;
        if !is_remote_version_newer(&release.tag_name, &current_version) {
            return Err("Already on the latest version".into());
        }

        let asset = select_release_asset(
            &release.assets,
            std::env::consts::OS,
            std::env::consts::ARCH,
        )
        .ok_or_else(|| "No installer found for the current platform".to_string())?;

        let client = build_http_client(120)?;
        let resp = client
            .get(&asset.browser_download_url)
            .send()
            .map_err(|e| format!("Download error: {}", e))?;

        if !resp.status().is_success() {
            return Err(format!("Download failed with {}", resp.status()));
        }

        let bytes = resp
            .bytes()
            .map_err(|e| format!("Read download failed: {}", e))?;

        let update_dir = data_dir(&app_handle).join("updates");
        fs::create_dir_all(&update_dir).map_err(|e| e.to_string())?;
        let file_path = update_dir.join(&asset.name);
        fs::write(&file_path, &bytes).map_err(|e| e.to_string())?;

        let file_path_str = file_path.to_string_lossy().to_string();
        open_with_system(&file_path_str)?;

        Ok(UpdateDownloadResult {
            latest_version: release.tag_name,
            file_name: asset.name,
            file_path: file_path_str,
            release_url: if release.html_url.is_empty() {
                format!("{}/releases", GITHUB_REPO_URL)
            } else {
                release.html_url
            },
        })
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?
}

#[tauri::command]
fn export_profiles(app: tauri::AppHandle, dest: String) -> Result<(), String> {
    let src = profiles_path(&app);
    if !src.exists() {
        return Err("配置文件不存在".into());
    }
    fs::copy(&src, &dest).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn import_profiles(app: tauri::AppHandle, src: String) -> Result<usize, String> {
    let src_path = PathBuf::from(&src);
    if !src_path.exists() {
        return Err("文件不存在".into());
    }
    let content = fs::read_to_string(&src_path).map_err(|e| e.to_string())?;
    let imported: ProfilesData =
        serde_json::from_str(&content).map_err(|_| "文件格式无效".to_string())?;
    let count = imported.profiles.len();
    if count == 0 {
        return Err("文件中没有配置".into());
    }
    // 合并到现有配置（跳过重复的 api_key+base_url）
    let mut data = read_profiles(&app);
    let mut added = 0;
    for mut p in imported.profiles {
        let exists = data
            .profiles
            .iter()
            .any(|x| x.api_key == p.api_key && x.base_url == p.base_url);
        if !exists {
            // 为空的 id 和 createdAt 生成有效值，确保导入的配置可以正常编辑/删除
            if p.id.is_empty() {
                p.id = uuid::Uuid::new_v4().to_string();
            }
            if p.created_at.is_empty() {
                p.created_at = chrono_now();
            }
            data.profiles.push(p);
            added += 1;
        }
    }
    write_profiles(&app, &data)?;
    Ok(added)
}

// ── Skills Commands ──────────────────────────────────

/// Recursively collect .md skill files from a directory.
/// Files in subdirectories get names like "subfolder:filename".
fn collect_skills_recursive(base: &PathBuf, current: &PathBuf, skills: &mut Vec<SkillInfo>) {
    let entries = match fs::read_dir(current) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_skills_recursive(base, &path, skills);
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            // Build relative prefix from base dir (e.g. "subfolder:command")
            let parent = path.parent().unwrap_or(base);
            let name = if parent != base.as_path() {
                if let Ok(rel) = parent.strip_prefix(base) {
                    let prefix = rel.to_string_lossy().replace(['/', '\\'], ":");
                    format!("{}:{}", prefix, stem)
                } else {
                    stem
                }
            } else {
                stem
            };
            let content = fs::read_to_string(&path).unwrap_or_default();
            skills.push(SkillInfo {
                name,
                content,
                source_type: "command".into(),
                description: String::new(),
            });
        }
    }
}

#[tauri::command]
fn get_skills() -> Result<Vec<SkillInfo>, String> {
    let mut skills = Vec::new();

    // 扫描 ~/.claude/commands/ (斜杠命令)
    let cmd_dir = claude_commands_dir();
    if cmd_dir.exists() {
        collect_skills_recursive(&cmd_dir, &cmd_dir, &mut skills);
    }

    // 扫描 ~/.claude/skills/ (自动加载技能)
    collect_skills_from_skills_dir(&mut skills);

    skills.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(skills)
}

/// Convert a skill name like "subfolder:command" to a file path (commands dir)
fn skill_name_to_path(name: &str) -> PathBuf {
    let dir = claude_commands_dir();
    let parts: Vec<&str> = name.split(':').collect();
    if parts.len() > 1 {
        let mut path = dir;
        for part in &parts[..parts.len() - 1] {
            path = path.join(part);
        }
        path.join(format!("{}.md", parts.last().unwrap()))
    } else {
        dir.join(format!("{}.md", name))
    }
}

/// 根据 sourceType 获取技能文件路径
fn skill_path_by_type(name: &str, source_type: &str) -> PathBuf {
    if source_type == "skill" {
        claude_skills_dir().join(name).join("SKILL.md")
    } else {
        skill_name_to_path(name)
    }
}

#[tauri::command]
fn save_skill(name: String, content: String, source_type: Option<String>) -> Result<(), String> {
    if name.is_empty() {
        return Err("技能名称不能为空".into());
    }
    let st = source_type.as_deref().unwrap_or("command");
    let path = skill_path_by_type(&name, st);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(path, content).map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_skill(name: String, source_type: Option<String>) -> Result<(), String> {
    let st = source_type.as_deref().unwrap_or("command");
    if st == "skill" {
        // 删除整个技能目录
        let dir = claude_skills_dir().join(&name);
        if dir.exists() && dir.is_dir() {
            fs::remove_dir_all(&dir).map_err(|e| e.to_string())
        } else {
            Ok(())
        }
    } else {
        let path = skill_name_to_path(&name);
        if path.exists() {
            fs::remove_file(path).map_err(|e| e.to_string())
        } else {
            Ok(())
        }
    }
}

// ── Skills Discovery ─────────────────────────────────

/// A skill available in the curated catalog or from GitHub search
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CatalogSkill {
    name: String,
    description: String,
    description_zh: String,
    /// GitHub raw URL to download the SKILL.md / command .md
    download_url: String,
    /// Source repo label e.g. "anthropics/skills"
    source: String,
    /// Category tag
    category: String,
    /// Whether this skill is installed locally
    installed: bool,
    /// GitHub stars count (0 for catalog items)
    stars: u64,
    /// GitHub repo URL for linking
    repo_url: String,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct SkillRepo {
    url: String,
    branch: String,
    enabled: bool,
}

#[derive(Serialize, Deserialize, Default)]
struct SkillReposData {
    repos: Vec<SkillRepo>,
}

// ── Skills Discovery Helpers ─────────────────────────

fn skill_repos_path(app: &tauri::AppHandle) -> PathBuf {
    data_dir(app).join("skill_repos.json")
}

fn read_skill_repos(app: &tauri::AppHandle) -> SkillReposData {
    let path = skill_repos_path(app);
    if !path.exists() {
        return SkillReposData::default();
    }
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn write_skill_repos(app: &tauri::AppHandle, data: &SkillReposData) -> Result<(), String> {
    let path = skill_repos_path(app);
    let json = serde_json::to_string_pretty(data).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())
}

fn collect_skill_names_recursive(base: &PathBuf, current: &PathBuf, names: &mut Vec<String>) {
    let entries = match fs::read_dir(current) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_skill_names_recursive(base, &path, names);
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            let parent = path.parent().unwrap_or(base);
            let name = if parent != base.as_path() {
                if let Ok(rel) = parent.strip_prefix(base) {
                    let prefix = rel.to_string_lossy().replace(['/', '\\'], ":");
                    format!("{}:{}", prefix, stem)
                } else {
                    stem
                }
            } else {
                stem
            };
            names.push(name);
        }
    }
}

fn get_installed_skill_names() -> Vec<String> {
    let mut names = Vec::new();

    // 从 commands 目录收集
    let cmd_dir = claude_commands_dir();
    if cmd_dir.exists() {
        collect_skill_names_recursive(&cmd_dir, &cmd_dir, &mut names);
    }

    // 从 skills 目录收集（目录名即技能名）
    let skills_dir = claude_skills_dir();
    if skills_dir.exists() {
        if let Ok(entries) = fs::read_dir(&skills_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() && path.join("SKILL.md").exists() {
                    if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                        names.push(name.to_string());
                    }
                }
            }
        }
    }

    names.sort();
    names.dedup();
    names
}

/// Build the curated catalog of skills with install status
fn build_catalog() -> Vec<CatalogSkill> {
    let installed = get_installed_skill_names();
    let mut catalog = vec![
        // ── anthropics/skills (official) ──
        CatalogSkill {
            name: "pdf".into(),
            description: "PDF processing: read, merge, split, rotate, watermark, encrypt, OCR".into(),
            description_zh: "PDF 处理：读取、合并、拆分、旋转、水印、加密、OCR".into(),
            download_url: "https://raw.githubusercontent.com/anthropics/skills/main/skills/pdf/SKILL.md".into(),
            source: "anthropics/skills".into(),
            category: "document".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "docx".into(),
            description: "Word document creation and manipulation with python-docx".into(),
            description_zh: "使用 python-docx 创建和操作 Word 文档".into(),
            download_url: "https://raw.githubusercontent.com/anthropics/skills/main/skills/docx/SKILL.md".into(),
            source: "anthropics/skills".into(),
            category: "document".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "xlsx".into(),
            description: "Excel spreadsheet creation and data processing with openpyxl".into(),
            description_zh: "使用 openpyxl 创建 Excel 电子表格和数据处理".into(),
            download_url: "https://raw.githubusercontent.com/anthropics/skills/main/skills/xlsx/SKILL.md".into(),
            source: "anthropics/skills".into(),
            category: "document".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "pptx".into(),
            description: "PowerPoint presentation creation with python-pptx".into(),
            description_zh: "使用 python-pptx 创建 PowerPoint 演示文稿".into(),
            download_url: "https://raw.githubusercontent.com/anthropics/skills/main/skills/pptx/SKILL.md".into(),
            source: "anthropics/skills".into(),
            category: "document".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "frontend-design".into(),
            description: "Create production-grade frontend interfaces with modern web technologies".into(),
            description_zh: "使用现代 Web 技术创建生产级前端界面".into(),
            download_url: "https://raw.githubusercontent.com/anthropics/skills/main/skills/frontend-design/SKILL.md".into(),
            source: "anthropics/skills".into(),
            category: "development".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "canvas-design".into(),
            description: "Create interactive HTML5 Canvas visualizations and animations".into(),
            description_zh: "创建交互式 HTML5 Canvas 可视化和动画".into(),
            download_url: "https://raw.githubusercontent.com/anthropics/skills/main/skills/canvas-design/SKILL.md".into(),
            source: "anthropics/skills".into(),
            category: "design".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "algorithmic-art".into(),
            description: "Generate algorithmic and generative art using code".into(),
            description_zh: "使用代码生成算法艺术和生成艺术".into(),
            download_url: "https://raw.githubusercontent.com/anthropics/skills/main/skills/algorithmic-art/SKILL.md".into(),
            source: "anthropics/skills".into(),
            category: "design".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "theme-factory".into(),
            description: "Create consistent design themes and color systems".into(),
            description_zh: "创建一致的设计主题和颜色系统".into(),
            download_url: "https://raw.githubusercontent.com/anthropics/skills/main/skills/theme-factory/SKILL.md".into(),
            source: "anthropics/skills".into(),
            category: "design".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "mcp-builder".into(),
            description: "Build Model Context Protocol servers and tools".into(),
            description_zh: "构建 MCP (Model Context Protocol) 服务器和工具".into(),
            download_url: "https://raw.githubusercontent.com/anthropics/skills/main/skills/mcp-builder/SKILL.md".into(),
            source: "anthropics/skills".into(),
            category: "development".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "skill-creator".into(),
            description: "Create new Claude skills with proper structure and metadata".into(),
            description_zh: "创建具有正确结构和元数据的新 Claude 技能".into(),
            download_url: "https://raw.githubusercontent.com/anthropics/skills/main/skills/skill-creator/SKILL.md".into(),
            source: "anthropics/skills".into(),
            category: "development".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "web-artifacts-builder".into(),
            description: "Build interactive web artifacts and single-page applications".into(),
            description_zh: "构建交互式 Web 工件和单页应用".into(),
            download_url: "https://raw.githubusercontent.com/anthropics/skills/main/skills/web-artifacts-builder/SKILL.md".into(),
            source: "anthropics/skills".into(),
            category: "development".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "webapp-testing".into(),
            description: "Automated web application testing with Playwright and other tools".into(),
            description_zh: "使用 Playwright 等工具进行自动化 Web 应用测试".into(),
            download_url: "https://raw.githubusercontent.com/anthropics/skills/main/skills/webapp-testing/SKILL.md".into(),
            source: "anthropics/skills".into(),
            category: "testing".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "doc-coauthoring".into(),
            description: "Collaborative document writing and editing assistance".into(),
            description_zh: "协作文档写作和编辑辅助".into(),
            download_url: "https://raw.githubusercontent.com/anthropics/skills/main/skills/doc-coauthoring/SKILL.md".into(),
            source: "anthropics/skills".into(),
            category: "writing".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "brand-guidelines".into(),
            description: "Create and maintain brand identity guidelines".into(),
            description_zh: "创建和维护品牌识别指南".into(),
            download_url: "https://raw.githubusercontent.com/anthropics/skills/main/skills/brand-guidelines/SKILL.md".into(),
            source: "anthropics/skills".into(),
            category: "design".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "internal-comms".into(),
            description: "Draft internal communications, memos, and announcements".into(),
            description_zh: "起草内部通信、备忘录和公告".into(),
            download_url: "https://raw.githubusercontent.com/anthropics/skills/main/skills/internal-comms/SKILL.md".into(),
            source: "anthropics/skills".into(),
            category: "writing".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "slack-gif-creator".into(),
            description: "Create animated GIFs for Slack and messaging platforms".into(),
            description_zh: "为 Slack 和消息平台创建动画 GIF".into(),
            download_url: "https://raw.githubusercontent.com/anthropics/skills/main/skills/slack-gif-creator/SKILL.md".into(),
            source: "anthropics/skills".into(),
            category: "design".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        // ── Community skills ──
        CatalogSkill {
            name: "git-commit-message".into(),
            description: "Generate conventional commit messages following best practices".into(),
            description_zh: "按照最佳实践生成规范的 Git 提交信息".into(),
            download_url: "".into(),
            source: "community".into(),
            category: "development".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "code-review".into(),
            description: "Thorough code review with security, performance, and style checks".into(),
            description_zh: "全面的代码审查，包括安全、性能和风格检查".into(),
            download_url: "".into(),
            source: "community".into(),
            category: "development".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "docker-compose".into(),
            description: "Generate and optimize Docker Compose configurations".into(),
            description_zh: "生成和优化 Docker Compose 配置".into(),
            download_url: "".into(),
            source: "community".into(),
            category: "devops".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "api-docs-generator".into(),
            description: "Generate OpenAPI/Swagger documentation from code".into(),
            description_zh: "从代码生成 OpenAPI/Swagger 文档".into(),
            download_url: "".into(),
            source: "community".into(),
            category: "development".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "sql-optimizer".into(),
            description: "Analyze and optimize SQL queries for better performance".into(),
            description_zh: "分析和优化 SQL 查询以提高性能".into(),
            download_url: "".into(),
            source: "community".into(),
            category: "database".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "regex-builder".into(),
            description: "Build and test regular expressions with explanations".into(),
            description_zh: "构建和测试正则表达式并提供解释".into(),
            download_url: "".into(),
            source: "community".into(),
            category: "development".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "terraform-generator".into(),
            description: "Generate Terraform IaC configurations for cloud resources".into(),
            description_zh: "为云资源生成 Terraform 基础设施即代码配置".into(),
            download_url: "".into(),
            source: "community".into(),
            category: "devops".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "unit-test-writer".into(),
            description: "Generate comprehensive unit tests for functions and classes".into(),
            description_zh: "为函数和类生成全面的单元测试".into(),
            download_url: "".into(),
            source: "community".into(),
            category: "testing".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "readme-generator".into(),
            description: "Generate professional README.md files for projects".into(),
            description_zh: "为项目生成专业的 README.md 文件".into(),
            download_url: "".into(),
            source: "community".into(),
            category: "writing".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "ci-cd-pipeline".into(),
            description: "Generate GitHub Actions / GitLab CI pipeline configurations".into(),
            description_zh: "生成 GitHub Actions / GitLab CI 流水线配置".into(),
            download_url: "".into(),
            source: "community".into(),
            category: "devops".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "codebase-explorer".into(),
            description: "Map unfamiliar repositories, identify entry points, data flow, and high-risk modules".into(),
            description_zh: "快速梳理陌生仓库，识别入口、数据流和高风险模块".into(),
            download_url: "".into(),
            source: "recommended".into(),
            category: "development".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "bug-root-cause".into(),
            description: "Debug production bugs with reproduction steps, fault isolation, and regression tests".into(),
            description_zh: "按复现、隔离、回归测试的流程定位生产 Bug 根因".into(),
            download_url: "".into(),
            source: "recommended".into(),
            category: "debugging".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "migration-planner".into(),
            description: "Plan framework, database, or API migrations with compatibility and rollback checks".into(),
            description_zh: "规划框架、数据库或 API 迁移，包含兼容性和回滚检查".into(),
            download_url: "".into(),
            source: "recommended".into(),
            category: "architecture".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "performance-profiler".into(),
            description: "Find bottlenecks, propose measurable optimizations, and define before/after benchmarks".into(),
            description_zh: "定位性能瓶颈，提出可度量优化，并定义优化前后基准".into(),
            download_url: "".into(),
            source: "recommended".into(),
            category: "performance".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "api-integration".into(),
            description: "Integrate third-party APIs with auth, retries, error mapping, and test fixtures".into(),
            description_zh: "集成第三方 API，覆盖认证、重试、错误映射和测试夹具".into(),
            download_url: "".into(),
            source: "recommended".into(),
            category: "development".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "tauri-rust-desktop".into(),
            description: "Build and debug Tauri desktop features across Rust commands, frontend state, and packaging".into(),
            description_zh: "开发和调试 Tauri 桌面功能，覆盖 Rust 命令、前端状态和打包".into(),
            download_url: "".into(),
            source: "recommended".into(),
            category: "desktop".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
    ];

    // Mark installed skills
    for skill in &mut catalog {
        skill.installed = installed.contains(&skill.name);
    }

    catalog
}

// ── Skills Discovery Commands ────────────────────────

#[tauri::command]
fn get_catalog_skills() -> Vec<CatalogSkill> {
    build_catalog()
}

#[tauri::command]
fn get_skill_repos(app: tauri::AppHandle) -> Vec<SkillRepo> {
    read_skill_repos(&app).repos
}

#[tauri::command]
fn add_skill_repo(app: tauri::AppHandle, url: String, branch: String) -> Result<(), String> {
    let url = url.trim().to_string();
    let branch = if branch.trim().is_empty() {
        "main".to_string()
    } else {
        branch.trim().to_string()
    };
    let mut data = read_skill_repos(&app);
    if data.repos.iter().any(|r| r.url == url) {
        return Err("Repository already exists".into());
    }
    data.repos.push(SkillRepo {
        url,
        branch,
        enabled: true,
    });
    write_skill_repos(&app, &data)
}

#[tauri::command]
fn remove_skill_repo(app: tauri::AppHandle, url: String) -> Result<(), String> {
    let mut data = read_skill_repos(&app);
    data.repos.retain(|r| r.url != url);
    write_skill_repos(&app, &data)
}

/// 通过 GitHub Tree API 查找仓库中 SKILL.md 的实际路径
fn find_skill_md_in_repo(
    client: &reqwest::blocking::Client,
    full_name: &str,
    branch: &str,
) -> Result<String, String> {
    let tree_url = format!(
        "https://api.github.com/repos/{}/git/trees/{}?recursive=1",
        full_name, branch
    );
    let resp = client
        .get(&tree_url)
        .send()
        .map_err(|e| format!("GitHub Tree API error: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("GitHub Tree API returned {}", resp.status()));
    }
    let body: serde_json::Value = resp
        .json()
        .map_err(|e| format!("JSON parse error: {}", e))?;

    let mut skill_paths: Vec<String> = Vec::new();
    if let Some(tree) = body.get("tree").and_then(|v| v.as_array()) {
        for item in tree {
            if let Some(path) = item.get("path").and_then(|v| v.as_str()) {
                if path.ends_with("SKILL.md")
                    && item.get("type").and_then(|v| v.as_str()) == Some("blob")
                {
                    skill_paths.push(path.to_string());
                }
            }
        }
    }

    if skill_paths.is_empty() {
        return Err("No SKILL.md found in repository".into());
    }

    // 优先选择 .claude/skills/ 下的，其次选最短路径
    skill_paths.sort_by(|a, b| {
        let a_pref = a.contains(".claude/skills/");
        let b_pref = b.contains(".claude/skills/");
        b_pref.cmp(&a_pref).then(a.len().cmp(&b.len()))
    });

    let path = &skill_paths[0];
    Ok(format!(
        "https://raw.githubusercontent.com/{}/{}/{}",
        full_name, branch, path
    ))
}

/// 尝试下载 URL，失败时尝试镜像
fn download_with_fallback(client: &reqwest::blocking::Client, url: &str) -> Result<String, String> {
    // 尝试原始 URL
    match client.get(url).send() {
        Ok(resp) if resp.status().is_success() => {
            return resp.text().map_err(|e| format!("Read failed: {}", e));
        }
        _ => {}
    }

    // 尝试 GitHub 镜像
    if url.contains("raw.githubusercontent.com") || url.contains("github.com") {
        let mirror_url = format!("https://ghfast.top/{}", url);
        if let Ok(resp) = client.get(&mirror_url).send() {
            if resp.status().is_success() {
                return resp.text().map_err(|e| format!("Read failed: {}", e));
            }
        }
    }

    Err(format!("Download failed: {}", url))
}

fn recommended_skill_content(name: &str) -> String {
    let (description, body) = match name {
        "codebase-explorer" => (
            "Map unfamiliar repositories and identify high-risk areas",
            "Use this skill when entering an unfamiliar repository. Start by identifying the app type, package manager, entry points, config files, persistence layer, and test commands. Summarize the architecture, then list the files most likely to matter for the requested change. Prefer evidence from local files over assumptions.",
        ),
        "bug-root-cause" => (
            "Debug bugs with reproduction, isolation, and regression tests",
            "Use this skill for failures, crashes, incorrect behavior, or flaky tests. Capture the observed symptom, expected behavior, likely execution path, and the smallest reproduction. Inspect logs and call sites before editing. Fix the narrow cause, then add or update a regression test when practical.",
        ),
        "migration-planner" => (
            "Plan framework, database, or API migrations safely",
            "Use this skill before migrations or compatibility upgrades. Inventory current versions and integration points, identify breaking changes, plan staged rollout and rollback, and separate mechanical edits from behavior changes. Validate with focused tests after each stage.",
        ),
        "performance-profiler" => (
            "Find bottlenecks and define measurable optimizations",
            "Use this skill for slow UI, slow commands, expensive queries, high memory use, or request latency. Establish a baseline, identify the hot path, avoid speculative rewrites, and propose changes that can be measured before and after.",
        ),
        "api-integration" => (
            "Integrate third-party APIs with robust error handling",
            "Use this skill when adding or debugging external API integrations. Verify current docs, model authentication and rate limits, handle retries and timeouts explicitly, normalize provider errors, and add fixtures or mocks for tests.",
        ),
        "tauri-rust-desktop" => (
            "Build and debug Tauri desktop features across Rust and frontend",
            "Use this skill for Tauri commands, file system access, tray behavior, frontend invoke calls, packaging, and Windows/macOS/Linux path issues. Keep Rust command contracts stable, validate frontend payload names, and test both JS behavior and Rust command registration.",
        ),
        _ => (
            "Installed from catalog",
            "Use this skill as a starting point. Edit this file to add project-specific instructions, examples, and constraints.",
        ),
    };

    format!("---\nname: {name}\ndescription: {description}\n---\n\n# {name}\n\n{body}\n")
}

/// Download a skill from a URL and install it to ~/.claude/skills/
#[tauri::command]
async fn install_skill_from_url(name: String, url: String) -> Result<(), String> {
    if name.is_empty() {
        return Err("Skill name is required".into());
    }

    let content = if url.is_empty() {
        // No URL — create a placeholder skill
        recommended_skill_content(&name)
    } else {
        let url_clone = url.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let client = build_http_client(30)?;

            // 先尝试直接下载
            if let Ok(text) = download_with_fallback(&client, &url_clone) {
                return Ok(text);
            }

            // 直接下载失败（可能 SKILL.md 不在根目录），尝试用 Tree API 查找真实路径
            // 从 URL 中提取 full_name 和 branch
            // URL 格式: https://raw.githubusercontent.com/{owner}/{repo}/{branch}/SKILL.md
            if url_clone.contains("raw.githubusercontent.com") {
                let parts: Vec<&str> = url_clone
                    .trim_start_matches("https://raw.githubusercontent.com/")
                    .splitn(4, '/')
                    .collect();
                if parts.len() >= 3 {
                    let full_name = format!("{}/{}", parts[0], parts[1]);
                    let branch = parts[2];
                    if let Ok(real_url) = find_skill_md_in_repo(&client, &full_name, branch) {
                        return download_with_fallback(&client, &real_url);
                    }
                }
            }

            Err(format!("Download failed: {}", url_clone))
        })
        .await
        .map_err(|e| format!("Task failed: {}", e))??
    };

    // 安装到 ~/.claude/skills/<name>/SKILL.md
    let skill_dir = claude_skills_dir().join(&name);
    fs::create_dir_all(&skill_dir).map_err(|e| e.to_string())?;
    let path = skill_dir.join("SKILL.md");
    fs::write(&path, content).map_err(|e| e.to_string())?;
    Ok(())
}

/// Search GitHub for MCP server repositories
#[tauri::command]
async fn search_github_mcp(query: String) -> Result<Vec<serde_json::Value>, String> {
    let query_clone = query.clone();

    let results = tauri::async_runtime::spawn_blocking(move || {
        let client = build_http_client(15)?;

        let search_query = if query_clone.is_empty() {
            "mcp+server+claude".to_string()
        } else {
            format!("mcp+server+{}", query_clone.replace(' ', "+"))
        };

        let url = format!(
            "https://api.github.com/search/repositories?q={}&sort=stars&per_page=20",
            search_query
        );

        let resp = client
            .get(&url)
            .send()
            .map_err(|e| format!("GitHub API error: {}", e))?;

        if !resp.status().is_success() {
            return Err(format!("GitHub API returned {}", resp.status()));
        }

        let body: serde_json::Value = resp
            .json::<serde_json::Value>()
            .map_err(|e| format!("JSON parse error: {}", e))?;

        let mut results = Vec::new();
        if let Some(items) = body.get("items").and_then(|v| v.as_array()) {
            for item in items {
                let full_name = item.get("full_name").and_then(|v| v.as_str()).unwrap_or("");
                let desc = item
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let stars = item
                    .get("stargazers_count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let html_url = item.get("html_url").and_then(|v| v.as_str()).unwrap_or("");

                if full_name.is_empty() {
                    continue;
                }

                let name = full_name.split('/').last().unwrap_or(full_name);

                results.push(serde_json::json!({
                    "id": name,
                    "name": name,
                    "nameZh": name,
                    "desc": format!("{} ({}★)", desc, stars),
                    "descZh": format!("{} ({}★)", desc, stars),
                    "source": full_name,
                    "url": html_url,
                    "stars": stars,
                    "config": {
                        "command": "npx",
                        "args": ["-y", full_name]
                    }
                }));
            }
        }

        Ok::<_, String>(results)
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))??;

    Ok(results)
}

/// Search GitHub for skills repositories
#[tauri::command]
async fn search_github_skills(query: String) -> Result<Vec<CatalogSkill>, String> {
    let installed = get_installed_skill_names();
    let query_clone = query.clone();

    let results = tauri::async_runtime::spawn_blocking(move || {
        let client = build_http_client(15)?;

        let search_query = if query_clone.is_empty() {
            "claude+skills+SKILL.md".to_string()
        } else {
            format!("claude+skills+{}", query_clone.replace(' ', "+"))
        };

        let url = format!(
            "https://api.github.com/search/repositories?q={}&sort=stars&per_page=20",
            search_query
        );

        let resp = client
            .get(&url)
            .send()
            .map_err(|e| format!("GitHub API error: {}", e))?;

        if !resp.status().is_success() {
            return Err(format!("GitHub API returned {}", resp.status()));
        }

        let body: serde_json::Value = resp
            .json::<serde_json::Value>()
            .map_err(|e| format!("JSON parse error: {}", e))?;

        let mut skills = Vec::new();
        if let Some(items) = body
            .get("items")
            .and_then(|v: &serde_json::Value| v.as_array())
        {
            for item in items {
                let full_name = item
                    .get("full_name")
                    .and_then(|v: &serde_json::Value| v.as_str())
                    .unwrap_or("");
                let desc = item
                    .get("description")
                    .and_then(|v: &serde_json::Value| v.as_str())
                    .unwrap_or("");
                let stars = item
                    .get("stargazers_count")
                    .and_then(|v: &serde_json::Value| v.as_u64())
                    .unwrap_or(0);
                let default_branch = item
                    .get("default_branch")
                    .and_then(|v: &serde_json::Value| v.as_str())
                    .unwrap_or("main");

                if full_name.is_empty() {
                    continue;
                }

                let html_url = item
                    .get("html_url")
                    .and_then(|v: &serde_json::Value| v.as_str())
                    .unwrap_or("");
                // 使用 raw.githubusercontent.com 直接下载 SKILL.md
                let raw_url = format!(
                    "https://raw.githubusercontent.com/{}/{}/SKILL.md",
                    full_name, default_branch
                );
                skills.push(CatalogSkill {
                    name: full_name.split('/').last().unwrap_or(full_name).to_string(),
                    description: format!("{} ({}★)", desc, stars),
                    description_zh: format!("{} ({}★)", desc, stars),
                    download_url: raw_url,
                    source: full_name.to_string(),
                    category: "github".into(),
                    installed: false,
                    stars,
                    repo_url: html_url.to_string(),
                });
            }
        }

        Ok::<_, String>(skills)
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))??;

    // Mark installed
    let mut results = results;
    for skill in &mut results {
        skill.installed = installed.contains(&skill.name);
    }

    Ok(results)
}

// ── Claude Prompts Commands ─────────────────────────

#[tauri::command]
fn get_claude_md() -> Result<String, String> {
    let path = claude_md_path();
    if !path.exists() {
        return Ok(String::new());
    }
    fs::read_to_string(path).map_err(|e| e.to_string())
}

#[tauri::command]
fn save_claude_md(content: String) -> Result<(), String> {
    let path = claude_md_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(path, content).map_err(|e| e.to_string())
}

/// Get built-in prompt templates
#[tauri::command]
fn get_prompt_templates() -> Vec<serde_json::Value> {
    vec![
        // ── 语言与风格 ──
        serde_json::json!({
            "id": "chinese-dev",
            "name": "Chinese Developer",
            "nameZh": "中文开发者",
            "category": "language",
            "desc": "Respond in Chinese with Chinese comments",
            "descZh": "使用中文回答，代码注释使用中文",
            "content": "## 语言偏好\n\n- 使用中文进行所有回答和解释\n- 代码注释使用中文\n- 错误信息使用中文\n- 变量名和函数名使用英文，但注释用中文解释\n- 技术术语可以保留英文原文，但需要附带中文解释\n- Git commit message 使用英文\n- 文档和 README 使用中文"
        }),
        serde_json::json!({
            "id": "concise-mode",
            "name": "Concise Mode",
            "nameZh": "简洁模式",
            "category": "style",
            "desc": "Minimal explanations, code-focused responses",
            "descZh": "最少解释，专注代码输出",
            "content": "## Response Style\n\n- Be extremely concise in all responses\n- Show code first, explain only if asked\n- No unnecessary preamble or summaries\n- Use bullet points instead of paragraphs\n- Skip obvious explanations\n- Only comment non-obvious code logic\n- Prefer showing diffs over full file rewrites\n- One-line answers when possible\n- Never repeat the question back\n- No filler phrases like \"Sure!\" or \"Great question!\""
        }),
        // ── 代码质量 ──
        serde_json::json!({
            "id": "code-quality",
            "name": "Code Quality Expert",
            "nameZh": "代码质量专家",
            "category": "quality",
            "desc": "Enforce strict code quality standards",
            "descZh": "强制执行严格的代码质量标准",
            "content": "## Code Quality Rules\n\n- Always follow SOLID principles\n- Write clean, self-documenting code\n- Use meaningful variable and function names\n- Keep functions small and focused (max 20 lines)\n- Prefer composition over inheritance\n- Write unit tests for all new code\n- Handle errors explicitly, never silently swallow exceptions\n- Use TypeScript strict mode when applicable\n- Follow the DRY principle but don't over-abstract\n- No magic numbers — use named constants\n- Prefer immutable data structures"
        }),
        serde_json::json!({
            "id": "security-first",
            "name": "Security First",
            "nameZh": "安全优先",
            "category": "quality",
            "desc": "Security-focused development guidelines",
            "descZh": "以安全为核心的开发指南",
            "content": "## Security Guidelines\n\n- Never hardcode secrets, API keys, or credentials\n- Always validate and sanitize user input\n- Use parameterized queries for database operations\n- Implement proper authentication and authorization\n- Follow OWASP Top 10 prevention guidelines\n- Use HTTPS for all external communications\n- Implement rate limiting for APIs\n- Log security events but never log sensitive data\n- Keep dependencies updated and audit regularly\n- Use Content Security Policy headers\n- Hash passwords with bcrypt/argon2, never MD5/SHA1"
        }),
        // ── 语言与框架 ──
        serde_json::json!({
            "id": "fullstack-ts",
            "name": "Full-Stack TypeScript",
            "nameZh": "全栈 TypeScript",
            "category": "framework",
            "desc": "TypeScript full-stack development standards",
            "descZh": "TypeScript 全栈开发标准",
            "content": "## TypeScript Full-Stack Standards\n\n- Use TypeScript strict mode for all projects\n- Prefer `interface` over `type` for object shapes\n- Use `zod` for runtime validation\n- Frontend: React with hooks, avoid class components\n- Backend: Express or Fastify with proper typing\n- Use `prisma` or `drizzle` for database ORM\n- API: Use tRPC or REST with OpenAPI spec\n- Testing: Vitest for unit tests, Playwright for E2E\n- Use ESLint + Prettier for code formatting\n- Prefer `const` over `let`, never use `var`\n- Use discriminated unions for state management\n- Avoid `any` — use `unknown` with type guards"
        }),
        serde_json::json!({
            "id": "python-expert",
            "name": "Python Expert",
            "nameZh": "Python 专家",
            "category": "framework",
            "desc": "Python best practices and standards",
            "descZh": "Python 最佳实践和标准",
            "content": "## Python Development Standards\n\n- Use Python 3.10+ features (match/case, type hints)\n- Always use type hints for function signatures\n- Follow PEP 8 style guide\n- Use `ruff` for linting and formatting\n- Prefer `pathlib` over `os.path`\n- Use `pydantic` for data validation\n- Use `pytest` for testing with fixtures\n- Use virtual environments (venv or poetry)\n- Handle exceptions with specific types, not bare except\n- Use dataclasses or pydantic models instead of dicts\n- Use `asyncio` for I/O-bound concurrency"
        }),
        serde_json::json!({
            "id": "rust-expert",
            "name": "Rust Expert",
            "nameZh": "Rust 专家",
            "category": "framework",
            "desc": "Rust development best practices",
            "descZh": "Rust 开发最佳实践",
            "content": "## Rust Development Standards\n\n- Use `clippy` with pedantic lints enabled\n- Prefer `Result` and `Option` over panicking\n- Use `thiserror` for library errors, `anyhow` for applications\n- Follow the ownership model — avoid unnecessary cloning\n- Use `serde` for serialization/deserialization\n- Prefer iterators over manual loops\n- Use `tokio` for async runtime\n- Write doc comments with examples for public APIs\n- Use `cargo fmt` for consistent formatting\n- Prefer `&str` over `String` in function parameters\n- Use newtype pattern for type safety"
        }),
        serde_json::json!({
            "id": "react-nextjs",
            "name": "React & Next.js",
            "nameZh": "React & Next.js",
            "category": "framework",
            "desc": "React and Next.js development patterns",
            "descZh": "React 和 Next.js 开发模式",
            "content": "## React & Next.js Standards\n\n- Use functional components with hooks exclusively\n- Prefer Server Components by default (Next.js App Router)\n- Use `use client` directive only when needed\n- Implement proper error boundaries\n- Use React.memo() only after profiling confirms need\n- Prefer `useReducer` for complex state logic\n- Use Suspense for data fetching and code splitting\n- Follow the container/presentational pattern\n- Use CSS Modules or Tailwind CSS for styling\n- Implement proper loading and error states\n- Prefer server actions for form handling"
        }),
        // ── 架构与设计 ──
        serde_json::json!({
            "id": "architect",
            "name": "Software Architect",
            "nameZh": "软件架构师",
            "category": "architecture",
            "desc": "Architecture-focused guidance and design patterns",
            "descZh": "架构导向的指导和设计模式",
            "content": "## Architecture Guidelines\n\n- Always consider scalability and maintainability\n- Use appropriate design patterns (don't force them)\n- Separate concerns: UI, business logic, data access\n- Design APIs contract-first\n- Use event-driven architecture for loose coupling\n- Implement proper caching strategies\n- Consider failure modes and graceful degradation\n- Document architectural decisions (ADRs)\n- Prefer microservices only when complexity warrants it\n- Use dependency injection for testability\n- Design for observability from the start"
        }),
        serde_json::json!({
            "id": "database-design",
            "name": "Database Design",
            "nameZh": "数据库设计",
            "category": "architecture",
            "desc": "Database schema design and query optimization",
            "descZh": "数据库模式设计和查询优化",
            "content": "## Database Design Guidelines\n\n- Normalize to 3NF, denormalize only for proven performance needs\n- Use appropriate indexes for query patterns\n- Implement proper foreign key constraints\n- Use UUIDs or ULIDs for distributed systems\n- Implement soft deletes with deleted_at timestamps\n- Use database migrations for schema changes\n- Implement proper connection pooling\n- Use read replicas for read-heavy workloads\n- Use EXPLAIN ANALYZE to optimize queries\n- Avoid SELECT * — specify needed columns\n- Use transactions for data consistency"
        }),
        // ── 测试 ──
        serde_json::json!({
            "id": "tdd",
            "name": "Test-Driven Development",
            "nameZh": "测试驱动开发",
            "category": "testing",
            "desc": "TDD methodology and testing best practices",
            "descZh": "TDD 方法论和测试最佳实践",
            "content": "## TDD Guidelines\n\n- Write tests BEFORE implementation code\n- Follow Red-Green-Refactor cycle\n- Each test should test one thing only\n- Use descriptive test names: should_[expected]_when_[condition]\n- Arrange-Act-Assert pattern for test structure\n- Mock external dependencies, not internal ones\n- Aim for 80%+ code coverage on business logic\n- Write integration tests for API endpoints\n- Use factories/fixtures for test data\n- Test edge cases and error paths\n- Keep tests fast — mock slow dependencies"
        }),
        // ── AI 与提示词 ──
        serde_json::json!({
            "id": "claude-best-practices",
            "name": "Claude Best Practices",
            "nameZh": "Claude 最佳实践",
            "category": "ai",
            "desc": "Optimized CLAUDE.md configuration for Claude Code",
            "descZh": "针对 Claude Code 优化的 CLAUDE.md 配置",
            "content": "## Claude Code Best Practices\n\n- Be specific rather than vague: \"Use 2-space indentation\" not \"Write good code\"\n- Structure with markdown headings, lists, and code blocks\n- Layer configurations: project CLAUDE.md for team, user CLAUDE.md for personal\n- Include project-specific conventions and patterns\n- Specify preferred libraries and tools\n- Define commit message format\n- Set code review standards\n- Include architecture decision records\n- Specify testing requirements\n- Define error handling patterns\n- Keep CLAUDE.md under 1000 lines for best performance\n- Update regularly as project evolves"
        }),
        // ── Git 与工作流 ──
        serde_json::json!({
            "id": "git-workflow",
            "name": "Git Workflow",
            "nameZh": "Git 工作流",
            "category": "workflow",
            "desc": "Git branching strategy and commit conventions",
            "descZh": "Git 分支策略和提交规范",
            "content": "## Git Workflow Rules\n\n- Use Conventional Commits: feat:, fix:, docs:, refactor:, test:, chore:\n- Branch naming: feature/*, bugfix/*, hotfix/*, release/*\n- Keep commits atomic — one logical change per commit\n- Write meaningful commit messages explaining WHY, not WHAT\n- Squash WIP commits before merging\n- Use pull requests for all changes\n- Require at least one code review approval\n- Rebase feature branches on main before merging\n- Tag releases with semantic versioning\n- Never force push to main/master\n- Use .gitignore for build artifacts and secrets"
        }),
        // ── 新增实用模板 ──
        serde_json::json!({
            "id": "error-handling",
            "name": "Error Handling Patterns",
            "nameZh": "错误处理模式",
            "category": "quality",
            "desc": "Comprehensive error handling and logging patterns",
            "descZh": "全面的错误处理和日志记录模式",
            "content": "## Error Handling Patterns\n\n- Use custom error types with meaningful error codes\n- Implement global error handler for uncaught exceptions\n- Log errors with context: timestamp, request ID, user ID, stack trace\n- Use structured logging (JSON format) for production\n- Distinguish between operational errors and programmer errors\n- Implement retry logic with exponential backoff for transient failures\n- Return user-friendly error messages, log detailed errors internally\n- Use error boundaries in frontend to prevent full-page crashes\n- Implement circuit breaker pattern for external service calls\n- Never expose internal error details to end users\n- Use correlation IDs to trace errors across microservices"
        }),
        serde_json::json!({
            "id": "code-review-guide",
            "name": "Code Review Guide",
            "nameZh": "代码审查指南",
            "category": "workflow",
            "desc": "Systematic code review checklist and standards",
            "descZh": "系统化的代码审查清单和标准",
            "content": "## Code Review Checklist\n\n### Correctness\n- Does the code do what it's supposed to?\n- Are edge cases handled?\n- Are there any race conditions?\n\n### Security\n- Input validation present?\n- No hardcoded secrets?\n- SQL injection prevention?\n\n### Performance\n- No unnecessary database queries?\n- Proper use of indexes?\n- No memory leaks?\n\n### Maintainability\n- Clear naming conventions?\n- Appropriate abstractions?\n- No code duplication?\n\n### Testing\n- Unit tests for new logic?\n- Edge cases tested?\n- Integration tests for APIs?"
        }),
        serde_json::json!({
            "id": "project-scaffold",
            "name": "Project Scaffolding",
            "nameZh": "项目脚手架",
            "category": "workflow",
            "desc": "Standards for initializing new projects",
            "descZh": "新项目初始化标准",
            "content": "## Project Scaffolding Standards\n\n- Include README.md with setup instructions and architecture overview\n- Configure linter and formatter from day one\n- Set up CI/CD pipeline before writing business logic\n- Use .env.example for environment variable documentation\n- Configure pre-commit hooks for linting and formatting\n- Set up Docker development environment\n- Include Makefile or package.json scripts for common tasks\n- Configure logging and monitoring from the start\n- Set up database migrations framework\n- Include health check endpoint\n- Configure CORS and security headers\n- Set up automated dependency updates (Dependabot/Renovate)"
        }),
        serde_json::json!({
            "id": "refactoring",
            "name": "Refactoring Guide",
            "nameZh": "重构指南",
            "category": "quality",
            "desc": "Safe refactoring strategies and code smell detection",
            "descZh": "安全的重构策略和代码异味检测",
            "content": "## Refactoring Guidelines\n\n- Always have tests before refactoring\n- Make small, incremental changes — one refactoring at a time\n- Run tests after each change to catch regressions\n- Common code smells to fix:\n  - Long methods (> 20 lines)\n  - God classes with too many responsibilities\n  - Feature envy (method uses another class's data excessively)\n  - Primitive obsession (use value objects)\n  - Shotgun surgery (one change requires editing many files)\n- Extract Method for repeated logic\n- Replace conditionals with polymorphism\n- Use Strategy pattern to eliminate switch statements\n- Introduce Parameter Object for methods with 3+ parameters\n- Never refactor and add features in the same commit"
        }),
    ]
}

// ── MCP Server Commands ─────────────────────────────

#[tauri::command]
fn get_mcp_servers_list() -> Result<serde_json::Value, String> {
    let path = claude_mcp_path();
    if !path.exists() {
        return Ok(serde_json::json!({}));
    }
    let settings = read_json(&path)?;
    Ok(settings
        .get("mcpServers")
        .cloned()
        .unwrap_or(serde_json::json!({})))
}

#[tauri::command]
fn save_mcp_server(name: String, config: serde_json::Value) -> Result<(), String> {
    if name.is_empty() {
        return Err("服务器名称不能为空".into());
    }
    let path = claude_mcp_path();
    let mut settings = if path.exists() {
        read_json(&path)?
    } else {
        serde_json::json!({})
    };
    if !settings.is_object() {
        settings = serde_json::json!({});
    }
    if settings.get("mcpServers").is_none() {
        settings["mcpServers"] = serde_json::json!({});
    }
    settings["mcpServers"][&name] = config;
    write_json(&path, &settings)
}

#[tauri::command]
fn delete_mcp_server_entry(name: String) -> Result<(), String> {
    let path = claude_mcp_path();
    if !path.exists() {
        return Ok(());
    }
    let mut settings = read_json(&path)?;
    if let Some(servers) = settings
        .get_mut("mcpServers")
        .and_then(|v| v.as_object_mut())
    {
        servers.remove(&name);
    }
    write_json(&path, &settings)
}

/// Get preset MCP server configurations
#[tauri::command]
fn get_mcp_presets() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({
            "id": "context7",
            "name": "Context7",
            "nameZh": "Context7 文档查询",
            "desc": "Up-to-date documentation for any library via Context7",
            "descZh": "通过 Context7 获取任何库的最新文档",
            "config": {
                "command": "npx",
                "args": ["-y", "@upstash/context7-mcp@latest"]
            }
        }),
        serde_json::json!({
            "id": "filesystem",
            "name": "Filesystem",
            "nameZh": "文件系统",
            "desc": "Read, write, and manage files on your local filesystem",
            "descZh": "读取、写入和管理本地文件系统",
            "config": {
                "command": "npx",
                "args": ["-y", "@anthropic/mcp-filesystem"]
            }
        }),
        serde_json::json!({
            "id": "github",
            "name": "GitHub",
            "nameZh": "GitHub",
            "desc": "Interact with GitHub repos, issues, PRs, and more",
            "descZh": "与 GitHub 仓库、Issues、PR 等交互",
            "config": {
                "command": "npx",
                "args": ["-y", "@anthropic/mcp-github"],
                "env": { "GITHUB_TOKEN": "<your-github-token>" }
            }
        }),
        serde_json::json!({
            "id": "playwright",
            "name": "Playwright",
            "nameZh": "Playwright 浏览器",
            "desc": "Browser automation and web scraping with Playwright",
            "descZh": "使用 Playwright 进行浏览器自动化和网页抓取",
            "config": {
                "command": "npx",
                "args": ["-y", "@anthropic/mcp-playwright"]
            }
        }),
        serde_json::json!({
            "id": "puppeteer",
            "name": "Puppeteer",
            "nameZh": "Puppeteer 浏览器",
            "desc": "Browser automation with Puppeteer",
            "descZh": "使用 Puppeteer 进行浏览器自动化",
            "config": {
                "command": "npx",
                "args": ["-y", "@anthropic/mcp-puppeteer"]
            }
        }),
        serde_json::json!({
            "id": "memory",
            "name": "Memory",
            "nameZh": "记忆存储",
            "desc": "Persistent memory storage for Claude conversations",
            "descZh": "为 Claude 对话提供持久化记忆存储",
            "config": {
                "command": "npx",
                "args": ["-y", "@anthropic/mcp-memory"]
            }
        }),
        serde_json::json!({
            "id": "fetch",
            "name": "Fetch",
            "nameZh": "网页抓取",
            "desc": "Fetch and parse web pages, APIs, and RSS feeds",
            "descZh": "抓取和解析网页、API 和 RSS 源",
            "config": {
                "command": "npx",
                "args": ["-y", "@anthropic/mcp-fetch"]
            }
        }),
        serde_json::json!({
            "id": "sequential-thinking",
            "name": "Sequential Thinking",
            "nameZh": "顺序思维",
            "desc": "Step-by-step reasoning and problem decomposition",
            "descZh": "逐步推理和问题分解",
            "config": {
                "command": "npx",
                "args": ["-y", "@anthropic/mcp-sequential-thinking"]
            }
        }),
        serde_json::json!({
            "id": "sqlite",
            "name": "SQLite",
            "nameZh": "SQLite 数据库",
            "desc": "Query and manage SQLite databases",
            "descZh": "查询和管理 SQLite 数据库",
            "config": {
                "command": "npx",
                "args": ["-y", "@anthropic/mcp-sqlite", "--db-path", "./database.db"]
            }
        }),
        serde_json::json!({
            "id": "postgres",
            "name": "PostgreSQL",
            "nameZh": "PostgreSQL 数据库",
            "desc": "Connect to and query PostgreSQL databases",
            "descZh": "连接和查询 PostgreSQL 数据库",
            "config": {
                "command": "npx",
                "args": ["-y", "@anthropic/mcp-postgres"],
                "env": { "POSTGRES_URL": "postgresql://user:password@localhost:5432/dbname" }
            }
        }),
        serde_json::json!({
            "id": "firecrawl",
            "name": "Firecrawl",
            "nameZh": "Firecrawl 爬虫",
            "desc": "Powerful web scraping and crawling with Firecrawl",
            "descZh": "使用 Firecrawl 进行强大的网页抓取和爬取",
            "config": {
                "command": "npx",
                "args": ["-y", "firecrawl-mcp"],
                "env": { "FIRECRAWL_API_KEY": "<your-api-key>" }
            }
        }),
        serde_json::json!({
            "id": "deepwiki",
            "name": "DeepWiki",
            "nameZh": "DeepWiki 文档",
            "desc": "Access documentation from DeepWiki for any open source project",
            "descZh": "从 DeepWiki 获取任何开源项目的文档",
            "config": {
                "command": "npx",
                "args": ["-y", "mcp-deepwiki"]
            }
        }),
        serde_json::json!({
            "id": "brave-search",
            "name": "Brave Search",
            "nameZh": "Brave 搜索",
            "desc": "Web search using Brave Search API",
            "descZh": "使用 Brave Search API 进行网页搜索",
            "config": {
                "command": "npx",
                "args": ["-y", "@anthropic/mcp-brave-search"],
                "env": { "BRAVE_API_KEY": "<your-api-key>" }
            }
        }),
        serde_json::json!({
            "id": "slack",
            "name": "Slack",
            "nameZh": "Slack",
            "desc": "Interact with Slack workspaces, channels, and messages",
            "descZh": "与 Slack 工作区、频道和消息交互",
            "config": {
                "command": "npx",
                "args": ["-y", "@anthropic/mcp-slack"],
                "env": { "SLACK_BOT_TOKEN": "<your-bot-token>" }
            }
        }),
    ]
}

/// 构建支持系统代理的 HTTP 客户端
fn resolve_proxy_url_from_values(values: &[Option<&str>]) -> Option<String> {
    for value in values {
        let candidate = match value {
            Some(raw) => raw.trim(),
            None => continue,
        };

        if candidate.is_empty() {
            continue;
        }

        let parsed = match reqwest::Url::parse(candidate) {
            Ok(url) => url,
            Err(_) => continue,
        };

        let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
        let port = parsed.port_or_known_default().unwrap_or(0);
        let is_loopback = matches!(host.as_str(), "127.0.0.1" | "localhost" | "::1");
        let is_disabled_proxy = is_loopback && matches!(port, 0 | 9);

        if is_disabled_proxy {
            continue;
        }

        return Some(candidate.to_string());
    }

    None
}

fn resolve_proxy_url_from_env() -> Option<String> {
    let https_upper = std::env::var("HTTPS_PROXY").ok();
    let https_lower = std::env::var("https_proxy").ok();
    let http_upper = std::env::var("HTTP_PROXY").ok();
    let http_lower = std::env::var("http_proxy").ok();

    resolve_proxy_url_from_values(&[
        https_upper.as_deref(),
        https_lower.as_deref(),
        http_upper.as_deref(),
        http_lower.as_deref(),
    ])
}

fn build_http_client(timeout_secs: u64) -> Result<reqwest::blocking::Client, String> {
    let mut builder = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .user_agent("VarSwitch/1.0");

    if let Some(proxy_url) = resolve_proxy_url_from_env() {
        if let Ok(proxy) = reqwest::Proxy::all(&proxy_url) {
            builder = builder.proxy(proxy);
        }
    }

    builder
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))
}

fn normalize_version_parts(version: &str) -> Vec<u64> {
    version
        .trim()
        .trim_start_matches(['v', 'V'])
        .split('.')
        .map(|part| {
            part.chars()
                .take_while(|ch| ch.is_ascii_digit())
                .collect::<String>()
        })
        .map(|digits| digits.parse::<u64>().unwrap_or(0))
        .collect()
}

fn compare_versions(left: &str, right: &str) -> CmpOrdering {
    let left_parts = normalize_version_parts(left);
    let right_parts = normalize_version_parts(right);
    let len = left_parts.len().max(right_parts.len()).max(3);

    for idx in 0..len {
        let left_num = *left_parts.get(idx).unwrap_or(&0);
        let right_num = *right_parts.get(idx).unwrap_or(&0);
        match left_num.cmp(&right_num) {
            CmpOrdering::Equal => continue,
            other => return other,
        }
    }

    CmpOrdering::Equal
}

fn is_remote_version_newer(remote: &str, local: &str) -> bool {
    compare_versions(remote, local) == CmpOrdering::Greater
}

fn asset_has_known_arch_marker(name_lower: &str) -> bool {
    ["x64", "x86_64", "amd64", "arm64", "aarch64", "universal"]
        .iter()
        .any(|token| name_lower.contains(token))
}

fn asset_matches_target_arch(name_lower: &str, target_arch: &str) -> Option<bool> {
    let aliases: Vec<&str> = match target_arch {
        "x86_64" => vec!["x64", "x86_64", "amd64"],
        "aarch64" => vec!["arm64", "aarch64"],
        other => vec![other],
    };

    if aliases.iter().any(|alias| name_lower.contains(alias)) {
        return Some(true);
    }

    if name_lower.contains("universal") {
        return Some(true);
    }

    if asset_has_known_arch_marker(name_lower) {
        return Some(false);
    }

    None
}

fn installer_extension_score(name_lower: &str, target_os: &str) -> Option<i32> {
    match target_os {
        "windows" => {
            if name_lower.ends_with(".msi") {
                Some(30)
            } else if name_lower.ends_with(".exe") {
                Some(25)
            } else {
                None
            }
        }
        "macos" => {
            if name_lower.ends_with(".dmg") {
                Some(30)
            } else {
                None
            }
        }
        "linux" => {
            if name_lower.ends_with(".appimage") {
                Some(30)
            } else if name_lower.ends_with(".deb") {
                Some(25)
            } else if name_lower.ends_with(".rpm") {
                Some(24)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn select_release_asset(
    assets: &[ReleaseAsset],
    target_os: &str,
    target_arch: &str,
) -> Option<ReleaseAsset> {
    assets
        .iter()
        .filter_map(|asset| {
            let name_lower = asset.name.to_ascii_lowercase();
            if name_lower.ends_with(".sig") {
                return None;
            }

            let mut score = installer_extension_score(&name_lower, target_os)?;
            match asset_matches_target_arch(&name_lower, target_arch) {
                Some(true) => score += 10,
                Some(false) => return None,
                None => score += 1,
            }

            Some((score, asset.size, asset.clone()))
        })
        .max_by_key(|(score, size, _)| (*score, *size))
        .map(|(_, _, asset)| asset)
}

fn fetch_latest_release() -> Result<GitHubRelease, String> {
    let client = build_http_client(20)?;
    let resp = client
        .get(GITHUB_LATEST_RELEASE_API)
        .send()
        .map_err(|e| format!("GitHub API error: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("GitHub API returned {}", resp.status()));
    }

    let body = resp
        .text()
        .map_err(|e| format!("Response read error: {}", e))?;

    serde_json::from_str::<GitHubRelease>(&body).map_err(|e| {
        let preview: String = body.chars().take(180).collect();
        format!("JSON parse error: {} | body: {}", e, preview)
    })
}

fn open_with_system(target: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let mut cmd = std::process::Command::new("cmd");
        cmd.args(["/C", "start", "", target]);
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd.spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(not(target_os = "windows"))]
    {
        #[cfg(target_os = "macos")]
        let cmd = "open";
        #[cfg(not(target_os = "macos"))]
        let cmd = "xdg-open";

        std::process::Command::new(cmd)
            .arg(target)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn chrono_now() -> String {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", d.as_millis())
}

fn chrono_timestamp_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

// ── App Entry ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn upsert_env_array_deduplicates_same_name() {
        let mut arr = vec![
            json!({ "name": "ANTHROPIC_AUTH_TOKEN", "value": "old-1" }),
            json!({ "name": "ANTHROPIC_AUTH_TOKEN", "value": "old-2" }),
        ];

        upsert_env_array(&mut arr, "ANTHROPIC_AUTH_TOKEN", "new");

        let count = arr
            .iter()
            .filter(|v| v.get("name").and_then(|n| n.as_str()) == Some("ANTHROPIC_AUTH_TOKEN"))
            .count();
        assert_eq!(count, 1, "should keep only one ANTHROPIC_AUTH_TOKEN");
    }

    #[test]
    fn apply_auth_to_env_array_removes_non_selected_auth_key() {
        let mut arr = vec![
            json!({ "name": "ANTHROPIC_AUTH_TOKEN", "value": "old-token" }),
            json!({ "name": "ANTHROPIC_AUTH_KEY", "value": "old-key" }),
        ];

        let selected = apply_auth_to_env_array(&mut arr, "new-token", "https://example.test");

        let has_key = arr
            .iter()
            .any(|v| v.get("name").and_then(|n| n.as_str()) == Some("ANTHROPIC_AUTH_KEY"));
        assert!(
            !has_key,
            "ANTHROPIC_AUTH_KEY should be removed when token is used"
        );
        assert_eq!(selected, "ANTHROPIC_AUTH_TOKEN");
    }

    #[test]
    fn apply_auth_to_env_array_converts_auth_key_to_auth_token() {
        let mut arr = vec![json!({ "name": "ANTHROPIC_AUTH_KEY", "value": "old-key" })];

        let selected = apply_auth_to_env_array(&mut arr, "new-key", "https://example.test");

        let has_key = arr
            .iter()
            .any(|v| v.get("name").and_then(|n| n.as_str()) == Some("ANTHROPIC_AUTH_KEY"));
        let token_value = arr
            .iter()
            .find(|v| v.get("name").and_then(|n| n.as_str()) == Some("ANTHROPIC_AUTH_TOKEN"))
            .and_then(|v| v.get("value"))
            .and_then(|v| v.as_str());

        assert!(
            !has_key,
            "ANTHROPIC_AUTH_KEY should be removed and converted to TOKEN"
        );
        assert_eq!(token_value, Some("new-key"));
        assert_eq!(selected, "ANTHROPIC_AUTH_TOKEN");
    }

    #[test]
    fn app_settings_defaults_keep_usage_guide_enabled() {
        let settings = AppSettings::default();

        assert_eq!(settings.language, "zh");
        assert_eq!(settings.theme, "light");
        assert!(!settings.auto_start);
        assert!(!settings.silent_startup);
        assert!(settings.minimize_to_tray);
        assert!(
            !settings.never_show_usage_guide,
            "usage guide should show by default until the user disables it"
        );
    }

    #[test]
    fn remove_all_plugin_marketplace_sections_keeps_other_config() {
        let config = r#"model = "gpt-5"

[marketplaces.old]
source = "https://example.test/old.git"
source_type = "git"

[model_providers.customer]
name = "customer"

[marketplaces.openai-bundled]
source = "C:\\bundle"
source_type = "local"
"#;

        let cleaned = remove_all_plugin_marketplace_sections(config);

        assert!(cleaned.contains(r#"model = "gpt-5""#));
        assert!(cleaned.contains("[model_providers.customer]"));
        assert!(!cleaned.contains("[marketplaces."));
    }

    #[test]
    fn normalize_toolbox_state_adds_mobile_channels() {
        let state = normalize_toolbox_state(ToolboxState::default());

        for channel in ["lark", "wechat", "qq"] {
            assert!(
                state
                    .mobile_channels
                    .iter()
                    .any(|item| item.channel == channel),
                "{channel} channel should be present"
            );
        }
        assert_eq!(
            state.plugin_marketplace_input,
            default_plugin_marketplace_url()
        );
        assert_eq!(state.mobile_remote.mode, "platform_bot");
    }

    #[test]
    fn default_plugin_marketplace_uses_varswitch_gitcode_mirror() {
        assert_eq!(default_plugin_marketplace_name(), "VarSwitch-Plugin");
        assert_eq!(
            default_plugin_marketplace_url(),
            "https://gitcode.com/2301_79703673/codex-plugins.git"
        );
    }

    #[test]
    fn supported_plugin_marketplace_normalizes_known_sources() {
        assert_eq!(
            supported_plugin_marketplace_source("git@github.com:ConcertoNotes/codex-plugins.git"),
            varswitch_github_plugin_marketplace_url()
        );
        assert_eq!(
            supported_plugin_marketplace_source(
                "git@github.com:hashgraph-online/awesome-codex-plugins.git"
            ),
            awesome_plugin_marketplace_url()
        );
        assert_eq!(
            supported_plugin_marketplace_source("https://example.test/custom.git"),
            default_plugin_marketplace_url()
        );
    }

    #[test]
    fn normalize_toolbox_state_sets_mobile_channel_defaults() {
        let state = normalize_toolbox_state(ToolboxState::default());
        let lark = state
            .mobile_channels
            .iter()
            .find(|item| item.channel == "lark")
            .expect("lark channel");
        let wechat = state
            .mobile_channels
            .iter()
            .find(|item| item.channel == "wechat")
            .expect("wechat channel");

        assert_eq!(lark.base_url, "https://open.feishu.cn");
        assert_eq!(wechat.base_url, "https://ilinkai.weixin.qq.com");
    }

    #[test]
    fn normalize_toolbox_state_clears_stale_text_qr_cache() {
        let mut state = ToolboxState::default();
        let mut qq = default_mobile_channel("qq");
        qq.qr_url = "raw-internal-token".into();
        qq.qr_data_url = "data:image/png;base64,abc".into();
        qq.qr_started_at = chrono_timestamp_millis().to_string();
        state.mobile_channels.push(qq);
        let mut wechat = default_mobile_channel("wechat");
        wechat.qr_device_code = "device-code".into();
        wechat.qr_data_url = "data:image/svg+xml;base64,abc".into();
        wechat.qr_started_at = chrono_timestamp_millis().to_string();
        state.mobile_channels.push(wechat);

        let state = normalize_toolbox_state(state);
        let qq = state
            .mobile_channels
            .iter()
            .find(|item| item.channel == "qq")
            .expect("qq channel");
        let wechat = state
            .mobile_channels
            .iter()
            .find(|item| item.channel == "wechat")
            .expect("wechat channel");

        assert!(qq.qr_data_url.is_empty());
        assert!(qq.qr_url.is_empty());
        assert!(wechat.qr_data_url.is_empty());
        assert!(wechat.qr_device_code.is_empty());
    }

    #[test]
    fn normalize_toolbox_state_keeps_fresh_platform_qr_cache() {
        let mut state = ToolboxState::default();
        let mut qq = default_mobile_channel("qq");
        qq.qr_url = "https://bots.qq.com/connect/abc".into();
        qq.qr_data_url = "data:image/png;base64,abc".into();
        qq.qr_started_at = chrono_timestamp_millis().to_string();
        state.mobile_channels.push(qq);

        let state = normalize_toolbox_state(state);
        let qq = state
            .mobile_channels
            .iter()
            .find(|item| item.channel == "qq")
            .expect("qq channel");

        assert_eq!(qq.qr_url, "https://bots.qq.com/connect/abc");
        assert_eq!(qq.qr_data_url, "data:image/png;base64,abc");
    }

    #[test]
    fn mobile_channel_credentials_require_platform_specific_fields() {
        let mut lark = default_mobile_channel("lark");
        assert!(!mobile_channel_has_credentials(&lark));
        lark.app_id = "cli_xxx".into();
        lark.app_secret = "secret".into();
        assert!(mobile_channel_has_credentials(&lark));

        let mut wechat = default_mobile_channel("wechat");
        assert!(!mobile_channel_has_credentials(&wechat));
        wechat.bot_token = "token".into();
        assert!(mobile_channel_has_credentials(&wechat));
    }

    #[test]
    fn app_settings_deserialize_old_files_without_usage_guide_field() {
        let settings: AppSettings = serde_json::from_value(json!({
            "language": "en",
            "theme": "dark",
            "autoStart": true,
            "silentStartup": true,
            "minimizeToTray": false
        }))
        .expect("old settings json should still deserialize");

        assert_eq!(settings.language, "en");
        assert_eq!(settings.theme, "dark");
        assert!(settings.auto_start);
        assert!(settings.silent_startup);
        assert!(!settings.minimize_to_tray);
        assert!(!settings.never_show_usage_guide);
        assert!(
            settings.editor_paths.is_empty(),
            "old settings json should default editor path overrides to empty"
        );
    }

    #[test]
    fn sanitize_endpoint_timeout_clamps_values() {
        assert_eq!(
            sanitize_endpoint_timeout(Some(1)),
            ENDPOINT_TEST_MIN_TIMEOUT_SECS
        );
        assert_eq!(
            sanitize_endpoint_timeout(Some(999)),
            ENDPOINT_TEST_MAX_TIMEOUT_SECS
        );
        assert_eq!(sanitize_endpoint_timeout(Some(10)), 10);
        assert_eq!(
            sanitize_endpoint_timeout(None),
            ENDPOINT_TEST_DEFAULT_TIMEOUT_SECS
        );
    }

    #[test]
    fn normalize_endpoint_url_trims_and_removes_trailing_slashes() {
        let normalized = normalize_endpoint_url(" https://api.example.com/v1/// ")
            .expect("valid endpoint should normalize");

        assert_eq!(normalized, "https://api.example.com/v1");
    }

    #[test]
    fn normalize_endpoint_url_rejects_invalid_values() {
        assert!(normalize_endpoint_url("").is_err());
        assert!(normalize_endpoint_url("not a url").is_err());
        assert!(normalize_endpoint_url("file:///tmp/config.json").is_err());
    }

    #[test]
    fn known_editors_include_vscodium() {
        assert!(
            KNOWN_EDITORS.iter().any(|editor| editor.id == "vscodium"),
            "VSCodium should be part of the built-in supported editor list"
        );
    }

    #[test]
    fn normalize_editor_path_value_appends_settings_json_for_directory_paths() {
        let normalized =
            normalize_editor_path_value(r"C:\Editors\Cursor\User").expect("path should normalize");

        assert!(
            normalized.ends_with(r"Cursor\User\settings.json"),
            "directory overrides should resolve to settings.json, got {}",
            normalized
        );
    }

    #[test]
    fn normalize_editor_path_value_preserves_explicit_settings_file_path() {
        let normalized = normalize_editor_path_value(r"C:\Editors\Cursor\User\settings.json")
            .expect("path should normalize");

        assert_eq!(normalized, r"C:\Editors\Cursor\User\settings.json");
    }

    #[test]
    fn resolved_editor_settings_path_prefers_saved_override() {
        let editor = KNOWN_EDITORS
            .iter()
            .find(|candidate| candidate.id == "vscode")
            .expect("vscode should be supported");
        let mut settings = AppSettings::default();
        settings
            .editor_paths
            .insert(editor.id.to_string(), r"C:\Custom\VSCode\User".to_string());

        let resolved = resolved_editor_settings_path(editor, &settings);

        assert_eq!(
            resolved,
            PathBuf::from(r"C:\Custom\VSCode\User\settings.json")
        );
    }

    #[test]
    fn detect_installed_editors_includes_manual_override_even_without_default_install_path() {
        let mut settings = AppSettings::default();
        settings.editor_paths.insert(
            "cursor".to_string(),
            r"C:\PortableApps\Cursor\User".to_string(),
        );

        let detected = detect_installed_editors(&settings);

        assert!(
            detected.iter().any(|editor| editor.id == "cursor"),
            "manual editor path overrides should count as detected editors"
        );
    }

    #[test]
    fn is_remote_version_newer_handles_optional_v_prefix() {
        assert!(is_remote_version_newer("v1.2.0", "1.1.9"));
        assert!(is_remote_version_newer("1.0.1", "v1.0.0"));
        assert!(!is_remote_version_newer("v1.0.0", "1.0.0"));
        assert!(!is_remote_version_newer("v0.9.9", "1.0.0"));
    }

    #[test]
    fn select_release_asset_for_windows_prefers_installer_extensions() {
        let assets = vec![
            ReleaseAsset {
                name: "VarSwitch_1.2.0_x64-setup.nsis.zip.sig".into(),
                browser_download_url: "https://example.test/app.sig".into(),
                size: 1,
            },
            ReleaseAsset {
                name: "VarSwitch_1.2.0_x64_en-US.msi".into(),
                browser_download_url: "https://example.test/app.msi".into(),
                size: 2,
            },
            ReleaseAsset {
                name: "VarSwitch_1.2.0_x64-setup.exe".into(),
                browser_download_url: "https://example.test/app.exe".into(),
                size: 3,
            },
        ];

        let selected = select_release_asset(&assets, "windows", "x86_64")
            .expect("should pick a Windows installer");

        assert!(
            selected.name.ends_with(".msi") || selected.name.ends_with(".exe"),
            "selected asset should be an installer, got {}",
            selected.name
        );
    }

    #[test]
    fn select_release_asset_for_macos_prefers_matching_architecture() {
        let assets = vec![
            ReleaseAsset {
                name: "VarSwitch_1.2.0_x64.dmg".into(),
                browser_download_url: "https://example.test/app-x64.dmg".into(),
                size: 2,
            },
            ReleaseAsset {
                name: "VarSwitch_1.2.0_aarch64.dmg".into(),
                browser_download_url: "https://example.test/app-arm64.dmg".into(),
                size: 3,
            },
        ];

        let selected =
            select_release_asset(&assets, "macos", "aarch64").expect("should pick a macOS dmg");

        assert_eq!(selected.name, "VarSwitch_1.2.0_aarch64.dmg");
    }

    #[test]
    fn resolve_proxy_url_ignores_disabled_loopback_proxy() {
        let proxy = resolve_proxy_url_from_values(&[
            Some("http://127.0.0.1:9"),
            Some("http://127.0.0.1:9"),
            None,
            None,
        ]);

        assert_eq!(
            proxy, None,
            "discard-style loopback proxy should be ignored instead of breaking GitHub requests"
        );
    }

    #[test]
    fn resolve_proxy_url_keeps_real_proxy_values() {
        let proxy = resolve_proxy_url_from_values(&[
            Some("http://proxy.example.com:8080"),
            None,
            None,
            None,
        ]);

        assert_eq!(proxy.as_deref(), Some("http://proxy.example.com:8080"));
    }

    #[test]
    fn github_release_struct_deserializes_snake_case_payload() {
        let release: GitHubRelease = serde_json::from_value(json!({
            "tag_name": "v1.0.2",
            "html_url": "https://github.com/ConcertoNotes/variable-switching/releases/tag/v1.0.2",
            "body": "notes",
            "published_at": "2026-02-25T06:03:09Z",
            "assets": [{
                "name": "VarSwitch_1.0.2_x64_en-US.msi",
                "browser_download_url": "https://example.test/VarSwitch_1.0.2_x64_en-US.msi",
                "size": 12345
            }]
        }))
        .expect("GitHub release JSON should deserialize");

        assert_eq!(release.tag_name, "v1.0.2");
        assert_eq!(
            release.assets[0].browser_download_url,
            "https://example.test/VarSwitch_1.0.2_x64_en-US.msi"
        );
    }

    #[test]
    fn tauri_config_does_not_define_static_tray_icon_when_tray_is_built_in_setup() {
        let config: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json"))
            .expect("valid tauri.conf.json");

        let static_tray_icon = config.get("app").and_then(|app| app.get("trayIcon"));

        assert!(
            static_tray_icon.is_none(),
            "tauri.conf.json should not also define app.trayIcon when setup() builds the tray icon"
        );
    }
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            cancel_flag: AtomicBool::new(false),
        })
        .setup(|app| {
            // 初始化日志系统（写入 app_data_dir/logs/varswitch.log）
            init_logging(&app.handle());

            // 读取应用设置
            let settings = read_app_settings(&app.handle());
            let silent_startup = settings.silent_startup;

            // Build tray menu
            let show_item = MenuItemBuilder::with_id("show", "显示主窗口").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "退出").build(app)?;
            let menu = MenuBuilder::new(app)
                .items(&[&show_item, &quit_item])
                .build()?;

            // Build tray icon
            let tray_builder = TrayIconBuilder::new()
                .tooltip("VarSwitch")
                .menu(&menu);
            // 图标不可用时不阻断启动，仅无图标显示
            let tray_builder = if let Some(icon) = app.default_window_icon() {
                tray_builder.icon(icon.clone())
            } else {
                tray_builder
            };
            tray_builder
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.unminimize();
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.unminimize();
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;

            // 窗口关闭行为：根据设置决定隐藏到托盘还是退出
            if let Some(window) = app.get_webview_window("main") {
                let window_clone = window.clone();
                let app_handle = app.handle().clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        // 运行时重新读取设置，以便用户更改后立即生效
                        let current_settings = read_app_settings(&app_handle);
                        if current_settings.minimize_to_tray {
                            api.prevent_close();
                            let _ = window_clone.hide();
                        }
                        // 否则不阻止关闭，正常退出
                    }
                });
            } else {
                log_error!("[setup] 未找到主窗口 main，跳过窗口事件绑定");
            }

            // 静默启动：启动时隐藏窗口到托盘
            if silent_startup {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_profiles,
            test_api_endpoints,
            add_profile,
            update_profile,
            delete_profile,
            switch_profile,
            get_status,
            get_detected_editors,
            import_current,
            snapshot_config,
            restore_config,
            list_config_backups,
            restore_config_backup,
            open_backups_folder,
            cancel_switch,
            get_codex_profiles,
            add_codex_profile,
            update_codex_profile,
            delete_codex_profile,
            switch_codex_profile,
            import_codex_current,
            get_codex_status,
            get_codex_toolbox,
            apply_plugin_marketplace,
            sync_codex_sessions,
            select_mobile_thread,
            configure_mobile_channel,
            start_lark_bot_registration,
            poll_lark_bot_registration,
            open_lark_bot_launcher,
            clear_mobile_channel_binding,
            start_qq_qr_binding,
            start_wechat_qr_binding,
            poll_wechat_qr_binding,
            bind_codex_thread,
            unbind_codex_thread,
            start_mobile_remote,
            stop_mobile_remote,
            get_app_settings,
            save_app_settings,
            get_app_paths,
            open_folder,
            open_logs_folder,
            open_external_target,
            check_app_update,
            download_and_open_update,
            export_profiles,
            import_profiles,
            get_skills,
            save_skill,
            delete_skill,
            get_claude_md,
            save_claude_md,
            get_prompt_templates,
            get_mcp_servers_list,
            save_mcp_server,
            delete_mcp_server_entry,
            get_mcp_presets,
            get_skill_repos,
            add_skill_repo,
            remove_skill_repo,
            get_catalog_skills,
            install_skill_from_url,
            search_github_skills,
            search_github_mcp,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
