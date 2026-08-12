mod claude_proxy;
mod usage_stats;

use base64::Engine;
use image::{DynamicImage, Luma};
use qrcode::{EcLevel, QrCode};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::cmp::Ordering as CmpOrdering;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::OnceLock;
use std::sync::{mpsc, Condvar, Mutex};
use std::time::{Duration, Instant};
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, State,
};
use tauri_plugin_updater::UpdaterExt;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
#[cfg(target_os = "windows")]
use winreg::enums::HKEY_CURRENT_USER;
#[cfg(target_os = "windows")]
use winreg::RegKey;

// Windows 常量：CREATE_NO_WINDOW 标志，用于隐藏子进程窗口
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

const AUTH_TOKEN_ENV: &str = "ANTHROPIC_AUTH_TOKEN";
const AUTH_KEY_ENV: &str = "ANTHROPIC_AUTH_KEY";
const LEGACY_AUTH_ENV: &str = "ANTHROPIC_API_KEY";
const BASE_URL_ENV: &str = "ANTHROPIC_BASE_URL";
/// xAI / Grok 官方环境变量
const XAI_API_KEY_ENV: &str = "XAI_API_KEY";
const XAI_BASE_URL_ENV: &str = "XAI_BASE_URL";
const XAI_MODEL_ENV: &str = "XAI_MODEL";
/// 部分工具使用的兼容别名
const GROK_API_KEY_ENV: &str = "GROK_API_KEY";
const GROK_BASE_URL_ENV: &str = "GROK_BASE_URL";
const DEFAULT_XAI_BASE_URL: &str = "https://api.x.ai/v1";
const GEMINI_API_KEY_ENV: &str = "GEMINI_API_KEY";
const GOOGLE_GEMINI_BASE_URL_ENV: &str = "GOOGLE_GEMINI_BASE_URL";
const GEMINI_MODEL_ENV: &str = "GEMINI_MODEL";
const CODEX_IMAGE_API_KEY_ENV: &str = "VARSWITCH_IMAGE_API_KEY";
const CODEX_IMAGE_BASE_URL_ENV: &str = "VARSWITCH_IMAGE_BASE_URL";
const CODEX_IMAGE_MODEL_ENV: &str = "VARSWITCH_IMAGE_MODEL";
const CODEX_IMAGE_MODEL: &str = "gpt-image-2";
const CODEX_IMAGE_SKILL_ID: &str = "varswitch-imagegen";
const CODEX_IMAGE_PRIORITY_START: &str = "<!-- VARSWITCH:IMAGE-SKILL-PRIORITY:START -->";
const CODEX_IMAGE_PRIORITY_END: &str = "<!-- VARSWITCH:IMAGE-SKILL-PRIORITY:END -->";
const CODEX_IMAGE_PRIORITY_INSTRUCTIONS: &str = r#"<!-- VARSWITCH:IMAGE-SKILL-PRIORITY:START -->
## VarSwitch image generation routing

- For image creation or rendering requests, prefer `varswitch-imagegen` and read its `SKILL.md` before selecting an image-generation path.
- Keep the built-in `imagegen` available as fallback. Use it when `varswitch-imagegen` is missing, not configured, or fails, or when the user explicitly requests the built-in path.
<!-- VARSWITCH:IMAGE-SKILL-PRIORITY:END -->"#;
/// VarSwitch 在 ~/.grok/config.toml 中管理的模型段 ID
const GROK_MANAGED_MODEL_ID: &str = "varswitch";
const SWITCH_TOTAL_STEPS: u32 = 6;
const APP_DOWNLOAD_PAGE_URL: &str = "https://download.varswitch.strova.top/";
const ENDPOINT_TEST_DEFAULT_TIMEOUT_SECS: u64 = 8;
const ENDPOINT_TEST_MIN_TIMEOUT_SECS: u64 = 2;
const ENDPOINT_TEST_MAX_TIMEOUT_SECS: u64 = 30;
static QQ_QR_ACTIVE: AtomicBool = AtomicBool::new(false);
// B2: QQ 扫码子进程句柄，用于主动取消
static QQ_QR_CHILD: Mutex<Option<std::process::Child>> = Mutex::new(None);
// QQ 扫码轮次。取消或超时后递增，防止旧扫码 worker 清理新一轮子进程。
static QQ_QR_GENERATION: AtomicU64 = AtomicU64::new(0);
// 取消与扫码 worker 的状态写入必须串行，避免取消后旧 worker 回写二维码或凭据。
static QQ_QR_STATE_LOCK: Mutex<()> = Mutex::new(());
static WECHAT_QR_ACTIVE: AtomicBool = AtomicBool::new(false);
static LARK_REGISTRATION_ACTIVE: AtomicBool = AtomicBool::new(false);
static MOBILE_REMOTE_START_ACTIVE: AtomicBool = AtomicBool::new(false);
static MOBILE_REMOTE_CANCEL_REQUESTED: AtomicBool = AtomicBool::new(false);
// B3: 保证看门狗单例，start→stop→start 快速操作不会产生多个看门狗
static WATCHDOG_ACTIVE: AtomicBool = AtomicBool::new(false);
// B6: 连接代际计数器，每次新 WS 连接递增，清理时校验代际避免新连接被旧连接收尾代码打翻
static SMART_CONTROL_WS_GENERATION: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);
// B16: 飞书/QQ token 缓存（避免每条消息重新鉴权，防止触发限频）
// 格式：app_id → (token, expiry_unix_secs)
static LARK_TOKEN_CACHE: OnceLock<Mutex<std::collections::HashMap<String, (String, u64)>>> =
    OnceLock::new();
static QQ_TOKEN_CACHE: OnceLock<Mutex<std::collections::HashMap<String, (String, u64)>>> =
    OnceLock::new();
// B9: 每通道一把消息处理串行锁，防止并发处理同一通道的消息导致 Codex 乱序响应
// （完整队列+超时+去重需重构整个消息流，这里做最小化串行互斥）
static LARK_MSG_LOCK: Mutex<()> = Mutex::new(());
static QQ_MSG_LOCK: Mutex<()> = Mutex::new(());
static WECHAT_MSG_LOCK: Mutex<()> = Mutex::new(());
static LARK_BRIDGE_ACTIVE: AtomicBool = AtomicBool::new(false);
static LARK_BRIDGE_CHILD: Mutex<Option<std::process::Child>> = Mutex::new(None);
static QQ_GATEWAY_ACTIVE: AtomicBool = AtomicBool::new(false);
static QQ_GATEWAY_CHILD: Mutex<Option<std::process::Child>> = Mutex::new(None);
static WECHAT_LISTENER_ACTIVE: AtomicBool = AtomicBool::new(false);
// B8: 全局写锁，保证多线程并发写 toolbox state 时不互相丢更新
static TOOLBOX_STATE_WRITE_LOCK: Mutex<()> = Mutex::new(());
// B18: QQ msg_seq 递增计数器，避免同毫秒并发时碰撞导致平台去重吞消息
static QQ_MSG_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
static SMART_CONTROL_STATUS_CACHE: Mutex<Option<SmartControlStatus>> = Mutex::new(None);
static SMART_CONTROL_SERVER_ACTIVE: AtomicBool = AtomicBool::new(false);
static SMART_CONTROL_SERVER_CANCEL_REQUESTED: AtomicBool = AtomicBool::new(false);
static SMART_CONTROL_REMOTE_CONNECTED: AtomicBool = AtomicBool::new(false);
static SMART_CONTROL_LAST_EVENT: Mutex<Option<SmartControlEvent>> = Mutex::new(None);
static SMART_CONTROL_WS_WRITER: Mutex<Option<TcpStream>> = Mutex::new(None);
static SMART_CONTROL_NEXT_REQUEST_ID: OnceLock<Mutex<u64>> = OnceLock::new();
static SMART_CONTROL_PENDING: OnceLock<(Mutex<HashMap<u64, SmartControlPendingResult>>, Condvar)> =
    OnceLock::new();
static SMART_CONTROL_EVENT_LOG: OnceLock<Mutex<Vec<SmartControlEvent>>> = OnceLock::new();
// B9: 各通道最近收到的 message_id 去重缓冲（上限 512 条，防止平台重投消息重复触发 Codex）
static LARK_SEEN_MSG_IDS: OnceLock<Mutex<std::collections::VecDeque<String>>> = OnceLock::new();
static QQ_SEEN_MSG_IDS: OnceLock<Mutex<std::collections::VecDeque<String>>> = OnceLock::new();
// B13: 飞书注册代际计数器，防止旧 worker 把过期凭据覆盖新凭据
static LARK_REGISTRATION_GENERATION: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);
static SMART_CONTROL_CLIENT_STATE: OnceLock<Mutex<SmartControlClientState>> = OnceLock::new();
static SMART_CONTROL_SERVER_CHUNKS: OnceLock<Mutex<HashMap<String, SmartControlChunkAssembly>>> =
    OnceLock::new();
static SMART_CONTROL_TURN_STREAMS: OnceLock<Mutex<HashMap<u64, SmartControlTurnAccumulator>>> =
    OnceLock::new();
static SMART_CONTROL_APPROVALS: OnceLock<Mutex<Vec<SmartControlApprovalRequest>>> = OnceLock::new();

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
    /// anthropic（默认，直连 Anthropic Messages 端点）
    /// | openai_chat（上游仅有 OpenAI Chat Completions 接口，经本地代理转换协议）
    #[serde(default = "default_claude_api_format")]
    api_format: String,
    is_active: bool,
    created_at: String,
}

fn default_claude_api_format() -> String {
    "anthropic".to_string()
}

fn normalize_claude_api_format(raw: &str) -> String {
    match raw.trim() {
        "openai_chat" => "openai_chat".to_string(),
        _ => default_claude_api_format(),
    }
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
    /// 上游协议：responses（OpenAI Responses，默认）| chat（OpenAI Chat Completions）
    #[serde(default = "default_codex_wire_api")]
    wire_api: String,
    #[serde(default)]
    model: String,
    #[serde(default)]
    provider_name: String,
    #[serde(default)]
    image_api_key: String,
    #[serde(default)]
    image_base_url: String,
    is_active: bool,
    created_at: String,
}

#[derive(Serialize, Deserialize, Default)]
struct CodexProfilesData {
    profiles: Vec<CodexProfile>,
    /// 首次读取旧档案时清理历史默认图片地址；迁移完成后保留用户手动填写的 URL。
    #[serde(default)]
    image_base_url_migrated: bool,
}

/// Grok / xAI API 配置档案（对应 ~/.grok/config.toml 的 [model.*]）
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct GrokProfile {
    id: String,
    name: String,
    api_key: String,
    base_url: String,
    #[serde(default)]
    model: String,
    /// chat_completions | responses | messages
    #[serde(default = "default_grok_api_backend")]
    api_backend: String,
    is_active: bool,
    created_at: String,
}

#[derive(Serialize, Deserialize, Default)]
struct GrokProfilesData {
    profiles: Vec<GrokProfile>,
}

/// Gemini CLI API Key 配置档案（对应 ~/.gemini/settings.json 与官方环境变量）。
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct GeminiProfile {
    id: String,
    name: String,
    api_key: String,
    base_url: String,
    #[serde(default)]
    model: String,
    is_active: bool,
    created_at: String,
}

#[derive(Serialize, Deserialize, Default)]
struct GeminiProfilesData {
    profiles: Vec<GeminiProfile>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct GeminiRuntimeStatus {
    api_key: String,
    base_url: String,
    model: String,
    auth_type: String,
    settings_path: String,
    settings_exists: bool,
    source: String,
}

fn default_grok_api_backend() -> String {
    "chat_completions".to_string()
}

/// 返回给前端的 Grok 运行时状态（比 LocationStatus 更完整）
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct GrokRuntimeStatus {
    api_key: String,
    base_url: String,
    model: String,
    default_model_id: String,
    api_backend: String,
    config_path: String,
    config_exists: bool,
    source: String, // "config.toml" | "env" | "none"
}

/// Grok 诊断信息
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct GrokConfigDiagnostics {
    config_path: String,
    config_exists: bool,
    has_default_model: bool,
    has_api_key: bool,
    has_base_url: bool,
    default_model_id: String,
    model: String,
    base_url: String,
    api_backend: String,
    active_profile_name: String,
    source: String,
    issues: Vec<String>,
    suggestions: Vec<String>,
    last_checked_at: String,
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
struct TrashedCodexThreadRecord {
    id: String,
    thread_name: String,
    updated_at: String,
    session_file: String,
    cwd: String,
    last_user_message: String,
    last_assistant_message: String,
    deleted_at: String,
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

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
struct CodexBuiltinPluginSkill {
    name: String,
    description: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
struct CodexBuiltinPluginItem {
    id: String,
    name: String,
    display_name: String,
    marketplace: String,
    version: String,
    description: String,
    root: String,
    enabled: bool,
    important: bool,
    skills: Vec<CodexBuiltinPluginSkill>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
struct CodexBuiltinPluginStatus {
    available: bool,
    marketplace_source: String,
    marketplace_configured: bool,
    enabled_count: usize,
    total_count: usize,
    important_enabled_count: usize,
    important_total_count: usize,
    plugins: Vec<CodexBuiltinPluginItem>,
    last_error: String,
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
    #[serde(default)]
    remote_control_preferred: bool,
    #[serde(default)]
    remote_control_status: String,
    #[serde(default)]
    remote_control_backend_url: String,
    #[serde(default)]
    remote_control_connected: bool,
    #[serde(default)]
    remote_control_detail: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
struct SmartControlStatus {
    available: bool,
    connected: bool,
    backend_url: String,
    status: String,
    detail: String,
    checked_at: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
struct SmartControlEvent {
    received_at: String,
    event_type: String,
    message_id: String,
    method: String,
    raw_preview: String,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
struct SmartControlDebugSnapshot {
    connected: bool,
    pending_count: usize,
    last_event: Option<SmartControlEvent>,
    events: Vec<SmartControlEvent>,
    client: SmartControlClientState,
    approvals: Vec<SmartControlApprovalRequest>,
}

#[derive(Clone, Debug, Default)]
struct SmartControlPendingResult {
    done: bool,
    text: String,
    error: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct SmartControlClientState {
    client_id: String,
    stream_id: String,
    next_seq_id: u64,
    initialized: bool,
    cursor: String,
    last_pong_status: String,
    last_initialize_id: u64,
    active_thread_id: String,
}

impl Default for SmartControlClientState {
    fn default() -> Self {
        Self {
            client_id: format!("varswitch-client-{}", uuid::Uuid::new_v4()),
            stream_id: format!("varswitch-stream-{}", uuid::Uuid::new_v4()),
            next_seq_id: 1,
            initialized: false,
            cursor: String::new(),
            last_pong_status: String::new(),
            last_initialize_id: 0,
            active_thread_id: String::new(),
        }
    }
}

#[derive(Clone, Debug, Default)]
struct SmartControlChunkAssembly {
    segment_count: usize,
    message_size_bytes: usize,
    raw: Vec<u8>,
    next_segment_id: usize,
}

#[derive(Clone, Debug, Default)]
struct SmartControlTurnAccumulator {
    text_parts: Vec<String>,
    final_text: String,
    error: String,
    done: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
struct SmartControlApprovalRequest {
    request_id: String,
    method: String,
    title: String,
    body: String,
    options: Vec<String>,
    received_at: String,
    raw_preview: String,
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
            remote_control_preferred: true,
            remote_control_status: "未检测".into(),
            remote_control_backend_url: "http://127.0.0.1:3847".into(),
            remote_control_connected: false,
            remote_control_detail: String::new(),
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
    trashed_codex_threads: Vec<TrashedCodexThreadRecord>,
    session_sync: CodexSessionSyncState,
    mobile_channels: Vec<MobileChannelBinding>,
    selected_mobile_thread_id: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ToolboxSnapshot {
    plugin_marketplace_input: String,
    plugin_marketplaces: Vec<PluginMarketplaceItem>,
    builtin_plugins: CodexBuiltinPluginStatus,
    session_bindings: Vec<CodexSessionBinding>,
    codex_threads: Vec<CodexThreadRecord>,
    synced_codex_threads: Vec<CodexThreadRecord>,
    trashed_codex_threads: Vec<TrashedCodexThreadRecord>,
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

fn default_codex_wire_api() -> String {
    "responses".to_string()
}

/// 规范化 Codex 上游协议，只允许 responses / chat 两种取值。
fn normalize_codex_wire_api(raw: &str) -> String {
    match raw.trim() {
        "chat" => "chat".to_string(),
        _ => default_codex_wire_api(),
    }
}

/// 各应用官方 API 默认地址：Base URL 留空时回退到官方端点（与 cc-switch 行为一致）。
const DEFAULT_ANTHROPIC_BASE_URL: &str = "https://api.anthropic.com";
const DEFAULT_OPENAI_BASE_URL: &str = "https://api.openai.com/v1";
const DEFAULT_GEMINI_BASE_URL: &str = "https://generativelanguage.googleapis.com";

/// trim 后为空则使用默认地址，否则去掉尾部斜杠。
fn resolve_base_url_or_default(raw: &str, default_url: &str) -> String {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        default_url.to_string()
    } else {
        trimmed.to_string()
    }
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
    image_api_key: String,
    image_base_url: String,
    image_skill_installed: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StatusResult {
    env_vars: Option<LocationStatus>,
    /// 动态编辑器状态: key = 编辑器 id, value = 状态
    editors: HashMap<String, LocationStatus>,
    claude: Option<LocationStatus>,
    claude_model: String,
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

#[derive(Clone, Debug)]
struct DownloadPageRelease {
    version: String,
    file_name: String,
    download_url: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct EndpointLatency {
    url: String,
    latency: Option<u128>,
    status: Option<u16>,
    error: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct AvailableModel {
    id: String,
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
    codex_settings: String,
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
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}.{ms:03}")
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
    let msg = redact_log_message(msg);
    let line = format!(
        "[{}] [{level}] {msg}",
        format_log_time(chrono_timestamp_millis())
    );
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

/// 日志可能包含平台网关地址或错误原文。统一隐藏 URL 查询参数和常见凭据赋值，
/// 避免后续新增日志点时意外写入 token、secret 或临时授权票据。
fn redact_log_message(msg: &str) -> String {
    let mut output = String::with_capacity(msg.len());
    let mut remainder = msg;
    loop {
        let http = remainder.find("http://");
        let https = remainder.find("https://");
        let start = match (http, https) {
            (Some(a), Some(b)) => a.min(b),
            (Some(a), None) | (None, Some(a)) => a,
            (None, None) => {
                output.push_str(remainder);
                break;
            }
        };
        output.push_str(&remainder[..start]);
        let url_and_rest = &remainder[start..];
        let end = url_and_rest
            .find(char::is_whitespace)
            .unwrap_or(url_and_rest.len());
        let url = &url_and_rest[..end];
        if let Some(query_start) = url.find('?') {
            output.push_str(&url[..query_start]);
            output.push_str("?[query-redacted]");
        } else {
            output.push_str(url);
        }
        remainder = &url_and_rest[end..];
    }

    const SENSITIVE_MARKERS: [&str; 11] = [
        "access_key=",
        "ticket=",
        "token=",
        "secret=",
        "authorization=",
        "app_secret=",
        "bot_token=",
        "appsecret=",
        "bottoken=",
        "api_key=",
        "apikey=",
    ];
    for marker in SENSITIVE_MARKERS {
        let mut search_from = 0;
        loop {
            let lower = output[search_from..].to_ascii_lowercase();
            let Some(relative) = lower.find(marker) else {
                break;
            };
            let value_start = search_from + relative + marker.len();
            let value_end = output[value_start..]
                .find(|c: char| {
                    c.is_whitespace() || matches!(c, '&' | ',' | ';' | ']' | '}' | '"' | '\'')
                })
                .map(|offset| value_start + offset)
                .unwrap_or(output.len());
            output.replace_range(value_start..value_end, "***");
            search_from = value_start + 3;
        }
    }
    output
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

/// 以「仅所有者可读写」的权限写入含敏感信息（API Key / token 等）的文件。
/// Unix 下写入后设置 0600；Windows 依赖 AppData 默认 ACL（仅当前用户可访问）。
fn write_private_file(path: &Path, contents: &str) -> Result<(), String> {
    fs::write(path, contents).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

fn write_profiles_to_path(path: &PathBuf, data: &ProfilesData) -> Result<(), String> {
    let json = serde_json::to_string_pretty(data).map_err(|e| e.to_string())?;
    write_private_file(path, &json)
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
        return CodexProfilesData {
            image_base_url_migrated: true,
            ..CodexProfilesData::default()
        };
    }
    let mut data: CodexProfilesData = fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    // 图片 Base URL 不设默认值。旧档案只在首次读取时清理一次，确保历史默认值
    // 为空；迁移完成后用户主动填写的自定义 URL 可以正常保留。
    let migrated = if data.image_base_url_migrated {
        false
    } else {
        let _ = clear_codex_image_base_urls(&mut data);
        data.image_base_url_migrated = true;
        true
    };
    if migrated {
        let _ = write_codex_profiles(app, &data);
    }
    data
}

fn clear_codex_image_base_urls(data: &mut CodexProfilesData) -> bool {
    let mut changed = false;
    for profile in &mut data.profiles {
        if !profile.image_base_url.is_empty() {
            profile.image_base_url.clear();
            changed = true;
        }
    }
    changed
}

fn write_codex_profiles(app: &tauri::AppHandle, data: &CodexProfilesData) -> Result<(), String> {
    let path = codex_profiles_path(app);
    let json = serde_json::to_string_pretty(data).map_err(|e| e.to_string())?;
    write_private_file(&path, &json)
}

fn grok_profiles_path(app: &tauri::AppHandle) -> PathBuf {
    data_dir(app).join("grok_profiles.json")
}

fn read_grok_profiles(app: &tauri::AppHandle) -> GrokProfilesData {
    let path = grok_profiles_path(app);
    if !path.exists() {
        return GrokProfilesData::default();
    }
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn write_grok_profiles(app: &tauri::AppHandle, data: &GrokProfilesData) -> Result<(), String> {
    let path = grok_profiles_path(app);
    let json = serde_json::to_string_pretty(data).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())
}

fn gemini_profiles_path(app: &tauri::AppHandle) -> PathBuf {
    data_dir(app).join("gemini_profiles.json")
}

fn read_gemini_profiles(app: &tauri::AppHandle) -> GeminiProfilesData {
    let path = gemini_profiles_path(app);
    if !path.exists() {
        return GeminiProfilesData::default();
    }
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn write_gemini_profiles(app: &tauri::AppHandle, data: &GeminiProfilesData) -> Result<(), String> {
    let json = serde_json::to_string_pretty(data).map_err(|e| e.to_string())?;
    fs::write(gemini_profiles_path(app), json).map_err(|e| e.to_string())
}

fn default_xai_base_url() -> String {
    DEFAULT_XAI_BASE_URL.to_string()
}

fn grok_config_dir() -> PathBuf {
    home_dir().join(".grok")
}

fn grok_config_path() -> PathBuf {
    grok_config_dir().join("config.toml")
}

fn gemini_settings_path() -> PathBuf {
    home_dir().join(".gemini").join("settings.json")
}

/// 删除 TOML 中指定 section（含表头与正文），保留其它内容。
fn remove_toml_section(config: &str, section: &str) -> String {
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
fn upsert_toml_table_string_key(config: &str, table: &str, key: &str, value: &str) -> String {
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
fn read_grok_runtime_status() -> GrokRuntimeStatus {
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
fn read_grok_status() -> Option<LocationStatus> {
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

fn read_grok_current_model_id() -> String {
    let status = read_grok_runtime_status();
    if !status.model.is_empty() {
        return status.model;
    }
    reg_get_env_opt(XAI_MODEL_ENV).unwrap_or_default()
}

fn normalize_grok_api_backend(value: &str) -> String {
    match value.trim() {
        "responses" => "responses".to_string(),
        "messages" => "messages".to_string(),
        _ => default_grok_api_backend(),
    }
}

fn read_grok_config_diagnostics(app: &tauri::AppHandle) -> GrokConfigDiagnostics {
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
fn apply_grok_to_system_env(api_key: &str, base_url: &str, model: &str) -> Result<(), String> {
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
fn write_grok_config(profile: &GrokProfile) -> Result<(), String> {
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

    fs::write(&path, content).map_err(|e| format!("写入 ~/.grok/config.toml 失败: {e}"))?;
    log_info!(
        "[grok] 已写入 ~/.grok/config.toml model={} base_url={} api_backend={}",
        model_id,
        base_url,
        api_backend
    );
    Ok(())
}

fn read_gemini_runtime_status() -> GeminiRuntimeStatus {
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

fn write_gemini_settings(profile: &GeminiProfile) -> Result<(), String> {
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

fn apply_gemini_to_system_env(profile: &GeminiProfile) -> Result<(), String> {
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

fn codex_global_agents_path() -> PathBuf {
    codex_config_dir().join("AGENTS.md")
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

fn default_smart_control_backend_url() -> &'static str {
    "http://127.0.0.1:3847"
}

fn normalize_smart_control_backend_url(value: &str) -> String {
    let mut raw = value.trim().trim_end_matches('/').to_string();
    if raw.is_empty() {
        raw = default_smart_control_backend_url().to_string();
    }
    if !raw.starts_with("http://") && !raw.starts_with("https://") {
        raw = format!("http://{raw}");
    }
    raw.trim_end_matches('/').to_string()
}

#[allow(dead_code)]
fn smart_control_status_from_cache() -> Option<SmartControlStatus> {
    SMART_CONTROL_STATUS_CACHE
        .lock()
        .ok()
        .and_then(|guard| guard.clone())
}

fn set_smart_control_status_cache(status: SmartControlStatus) {
    if let Ok(mut guard) = SMART_CONTROL_STATUS_CACHE.lock() {
        *guard = Some(status);
    }
}

fn probe_smart_control_backend(base_url: &str) -> SmartControlStatus {
    let backend_url = normalize_smart_control_backend_url(base_url);
    let checked_at = chrono_now();
    let client = match reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(1500))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            return SmartControlStatus {
                available: false,
                connected: false,
                backend_url,
                status: "高级控制通道不可用".into(),
                detail: format!("创建 HTTP 客户端失败: {error}"),
                checked_at,
            };
        }
    };

    let status_url = format!("{backend_url}/api/remote-control/status");
    let response = client.get(&status_url).send();
    let Ok(response) = response else {
        return SmartControlStatus {
            available: false,
            connected: false,
            backend_url,
            status: "未连接高级控制通道".into(),
            detail: "未检测到本机 Codex 控制服务；将自动使用兼容模式。".into(),
            checked_at,
        };
    };
    let http_status = response.status();
    let body = response.text().unwrap_or_default();
    if !http_status.is_success() {
        return SmartControlStatus {
            available: false,
            connected: false,
            backend_url,
            status: "高级控制通道响应异常".into(),
            detail: format!("HTTP {}：{}", http_status.as_u16(), body.trim()),
            checked_at,
        };
    }

    let json: serde_json::Value = serde_json::from_str(&body).unwrap_or_else(|_| {
        serde_json::json!({
            "raw": body,
        })
    });
    let connected = json
        .get("connected")
        .or_else(|| json.get("remoteControlConnected"))
        .or_else(|| json.get("ready"))
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let detail = if connected {
        json_string(
            &json,
            &["clientName", "deviceName", "activeClient", "detail"],
        )
    } else {
        json_string(&json, &["lastError", "error", "message", "detail"])
    };
    let last_event_summary = json
        .get("lastEvent")
        .and_then(|event| event.as_object())
        .map(|event| {
            let method = event
                .get("method")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            let event_type = event
                .get("eventType")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            let received_at = event
                .get("receivedAt")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            [event_type, method, received_at]
                .into_iter()
                .filter(|part| !part.trim().is_empty())
                .collect::<Vec<_>>()
                .join(" · ")
        })
        .filter(|summary| !summary.trim().is_empty())
        .unwrap_or_default();
    SmartControlStatus {
        available: true,
        connected,
        backend_url,
        status: if connected {
            "高级控制通道已连接".into()
        } else {
            "高级控制通道等待 Codex 连接".into()
        },
        detail: if detail.is_empty() {
            if connected {
                if last_event_summary.is_empty() {
                    "Codex 已连接，可以使用协议级手机控制。".into()
                } else {
                    format!("Codex 已连接，最近事件：{last_event_summary}")
                }
            } else {
                "本机控制服务已响应，但尚未检测到 Codex App / CLI remote-control 连接。".into()
            }
        } else {
            detail
        },
        checked_at,
    }
}

fn refresh_smart_control_status_for_state(state: &mut ToolboxState) -> SmartControlStatus {
    let backend_url =
        normalize_smart_control_backend_url(&state.mobile_remote.remote_control_backend_url);
    let status = probe_smart_control_backend(&backend_url);
    state.mobile_remote.remote_control_backend_url = status.backend_url.clone();
    state.mobile_remote.remote_control_connected = status.connected;
    state.mobile_remote.remote_control_status = status.status.clone();
    state.mobile_remote.remote_control_detail = status.detail.clone();
    state.mobile_remote.remote_control_preferred = true;
    set_smart_control_status_cache(status.clone());
    status
}

fn smart_control_bind_addr_from_url(base_url: &str) -> String {
    let normalized = normalize_smart_control_backend_url(base_url);
    // 安全：本机控制服务（HTTP/WebSocket）没有鉴权，只从 URL 中取端口，
    // 强制绑定到回环地址，避免因 URL 中出现 0.0.0.0 或局域网 IP 而把无鉴权
    // 的控制通道暴露给同一局域网内的其他设备。
    let port = reqwest::Url::parse(&normalized)
        .ok()
        .and_then(|url| url.port_or_known_default())
        .unwrap_or(3847);
    format!("127.0.0.1:{port}")
}

fn smart_control_http_response(status: &str, content_type: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}; charset=utf-8\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: authorization, content-type, openai-sentinel-chat-requirements-token\r\nConnection: close\r\n\r\n{body}",
        body.as_bytes().len()
    )
}

fn http_header_value<'a>(head: &'a str, name: &str) -> Option<&'a str> {
    let target = name.to_ascii_lowercase();
    for line in head.lines().skip(1) {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        if key.trim().to_ascii_lowercase() == target {
            return Some(value.trim());
        }
    }
    None
}

fn is_websocket_upgrade(head: &str) -> bool {
    let upgrade = http_header_value(head, "upgrade")
        .map(|value| value.to_ascii_lowercase().contains("websocket"))
        .unwrap_or(false);
    let connection = http_header_value(head, "connection")
        .map(|value| value.to_ascii_lowercase().contains("upgrade"))
        .unwrap_or(false);
    upgrade && connection && http_header_value(head, "sec-websocket-key").is_some()
}

fn websocket_accept_key(key: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(key.trim().as_bytes());
    hasher.update(b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
    let digest = hasher.finalize();
    base64_encode_bytes(&digest)
}

fn websocket_upgrade_response(key: &str) -> String {
    format!(
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n",
        websocket_accept_key(key)
    )
}

fn websocket_send_text(stream: &mut TcpStream, text: &str) -> Result<(), String> {
    let data = text.as_bytes();
    let mut frame = Vec::with_capacity(data.len() + 10);
    frame.push(0x81);
    if data.len() < 126 {
        frame.push(data.len() as u8);
    } else if data.len() <= u16::MAX as usize {
        frame.push(126);
        frame.extend_from_slice(&(data.len() as u16).to_be_bytes());
    } else {
        frame.push(127);
        frame.extend_from_slice(&(data.len() as u64).to_be_bytes());
    }
    frame.extend_from_slice(data);
    stream
        .write_all(&frame)
        .map_err(|e| format!("发送 WebSocket 文本帧失败: {e}"))
}

fn websocket_send_pong(stream: &mut TcpStream, payload: &[u8]) -> Result<(), String> {
    let payload = &payload[..payload.len().min(125)];
    let mut frame = Vec::with_capacity(payload.len() + 2);
    frame.push(0x8a);
    frame.push(payload.len() as u8);
    frame.extend_from_slice(payload);
    stream
        .write_all(&frame)
        .map_err(|e| format!("发送 WebSocket pong 失败: {e}"))
}

// B6: timeout 与 EOF 使用不同返回值区分：
//   Ok(None)         → 对端正常关闭（EOF/ConnectionReset）
//   Err("__timeout__") → 读超时，调用方应发 ping 继续等
//   Err(msg)         → 其他错误，断开
// B19: fragment_buf 用于累积分片帧（opcode 0x0 continuation），
//      首片(opcode=0x1, FIN=0)开始写入，末片(opcode=0x0, FIN=1)完成后返回完整消息
fn websocket_read_text_frame(
    stream: &mut TcpStream,
    fragment_buf: &mut Vec<u8>,
) -> Result<Option<String>, String> {
    let mut head = [0u8; 2];
    match stream.read_exact(&mut head) {
        Ok(()) => {}
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            ) =>
        {
            // B6: 超时 → 返回特殊标记，调用方发 ping 继续等
            return Err("__timeout__".into());
        }
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::UnexpectedEof | std::io::ErrorKind::ConnectionReset
            ) =>
        {
            return Ok(None);
        }
        Err(error) => return Err(format!("读取 WebSocket 帧头失败: {error}")),
    }
    let fin = (head[0] & 0x80) != 0;
    let opcode = head[0] & 0x0f;
    let masked = (head[1] & 0x80) != 0;
    let mut len = (head[1] & 0x7f) as usize;
    if len == 126 {
        let mut ext = [0u8; 2];
        stream
            .read_exact(&mut ext)
            .map_err(|e| format!("读取 WebSocket 16-bit 长度失败: {e}"))?;
        len = u16::from_be_bytes(ext) as usize;
    } else if len == 127 {
        let mut ext = [0u8; 8];
        stream
            .read_exact(&mut ext)
            .map_err(|e| format!("读取 WebSocket 64-bit 长度失败: {e}"))?;
        len = u64::from_be_bytes(ext) as usize;
        if len > 8 * 1024 * 1024 {
            return Err("WebSocket 帧过大".into());
        }
    }
    let mut mask = [0u8; 4];
    if masked {
        stream
            .read_exact(&mut mask)
            .map_err(|e| format!("读取 WebSocket mask 失败: {e}"))?;
    }
    let mut payload = vec![0u8; len];
    if len > 0 {
        stream
            .read_exact(&mut payload)
            .map_err(|e| format!("读取 WebSocket payload 失败: {e}"))?;
    }
    if masked {
        for (idx, byte) in payload.iter_mut().enumerate() {
            *byte ^= mask[idx % 4];
        }
    }
    // 安全：分片消息在 fragment_buf 中累积。单帧大小已受 8MB 上限约束，但需要额外
    // 限制整条消息的累积总量，防止对端用大量续帧持续累积导致内存耗尽。
    const WS_MAX_MESSAGE: usize = 64 * 1024 * 1024;
    if opcode == 0x0 && fragment_buf.len().saturating_add(payload.len()) > WS_MAX_MESSAGE {
        fragment_buf.clear();
        return Err("WebSocket 分片消息过大".into());
    }
    match opcode {
        0x1 if fin => {
            // 单帧完整文本消息
            fragment_buf.clear();
            Ok(Some(String::from_utf8_lossy(&payload).to_string()))
        }
        0x1 => {
            // B19: 首片，FIN=0，开始累积
            fragment_buf.clear();
            fragment_buf.extend_from_slice(&payload);
            Ok(Some(String::new())) // 还未完整，返回空表示继续
        }
        0x0 if fin => {
            // B19: 末片，拼完整消息
            fragment_buf.extend_from_slice(&payload);
            let complete = String::from_utf8_lossy(fragment_buf).to_string();
            fragment_buf.clear();
            Ok(Some(complete))
        }
        0x0 => {
            // B19: 中间片，继续累积
            fragment_buf.extend_from_slice(&payload);
            Ok(Some(String::new()))
        }
        0x8 => Ok(None),
        0x9 => {
            let _ = websocket_send_pong(stream, &payload);
            Ok(Some(String::new()))
        }
        0xA => Ok(Some(String::new())),
        _ => Ok(Some(String::new())),
    }
}

fn smart_control_preview(text: &str, limit: usize) -> String {
    let mut preview = text.chars().take(limit).collect::<String>();
    if text.chars().count() > limit {
        preview.push('…');
    }
    preview
}

fn smart_control_client_state_store() -> &'static Mutex<SmartControlClientState> {
    SMART_CONTROL_CLIENT_STATE.get_or_init(|| Mutex::new(SmartControlClientState::default()))
}

fn smart_control_chunk_store() -> &'static Mutex<HashMap<String, SmartControlChunkAssembly>> {
    SMART_CONTROL_SERVER_CHUNKS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn smart_control_turn_store() -> &'static Mutex<HashMap<u64, SmartControlTurnAccumulator>> {
    SMART_CONTROL_TURN_STREAMS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn smart_control_approval_store() -> &'static Mutex<Vec<SmartControlApprovalRequest>> {
    SMART_CONTROL_APPROVALS.get_or_init(|| Mutex::new(Vec::new()))
}

fn smart_control_client_snapshot() -> SmartControlClientState {
    smart_control_client_state_store()
        .lock()
        .map(|state| state.clone())
        .unwrap_or_default()
}

fn smart_control_reset_client_state() {
    if let Ok(mut state) = smart_control_client_state_store().lock() {
        *state = SmartControlClientState::default();
    }
    if let Ok(mut chunks) = smart_control_chunk_store().lock() {
        chunks.clear();
    }
    if let Ok(mut turns) = smart_control_turn_store().lock() {
        turns.clear();
    }
}

fn smart_control_next_envelope_parts() -> (String, String, u64, Option<String>) {
    let mut state = smart_control_client_state_store()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if state.client_id.trim().is_empty() {
        state.client_id = format!("varswitch-client-{}", uuid::Uuid::new_v4());
    }
    if state.stream_id.trim().is_empty() {
        state.stream_id = format!("varswitch-stream-{}", uuid::Uuid::new_v4());
    }
    let seq_id = state.next_seq_id;
    state.next_seq_id = state.next_seq_id.saturating_add(1);
    let cursor = if state.cursor.trim().is_empty() {
        None
    } else {
        Some(state.cursor.clone())
    };
    (
        state.client_id.clone(),
        state.stream_id.clone(),
        seq_id,
        cursor,
    )
}

fn smart_control_mark_initialized(request_id: u64, result: &serde_json::Value) {
    if let Ok(mut state) = smart_control_client_state_store().lock() {
        if state.last_initialize_id == request_id || state.last_initialize_id == 0 {
            state.initialized = true;
        }
        if let Some(cursor) = result
            .get("cursor")
            .or_else(|| result.get("subscribeCursor"))
            .and_then(|v| v.as_str())
        {
            state.cursor = cursor.to_string();
        }
    }
}

fn smart_control_set_stream_identity(client_id: &str, stream_id: &str) {
    if let Ok(mut state) = smart_control_client_state_store().lock() {
        if !client_id.trim().is_empty() {
            state.client_id = client_id.trim().to_string();
        }
        if !stream_id.trim().is_empty() {
            state.stream_id = stream_id.trim().to_string();
        }
    }
}

fn smart_control_update_pong_status(status: &str) {
    if let Ok(mut state) = smart_control_client_state_store().lock() {
        state.last_pong_status = status.to_string();
        if status.eq_ignore_ascii_case("unknown") || status.contains("unknown") {
            state.initialized = false;
        }
    }
}

fn smart_control_envelope_message(value: &serde_json::Value) -> Option<&serde_json::Value> {
    value.get("message").or_else(|| {
        if value.get("type").and_then(|v| v.as_str()) == Some("server_message")
            || value.get("type").and_then(|v| v.as_str()) == Some("client_message")
        {
            value.get("message")
        } else {
            None
        }
    })
}

fn smart_control_is_server_envelope(value: &serde_json::Value) -> bool {
    matches!(
        value.get("type").and_then(|v| v.as_str()),
        Some("server_message") | Some("server_message_chunk") | Some("ack") | Some("pong")
    ) && value.get("client_id").is_some()
}

fn smart_control_build_client_envelope(
    client_id: &str,
    stream_id: &str,
    seq_id: u64,
    message: serde_json::Value,
    cursor: Option<&str>,
) -> serde_json::Value {
    let mut envelope = serde_json::json!({
        "type": "client_message",
        "client_id": client_id,
        "stream_id": stream_id,
        "seq_id": seq_id,
        "message": message,
    });
    if let Some(cursor) = cursor.filter(|value| !value.trim().is_empty()) {
        envelope["cursor"] = serde_json::json!(cursor);
    }
    envelope
}

fn smart_control_build_ack_envelope(
    client_id: &str,
    stream_id: &str,
    seq_id: u64,
    segment_id: Option<u64>,
) -> serde_json::Value {
    let mut envelope = serde_json::json!({
        "type": "ack",
        "client_id": client_id,
        "stream_id": stream_id,
        "seq_id": seq_id,
    });
    if let Some(segment_id) = segment_id {
        envelope["segment_id"] = serde_json::json!(segment_id);
    }
    envelope
}

fn smart_control_build_ping_envelope() -> serde_json::Value {
    let state = smart_control_client_snapshot();
    let mut envelope = serde_json::json!({
        "type": "ping",
        "client_id": state.client_id,
        "stream_id": state.stream_id,
    });
    if !state.cursor.trim().is_empty() {
        envelope["cursor"] = serde_json::json!(state.cursor);
    }
    envelope
}

fn smart_control_reassemble_server_chunk(value: &serde_json::Value) -> Option<serde_json::Value> {
    let client_id = value
        .get("client_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let stream_id = value
        .get("stream_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let seq_id = value.get("seq_id").and_then(|v| v.as_u64()).unwrap_or(0);
    let segment_id = value
        .get("segment_id")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let segment_count = value
        .get("segment_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let message_size_bytes = value
        .get("message_size_bytes")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let chunk_b64 = value
        .get("message_chunk_base64")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if client_id.is_empty()
        || stream_id.is_empty()
        || seq_id == 0
        || segment_count == 0
        || segment_id >= segment_count
        || message_size_bytes == 0
        || message_size_bytes > 100 * 1024 * 1024
        || segment_count > 1024
        || chunk_b64.is_empty()
    {
        return None;
    }
    let chunk = base64::engine::general_purpose::STANDARD
        .decode(chunk_b64)
        .ok()?;
    let key = format!("{client_id}:{stream_id}:{seq_id}");
    let mut chunks = smart_control_chunk_store().lock().ok()?;
    let assembly = chunks
        .entry(key.clone())
        .or_insert_with(|| SmartControlChunkAssembly {
            segment_count,
            message_size_bytes,
            raw: Vec::new(),
            next_segment_id: 0,
        });
    if assembly.segment_count != segment_count
        || assembly.message_size_bytes != message_size_bytes
        || assembly.next_segment_id != segment_id
        || assembly.raw.len().saturating_add(chunk.len()) > message_size_bytes
    {
        chunks.remove(&key);
        return None;
    }
    assembly.raw.extend_from_slice(&chunk);
    assembly.next_segment_id = assembly.next_segment_id.saturating_add(1);
    if assembly.next_segment_id < assembly.segment_count {
        return None;
    }
    let assembly = chunks.remove(&key)?;
    if assembly.raw.len() != assembly.message_size_bytes {
        return None;
    }
    serde_json::from_slice::<serde_json::Value>(&assembly.raw).ok()
}

fn json_u64(value: &serde_json::Value, keys: &[&str]) -> Option<u64> {
    for key in keys {
        if let Some(number) = value.get(*key).and_then(|item| item.as_u64()) {
            return Some(number);
        }
        if let Some(text) = value.get(*key).and_then(|item| item.as_str()) {
            if let Ok(number) = text.trim().parse::<u64>() {
                return Some(number);
            }
        }
    }
    None
}

fn smart_control_extract_text(value: &serde_json::Value) -> String {
    if let Some(message) = smart_control_envelope_message(value) {
        let nested = smart_control_extract_text(message);
        if !nested.trim().is_empty() {
            return nested;
        }
    }
    for path in [
        &["result", "text"][..],
        &["result", "message"][..],
        &["result", "outputText"][..],
        &["result", "response", "text"][..],
        &["response", "text"][..],
        &["response", "message"][..],
        &["text"][..],
        &["message"][..],
        &["delta", "text"][..],
        &["delta", "content"][..],
        &["item", "text"][..],
        &["item", "content", "text"][..],
        &["params", "text"][..],
        &["params", "delta", "text"][..],
        &["params", "item", "text"][..],
    ] {
        let mut current = value;
        let mut found = true;
        for key in path {
            if let Some(next) = current.get(*key) {
                current = next;
            } else {
                found = false;
                break;
            }
        }
        if found {
            if let Some(text) = current.as_str() {
                if !text.trim().is_empty() {
                    return text.trim().to_string();
                }
            }
        }
    }
    String::new()
}

fn smart_control_extract_error(value: &serde_json::Value) -> String {
    if let Some(message) = smart_control_envelope_message(value) {
        let nested = smart_control_extract_error(message);
        if !nested.trim().is_empty() {
            return nested;
        }
    }
    for path in [
        &["error", "message"][..],
        &["error"][..],
        &["result", "error"][..],
        &["response", "error"][..],
    ] {
        let mut current = value;
        let mut found = true;
        for key in path {
            if let Some(next) = current.get(*key) {
                current = next;
            } else {
                found = false;
                break;
            }
        }
        if found {
            if let Some(text) = current.as_str() {
                if !text.trim().is_empty() {
                    return text.trim().to_string();
                }
            }
            if current.is_object() || current.is_array() {
                return current.to_string();
            }
        }
    }
    String::new()
}

fn smart_control_extract_delta_text_lossless(value: &serde_json::Value) -> String {
    for path in [
        &["delta", "text"][..],
        &["delta", "content"][..],
        &["params", "delta", "text"][..],
        &["item", "text"][..],
        &["params", "item", "text"][..],
    ] {
        let mut current = value;
        let mut found = true;
        for key in path {
            if let Some(next) = current.get(*key) {
                current = next;
            } else {
                found = false;
                break;
            }
        }
        if found {
            if let Some(text) = current.as_str() {
                if !text.is_empty() {
                    return text.to_string();
                }
            }
        }
    }
    String::new()
}

fn smart_control_message_id(value: &serde_json::Value) -> Option<u64> {
    json_u64(value, &["id", "requestId", "responseTo"]).or_else(|| {
        smart_control_envelope_message(value)
            .and_then(|message| json_u64(message, &["id", "requestId", "responseTo"]))
    })
}

fn smart_control_message_method(value: &serde_json::Value) -> String {
    let direct = json_string(value, &["method", "name", "op"]);
    if !direct.is_empty() {
        return direct;
    }
    smart_control_envelope_message(value)
        .map(|message| json_string(message, &["method", "name", "op"]))
        .unwrap_or_default()
}

fn smart_control_is_turn_done(value: &serde_json::Value) -> bool {
    let method = smart_control_message_method(value);
    let status = json_string(value, &["status", "state"]);
    let event_type = json_string(value, &["type", "event", "kind"]);
    method.contains("turn/complete")
        || method.contains("turn/done")
        || method.contains("turn/finished")
        || status.eq_ignore_ascii_case("completed")
        || status.eq_ignore_ascii_case("done")
        || status.eq_ignore_ascii_case("failed")
        || event_type.contains("completed")
        || event_type.contains("done")
}

fn smart_control_observe_turn_item(value: &serde_json::Value) {
    let Some(id) = smart_control_message_id(value) else {
        return;
    };
    let lossless_delta = smart_control_extract_delta_text_lossless(value);
    let text = if lossless_delta.is_empty() {
        smart_control_extract_text(value)
    } else {
        lossless_delta
    };
    let error = smart_control_extract_error(value);
    let done = smart_control_is_turn_done(value)
        || value
            .get("result")
            .is_some_and(|result| result.is_object() || result.is_string());
    if text.is_empty() && error.is_empty() && !done {
        return;
    }
    let mut turns = match smart_control_turn_store().lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };
    let acc = turns.entry(id).or_default();
    if !text.is_empty() {
        if done && value.get("result").is_some() && value.get("delta").is_none() {
            acc.final_text = text;
        } else {
            acc.text_parts.push(text);
        }
    }
    if !error.is_empty() {
        acc.error = error;
    }
    if done || !acc.error.is_empty() {
        acc.done = true;
        let final_text = if !acc.final_text.trim().is_empty() {
            acc.final_text.trim().to_string()
        } else {
            acc.text_parts.join("").trim().to_string()
        };
        smart_control_complete_pending(id, final_text, acc.error.clone());
        turns.remove(&id);
    }
}

fn smart_control_extract_approval(
    value: &serde_json::Value,
) -> Option<SmartControlApprovalRequest> {
    let message = smart_control_envelope_message(value).unwrap_or(value);
    let method = smart_control_message_method(message);
    let looks_like_approval = method.contains("approval")
        || method.contains("confirm")
        || message.get("approval").is_some()
        || message.get("decisions").is_some()
        || message.get("options").is_some_and(|v| v.is_array());
    if !looks_like_approval {
        return None;
    }
    let request_id = message
        .get("id")
        .map(|v| v.to_string().trim_matches('"').to_string())
        .or_else(|| {
            value
                .get("seq_id")
                .map(|v| v.to_string().trim_matches('"').to_string())
        })
        .unwrap_or_default();
    let params = message.get("params").unwrap_or(message);
    let title = json_string(params, &["title", "name", "command", "action"]);
    let body = [
        json_string(params, &["body", "message", "description", "reason"]),
        params
            .get("approval")
            .map(|v| v.to_string())
            .unwrap_or_default(),
    ]
    .into_iter()
    .find(|v| !v.trim().is_empty())
    .unwrap_or_else(|| smart_control_preview(&message.to_string(), 360));
    let mut options = Vec::new();
    for key in ["options", "decisions", "choices"] {
        if let Some(items) = params.get(key).and_then(|v| v.as_array()) {
            for item in items {
                if let Some(text) = item.as_str() {
                    options.push(text.to_string());
                } else {
                    let label = json_string(item, &["label", "title", "decision", "value"]);
                    if !label.is_empty() {
                        options.push(label);
                    }
                }
            }
        }
    }
    if options.is_empty() {
        options = vec!["approve".into(), "deny".into()];
    }
    Some(SmartControlApprovalRequest {
        request_id,
        method,
        title: if title.is_empty() {
            "需要审批".into()
        } else {
            title
        },
        body,
        options,
        received_at: chrono_now(),
        raw_preview: smart_control_preview(&message.to_string(), 800),
    })
}

fn smart_control_remember_approval(value: &serde_json::Value) {
    let Some(approval) = smart_control_extract_approval(value) else {
        return;
    };
    if let Ok(mut approvals) = smart_control_approval_store().lock() {
        approvals.push(approval);
        if approvals.len() > 20 {
            let drain_count = approvals.len().saturating_sub(20);
            approvals.drain(0..drain_count);
        }
    }
}

fn smart_control_maybe_complete_pending(value: &serde_json::Value) {
    let Some(id) = smart_control_message_id(value) else {
        return;
    };
    let method = smart_control_message_method(value);
    if method == "initialize" {
        if let Some(result) = value.get("result") {
            smart_control_mark_initialized(id, result);
        }
    }
    smart_control_observe_turn_item(value);
    if !smart_control_extract_delta_text_lossless(value).is_empty()
        && value.get("result").is_none()
        && value.get("response").is_none()
    {
        return;
    }
    let text = smart_control_extract_text(value);
    let error = smart_control_extract_error(value);
    if !text.is_empty() || !error.is_empty() {
        smart_control_complete_pending(id, text, error);
    }
}

fn remember_smart_control_event(text: &str) {
    let parsed = serde_json::from_str::<serde_json::Value>(text).ok();
    let event_type = parsed
        .as_ref()
        .map(|value| json_string(value, &["type", "event", "kind", "messageType"]))
        .unwrap_or_default();
    let message_id = parsed
        .as_ref()
        .map(|value| json_string(value, &["id", "messageId", "requestId"]))
        .unwrap_or_default();
    let method = parsed
        .as_ref()
        .map(|value| json_string(value, &["method", "name", "op"]))
        .unwrap_or_default();
    let event = SmartControlEvent {
        received_at: chrono_now(),
        event_type,
        message_id,
        method,
        raw_preview: smart_control_preview(text, 800),
    };
    push_smart_control_event(event);
    if let Some(value) = parsed.as_ref() {
        smart_control_handle_inbound_value(value);
    }
}

fn smart_control_handle_inbound_value(value: &serde_json::Value) {
    if smart_control_is_server_envelope(value) {
        let client_id = value
            .get("client_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let stream_id = value
            .get("stream_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let seq_id = value.get("seq_id").and_then(|v| v.as_u64()).unwrap_or(0);
        smart_control_set_stream_identity(client_id, stream_id);
        match value.get("type").and_then(|v| v.as_str()) {
            Some("pong") => {
                smart_control_update_pong_status(&json_string(value, &["status"]));
                let _ = smart_control_send_json(&smart_control_build_ack_envelope(
                    client_id, stream_id, seq_id, None,
                ));
            }
            Some("ack") => {}
            Some("server_message") => {
                let _ = smart_control_send_json(&smart_control_build_ack_envelope(
                    client_id, stream_id, seq_id, None,
                ));
                if let Some(message) = value.get("message") {
                    smart_control_remember_approval(message);
                    smart_control_maybe_complete_pending(message);
                }
            }
            Some("server_message_chunk") => {
                let segment_id = value.get("segment_id").and_then(|v| v.as_u64());
                let _ = smart_control_send_json(&smart_control_build_ack_envelope(
                    client_id, stream_id, seq_id, segment_id,
                ));
                if let Some(message) = smart_control_reassemble_server_chunk(value) {
                    smart_control_remember_approval(&message);
                    smart_control_maybe_complete_pending(&message);
                }
            }
            _ => {}
        }
        return;
    }
    smart_control_remember_approval(value);
    smart_control_maybe_complete_pending(value);
}

fn smart_control_last_event() -> Option<SmartControlEvent> {
    SMART_CONTROL_LAST_EVENT
        .lock()
        .ok()
        .and_then(|guard| guard.clone())
}

fn smart_control_event_log() -> &'static Mutex<Vec<SmartControlEvent>> {
    SMART_CONTROL_EVENT_LOG.get_or_init(|| Mutex::new(Vec::new()))
}

fn push_smart_control_event(event: SmartControlEvent) {
    if let Ok(mut last) = SMART_CONTROL_LAST_EVENT.lock() {
        *last = Some(event.clone());
    }
    if let Ok(mut events) = smart_control_event_log().lock() {
        events.push(event);
        if events.len() > 80 {
            let drain_count = events.len().saturating_sub(80);
            events.drain(0..drain_count);
        }
    }
}

fn smart_control_debug_snapshot() -> SmartControlDebugSnapshot {
    let pending_count = smart_control_pending_store()
        .0
        .lock()
        .map(|pending| pending.len())
        .unwrap_or(0);
    let events = smart_control_event_log()
        .lock()
        .map(|events| events.clone())
        .unwrap_or_default();
    let approvals = smart_control_approval_store()
        .lock()
        .map(|items| items.clone())
        .unwrap_or_default();
    SmartControlDebugSnapshot {
        connected: SMART_CONTROL_REMOTE_CONNECTED.load(Ordering::SeqCst),
        pending_count,
        last_event: smart_control_last_event(),
        events,
        client: smart_control_client_snapshot(),
        approvals,
    }
}

fn next_smart_control_request_id() -> u64 {
    let lock = SMART_CONTROL_NEXT_REQUEST_ID.get_or_init(|| Mutex::new(1));
    let mut guard = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let id = *guard;
    *guard = guard.saturating_add(1);
    id
}

fn smart_control_pending_store(
) -> &'static (Mutex<HashMap<u64, SmartControlPendingResult>>, Condvar) {
    SMART_CONTROL_PENDING.get_or_init(|| (Mutex::new(HashMap::new()), Condvar::new()))
}

fn smart_control_register_pending(id: u64) {
    let (lock, _) = smart_control_pending_store();
    if let Ok(mut guard) = lock.lock() {
        guard.insert(id, SmartControlPendingResult::default());
    }
}

fn smart_control_complete_pending(id: u64, text: String, error: String) {
    let (lock, condvar) = smart_control_pending_store();
    if let Ok(mut guard) = lock.lock() {
        guard.insert(
            id,
            SmartControlPendingResult {
                done: true,
                text,
                error,
            },
        );
        condvar.notify_all();
    }
}

fn smart_control_wait_pending(id: u64, timeout: Duration) -> Option<SmartControlPendingResult> {
    let (lock, condvar) = smart_control_pending_store();
    let guard = lock.lock().ok()?;
    let (mut guard, _) = condvar
        .wait_timeout_while(guard, timeout, |pending| {
            pending.get(&id).map(|result| !result.done).unwrap_or(true)
        })
        .ok()?;
    guard.remove(&id).filter(|result| result.done)
}

fn smart_control_initialize_message(id: u64) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "method": "initialize",
        "params": {
            "clientInfo": {
                "name": "VarSwitch",
                "version": env!("CARGO_PKG_VERSION")
            },
            "capabilities": {
                "threads": true,
                "turns": true,
                "streaming": true,
                "approvals": true
            }
        }
    })
}

fn smart_control_turn_start_message(id: u64, thread_id: &str, text: &str) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "method": "turn/start",
        "params": {
            "threadId": thread_id.trim(),
            "input": [
                {
                    "type": "text",
                    "text": text.trim()
                }
            ]
        }
    })
}

fn smart_control_wrap_client_message(message: serde_json::Value) -> serde_json::Value {
    let (client_id, stream_id, seq_id, cursor) = smart_control_next_envelope_parts();
    smart_control_build_client_envelope(&client_id, &stream_id, seq_id, message, cursor.as_deref())
}

fn smart_control_send_json(value: &serde_json::Value) -> Result<(), String> {
    let text = serde_json::to_string(value).map_err(|e| format!("序列化协议消息失败: {e}"))?;
    let mut guard = SMART_CONTROL_WS_WRITER
        .lock()
        .map_err(|_| "高级控制通道写入锁已损坏".to_string())?;
    let Some(stream) = guard.as_mut() else {
        return Err("高级控制通道未连接".into());
    };
    websocket_send_text(stream, &text)?;
    push_smart_control_event(SmartControlEvent {
        received_at: chrono_now(),
        event_type: "outbound".into(),
        message_id: value
            .get("id")
            .or_else(|| value.get("seq_id"))
            .map(|id| id.to_string())
            .unwrap_or_default(),
        method: smart_control_message_method(value),
        raw_preview: smart_control_preview(&text, 800),
    });
    Ok(())
}

fn smart_control_ensure_initialized() -> Result<(), String> {
    if smart_control_client_snapshot().initialized {
        return Ok(());
    }
    let request_id = next_smart_control_request_id();
    {
        if let Ok(mut state) = smart_control_client_state_store().lock() {
            state.last_initialize_id = request_id;
        }
    }
    smart_control_register_pending(request_id);
    let message = smart_control_initialize_message(request_id);
    let envelope = smart_control_wrap_client_message(message);
    smart_control_send_json(&envelope)?;
    match smart_control_wait_pending(request_id, Duration::from_secs(30)) {
        Some(result) if !result.error.trim().is_empty() => Err(result.error),
        Some(_) => {
            if let Ok(mut state) = smart_control_client_state_store().lock() {
                state.initialized = true;
            }
            Ok(())
        }
        None => Err("高级控制 initialize 超时".into()),
    }
}

fn try_smart_control_dispatch(thread_id: &str, text: &str) -> Result<Option<String>, String> {
    if !SMART_CONTROL_REMOTE_CONNECTED.load(Ordering::SeqCst) {
        return Ok(None);
    }
    smart_control_ensure_initialized()?;
    let request_id = next_smart_control_request_id();
    smart_control_register_pending(request_id);
    let message = smart_control_turn_start_message(request_id, thread_id, text);
    let envelope = smart_control_wrap_client_message(message);
    smart_control_send_json(&envelope)?;
    log_info!(
        "[smart-control][dispatch] sent turn/start over protocol channel preview={}",
        smart_control_preview(text, 240)
    );
    match smart_control_wait_pending(request_id, Duration::from_secs(180)) {
        Some(result) if !result.error.trim().is_empty() => Err(result.error),
        Some(result) if !result.text.trim().is_empty() => Ok(Some(result.text)),
        Some(_) => Ok(Some("高级控制通道已完成，但没有返回可显示文本。".into())),
        None => Err("高级控制通道等待 Codex 回复超时".into()),
    }
}

fn smart_control_json_response(value: serde_json::Value) -> String {
    let body = serde_json::to_string(&value).unwrap_or_else(|_| "{}".into());
    smart_control_http_response("200 OK", "application/json", &body)
}

fn smart_control_not_found(path: &str) -> String {
    smart_control_json_response(serde_json::json!({
        "ok": false,
        "error": "not_found",
        "path": path,
    }))
    .replacen("HTTP/1.1 200 OK", "HTTP/1.1 404 Not Found", 1)
}

/// 校验 HTTP Host 头是否指向本机回环地址。
/// 用于 DNS 重绑定防护：服务虽只绑定 127.0.0.1，但恶意网页可把某个域名
/// 重绑定到 127.0.0.1 后访问本地服务，因此需要额外校验 Host 头。
fn smart_control_host_is_local(host_header: &str) -> bool {
    let host = host_header.trim();
    if host.is_empty() {
        return false;
    }
    // 去掉端口；IPv6 形如 [::1]:3847
    let hostname = if let Some(rest) = host.strip_prefix('[') {
        rest.split(']').next().unwrap_or("")
    } else {
        host.split(':').next().unwrap_or("")
    };
    matches!(hostname, "127.0.0.1" | "localhost" | "::1")
}

fn read_http_request_head(stream: &mut TcpStream) -> Result<String, String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .map_err(|e| format!("设置读取超时失败: {e}"))?;
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    while buf.len() < 64 * 1024 {
        let n = stream
            .read(&mut byte)
            .map_err(|e| format!("读取 HTTP 请求失败: {e}"))?;
        if n == 0 {
            break;
        }
        buf.push(byte[0]);
        if buf.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    Ok(String::from_utf8_lossy(&buf).to_string())
}

fn handle_smart_control_http_connection(mut stream: TcpStream) -> Result<(), String> {
    let head = read_http_request_head(&mut stream)?;
    let request_line = head.lines().next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let raw_path = parts.next().unwrap_or("/");
    let path = raw_path.split('?').next().unwrap_or(raw_path);
    // 安全：DNS 重绑定防护。仅接受指向本机回环的 Host，阻止恶意网页
    // 通过把域名解析到 127.0.0.1 来访问本地无鉴权控制服务。
    if !smart_control_host_is_local(http_header_value(&head, "host").unwrap_or("")) {
        let response = smart_control_http_response(
            "403 Forbidden",
            "application/json",
            "{\"ok\":false,\"error\":\"forbidden_host\"}",
        );
        let _ = stream.write_all(response.as_bytes());
        return Ok(());
    }
    if method == "GET"
        && path == "/backend-api/wham/remote/control/server"
        && is_websocket_upgrade(&head)
    {
        let key = http_header_value(&head, "sec-websocket-key")
            .ok_or("WebSocket 缺少 Sec-WebSocket-Key")?;
        let response = websocket_upgrade_response(key);
        stream
            .write_all(response.as_bytes())
            .map_err(|e| format!("写入 WebSocket 握手响应失败: {e}"))?;
        SMART_CONTROL_REMOTE_CONNECTED.store(true, Ordering::SeqCst);
        smart_control_reset_client_state();
        // B6: 每次新连接分配唯一代际 id，退出时校验后再清全局状态，避免新连接被旧连接收尾代码打翻
        let my_gen = SMART_CONTROL_WS_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
        if let Ok(writer) = stream.try_clone() {
            if let Ok(mut guard) = SMART_CONTROL_WS_WRITER.lock() {
                *guard = Some(writer);
            }
        }
        set_smart_control_status_cache(SmartControlStatus {
            available: true,
            connected: true,
            backend_url: default_smart_control_backend_url().into(),
            status: "高级控制通道已连接".into(),
            detail: "Codex 已建立 remote-control WebSocket，等待协议事件。".into(),
            checked_at: chrono_now(),
        });
        // B6: 每 30 秒超时后发 ping，连续 3 次无应答（90 秒空闲）才判定断线
        stream.set_read_timeout(Some(Duration::from_secs(30))).ok();
        let _ = smart_control_send_json(&smart_control_build_ping_envelope());
        let mut consecutive_timeouts: u32 = 0;
        // B19: 分片帧累积缓冲区
        let mut fragment_buf: Vec<u8> = Vec::new();
        loop {
            match websocket_read_text_frame(&mut stream, &mut fragment_buf) {
                Ok(Some(text)) if text.trim().is_empty() => {
                    // pong 或空帧 → 重置超时计数
                    consecutive_timeouts = 0;
                    continue;
                }
                Ok(Some(text)) => {
                    consecutive_timeouts = 0;
                    log_info!(
                        "[smart-control][ws] received frame: {}",
                        smart_control_preview(&text, 240)
                    );
                    remember_smart_control_event(&text);
                }
                Ok(None) => break, // 正常关闭（EOF / ConnectionReset）
                Err(e) if e == "__timeout__" => {
                    // B6: 超时不等于断线，发 ping 继续等；连续 3 次（90 秒）无响应才断
                    consecutive_timeouts += 1;
                    if consecutive_timeouts >= 3 {
                        log_info!("[smart-control][ws] 持续 90s 无活动，主动关闭连接");
                        break;
                    }
                    let _ = smart_control_send_json(&smart_control_build_ping_envelope());
                }
                Err(error) => {
                    log_warn!("[smart-control][ws] frame read failed: {}", error);
                    break;
                }
            }
        }
        // B6: 仅在代际匹配时才清全局状态，避免新连接状态被旧连接收尾代码覆盖
        let current_gen = SMART_CONTROL_WS_GENERATION.load(Ordering::SeqCst);
        if current_gen == my_gen {
            SMART_CONTROL_REMOTE_CONNECTED.store(false, Ordering::SeqCst);
            if let Ok(mut guard) = SMART_CONTROL_WS_WRITER.lock() {
                *guard = None;
            }
        }
        return Ok(());
    }
    let response = match (method, path) {
        ("OPTIONS", _) => smart_control_http_response("204 No Content", "text/plain", ""),
        ("GET", "/api/status") => smart_control_json_response(serde_json::json!({
            "ok": true,
            "service": "varswitch-control",
            "running": true,
            "time": chrono_now(),
        })),
        ("GET", "/api/remote-control/status") => smart_control_json_response(serde_json::json!({
            "ok": true,
            "available": true,
            "connected": SMART_CONTROL_REMOTE_CONNECTED.load(Ordering::SeqCst),
            "ready": SMART_CONTROL_REMOTE_CONNECTED.load(Ordering::SeqCst) && smart_control_client_snapshot().initialized,
            "service": "varswitch-control",
            "lastEvent": smart_control_last_event(),
            "client": smart_control_client_snapshot(),
            "approvals": smart_control_approval_store().lock().map(|items| items.clone()).unwrap_or_default(),
            "message": if SMART_CONTROL_REMOTE_CONNECTED.load(Ordering::SeqCst) {
                "Codex 已连接 VarSwitch 本机控制服务。"
            } else {
                "VarSwitch 本机控制服务已启动，等待 Codex remote-control 连接。"
            },
            "time": chrono_now(),
        })),
        ("POST", "/backend-api/wham/remote/control/server/enroll") => {
            smart_control_json_response(serde_json::json!({
                "ok": true,
                "server": {
                    "id": "varswitch-local-control",
                    "name": "VarSwitch Local Control"
                },
                "message": "VarSwitch 已收到 Codex remote-control enroll 请求，完整双向协议将在控制通道建立后接管。"
            }))
        }
        ("GET", "/backend-api/wham/remote/control/server") => {
            smart_control_json_response(serde_json::json!({
                "ok": true,
                "connected": false,
                "message": "VarSwitch 本机控制服务已就绪，当前轻量接口用于状态探测。"
            }))
        }
        _ => smart_control_not_found(path),
    };
    stream
        .write_all(response.as_bytes())
        .map_err(|e| format!("写入 HTTP 响应失败: {e}"))?;
    Ok(())
}

fn start_smart_control_server(app: tauri::AppHandle, backend_url: String) {
    if SMART_CONTROL_SERVER_ACTIVE.swap(true, Ordering::SeqCst) {
        return;
    }
    SMART_CONTROL_SERVER_CANCEL_REQUESTED.store(false, Ordering::SeqCst);
    std::thread::spawn(move || {
        let bind_addr = smart_control_bind_addr_from_url(&backend_url);
        log_info!("[smart-control] starting local service at {}", bind_addr);
        let listener = match TcpListener::bind(&bind_addr) {
            Ok(listener) => listener,
            Err(error) => {
                log_warn!("[smart-control] bind failed at {}: {}", bind_addr, error);
                SMART_CONTROL_SERVER_ACTIVE.store(false, Ordering::SeqCst);
                let mut state = read_toolbox_state(&app);
                state.mobile_remote.remote_control_connected = false;
                state.mobile_remote.remote_control_status = "高级控制服务启动失败".into();
                state.mobile_remote.remote_control_detail =
                    format!("监听 {bind_addr} 失败：{error}");
                let _ = write_toolbox_state(&app, &state);
                return;
            }
        };
        if let Err(error) = listener.set_nonblocking(true) {
            log_warn!("[smart-control] set_nonblocking failed: {}", error);
        }
        {
            let mut state = read_toolbox_state(&app);
            state.mobile_remote.remote_control_backend_url =
                normalize_smart_control_backend_url(&backend_url);
            state.mobile_remote.remote_control_status = "高级控制服务已启动".into();
            state.mobile_remote.remote_control_detail =
                format!("正在监听 {bind_addr}，等待 Codex 连接。");
            state.mobile_remote.remote_control_connected = false;
            let _ = write_toolbox_state(&app, &state);
        }
        while !SMART_CONTROL_SERVER_CANCEL_REQUESTED.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((stream, _addr)) => {
                    // nonblocking listener 接受的 socket 在 Windows 上也会保持非阻塞；
                    // 请求刚到达时直接 read 会返回 WSAEWOULDBLOCK (10035)。
                    if let Err(error) = stream.set_nonblocking(false) {
                        log_warn!(
                            "[smart-control] set accepted socket blocking failed: {}",
                            error
                        );
                        continue;
                    }
                    std::thread::spawn(move || {
                        if let Err(error) = handle_smart_control_http_connection(stream) {
                            log_warn!("[smart-control] request failed: {}", error);
                        }
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(80));
                }
                Err(error) => {
                    log_warn!("[smart-control] accept failed: {}", error);
                    std::thread::sleep(Duration::from_millis(300));
                }
            }
        }
        log_info!("[smart-control] local service stopped");
        SMART_CONTROL_SERVER_ACTIVE.store(false, Ordering::SeqCst);
    });
}

fn stop_smart_control_server() {
    SMART_CONTROL_SERVER_CANCEL_REQUESTED.store(true, Ordering::SeqCst);
}

fn smart_control_backend_api_url() -> String {
    format!("{}/backend-api", default_smart_control_backend_url())
}

#[cfg(test)]
fn codex_config_toml_content(
    provider: &str,
    model: &str,
    base_url: &str,
    api_key: &str,
    official_account_mode: bool,
) -> String {
    codex_config_toml_content_with_image(
        provider,
        model,
        base_url,
        api_key,
        "responses",
        official_account_mode,
        "",
        "",
    )
}

fn codex_image_skill_dir() -> PathBuf {
    codex_config_dir().join("skills").join(CODEX_IMAGE_SKILL_ID)
}

fn codex_image_skill_script_path() -> PathBuf {
    codex_image_skill_dir()
        .join("scripts")
        .join("generate-image.ps1")
}

fn codex_image_skill_manifest(script_path: &Path) -> String {
    format!(
        r#"---
name: varswitch-imagegen
description: Use when the user asks to create, generate, or render an image and VarSwitch image settings are available. This is the preferred image-generation Skill over the built-in imagegen path while configured.
---

# VarSwitch Image Generation

Use the configured image endpoint through the bundled script as the first choice for image generation. Keep the built-in `imagegen` as fallback only when this Skill is missing, not configured, or fails, or when the user explicitly requests the built-in path. Never print or copy image API credentials into chat, commands, logs, or generated files.

Run:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "{script_path}" -Prompt "<prompt>" -OutputPath "<absolute-output-path.png>" -Size "1024x1024"
```

- Use an absolute output path inside the current workspace unless the user requests another location.
- Supported default sizes are `1024x1024`, `1536x1024`, and `1024x1536`; pass through another size only when the configured provider supports it.
- After the script succeeds, inspect the saved image with the available image viewing tool before handing it off.
- If configuration is missing, tell the user to configure and enable the Codex Image Skill in VarSwitch, then restart Codex.
"#,
        script_path = script_path.display()
    )
}

fn codex_image_skill_script() -> &'static str {
    r##"param(
  [Parameter(Mandatory = $true)][string]$Prompt,
  [Parameter(Mandatory = $true)][string]$OutputPath,
  [string]$Size = "1024x1024"
)

$ErrorActionPreference = "Stop"

function Get-VarSwitchImageSetting([string]$Name) {
  $value = [Environment]::GetEnvironmentVariable($Name, "Process")
  if ([string]::IsNullOrWhiteSpace($value)) {
    $value = [Environment]::GetEnvironmentVariable($Name, "User")
  }
  return $value
}

$apiKey = Get-VarSwitchImageSetting "VARSWITCH_IMAGE_API_KEY"
$baseUrl = Get-VarSwitchImageSetting "VARSWITCH_IMAGE_BASE_URL"
$model = Get-VarSwitchImageSetting "VARSWITCH_IMAGE_MODEL"

if ([string]::IsNullOrWhiteSpace($apiKey) -or [string]::IsNullOrWhiteSpace($baseUrl)) {
  throw "VarSwitch image generation is not configured. Enable a Codex profile with image settings and restart Codex."
}
if ([string]::IsNullOrWhiteSpace($model)) {
  $model = "gpt-image-2"
}

$endpoint = $baseUrl.TrimEnd('/') + "/images/generations"
$headers = @{ Authorization = "Bearer $apiKey" }
$payload = @{
  model = $model
  prompt = $Prompt
  size = $Size
} | ConvertTo-Json -Compress

$response = Invoke-RestMethod -Method Post -Uri $endpoint -Headers $headers -ContentType "application/json; charset=utf-8" -Body ([Text.Encoding]::UTF8.GetBytes($payload))
if (-not $response.data -or $response.data.Count -lt 1) {
  throw "The image API returned no image data."
}

$resolvedPath = [IO.Path]::GetFullPath($OutputPath)
$parent = Split-Path -Parent $resolvedPath
if ($parent -and -not (Test-Path -LiteralPath $parent)) {
  New-Item -ItemType Directory -Path $parent -Force | Out-Null
}

$image = $response.data[0]
if (-not [string]::IsNullOrWhiteSpace($image.b64_json)) {
  [IO.File]::WriteAllBytes($resolvedPath, [Convert]::FromBase64String($image.b64_json))
} elseif (-not [string]::IsNullOrWhiteSpace($image.url)) {
  Invoke-WebRequest -UseBasicParsing -Uri $image.url -OutFile $resolvedPath
} else {
  throw "The image API response contains neither b64_json nor url."
}

@{ success = $true; path = $resolvedPath; model = $model } | ConvertTo-Json -Compress
"##
}

fn install_codex_image_skill_at(skill_dir: &Path) -> Result<(), String> {
    let script_path = skill_dir.join("scripts").join("generate-image.ps1");
    let scripts_dir = script_path
        .parent()
        .ok_or("无法确定 Codex 图片 Skill 脚本目录")?;
    fs::create_dir_all(scripts_dir).map_err(|e| format!("创建 Codex 图片 Skill 目录失败: {e}"))?;
    fs::write(
        skill_dir.join("SKILL.md"),
        codex_image_skill_manifest(&script_path),
    )
    .map_err(|e| format!("写入 Codex 图片 Skill 说明失败: {e}"))?;
    fs::write(&script_path, codex_image_skill_script())
        .map_err(|e| format!("写入 Codex 图片生成脚本失败: {e}"))?;
    Ok(())
}

fn install_codex_image_skill() -> Result<(), String> {
    install_codex_image_skill_at(&codex_image_skill_dir())
}

fn remove_codex_image_skill() -> Result<(), String> {
    let dir = codex_image_skill_dir();
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|e| format!("移除 Codex 图片 Skill 失败: {e}"))?;
    }
    Ok(())
}

fn merge_codex_image_priority_instructions(existing: &str, enabled: bool) -> String {
    let mut output = existing.to_string();

    while let Some(start) = output.find(CODEX_IMAGE_PRIORITY_START) {
        let Some(relative_end) = output[start..].find(CODEX_IMAGE_PRIORITY_END) else {
            break;
        };
        let end = start + relative_end + CODEX_IMAGE_PRIORITY_END.len();
        let removal_start =
            if output.as_bytes().get(start.saturating_sub(2)..start) == Some(b"\r\n") {
                start - 2
            } else if start > 0 && output.as_bytes()[start - 1] == b'\n' {
                start - 1
            } else {
                start
            };
        output.replace_range(removal_start..end, "");
    }

    if !enabled {
        return output;
    }
    if output.is_empty() {
        return CODEX_IMAGE_PRIORITY_INSTRUCTIONS.to_string();
    }

    let newline = if existing.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    output.push_str(newline);
    if newline == "\r\n" {
        output.push_str(&CODEX_IMAGE_PRIORITY_INSTRUCTIONS.replace('\n', "\r\n"));
    } else {
        output.push_str(CODEX_IMAGE_PRIORITY_INSTRUCTIONS);
    }
    output
}

fn configure_codex_image_priority_instructions(enabled: bool) -> Result<(), String> {
    let path = codex_global_agents_path();
    if !enabled && !path.exists() {
        return Ok(());
    }

    let existing = fs::read_to_string(&path).unwrap_or_default();
    let updated = merge_codex_image_priority_instructions(&existing, enabled);
    if updated == existing {
        return Ok(());
    }

    fs::create_dir_all(codex_config_dir()).map_err(|e| format!("创建 Codex 配置目录失败: {e}"))?;
    fs::write(&path, updated).map_err(|e| format!("写入 Codex 全局指令失败: {e}"))
}

fn configure_codex_image_skill(profile: &CodexProfile) -> Result<(), String> {
    // 图片 Skill 只有在 Key 与 URL 都由用户明确填写时才启用。绝不回退到内置地址。
    if profile.image_api_key.trim().is_empty() || profile.image_base_url.trim().is_empty() {
        for name in [
            CODEX_IMAGE_API_KEY_ENV,
            CODEX_IMAGE_BASE_URL_ENV,
            CODEX_IMAGE_MODEL_ENV,
        ] {
            if reg_get_env_opt(name).is_some() {
                reg_delete_env(name)?;
            }
        }
        remove_codex_image_skill()?;
        configure_codex_image_priority_instructions(false)?;
        broadcast_env_change();
        return Ok(());
    }

    let base_url = profile
        .image_base_url
        .trim()
        .trim_end_matches('/')
        .to_string();
    reg_set_env(CODEX_IMAGE_API_KEY_ENV, profile.image_api_key.trim())?;
    reg_set_env(CODEX_IMAGE_BASE_URL_ENV, &base_url)?;
    reg_set_env(CODEX_IMAGE_MODEL_ENV, CODEX_IMAGE_MODEL)?;
    install_codex_image_skill()?;
    configure_codex_image_priority_instructions(true)?;
    broadcast_env_change();
    Ok(())
}

fn codex_config_toml_content_with_image(
    provider: &str,
    model: &str,
    base_url: &str,
    api_key: &str,
    wire_api: &str,
    official_account_mode: bool,
    _image_api_key: &str,
    _image_base_url: &str,
) -> String {
    if official_account_mode {
        let content = format!(
            r#"model_provider = "customer"
model = "gpt-5.5"
review_model = "gpt-5.5"
model_reasoning_effort = "xhigh"
disable_response_storage = true
preferred_auth_method = "apikey"
chatgpt_base_url = "{chatgpt_base_url}"

[model_providers.customer]
name = "customer"
wire_api = "responses"
requires_openai_auth = true
base_url = "{base_url}"
experimental_bearer_token = "{api_key}"
"#,
            base_url = base_url,
            api_key = api_key,
            chatgpt_base_url = smart_control_backend_api_url(),
        );
        content
    } else {
        let content = format!(
            r#"model_provider = "{provider}"
model = "{model}"
chatgpt_base_url = "{chatgpt_base_url}"

[model_providers.{provider}]
name = "{provider}"
base_url = "{base_url}"
wire_api = "{wire_api}"
requires_openai_auth = true
"#,
            provider = provider,
            model = model,
            base_url = base_url,
            wire_api = normalize_codex_wire_api(wire_api),
            chatgpt_base_url = smart_control_backend_api_url(),
        );
        content
    }
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
        codex_config_toml_content_with_image(
            &provider,
            &model,
            base_url,
            &profile.api_key,
            &profile.wire_api,
            true,
            &profile.image_api_key,
            &profile.image_base_url,
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
        write_private_file(&auth_path, &auth_str)
            .map_err(|e| format!("写入 codex auth.json 失败: {}", e))?;

        codex_config_toml_content_with_image(
            &provider,
            &model,
            base_url,
            &profile.api_key,
            &profile.wire_api,
            false,
            &profile.image_api_key,
            &profile.image_base_url,
        )
    };
    let final_toml = merge_codex_config_with_preserved_sections(&toml_content, &existing_config);
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

    let provider_name = toml_line_value(&config_str, "model_provider");
    let provider_base_url = if provider_name.trim().is_empty() {
        String::new()
    } else {
        toml_section_value(
            &config_str,
            &format!("model_providers.{}", provider_name.trim()),
            "base_url",
        )
    };
    let base_url = if !provider_base_url.trim().is_empty() {
        provider_base_url
    } else {
        config_str
            .lines()
            .find(|l| l.trim().starts_with("base_url"))
            .and_then(|l| l.split('=').nth(1))
            .map(|v| v.trim().trim_matches('"').to_string())
            .unwrap_or_default()
    };

    let image_api_key = reg_get_env_opt(CODEX_IMAGE_API_KEY_ENV)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| toml_section_value(&config_str, "gpt_image_2", "api_key"));
    let image_base_url = reg_get_env_opt(CODEX_IMAGE_BASE_URL_ENV)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| toml_section_value(&config_str, "gpt_image_2", "base_url"));

    Some(LocationStatus {
        api_key,
        base_url,
        image_api_key,
        image_base_url,
        image_skill_installed: codex_image_skill_dir().join("SKILL.md").exists()
            && codex_image_skill_script_path().exists(),
    })
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

fn trashed_codex_thread_ids(state: &ToolboxState) -> HashSet<String> {
    state
        .trashed_codex_threads
        .iter()
        .map(|thread| thread.id.clone())
        .filter(|id| !id.trim().is_empty())
        .collect()
}

fn visible_codex_threads(state: &ToolboxState) -> Vec<CodexThreadRecord> {
    let trashed = trashed_codex_thread_ids(state);
    state
        .synced_codex_threads
        .iter()
        .filter(|thread| !trashed.contains(&thread.id))
        .cloned()
        .collect()
}

fn refresh_selected_mobile_thread_after_session_change(state: &mut ToolboxState) {
    let visible = visible_codex_threads(state);
    let selected_visible = visible
        .iter()
        .any(|thread| thread.id == state.selected_mobile_thread_id);
    if !selected_visible {
        state.selected_mobile_thread_id = visible
            .first()
            .map(|thread| thread.id.clone())
            .unwrap_or_default();
    }
    if let Some(thread) = visible
        .iter()
        .find(|thread| thread.id == state.selected_mobile_thread_id)
    {
        state.mobile_remote.active_thread_id = thread.id.clone();
        state.mobile_remote.active_thread_name = thread.thread_name.clone();
    } else {
        state.mobile_remote.active_thread_id.clear();
        state.mobile_remote.active_thread_name.clear();
    }
}

fn codex_thread_to_trash_record(
    thread: CodexThreadRecord,
    deleted_at: &str,
) -> TrashedCodexThreadRecord {
    TrashedCodexThreadRecord {
        id: thread.id,
        thread_name: thread.thread_name,
        updated_at: thread.updated_at,
        session_file: thread.session_file,
        cwd: thread.cwd,
        last_user_message: thread.last_user_message,
        last_assistant_message: thread.last_assistant_message,
        deleted_at: deleted_at.to_string(),
    }
}

fn trash_record_to_codex_thread(thread: &TrashedCodexThreadRecord) -> CodexThreadRecord {
    CodexThreadRecord {
        id: thread.id.clone(),
        thread_name: thread.thread_name.clone(),
        updated_at: thread.updated_at.clone(),
        session_file: thread.session_file.clone(),
        cwd: thread.cwd.clone(),
        last_user_message: thread.last_user_message.clone(),
        last_assistant_message: thread.last_assistant_message.clone(),
    }
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
    if state
        .mobile_remote
        .remote_control_backend_url
        .trim()
        .is_empty()
    {
        state.mobile_remote.remote_control_backend_url = "http://127.0.0.1:3847".into();
    }
    if state.mobile_remote.remote_control_status.trim().is_empty() {
        state.mobile_remote.remote_control_status = "未检测".into();
    }
    state.mobile_remote.remote_control_preferred = true;
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

    state
        .trashed_codex_threads
        .retain(|thread| !thread.id.trim().is_empty());
    let mut seen_trashed = HashSet::new();
    state
        .trashed_codex_threads
        .retain(|thread| seen_trashed.insert(thread.id.clone()));

    refresh_selected_mobile_thread_after_session_change(&mut state);

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
            let has_image_qr = binding.qr_data_url.trim().starts_with("data:image/");
            let has_openable_target = is_qq_authorization_target(&binding.qr_url);
            if !(has_image_qr || has_openable_target)
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
    // B8: 全局写锁，保证多线程并发时不互相丢更新
    let _write_guard = TOOLBOX_STATE_WRITE_LOCK
        .lock()
        .map_err(|e| format!("获取状态写锁失败: {e}"))?;
    let path = toolbox_state_path(app);
    let json = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    // B8: 临时文件写入后 rename 原子替换，避免进程崩溃时写一半清空文件
    let tmp_path = path.with_extension("json.tmp");
    fs::write(&tmp_path, &json).map_err(|e| format!("写入临时状态文件失败: {e}"))?;
    // B5: Windows 下设置文件权限为当前用户独占，Linux/macOS 设 0600（只有所有者可读写）
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        let _ = fs::set_permissions(&tmp_path, perms);
    }
    #[cfg(windows)]
    {
        // Windows 没有 POSIX 权限位，依赖文件系统 ACL（默认只有创建者有完全控制）
        // 如需更严格，可用 winapi 设 ACL，但常规场景下 AppData 已足够隐私
    }
    fs::rename(&tmp_path, &path).map_err(|e| {
        let _ = fs::remove_file(&tmp_path);
        format!("原子替换状态文件失败: {e}")
    })
}

const OPENAI_BUNDLED_MARKETPLACE_NAME: &str = "openai-bundled";

fn codex_plugin_config_id(name: &str, marketplace: &str) -> String {
    format!("{}@{}", name.trim(), marketplace.trim())
}

fn important_codex_builtin_plugin(
    name: &str,
    description: &str,
    skills: &[CodexBuiltinPluginSkill],
) -> bool {
    let haystack = format!(
        "{} {} {}",
        name,
        description,
        skills
            .iter()
            .map(|skill| format!("{} {}", skill.name, skill.description))
            .collect::<Vec<_>>()
            .join(" ")
    )
    .to_ascii_lowercase();
    ["computer", "chrome", "browser", "devtools", "fast", "speed"]
        .iter()
        .any(|needle| haystack.contains(needle))
}

fn yaml_front_matter_value(contents: &str, key: &str) -> Option<String> {
    let mut lines = contents.lines();
    if lines.next()? != "---" {
        return None;
    }
    for line in lines {
        if line == "---" {
            break;
        }
        let Some((left, right)) = line.split_once(':') else {
            continue;
        };
        if left.trim() == key {
            return Some(
                right
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_string(),
            )
            .filter(|value| !value.is_empty());
        }
    }
    None
}

fn load_codex_builtin_plugin_skills(
    root: &Path,
    manifest: &serde_json::Value,
) -> Vec<CodexBuiltinPluginSkill> {
    let skills_path = manifest
        .get("skills")
        .and_then(|value| value.as_str())
        .map(|value| root.join(value.trim_start_matches("./")))
        .unwrap_or_else(|| root.join("skills"));
    let Ok(skill_dirs) = fs::read_dir(skills_path) else {
        return Vec::new();
    };
    let mut skills = skill_dirs
        .flatten()
        .filter_map(|entry| {
            let path = entry.path().join("SKILL.md");
            let contents = fs::read_to_string(path).ok()?;
            let name = yaml_front_matter_value(&contents, "name")
                .unwrap_or_else(|| entry.file_name().to_string_lossy().to_string());
            let description =
                yaml_front_matter_value(&contents, "description").unwrap_or_else(|| name.clone());
            Some(CodexBuiltinPluginSkill { name, description })
        })
        .collect::<Vec<_>>();
    skills.sort_by(|left, right| left.name.cmp(&right.name));
    skills
}

fn codex_builtin_plugin_from_root(
    marketplace: &str,
    root: PathBuf,
    enabled_ids: &HashSet<String>,
) -> Option<CodexBuiltinPluginItem> {
    let manifest_path = root.join(".codex-plugin").join("plugin.json");
    let manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(manifest_path).ok()?).ok()?;
    let name = manifest.get("name")?.as_str()?.trim().to_string();
    if name.is_empty() || manifest.get("apps").is_some() {
        return None;
    }
    let skills = load_codex_builtin_plugin_skills(&root, &manifest);
    if skills.is_empty() {
        return None;
    }
    let interface = manifest
        .get("interface")
        .cloned()
        .unwrap_or_else(|| serde_json::Value::Object(Default::default()));
    let display_name = interface
        .get("displayName")
        .and_then(|value| value.as_str())
        .or_else(|| manifest.get("displayName").and_then(|value| value.as_str()))
        .unwrap_or(&name)
        .to_string();
    let description = manifest
        .get("description")
        .and_then(|value| value.as_str())
        .or_else(|| {
            interface
                .get("shortDescription")
                .and_then(|value| value.as_str())
        })
        .unwrap_or_default()
        .to_string();
    let version = manifest
        .get("version")
        .and_then(|value| value.as_str())
        .unwrap_or("local")
        .to_string();
    let id = codex_plugin_config_id(&name, marketplace);
    let important = important_codex_builtin_plugin(&name, &description, &skills);
    Some(CodexBuiltinPluginItem {
        id: id.clone(),
        name,
        display_name,
        marketplace: marketplace.to_string(),
        version,
        description,
        root: root.to_string_lossy().to_string(),
        enabled: enabled_ids.contains(&id),
        important,
        skills,
    })
}

fn newest_codex_builtin_plugin_entry(
    marketplace: &str,
    plugin_path: &Path,
    enabled_ids: &HashSet<String>,
) -> Option<CodexBuiltinPluginItem> {
    if plugin_path
        .join(".codex-plugin")
        .join("plugin.json")
        .is_file()
    {
        return codex_builtin_plugin_from_root(marketplace, plugin_path.to_path_buf(), enabled_ids);
    }
    let mut versions = fs::read_dir(plugin_path)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir() && path.join(".codex-plugin").join("plugin.json").is_file())
        .collect::<Vec<_>>();
    versions.sort();
    versions.reverse();
    versions
        .into_iter()
        .find_map(|root| codex_builtin_plugin_from_root(marketplace, root, enabled_ids))
}

fn openai_bundled_plugins_root_from_install_location(install_location: PathBuf) -> Option<PathBuf> {
    let root = install_location
        .join("app")
        .join("resources")
        .join("plugins");
    root.join(OPENAI_BUNDLED_MARKETPLACE_NAME)
        .join(".agents")
        .join("plugins")
        .join("marketplace.json")
        .is_file()
        .then_some(root)
}

#[cfg(target_os = "windows")]
fn find_openai_codex_install_locations_from_appx() -> Vec<PathBuf> {
    let mut command = std::process::Command::new("powershell");
    command.args([
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        "Get-AppxPackage -Name OpenAI.Codex | Select-Object -ExpandProperty InstallLocation",
    ]);
    #[cfg(target_os = "windows")]
    command.creation_flags(CREATE_NO_WINDOW);
    let Ok(output) = command.output() else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect()
}

#[cfg(target_os = "windows")]
fn find_openai_codex_install_locations_from_windows_apps() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let mut bases = Vec::new();
    if let Some(program_files) = std::env::var_os("ProgramFiles").map(PathBuf::from) {
        bases.push(program_files.join("WindowsApps"));
    }
    bases.push(PathBuf::from("C:/Program Files/WindowsApps"));
    for windows_apps in bases {
        let Ok(entries) = fs::read_dir(windows_apps) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("OpenAI.Codex_") {
                roots.push(entry.path());
            }
        }
    }
    roots.sort();
    roots.dedup();
    roots
}

fn find_openai_bundled_plugins_root() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let mut install_locations = find_openai_codex_install_locations_from_appx();
        install_locations.extend(find_openai_codex_install_locations_from_windows_apps());
        install_locations.sort();
        install_locations.dedup();
        install_locations
            .into_iter()
            .rev()
            .find_map(openai_bundled_plugins_root_from_install_location)
    }
    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

fn codex_plugin_cache_root() -> PathBuf {
    codex_config_dir().join("plugins").join("cache")
}

fn cached_marketplace_has_plugins(path: &Path) -> bool {
    let Ok(entries) = fs::read_dir(path) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let plugin_path = entry.path();
        plugin_path.is_dir()
            && plugin_path.file_name().and_then(|n| n.to_str()) != Some(".agents")
            && newest_codex_builtin_plugin_entry("probe", &plugin_path, &HashSet::new()).is_some()
    })
}

fn find_cached_codex_plugin_marketplaces() -> Vec<(String, PathBuf)> {
    let cache_root = codex_plugin_cache_root();
    let Ok(entries) = fs::read_dir(cache_root) else {
        return Vec::new();
    };
    let mut marketplaces = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_dir() || !cached_marketplace_has_plugins(&path) {
                return None;
            }
            let name = entry.file_name().to_string_lossy().trim().to_string();
            (!name.is_empty()).then_some((name, path))
        })
        .collect::<Vec<_>>();
    marketplaces
        .sort_by(|left, right| marketplace_priority(&left.0).cmp(&marketplace_priority(&right.0)));
    marketplaces
}

fn marketplace_priority(name: &str) -> usize {
    match name {
        "openai-bundled" => 0,
        "varswitch-plugins" => 1,
        "cooper-plugins" => 2,
        "openai-primary-runtime" => 3,
        "openai-curated" | "openai-curated-remote" | "openai-api-curated" => 4,
        "personal" => 8,
        _ => 9,
    }
}

fn find_openai_bundled_marketplace_root() -> Option<PathBuf> {
    find_openai_bundled_plugins_root()
        .map(|root| root.join(OPENAI_BUNDLED_MARKETPLACE_NAME))
        .or_else(|| {
            let cached = codex_plugin_cache_root().join(OPENAI_BUNDLED_MARKETPLACE_NAME);
            cached_marketplace_has_plugins(&cached).then_some(cached)
        })
}

fn discover_codex_plugin_marketplaces() -> Vec<(String, PathBuf)> {
    let mut marketplaces = Vec::new();
    if let Some(root) = find_openai_bundled_marketplace_root() {
        marketplaces.push((OPENAI_BUNDLED_MARKETPLACE_NAME.to_string(), root));
    }
    marketplaces.extend(find_cached_codex_plugin_marketplaces());
    marketplaces.sort_by(|left, right| {
        marketplace_priority(&left.0)
            .cmp(&marketplace_priority(&right.0))
            .then_with(|| left.1.cmp(&right.1))
    });
    let mut seen = HashSet::new();
    marketplaces
        .into_iter()
        .filter(|(_, path)| seen.insert(path.clone()))
        .collect()
}

fn enabled_codex_plugin_ids_from_config(config_text: &str) -> HashSet<String> {
    let mut ids = HashSet::new();
    let lines = config_text.lines().collect::<Vec<_>>();
    let mut index = 0;
    while index < lines.len() {
        if lines[index].trim() != "[plugins]" {
            index += 1;
            continue;
        }
        index += 1;
        while index < lines.len() {
            let trimmed = lines[index].trim();
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                break;
            }
            if trimmed.contains("enabled") && trimmed.contains("true") {
                if let Some((left, _)) = trimmed.split_once('=') {
                    let id = left.trim().trim_matches('"').to_string();
                    if !id.is_empty() {
                        ids.insert(id);
                    }
                }
            }
            index += 1;
        }
        break;
    }
    ids
}

fn list_codex_builtin_plugins(config_text: &str) -> CodexBuiltinPluginStatus {
    let marketplaces = discover_codex_plugin_marketplaces();
    if marketplaces.is_empty() {
        return CodexBuiltinPluginStatus {
            available: false,
            last_error: "未找到 Codex App 自带插件或本地插件缓存；已检查 Codex App openai-bundled 与 ~/.codex/plugins/cache".into(),
            ..Default::default()
        };
    }

    let enabled_ids = enabled_codex_plugin_ids_from_config(config_text);
    let mut plugins = Vec::new();
    let mut sources = Vec::new();
    let mut configured_any = false;
    let mut seen_names = HashSet::new();

    for (marketplace, marketplace_root) in marketplaces {
        let marketplace_source = marketplace_root.to_string_lossy().to_string();
        sources.push(format!("{}={}", marketplace, marketplace_source));
        configured_any |= config_has_plugin_marketplace_source(config_text, &marketplace_source);

        let Ok(plugin_dirs) = fs::read_dir(&marketplace_root) else {
            continue;
        };
        for plugin_dir in plugin_dirs.flatten() {
            let plugin_path = plugin_dir.path();
            if !plugin_path.is_dir()
                || plugin_path.file_name().and_then(|n| n.to_str()) == Some(".agents")
            {
                continue;
            }
            if let Some(entry) =
                newest_codex_builtin_plugin_entry(&marketplace, &plugin_path, &enabled_ids)
            {
                let key = entry.name.clone();
                if seen_names.insert(key) {
                    plugins.push(entry);
                }
            }
        }
    }

    plugins.sort_by(|left, right| {
        right
            .important
            .cmp(&left.important)
            .then_with(|| {
                marketplace_priority(&left.marketplace)
                    .cmp(&marketplace_priority(&right.marketplace))
            })
            .then_with(|| left.display_name.cmp(&right.display_name))
    });
    let total_count = plugins.len();
    let enabled_count = plugins.iter().filter(|plugin| plugin.enabled).count();
    let important_total_count = plugins.iter().filter(|plugin| plugin.important).count();
    let important_enabled_count = plugins
        .iter()
        .filter(|plugin| plugin.important && plugin.enabled)
        .count();
    CodexBuiltinPluginStatus {
        available: total_count > 0,
        marketplace_source: sources.join("; "),
        marketplace_configured: configured_any,
        enabled_count,
        total_count,
        important_enabled_count,
        important_total_count,
        plugins,
        last_error: if total_count > 0 {
            String::new()
        } else {
            "已发现插件来源，但没有解析到可启用的 Codex 插件".into()
        },
    }
}

fn ensure_discovered_plugin_marketplaces_config(config_text: &str) -> Result<String, String> {
    let marketplaces = discover_codex_plugin_marketplaces();
    let mut next = remove_invalid_local_plugin_marketplace_sections(config_text);
    for (marketplace, root) in marketplaces {
        if !plugin_marketplace_root_has_supported_manifest(&root) {
            continue;
        }
        next = ensure_plugin_marketplace_section(
            &next,
            &marketplace,
            &root.to_string_lossy(),
            "local",
        );
    }
    Ok(next)
}

fn toml_double_quoted_value(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn plugin_inline_entry_matches(line: &str, plugin_id: &str) -> bool {
    let trimmed = line.trim_start();
    let quoted = format!("\"{}\"", toml_double_quoted_value(plugin_id));
    trimmed.starts_with(&quoted) || trimmed.starts_with(plugin_id)
}

fn plugin_table_header_matches(line: &str, plugin_id: &str) -> bool {
    let trimmed = line.trim();
    let double_header = format!("[plugins.\"{}\"]", toml_double_quoted_value(plugin_id));
    let single_header = format!("[plugins.'{}']", plugin_id.replace('\'', "\\'"));
    trimmed == double_header || trimmed == single_header
}

fn write_enabled_codex_plugin_config(config_text: &str, plugin_id: &str) -> String {
    let plugins_header = "[plugins]";
    let plugin_line = format!(
        "\"{}\" = {{ enabled = true }}",
        toml_double_quoted_value(plugin_id)
    );
    let mut lines = config_text.lines().map(str::to_string).collect::<Vec<_>>();

    let mut table_start = None;
    let mut table_end = lines.len();
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if plugin_table_header_matches(trimmed, plugin_id) {
            table_start = Some(index);
            table_end = lines.len();
            continue;
        }
        if table_start.is_some() && trimmed.starts_with('[') && trimmed.ends_with(']') {
            table_end = index;
            break;
        }
    }

    if let Some(start) = table_start {
        let mut enabled_index = None;
        for index in (start + 1)..table_end {
            if lines[index].trim_start().starts_with("enabled") {
                enabled_index = Some(index);
                break;
            }
        }
        if let Some(index) = enabled_index {
            lines[index] = "enabled = true".to_string();
        } else {
            lines.insert(table_end, "enabled = true".to_string());
        }

        let mut plugins_start = None;
        let mut plugins_end = lines.len();
        for (index, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if trimmed == plugins_header {
                plugins_start = Some(index);
                continue;
            }
            if plugins_start.is_some() && trimmed.starts_with('[') && trimmed.ends_with(']') {
                plugins_end = index;
                break;
            }
        }
        if let Some(start) = plugins_start {
            for index in ((start + 1)..plugins_end).rev() {
                if plugin_inline_entry_matches(&lines[index], plugin_id) {
                    lines.remove(index);
                }
            }
        }
        return lines.join("\n");
    }

    let mut plugins_start = None;
    let mut plugins_end = lines.len();
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed == plugins_header {
            plugins_start = Some(index);
            continue;
        }
        if plugins_start.is_some() && trimmed.starts_with('[') && trimmed.ends_with(']') {
            plugins_end = index;
            break;
        }
    }
    if let Some(start) = plugins_start {
        for index in (start + 1)..plugins_end {
            if plugin_inline_entry_matches(&lines[index], plugin_id) {
                lines[index] = plugin_line;
                return lines.join("\n");
            }
        }
        lines.insert(plugins_end, plugin_line);
        return lines.join("\n");
    }

    let mut text = config_text.trim_end().to_string();
    if !text.is_empty() {
        text.push_str("\n\n");
    }
    text.push_str(plugins_header);
    text.push('\n');
    text.push_str(&plugin_line);
    text.push('\n');
    text
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
    "varswitch-plugins"
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
    let image = code.render::<Luma<u8>>().min_dimensions(240, 240).build();
    let dyn_img = DynamicImage::ImageLuma8(image);
    let mut png_bytes = Vec::new();
    dyn_img
        .write_to(
            &mut std::io::Cursor::new(&mut png_bytes),
            image::ImageFormat::Png,
        )
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

fn escape_toml_string_value(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn toml_section_value(config: &str, section: &str, key: &str) -> String {
    let target_header = format!("[{section}]");
    let prefix = format!("{key} =");
    let mut in_target = false;
    for line in config.lines() {
        let trimmed = line.trim();
        if is_toml_section_header(trimmed) {
            in_target = trimmed == target_header;
            continue;
        }
        if in_target && trimmed.starts_with(&prefix) {
            return trimmed
                .split_once('=')
                .map(|(_, value)| parse_toml_string_value(value))
                .unwrap_or_default();
        }
    }
    String::new()
}

fn is_managed_gpt_image_2_section(header: &str) -> bool {
    matches!(header.trim(), "[gpt_image_2]")
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

fn is_toml_section_header(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with('[') && trimmed.ends_with(']')
}

fn toml_root_key(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('[') {
        return None;
    }
    trimmed
        .split_once('=')
        .map(|(key, _)| key.trim().trim_matches('"').to_string())
        .filter(|key| !key.is_empty())
}

fn split_codex_config_root_and_sections(config_text: &str) -> (String, String) {
    const MANAGED_ROOT_KEYS: &[&str] = &[
        "model_provider",
        "model",
        "review_model",
        "model_reasoning_effort",
        "disable_response_storage",
        "preferred_auth_method",
        "chatgpt_base_url",
    ];
    let lines: Vec<&str> = config_text.lines().collect();
    let mut index = 0usize;
    let mut root_lines = Vec::new();
    let mut sections = Vec::new();

    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim();
        if is_toml_section_header(trimmed) {
            break;
        }
        if let Some(key) = toml_root_key(line) {
            if !MANAGED_ROOT_KEYS.contains(&key.as_str()) {
                root_lines.push(line.to_string());
            }
        } else if !trimmed.is_empty() {
            root_lines.push(line.to_string());
        }
        index += 1;
    }

    while index < lines.len() {
        let header = lines[index].trim();
        if !is_toml_section_header(header) {
            index += 1;
            continue;
        }
        let skip_managed_provider =
            header.starts_with("[model_providers.") || is_managed_gpt_image_2_section(header);
        let mut block = vec![lines[index].to_string()];
        index += 1;
        while index < lines.len() {
            let current = lines[index].trim();
            if is_toml_section_header(current) {
                break;
            }
            block.push(lines[index].to_string());
            index += 1;
        }
        if !skip_managed_provider {
            sections.push(block.join("\n").trim().to_string());
        }
    }

    (
        root_lines.join("\n").trim().to_string(),
        sections.join("\n\n").trim().to_string(),
    )
}

fn merge_codex_config_with_preserved_sections(generated: &str, existing: &str) -> String {
    let (preserved_root, preserved_sections) = split_codex_config_root_and_sections(existing);
    let mut output = generated.trim_end().to_string();

    if !preserved_root.trim().is_empty() {
        if let Some(pos) = output.find("\n[model_providers.") {
            output.insert_str(pos, &format!("\n{}", preserved_root.trim()));
        } else {
            output.push('\n');
            output.push_str(preserved_root.trim());
        }
    }

    if !preserved_sections.trim().is_empty() {
        output.push_str("\n\n");
        output.push_str(preserved_sections.trim());
    }

    output.push('\n');
    output
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

fn plugin_marketplace_root_has_supported_manifest(path: &Path) -> bool {
    [
        path.join(".codex-plugin").join("marketplace.json"),
        path.join(".agents")
            .join("plugins")
            .join("marketplace.json"),
        path.join("marketplace.json"),
    ]
    .into_iter()
    .any(|candidate| candidate.is_file())
}

fn marketplace_config_value(line: &str, key: &str) -> Option<String> {
    let (left, right) = line.split_once('=')?;
    if left.trim() != key {
        return None;
    }
    let raw = right.trim();
    if raw.len() >= 2 && raw.starts_with('"') && raw.ends_with('"') {
        return Some(
            raw[1..raw.len() - 1]
                .replace("\\\"", "\"")
                .replace("\\\\", "\\"),
        );
    }
    Some(raw.trim_matches('\'').to_string())
}

fn remove_invalid_local_plugin_marketplace_sections(existing: &str) -> String {
    let lines = existing.lines().collect::<Vec<_>>();
    let mut output = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let trimmed = lines[index].trim();
        if !(trimmed.starts_with("[marketplaces.") && trimmed.ends_with(']')) {
            output.push(lines[index].to_string());
            index += 1;
            continue;
        }

        let section_start = index;
        index += 1;
        while index < lines.len() {
            let next = lines[index].trim();
            if next.starts_with('[') && next.ends_with(']') {
                break;
            }
            index += 1;
        }
        let section = &lines[section_start..index];
        let source_type = section
            .iter()
            .find_map(|line| marketplace_config_value(line, "source_type"))
            .unwrap_or_default();
        let source = section
            .iter()
            .find_map(|line| marketplace_config_value(line, "source"))
            .unwrap_or_default();
        let invalid_local = source_type.eq_ignore_ascii_case("local")
            && !source.is_empty()
            && !plugin_marketplace_root_has_supported_manifest(Path::new(&source));

        if !invalid_local {
            output.extend(section.iter().map(|line| (*line).to_string()));
        }
    }

    output.join("\n").trim().to_string()
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
    let output = command.output().map_err(|e| {
        format!(
            "启动 Codex CLI 安装插件市场失败({}): {e}",
            executable.display()
        )
    })?;
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

fn run_codex_plugin_marketplace_remove(name: &str) -> Result<(), String> {
    let executable = resolve_codex_command()?;
    let mut command = codex_command(&executable);
    command
        .args(["plugin", "marketplace", "remove", name, "--json"])
        .env("CODEX_HOME", codex_config_dir())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = command.output().map_err(|e| {
        format!(
            "启动 Codex CLI 移除插件市场失败({}): {e}",
            executable.display()
        )
    })?;
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
    if detail.to_ascii_lowercase().contains("not found")
        || detail.to_ascii_lowercase().contains("unknown marketplace")
    {
        return Ok(());
    }
    Err(format!(
        "Codex CLI 移除插件市场失败(exit {})：{}",
        output.status.code().unwrap_or(-1),
        if detail.is_empty() {
            "无错误输出".to_string()
        } else {
            detail
        }
    ))
}

fn configure_git_longpaths_for_windows() {
    if !cfg!(windows) {
        return;
    }
    let mut command = Command::new("git");
    command.args(["config", "--global", "core.longpaths", "true"]);
    #[cfg(target_os = "windows")]
    command.creation_flags(CREATE_NO_WINDOW);
    let _ = command.output();
}

fn clean_plugin_marketplace_cache(name: &str) {
    let codex_home = codex_config_dir();
    let paths = [
        codex_home.join(".tmp").join("marketplaces").join(name),
        codex_home
            .join(".tmp")
            .join("marketplaces")
            .join(".staging"),
        codex_home.join("plugins").join("cache").join(name),
    ];
    for path in paths {
        if path.exists() {
            let _ = fs::remove_dir_all(path);
        }
    }
}

fn plugin_marketplace_snapshot_exists(name: &str) -> bool {
    let config_dir = codex_config_dir();
    cached_marketplace_has_plugins(&config_dir.join("plugins").join("cache").join(name))
        || plugin_marketplace_root_has_supported_manifest(
            &config_dir
                .join("plugins")
                .join(".tmp")
                .join("marketplaces")
                .join(name),
        )
        || plugin_marketplace_root_has_supported_manifest(
            &config_dir.join(".tmp").join("marketplaces").join(name),
        )
}

fn is_marketplace_different_source_error(error: &str) -> bool {
    let lowered = error.to_ascii_lowercase();
    lowered.contains("already added from a different source")
        || (lowered.contains("different source") && lowered.contains("marketplace"))
}

fn repair_and_add_codex_plugin_marketplace(source: &str) -> Result<(), String> {
    configure_git_longpaths_for_windows();
    match run_codex_plugin_marketplace_add(source) {
        Ok(()) => return Ok(()),
        Err(first_error) if is_marketplace_different_source_error(&first_error) => {
            let name = default_plugin_marketplace_name();
            let _ = run_codex_plugin_marketplace_remove(name);
            clean_plugin_marketplace_cache(name);
            configure_git_longpaths_for_windows();
            run_codex_plugin_marketplace_add(source).map_err(|second_error| {
                format!(
                    "{second_error}\n已检测到同名插件市场来源冲突，并自动执行 remove + 清理缓存 + 重新 add，但重新添加仍失败。\n原始错误：{first_error}"
                )
            })?;
            Ok(())
        }
        Err(error) => Err(error),
    }
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
    // B5: Bearer token 走明文 http 会泄露凭据，强制升级为 https
    if raw.starts_with("http://") {
        raw = format!("https://{}", &raw["http://".len()..]);
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
    _client: &reqwest::blocking::Client,
    binding: &MobileChannelBinding,
) -> Result<(String, String), String> {
    // B17: 不再用 notifystart 当探活（语义重复且会让 start_wechat_listener 再次 notifystart 两次）
    // 只做凭据格式校验 + URL 规范化，真正的 notifystart 由 start_wechat_listener 调用
    if !mobile_channel_has_credentials(binding) {
        return Err(mobile_channel_credential_hint("wechat").into());
    }
    let base_url = normalize_mobile_base_url(&binding.base_url, "wechat");
    Ok((
        base_url.clone(),
        format!("微信 iLink 凭据已验证：{}", gateway_url_label(&base_url)),
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
    cmd.status().map(|status| status.success()).unwrap_or(false)
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
    let lastPongAt = Date.now(); // B14: 记录最近一次收到心跳 ACK 的时间
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
      // B14: op 11 = HeartbeatACK，更新最近 pong 时间
      if (payload.op === 11) {
        lastPongAt = Date.now();
        return;
      }
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
        lastPongAt = Date.now(); // 刚连接重置计时
        heartbeatTimer = setInterval(() => {
          try {
            if (ws?.readyState === WebSocket.OPEN) {
              // B14: 连续两个心跳周期没收到 ACK 则主动断线重连
              if (Date.now() - lastPongAt > heartbeatMs * 2 + 5000) {
                fail(new Error(`QQ 心跳超时（${Math.round((Date.now() - lastPongAt) / 1000)}s 无应答）`));
                return;
              }
              ws.send(JSON.stringify({ op: 1, d: ws.nextSeq ?? null }));
            }
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
  let reconnectDelay = 1000; // B14: 指数退避初始值1s，上限60s
  while (!stopping) {
    try {
      await connectOnce();
      reconnectDelay = 1000; // B14: 连接成功后重置退避
    } catch (error) {
      if (stopping) break;
      const msg = error?.message || String(error);
      // B14: 鉴权类错误直接 failure 退出，无需重连（避免无限用错误凭据刷接口）
      if (/401|unauthorized|access_token|invalid_client|appId|appSecret|鉴权/i.test(msg)) {
        emit({ type: 'failure', message: `QQ 鉴权失败，请重新绑定：${msg}` });
        process.exit(1);
      }
      emit({ type: 'status', message: `QQ 网关断开，${reconnectDelay / 1000}s 后重连：${msg}` });
      await new Promise((resolve) => setTimeout(resolve, reconnectDelay));
      reconnectDelay = Math.min(reconnectDelay * 2, 60000); // B14: 指数退避
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
  let reconnectDelay = 1000; // B14: 指数退避初始值1s，上限60s
  while (!stopping) {
    try {
      await connectOnce();
      reconnectDelay = 1000; // B14: 连接成功后重置退避
    } catch (error) {
      if (stopping) break;
      const msg = error?.message || String(error);
      // B14: 飞书鉴权类错误（token无效/AppID错误）直接 failure 退出，不重连
      if (/99991663|99991672|token.*invalid|invalid.*token|AppID.*不|unauthorized|10003/i.test(msg)) {
        emit({ type: 'failure', message: `飞书鉴权失败，请重新绑定：${msg}` });
        process.exit(1);
      }
      emit({ type: 'status', message: `飞书 WebSocket 异常，${reconnectDelay / 1000}s 后重连：${msg}` });
      await new Promise((resolve) => setTimeout(resolve, reconnectDelay));
      reconnectDelay = Math.min(reconnectDelay * 2, 60000); // B14: 指数退避
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
        let output = cmd
            .output()
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
        let output = cmd
            .output()
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
        let now_ms = chrono_timestamp_millis().to_string();
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
            binding.qr_started_at = now_ms;
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
    generation: u64,
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
        let mut consecutive_errors = 0u32;
        for _ in 0..120 {
            std::thread::sleep(Duration::from_secs(interval));
            // B13: 代际检查，新注册流程已启动时旧 worker 应退出，不覆盖新凭据
            if LARK_REGISTRATION_GENERATION.load(Ordering::SeqCst) != generation {
                log_info!("[mobile-control][lark-reg] generation changed, worker exiting");
                return;
            }
            let poll = match poll_lark_registration_device(&client, &device_code) {
                Ok(poll) => {
                    consecutive_errors = 0; // 成功则重置连续错误计数
                    poll
                }
                Err(error) => {
                    consecutive_errors += 1;
                    // 已被新注册流程取代的旧 worker 不再回写错误状态，避免覆盖新流程的 UI 提示
                    if LARK_REGISTRATION_GENERATION.load(Ordering::SeqCst) != generation {
                        log_info!(
                            "[mobile-control][lark-reg] generation changed during error, worker exiting"
                        );
                        return;
                    }
                    update_channel_status(&app, "lark", "飞书注册轮询失败", &error);
                    // B13: 最多容忍 3 次连续网络错误，避免瞬时抖动中断整个注册流程
                    if consecutive_errors >= 3 {
                        log_info!(
                            "[mobile-control][lark-reg] 3 consecutive errors, worker exiting"
                        );
                        LARK_REGISTRATION_ACTIVE.store(false, Ordering::SeqCst);
                        return;
                    }
                    continue;
                }
            };
            // 轮询请求可能持续数秒；请求返回后再次检查，避免清除绑定后旧 worker 回写凭据。
            if LARK_REGISTRATION_GENERATION.load(Ordering::SeqCst) != generation {
                log_info!(
                    "[mobile-control][lark-reg] generation changed after poll, worker exiting"
                );
                return;
            }
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

/// B9: 检查并记录消息ID，返回true=重复消息应忽略，false=新消息
fn check_and_record_msg_id(
    store: &OnceLock<Mutex<std::collections::VecDeque<String>>>,
    id: &str,
) -> bool {
    if id.is_empty() {
        return false;
    }
    let deque = store.get_or_init(|| Mutex::new(std::collections::VecDeque::new()));
    if let Ok(mut guard) = deque.lock() {
        if guard.iter().any(|s| s.as_str() == id) {
            return true;
        }
        guard.push_back(id.to_string());
        if guard.len() > 512 {
            guard.pop_front();
        }
    }
    false
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
        // B5: build_toolbox_snapshot 把 secret 掩码成 "*"，防止该掩码值被当成真凭据回写
        if !app_secret.trim().is_empty() && app_secret.trim() != "*" {
            binding.app_secret = app_secret.trim().to_string();
        }
        if !bot_token.trim().is_empty() && bot_token.trim() != "*" {
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
        binding.qr_url.clear();
        binding.qr_data_url.clear();
        binding.qr_device_code.clear();
        binding.qr_started_at.clear();
        binding.credential_status = "平台凭据已保存".into();
        binding.last_error.clear();
        binding.updated_at = chrono_now();
    }
    attach_selected_thread_to_mobile_channel(&mut state, &normalized);
    write_toolbox_state(app, &state)?;
    Ok(build_toolbox_snapshot(app))
}

/// 用后台 Codex CLI `exec resume <thread_id>` 续接指定会话并返回最终回复。
/// 作为 App 注入不可用时的兜底路径（CLI 与桌面 App 共享 ~/.codex 会话存储）。
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
    // B9: 分离 stdout/stderr 用后台线程排干，主线程以 try_wait() 轮询 + 5 分钟超时，
    // 避免 wait_with_output() 无限阻塞，也防止 pipe 缓冲区满后子进程反过来卡死。
    let child_stdout = child.stdout.take();
    let child_stderr = child.stderr.take();
    let stdout_handle = std::thread::spawn(move || {
        use std::io::Read;
        let mut buf = Vec::new();
        if let Some(mut out) = child_stdout {
            let _ = out.read_to_end(&mut buf);
        }
        buf
    });
    let stderr_handle = std::thread::spawn(move || {
        use std::io::Read;
        let mut buf = Vec::new();
        if let Some(mut err) = child_stderr {
            let _ = err.read_to_end(&mut buf);
        }
        buf
    });
    let deadline = Instant::now() + Duration::from_secs(300);
    let exit_status = loop {
        match child
            .try_wait()
            .map_err(|e| format!("等待 Codex CLI 失败: {e}"))?
        {
            Some(status) => break status,
            None => {
                if Instant::now() > deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = fs::remove_file(&output_path);
                    return Err("Codex CLI 执行超时（5分钟），已强制终止".into());
                }
                std::thread::sleep(Duration::from_millis(500));
            }
        }
    };
    let stdout_bytes = stdout_handle.join().unwrap_or_default();
    let stderr_bytes = stderr_handle.join().unwrap_or_default();
    let reply = fs::read_to_string(&output_path)
        .unwrap_or_else(|_| String::from_utf8_lossy(&stdout_bytes).to_string())
        .trim()
        .to_string();
    let _ = fs::remove_file(&output_path);
    if !exit_status.success() {
        let mut detail = String::from_utf8_lossy(&stderr_bytes).trim().to_string();
        if detail.is_empty() {
            detail = String::from_utf8_lossy(&stdout_bytes).trim().to_string();
        }
        return Err(format!("Codex CLI 执行失败: {detail}"));
    }
    if reply.is_empty() {
        // 退出码成功但没有 last-message 文件时，退回用 stdout 内容。
        let stdout_text = String::from_utf8_lossy(&stdout_bytes).trim().to_string();
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

/// 从正在运行的 Codex.exe / ChatGPT.exe 命令行里解析 `--remote-debugging-port=<port>`。
#[cfg(windows)]
fn codex_debug_port_from_process() -> Option<u16> {
    let mut cmd = Command::new("powershell");
    cmd.args([
            "-NoProfile",
            "-Command",
            "Get-CimInstance Win32_Process -Filter \"name='ChatGPT.exe' OR name='Codex.exe'\" | Select-Object -ExpandProperty CommandLine",
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
                // 命令行里带参数不代表端口已在监听（进程可能刚启动、或是同名 CLI），
                // 必须实际探测 HTTP 端点，避免误报导致跳过重启流程。
                if port > 0 && codex_cdp_port_responds(port) {
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
#[allow(dead_code)]
const CODEX_PREFERRED_DEBUG_PORT: u16 = 9229;

/// 探测指定端口上的 CDP HTTP 端点是否真正可用。
/// 注意不能用「进程命令行里带 --remote-debugging-port」来判断就绪：
/// 参数在进程启动瞬间就存在，而端口要等页面初始化后才开始监听。
fn codex_cdp_port_responds(port: u16) -> bool {
    let Ok(client) = reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(800))
        .build()
    else {
        return false;
    };
    client
        .get(format!("http://127.0.0.1:{port}/json/version"))
        .send()
        .map(|resp| resp.status().is_success())
        .unwrap_or(false)
}

/// 探测调试端口。
///
/// CDP 不可用时，自动重启当前桌面 App 并开启本地调试端口，
/// 确保手机消息进入用户选择的 Codex App 对话，而不是静默转到 CLI。
fn codex_debug_port_or_relaunch() -> Result<u16, String> {
    if let Some(port) = codex_debug_port() {
        return Ok(port);
    }
    let port = CODEX_PREFERRED_DEBUG_PORT;
    relaunch_codex_with_debug_port(port)?;
    // Windows 桌面 App 启动后端口通常需要几秒才出现，等待期间不回退到 CLI，
    // 确保本次消息仍然进入用户选择的 Codex App 对话。冷启动(尤其 MSIX 包)可能
    // 较慢，这里直接探测 HTTP 端点，最多等约 60 秒。
    for _ in 0..60 {
        if codex_cdp_port_responds(port) {
            return Ok(port);
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    Err(format!(
        "Codex App 已尝试开启本地调试端口 {port}，但端口仍未就绪；请手动关闭并重新打开 Codex 桌面应用后重试"
    ))
}

/// 从正在运行的 Codex/ChatGPT 桌面 App 拿到 PID 和完整路径(动态获取，不写死安装位置)。
/// 2026-07 起 Codex 桌面 App 并入统一的 ChatGPT 桌面应用：Windows 主程序从 Codex.exe
/// 改名为 ChatGPT.exe（商店包标识沿用 OpenAI.Codex_*）。这里同时匹配两个进程名，
/// 并按可信度排序，避免匹配到旧版残留或 ChatGPT Classic。
#[cfg(windows)]
fn running_codex_desktop_process() -> Option<(u32, String)> {
    fn is_desktop_app_path(path: &str) -> bool {
        let trimmed = path.trim();
        if trimmed.is_empty() || !Path::new(trimmed).is_file() {
            return false;
        }
        let lower = trimmed.to_ascii_lowercase();
        // 系统里有很多同名的 codex.exe 并非桌面 App 主程序：npm 全局包
        // (node_modules/vendor)、桌面 App 包内置的 CLI(app\resources\codex.exe)、
        // 编辑器扩展自带的 CLI(.cursor/.vscode 下 extensions\...\bin\...)。
        // 杀掉/重启这些进程不会让桌面 App 打开调试端口，反而会误杀其他工具的
        // 后台进程，或者让仍在运行的 GUI 单实例吞掉激活参数导致端口永远不就绪。
        const EXCLUDED_SEGMENTS: [&str; 7] = [
            "node_modules",
            "\\vendor\\",
            "\\resources\\",
            "\\.cursor\\",
            "\\.vscode\\",
            "\\extensions\\",
            "\\bin\\",
        ];
        !EXCLUDED_SEGMENTS
            .iter()
            .any(|segment| lower.contains(segment))
    }

    fn parse_process_line(line: &str) -> Option<(u32, String)> {
        let (pid, path) = line.trim().split_once('\t')?;
        let pid = pid.trim().parse::<u32>().ok()?;
        let path = path.trim().to_string();
        is_desktop_app_path(&path).then_some((pid, path))
    }

    /// 候选可信度：统一版包（OpenAI.Codex_*，即合并后的 ChatGPT 桌面应用）最优，
    /// 其次是 ChatGPT.exe，再次旧版 Codex.exe；路径含 classic 的（ChatGPT Classic
    /// 旧聊天版，没有 Codex 模式）殿后。同分保持启动时间先后（主进程先启动）。
    fn candidate_rank(path: &str) -> u32 {
        let lower = path.to_ascii_lowercase();
        let mut rank = if lower.contains("openai.codex") {
            0
        } else if lower.ends_with("\\chatgpt.exe") {
            1
        } else {
            2
        };
        if lower.contains("classic") {
            rank += 10;
        }
        rank
    }

    fn pick_best(text: &str) -> Option<(u32, String)> {
        let mut candidates: Vec<(u32, String)> =
            text.lines().filter_map(parse_process_line).collect();
        // 稳定排序：先按可信度，同分保持 PowerShell 输出顺序（已按启动时间排序）。
        candidates.sort_by_key(|(_, path)| candidate_rank(path));
        candidates.into_iter().next()
    }

    let mut cmd = Command::new("powershell");
    cmd.args([
            "-NoProfile",
            "-Command",
            // 强制 UTF-8，避免用户名含中文时 Rust 把 PowerShell 输出解码成乱码。
            // 列出全部候选（不能只取第一个：同名的 CLI 进程可能排在桌面 App 前面），
            // 由 Rust 侧按路径过滤后选出真正的桌面 App 主程序。
            "$OutputEncoding = [Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false); Get-CimInstance Win32_Process -Filter \"name='ChatGPT.exe' OR name='Codex.exe'\" | Where-Object { $_.ExecutablePath -and $_.ExecutablePath -notlike '*node_modules*' -and $_.CommandLine -notlike '*--type=*' } | Sort-Object CreationDate | ForEach-Object { \"$($_.ProcessId)`t$($_.ExecutablePath)\" }",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);
    if let Ok(output) = cmd.output() {
        if let Some(process) = pick_best(&String::from_utf8_lossy(&output.stdout)) {
            return Some(process);
        }
    }
    // Windows 的 WMI 权限受限时，Get-Process 仍能读取桌面版 Store App 路径。
    let mut fallback = Command::new("powershell");
    fallback
        .args([
            "-NoProfile",
            "-Command",
            // 按启动时间排序：Electron 主进程先于渲染子进程启动，这里拿不到命令行
            // 无法按 --type= 排除子进程，取最早启动的同名进程即主进程。
            "$OutputEncoding = [Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false); Get-Process -Name ChatGPT,Codex -ErrorAction SilentlyContinue | Where-Object { $_.Path -and $_.Path -notlike '*node_modules*' } | Sort-Object StartTime | ForEach-Object { \"$($_.Id)`t$($_.Path)\" }",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(target_os = "windows")]
    fallback.creation_flags(CREATE_NO_WINDOW);
    let output = fallback.output().ok()?;
    pick_best(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(not(windows))]
fn running_codex_desktop_process() -> Option<(u32, String)> {
    None
}

#[cfg(windows)]
fn windows_package_family_from_exe_path(exe_path: &str) -> Option<String> {
    // WindowsApps 包目录形如 <Name>_<Version>_<Arch>__<PublisherId>。
    // 合并后的 ChatGPT 桌面应用沿用 OpenAI.Codex_* 包标识，但为兼容今后
    // 可能的包名变更，这里接受任何 <identity>__<publisherId> 形态的目录段
    // （本函数只在路径位于 WindowsApps 下时被调用，该形态即包目录）。
    let package_dir = Path::new(exe_path)
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .find(|component| component.contains("__"))?;
    let (identity, publisher_id) = package_dir.rsplit_once("__")?;
    let package_name = identity.split('_').next()?;
    if package_name.is_empty() || publisher_id.is_empty() {
        return None;
    }
    Some(format!("{package_name}_{publisher_id}"))
}

/// WindowsApps 中的 MSIX 可执行文件受 ACL 保护，普通桌面进程直接 spawn 会得到
/// ERROR_ACCESS_DENIED。通过系统的 ApplicationActivationManager 按 AUMID 激活，
/// 同时把 Electron 的 CDP 参数作为 activation arguments 传入。
#[cfg(windows)]
fn activate_packaged_codex(exe_path: &str, port: u16) -> Result<(), String> {
    let package_family = windows_package_family_from_exe_path(exe_path)
        .ok_or_else(|| format!("未能从 Codex App 路径解析包标识: {exe_path}"))?;
    let arguments =
        format!("--remote-debugging-port={port} --remote-allow-origins=http://127.0.0.1:{port}");
    let script = r#"
$ErrorActionPreference = 'Stop'
$OutputEncoding = [Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
$app = Get-StartApps | Where-Object { $_.AppID -like ($env:VARSWITCH_CODEX_PACKAGE_FAMILY + '!*') } | Select-Object -First 1
if (-not $app) { throw ('未找到 Codex AppUserModelId，package_family=' + $env:VARSWITCH_CODEX_PACKAGE_FAMILY) }
if (-not ('VarSwitch.ApplicationActivationManager' -as [type])) {
  Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace VarSwitch {
  [Flags] public enum ActivateOptions { None = 0 }
  [ComImport, Guid("2e941141-7f97-4756-ba1d-9decde894a3d"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
  public interface IApplicationActivationManager {
    [PreserveSig] int ActivateApplication(
      [MarshalAs(UnmanagedType.LPWStr)] string appUserModelId,
      [MarshalAs(UnmanagedType.LPWStr)] string arguments,
      ActivateOptions options, out uint processId);
  }
  [ComImport, Guid("45BA127D-10A8-46EA-8AB7-56EA9078943C")]
  public class ApplicationActivationManager {}
  public static class PackagedAppActivator {
    public static uint Activate(string appUserModelId, string arguments) {
      var manager = (IApplicationActivationManager)new ApplicationActivationManager();
      uint processId;
      int hr = manager.ActivateApplication(appUserModelId, arguments, ActivateOptions.None, out processId);
      if (hr < 0) Marshal.ThrowExceptionForHR(hr);
      return processId;
    }
  }
}
'@
}
$pidValue = [VarSwitch.PackagedAppActivator]::Activate($app.AppID, $env:VARSWITCH_CODEX_ACTIVATION_ARGS)
Write-Output ($app.AppID + "`t" + $pidValue)
"#;
    let mut command = Command::new("powershell");
    command
        .args(["-NoProfile", "-Sta", "-Command", script])
        .env("VARSWITCH_CODEX_PACKAGE_FAMILY", &package_family)
        .env("VARSWITCH_CODEX_ACTIVATION_ARGS", &arguments)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command.creation_flags(CREATE_NO_WINDOW);
    let output = command
        .output()
        .map_err(|e| format!("调用 Windows 应用激活器失败: {e}"))?;
    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if error.is_empty() {
            format!("Windows 应用激活器返回退出码 {}", output.status)
        } else {
            format!("Windows 应用激活器失败: {error}")
        });
    }
    let detail = String::from_utf8_lossy(&output.stdout).trim().to_string();
    log_info!(
        "[mobile-control][codex-app] packaged App activated: package_family={}, detail={}",
        package_family,
        detail
    );
    Ok(())
}

#[cfg(windows)]
fn relaunch_codex_with_debug_port(port: u16) -> Result<(), String> {
    let (pid, exe_path) = running_codex_desktop_process()
        .ok_or("未找到正在运行的 Codex/ChatGPT 桌面应用，请先打开 ChatGPT 桌面应用（原 Codex App，2026-07 起已改名）")?;
    log_info!(
        "[mobile-control][codex-app] restarting selected desktop App with CDP: pid={}, executable={}, port={}",
        pid,
        exe_path,
        port
    );
    let mut kill_cmd = Command::new("taskkill");
    kill_cmd
        .args(["/F", "/T", "/PID", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(target_os = "windows")]
    kill_cmd.creation_flags(CREATE_NO_WINDOW);
    let status = kill_cmd
        .status()
        .map_err(|e| format!("终止 Codex App 失败(PID {pid}): {e}"))?;
    if !status.success() {
        return Err(format!("终止 Codex App 失败(PID {pid})"));
    }
    std::thread::sleep(Duration::from_millis(1500));
    if exe_path.to_ascii_lowercase().contains("\\windowsapps\\") {
        activate_packaged_codex(&exe_path, port)
            .map_err(|e| format!("重启 Codex App 失败({exe_path}): {e}"))?;
    } else {
        let allow_origin = format!("http://127.0.0.1:{port}");
        let mut launch_cmd = Command::new(&exe_path);
        launch_cmd
            .arg(format!("--remote-debugging-port={port}"))
            .arg(format!("--remote-allow-origins={allow_origin}"))
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        launch_cmd.creation_flags(CREATE_NO_WINDOW);
        launch_cmd
            .spawn()
            .map_err(|e| format!("重启 Codex App 失败({exe_path}): {e}"))?;
    }
    log_info!(
        "[mobile-control][codex-app] desktop App restart command started: port={}",
        port
    );
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
                || title.to_lowercase().contains("chatgpt")
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
        let path = if slash < rest.len() {
            &rest[slash..]
        } else {
            "/"
        };
        let (host, port) = match host_port.rsplit_once(':') {
            Some((h, p)) => (h.to_string(), p.parse::<u16>().unwrap_or(9229)),
            None => (host_port.to_string(), 9229),
        };
        let mut stream = TcpStream::connect((host.as_str(), port))
            .map_err(|e| format!("连接 Codex 调试端口失败: {e}"))?;
        // 读超时设短（30s），recv_text 在帧边界超时时发 ping 保活并继续等待；
        // 整体等待上限由 recv_text 的 deadline 控制（须大于注入脚本的 300s 轮询上限）。
        stream.set_read_timeout(Some(Duration::from_secs(30))).ok();
        stream.set_write_timeout(Some(Duration::from_secs(10))).ok();
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
        Ok(CdpClient { stream, next_id: 0 })
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

    /// 发送一个带掩码的空 ping 帧（客户端→服务端必须带掩码）。
    fn send_ping(&mut self) -> Result<(), String> {
        let mask = uuid::Uuid::new_v4().into_bytes();
        let mut frame = Vec::with_capacity(6);
        frame.push(0x89); // FIN + ping
        frame.push(0x80); // masked, len=0
        frame.extend_from_slice(&mask[..4]);
        self.stream
            .write_all(&frame)
            .map_err(|e| format!("发送 CDP ping 失败: {e}"))
    }

    /// 读取 2 字节帧头。仅在「帧边界」（首字节尚未到达）容忍读超时：
    /// Codex 执行长任务时页面可能几分钟不发任何帧，读超时不代表连接断开，
    /// 发 ping 保活并继续等待，直到 deadline。帧中途超时视为真实错误。
    fn read_frame_head(&mut self, deadline: Instant) -> Result<[u8; 2], String> {
        let mut head = [0u8; 2];
        loop {
            match self.stream.read(&mut head[..1]) {
                Ok(0) => return Err("CDP 连接已关闭".into()),
                Ok(_) => break,
                Err(e)
                    if e.kind() == std::io::ErrorKind::TimedOut
                        || e.kind() == std::io::ErrorKind::WouldBlock =>
                {
                    if Instant::now() > deadline {
                        return Err("等待 CDP 响应超时（Codex 长时间未返回结果）".into());
                    }
                    self.send_ping()?;
                }
                Err(e) => return Err(format!("读取 CDP 数据失败: {e}")),
            }
        }
        self.stream
            .read_exact(&mut head[1..])
            .map_err(|e| format!("读取 CDP 数据失败: {e}"))?;
        Ok(head)
    }

    /// 读取一个完整帧的 payload(自动应答 ping，跳过非文本控制帧)。
    fn recv_text(&mut self) -> Result<String, String> {
        // 整体等待上限：略大于注入脚本自身的 300s 轮询上限。
        let deadline = Instant::now() + Duration::from_secs(330);
        loop {
            let head = self.read_frame_head(deadline)?;
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
            // 安全：限制单帧大小，避免对端声明超大长度导致一次性分配巨量内存。
            if len > 64 * 1024 * 1024 {
                return Err("CDP WebSocket 帧过大".into());
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
        // 墙钟总时限：页面持续发事件帧（如 console 日志）会让每次读取都成功，
        // 若响应因页面跳转等原因永远不来，仅靠读取次数上限可能等非常久。
        let deadline = Instant::now() + Duration::from_secs(360);
        for _ in 0..10000 {
            if Instant::now() > deadline {
                return Err(format!("CDP 命令 {method} 等待响应超过总时限"));
            }
            let text = self.recv_text()?;
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
                continue;
            };
            if value.get("id").and_then(|v| v.as_i64()) == Some(id) {
                if let Some(err) = value.get("error") {
                    return Err(format!("CDP 命令 {method} 失败: {err}"));
                }
                return Ok(value
                    .get("result")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null));
            }
            // 否则是事件(method 字段)，继续等。
        }
        Err(format!("CDP 命令 {method} 等待响应超时"))
    }
}

/// 在 Codex 页面执行一段 JS 表达式，返回其(已 returnByValue 的)结果值。
fn cdp_evaluate(client: &mut CdpClient, expression: &str) -> Result<serde_json::Value, String> {
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

/// 注入到 Codex 页面的“只发送”脚本。`{PROMPT_JSON}` 处会被替换为 JSON 安全转义后的消息。
/// 逻辑:找输入框 → 写入消息 → 点发送(兜底回车) → 确认输入框清空后立刻返回 {ok:true,sent:true}。
/// 不在页面里等待/抓取回复——回复由后端 `codex_capture_reply_from_session` 从共享会话文件读取，
/// 避免新版 ChatGPT 界面 DOM 抓取失败导致的 300 秒空等（会拖过平台被动回复窗口）。
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
    // 优先用 markdown 内容块(每条回复一个 _markdownContent_ 容器；
    // 合并后的 ChatGPT 桌面应用也常用 prose 类渲染 markdown)。
    let blocks = Array.from(document.querySelectorAll("[class*='markdownContent'], [class*='markdown-content'], [class*='_markdown'], [class*='prose']"))
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
        if (value.length > 200) {
          // 长文本一次性写入：逐字符模拟在重型 React 组件上可能耗时数分钟。
          setter.call(input, value);
          input.dispatchEvent(new InputEvent("input", { bubbles: true, inputType: "insertText", data: value }));
        } else {
          for (const char of value) {
            const current = input.value;
            setter.call(input, current + char);
            input.dispatchEvent(new InputEvent("input", { bubbles: true, inputType: "insertText", data: char }));
          }
        }
      } else {
        input.value = value;
        input.dispatchEvent(new InputEvent("input", { bubbles: true, inputType: "insertText", data: value }));
      }
      input.dispatchEvent(new Event("change", { bubbles: true }));
    } else {
      // contenteditable: execCommand 逐字符插入(长文本一次性写入)。
      input.textContent = "";
      if (value.length > 200 || !document.execCommand) {
        input.textContent = value;
      } else {
        for (const char of value) {
          document.execCommand("insertText", false, char);
        }
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
    // 不再强制滚动按钮到视口中央，避免兼容注入时改变用户当前 Codex 窗口滚动位置/视觉布局。
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
  // 等待输入框出现（重启 Codex 后页面需要时间加载，最多等约 15 秒）。
  let input = null;
  for (let i = 0; i < 100; i++) {
    input = findInput();
    if (input) break;
    await sleep(150);
  }
  if (!input) return { ok: false, error: "未找到 Codex 输入框（页面可能还在加载）" };
  setInputValue(input, prompt);
  let sent = false;
  for (let i = 0; i < 20; i++) {
    await sleep(150);
    const button = findSendButton(input);
    if (button && !button.disabled) { sent = activateButton(button); break; }
  }
  if (!sent) submitByKeyboard(input);
  // 只负责把消息发出去，不在页面里等待/抓取回复：新版 ChatGPT 桌面应用界面
  // DOM 结构多变，抓取回复常常空等到 300 秒超时，拖过平台被动回复窗口（QQ 仅 5 分钟）。
  // 回复改由后端从共享会话文件读取（稳定且快）。这里确认输入框已清空作为“已发送”信号，
  // 最多等约 3 秒。
  let cleared = false;
  for (let i = 0; i < 20; i++) {
    await sleep(150);
    const v = ("value" in input) ? input.value : (input.textContent || "");
    if (!v || !v.trim()) { cleared = true; break; }
  }
  return { ok: true, sent: true, cleared };
})()
"#;
    TEMPLATE.replace("__PROMPT_JSON__", prompt_json)
}

/// 最近一次通过深链接激活的对话（thread_id + 时间），用于缩短重复激活的等待。
static CODEX_LAST_ACTIVATED_THREAD: OnceLock<Mutex<Option<(String, Instant)>>> = OnceLock::new();

/// 用 codex://threads/<id> deep link 让 Codex App 切到指定对话，并等待切换完成。
/// （合并后的 ChatGPT 桌面应用仍注册 codex:// 协议处理 Codex 线程链接。）
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
        let opener = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        Command::new(opener)
            .arg(&deep_link)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("打开 Codex 对话失败: {e}"))?;
    }
    // 等待 App 切换对话并渲染。若上一条消息刚激活过同一对话（App 已停在该对话上），
    // 深链接等同无操作，缩短等待以降低每条消息的固定延迟。
    let cache = CODEX_LAST_ACTIVATED_THREAD.get_or_init(|| Mutex::new(None));
    let recently_same = cache
        .lock()
        .ok()
        .and_then(|guard| guard.clone())
        .map(|(last_id, at)| last_id == id && at.elapsed() < Duration::from_secs(600))
        .unwrap_or(false);
    let settle = if recently_same {
        Duration::from_millis(700)
    } else {
        Duration::from_millis(2000)
    };
    std::thread::sleep(settle);
    if let Ok(mut guard) = cache.lock() {
        *guard = Some((id.to_string(), Instant::now()));
    }
    Ok(())
}

/// Codex App 注入失败的两种性质，决定兜底策略：
enum CodexAppSendFailure {
    /// 消息确定没有进入 Codex（探测/重启/连接/找输入框失败），可安全改投后台 CLI 重发。
    NotSent(String),
    /// 消息可能已经发送成功（连接中断或回复未捕获），绝不能重发，只允许只读回捞回复。
    MaybeSent { port: u16, error: String },
}

/// 把消息注入 Codex 桌面 App 选中的对话，等待并返回助手回复。
/// 流程:切对话 → 探测调试端口 → 找页面 target → CDP 注入发送脚本 → 解析回复。
fn send_prompt_to_codex_app(thread_id: &str, prompt: &str) -> Result<String, CodexAppSendFailure> {
    use CodexAppSendFailure::{MaybeSent, NotSent};
    let text = prompt.trim();
    if text.is_empty() {
        return Err(NotSent("消息内容为空".into()));
    }
    // 1) 探测调试端口；没有则自动用调试端口重启 Codex App(放在切对话之前，
    //    因为重启会重置页面，切对话必须在重启之后做)。
    let port = codex_debug_port_or_relaunch().map_err(NotSent)?;
    // 2) 切到选中的对话(thread_id 为空则注入当前打开的对话)。
    if let Err(error) = activate_codex_thread(thread_id) {
        log_warn!("[mobile-control][codex-app] 切换对话失败(继续尝试注入当前对话): {error}");
    }
    // 3) 找页面 target。
    let ws_url = codex_find_page_target(port).map_err(NotSent)?;
    // 4) 连接 CDP 并注入。消息在脚本点击发送后才算「可能已发出」；
    //    在那之前的所有失败都可以安全重投 CLI。
    let mut client = CdpClient::connect(&ws_url).map_err(NotSent)?;
    client
        .command("Runtime.enable", serde_json::json!({}))
        .map_err(NotSent)?;
    let prompt_json = serde_json::to_string(text)
        .map_err(|e| NotSent(format!("序列化消息失败: {e}")))?;
    let script = codex_inject_send_script(&prompt_json);
    let result = client
        .command(
            "Runtime.evaluate",
            serde_json::json!({
                "expression": script,
                "returnByValue": true,
                "awaitPromise": true,
                "allowUnsafeEvalBlockedByCSP": true,
                "userGesture": true,
            }),
        )
        .map_err(|e| MaybeSent {
            port,
            error: format!("CDP 连接中断（消息可能已发送）: {e}"),
        })?;
    if let Some(exception) = result.get("exceptionDetails") {
        return Err(MaybeSent {
            port,
            error: format!("Codex 页面脚本异常: {exception}"),
        });
    }
    let value = result
        .get("result")
        .and_then(|r| r.get("value"))
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    // 5) 解析脚本返回 {ok, sent, [text]}。当前脚本只负责发送，正常返回 {ok:true}
    //    且不含 text；回复交由调用方从共享会话文件读取。若某个旧路径仍返回了 text，
    //    也照常直接使用。
    let ok = value.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
    if ok {
        let reply = json_string(&value, &["text"]);
        if reply.trim().is_empty() {
            return Err(MaybeSent {
                port,
                error: "消息已发送到 Codex，正在从会话文件读取回复".into(),
            });
        }
        Ok(reply.trim().to_string())
    } else {
        let error = json_string(&value, &["error"]);
        let error = if error.is_empty() {
            "Codex App 注入失败".to_string()
        } else {
            error
        };
        // 「未找到输入框」发生在输入/点击之前，消息一定没有发出去。
        if error.contains("未找到 Codex 输入框") {
            Err(NotSent(error))
        } else {
            Err(MaybeSent { port, error })
        }
    }
}

/// 只读回捞：消息可能已送达 Codex 但回复没拿到时，重新连接页面轮询最新的
/// 助手回复，直到生成结束且文本稳定。绝不重新发送消息，避免重复执行。
fn codex_capture_reply_readonly(port: u16) -> Result<String, String> {
    const CAPTURE_SCRIPT: &str = r#"
(() => {
  const textOf = (n) => (n?.innerText || n?.textContent || "").trim();
  function visible(n) {
    if (!n) return false;
    const s = getComputedStyle(n);
    const r = n.getBoundingClientRect();
    return s.visibility !== "hidden" && s.display !== "none" && r.width > 0 && r.height > 0;
  }
  function inUserBubble(node) {
    let c = node;
    for (let i = 0; c && i < 12; i++, c = c.parentElement) {
      const cl = (c.className || "").toString();
      if (cl.includes("items-end") || cl.includes("bg-token-foreground")) return true;
    }
    return false;
  }
  let blocks = Array.from(document.querySelectorAll("[class*='markdownContent'], [class*='markdown-content'], [class*='_markdown'], [class*='prose']"))
    .filter(visible)
    .filter(n => !inUserBubble(n));
  if (!blocks.length) {
    blocks = Array.from(document.querySelectorAll("[data-message-author-role='assistant'], [data-role='assistant'], [class*='assistant'], article"))
      .filter(visible);
  }
  const values = [];
  for (const n of blocks) {
    const t = textOf(n);
    if (t && !values.includes(t)) values.push(t);
  }
  let busy = false;
  const stopBtn = document.querySelector(
    "[data-testid*='stop'], [data-testid*='Stop'], button[aria-label*='Stop'], button[aria-label*='停止'], " +
    "button[aria-label*='Cancel'], button[aria-label*='取消'], button[aria-label*='中断']"
  );
  if (stopBtn) {
    const s = getComputedStyle(stopBtn);
    const r = stopBtn.getBoundingClientRect();
    busy = s.visibility !== "hidden" && s.display !== "none" && r.width > 0 && r.height > 0;
  }
  if (!busy) {
    busy = !!document.querySelector(
      "[class*='thinking'], [class*='Thinking'], [class*='loading'], [class*='spinner'], [class*='Spinner'], " +
      "[role='progressbar'], [class*='generating'], [class*='Generating'], [aria-busy='true']"
    );
  }
  return { busy, last: values[values.length - 1] || "" };
})()
"#;
    let ws_url = codex_find_page_target(port)?;
    let mut client = CdpClient::connect(&ws_url)?;
    let deadline = Instant::now() + Duration::from_secs(240);
    let mut stable: Option<(String, Instant)> = None;
    loop {
        let value = cdp_evaluate(&mut client, CAPTURE_SCRIPT)?;
        let busy = value.get("busy").and_then(|v| v.as_bool()).unwrap_or(false);
        let last = json_string(&value, &["last"]);
        if !busy && !last.is_empty() {
            match &stable {
                Some((prev, since)) if *prev == last => {
                    // 连续两次读取一致且间隔 ≥3s，认为回复已完成。
                    if since.elapsed() >= Duration::from_secs(3) {
                        return Ok(last);
                    }
                }
                _ => stable = Some((last.clone(), Instant::now())),
            }
        } else {
            stable = None;
        }
        if Instant::now() > deadline {
            return Err("回捞超时：Codex 长时间未完成回复".into());
        }
        std::thread::sleep(Duration::from_secs(3));
    }
}

/// 解析 Codex 会话 jsonl，返回 (助手 final_answer 条数, 最后一条 final_answer 文本)。
/// 桌面 App 与 CLI 共享该文件，App 生成的每条最终回复都会实时追加进去，因此这是
/// 比抓取 GUI DOM 更可靠的回复来源。若该会话使用旧格式（消息不带 phase 字段），
/// 退化为统计全部助手消息。返回 None 表示定位不到会话文件。
fn codex_session_reply_state(thread_id: &str) -> Option<(usize, String)> {
    let relative = codex_session_relative_file(thread_id)?;
    let path = codex_session_path_from_relative(&relative);
    Some(codex_session_reply_state_at(&path))
}

/// 直接解析给定路径的会话 jsonl（避免轮询期间反复递归扫描 sessions 目录）。
fn codex_session_reply_state_at(path: &Path) -> (usize, String) {
    let content = fs::read_to_string(path).unwrap_or_default();
    let mut final_count = 0usize;
    let mut last_final = String::new();
    let mut assistant_count = 0usize;
    let mut last_assistant = String::new();
    let mut saw_phase = false;
    for line in content.lines() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        // 只认 response_item 里的 assistant 消息（type=message, role=assistant）。
        if value.pointer("/payload/type").and_then(|v| v.as_str()) != Some("message") {
            continue;
        }
        if value.pointer("/payload/role").and_then(|v| v.as_str()) != Some("assistant") {
            continue;
        }
        let text = value
            .pointer("/payload/content")
            .map(extract_text_from_content_value)
            .unwrap_or_default();
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        assistant_count += 1;
        last_assistant = text.to_string();
        if let Some(phase) = value.pointer("/payload/phase").and_then(|v| v.as_str()) {
            saw_phase = true;
            if phase == "final_answer" {
                final_count += 1;
                last_final = text.to_string();
            }
        }
    }
    if saw_phase {
        (final_count, last_final)
    } else {
        (assistant_count, last_assistant)
    }
}

/// 从共享会话文件里捕获「本次发送后新增的」助手最终回复。
/// baseline 是发送前的 final_answer 计数；轮询直到计数增长且文本稳定。
fn codex_capture_reply_from_session(thread_id: &str, baseline: usize) -> Result<String, String> {
    let relative = codex_session_relative_file(thread_id)
        .ok_or("未能定位该会话的存档文件")?;
    let path = codex_session_path_from_relative(&relative);
    let deadline = Instant::now() + Duration::from_secs(300);
    let mut stable: Option<(String, Instant)> = None;
    loop {
        let (count, last) = codex_session_reply_state_at(&path);
        if count > baseline && !last.is_empty() {
            match &stable {
                Some((prev, since)) if *prev == last => {
                    // 文本连续 3 秒不变，认为该轮回复已写完。
                    if since.elapsed() >= Duration::from_secs(3) {
                        return Ok(last);
                    }
                }
                _ => stable = Some((last.clone(), Instant::now())),
            }
        }
        if Instant::now() > deadline {
            return Err("等待会话文件出现新回复超时".into());
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}

/// 所有手机消息都必须进入用户选择的 Codex 对话（绝不改投其他会话）。
///
/// 首选把消息注入 Codex/ChatGPT 桌面 App（回复会实时显示在 App 界面里）。
/// 无论走哪条路，回复优先从「共享会话文件」读取——桌面 App 每条最终回复都会
/// 实时写进该 jsonl，比抓取 GUI DOM 可靠得多（DOM 抓取受调试连接超时影响）。
/// 失败时按性质兜底：
/// - 消息「确定没发出去」（探测/重启/连接/找输入框失败）→ 用后台 Codex CLI
///   `exec resume <thread_id>` 续接同一个会话重投（Happy/Paseo 式的 CLI 驱动）；
/// - 消息「可能已发出」（连接中断/回复未捕获）→ 绝不重发（防止 Codex 重复执行），
///   从会话文件捕获本轮新回复；文件不可用时再退回只读抓取 DOM。
fn dispatch_codex_reply(thread_id: &str, text: &str, cwd: &str) -> Result<String, String> {
    // 发送前记录会话文件里的 final_answer 基线，用于识别「本次新增」的回复。
    let baseline = codex_session_reply_state(thread_id)
        .map(|(count, _)| count)
        .unwrap_or(0);
    let failure = match send_prompt_to_codex_app(thread_id, text) {
        Ok(reply) => {
            log_info!(
                "[mobile-control][codex-app] message sent to selected Codex App thread: thread_id={}",
                thread_id
            );
            return Ok(reply);
        }
        Err(failure) => failure,
    };
    match failure {
        CodexAppSendFailure::NotSent(app_error) => {
            log_warn!(
                "[mobile-control][codex-app] 消息未能送入 App，改用后台 CLI 续接同一会话: thread_id={}, error={}",
                thread_id,
                app_error
            );
            match run_codex_cli_reply(text, cwd, thread_id) {
                Ok(reply) => {
                    log_info!(
                        "[mobile-control][codex-cli] reply produced via CLI resume: thread_id={}",
                        thread_id
                    );
                    Ok(reply)
                }
                Err(cli_error) => Err(format!(
                    "Codex App 注入失败（{app_error}）；后台 CLI 续接同一会话也失败（{cli_error}）"
                )),
            }
        }
        CodexAppSendFailure::MaybeSent { port, error } => {
            log_warn!(
                "[mobile-control][codex-app] 消息可能已送达但回复未捕获，改从会话文件捕获（不重发）: thread_id={}, error={}",
                thread_id,
                error
            );
            // 首选：从共享会话文件捕获本轮新回复（App 一定会把 final_answer 写进去）。
            match codex_capture_reply_from_session(thread_id, baseline) {
                Ok(reply) => {
                    log_info!(
                        "[mobile-control][codex-app] reply captured from session file: thread_id={}",
                        thread_id
                    );
                    return Ok(reply);
                }
                Err(session_error) => {
                    log_warn!(
                        "[mobile-control][codex-app] 会话文件捕获失败，改为只读抓取 DOM: thread_id={}, error={}",
                        thread_id,
                        session_error
                    );
                }
            }
            // 退路：直接从页面 DOM 只读抓取最新回复。
            match codex_capture_reply_readonly(port) {
                Ok(reply) => {
                    log_info!(
                        "[mobile-control][codex-app] reply captured via readonly poll: thread_id={}",
                        thread_id
                    );
                    Ok(reply)
                }
                Err(capture_error) => Err(format!(
                    "消息已发送到 Codex App，但未能带回回复（{error}；会话文件与页面抓取均失败：{capture_error}）；请在电脑上查看 Codex 的回答"
                )),
            }
        }
    }
}

fn lark_tenant_access_token(
    client: &reqwest::blocking::Client,
    binding: &MobileChannelBinding,
) -> Result<String, String> {
    // B16: 先查缓存，避免每条消息重新鉴权触发限频。飞书 token TTL=7200s，提前 60s 刷新
    let cache_key = format!("lark:{}", binding.app_id.trim());
    let now = (chrono_timestamp_millis() / 1000) as u64;
    let cache = LARK_TOKEN_CACHE.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    if let Ok(guard) = cache.lock() {
        if let Some((token, expiry)) = guard.get(&cache_key) {
            if now < *expiry {
                return Ok(token.clone());
            }
        }
    }
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
        // token 默认有效期 7200s，提前 60s 刷新
        let ttl = result
            .get("expire")
            .and_then(|v| v.as_u64())
            .unwrap_or(7200);
        let expiry = now + ttl.saturating_sub(60);
        if let Ok(mut guard) = cache.lock() {
            guard.insert(cache_key, (token.clone(), expiry));
        }
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
    // B16: 先查缓存，QQ token 有 expires_in 字段，提前 60s 刷新
    let cache_key = format!("qq:{}", binding.app_id.trim());
    let now = (chrono_timestamp_millis() / 1000) as u64;
    let cache = QQ_TOKEN_CACHE.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    if let Ok(guard) = cache.lock() {
        if let Some((token, expiry)) = guard.get(&cache_key) {
            if now < *expiry {
                return Ok(token.clone());
            }
        }
    }
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
        let ttl = result
            .get("expires_in")
            .and_then(|v| v.as_u64())
            .unwrap_or(1800);
        let expiry = now + ttl.saturating_sub(60);
        if let Ok(mut guard) = cache.lock() {
            guard.insert(cache_key, (token.clone(), expiry));
        }
        Ok(token)
    }
}

/// 在字符序列里从 `from` 起找到第一个目标字符的下标。
fn md_find_char(chars: &[char], from: usize, target: char) -> Option<usize> {
    (from..chars.len()).find(|&j| chars[j] == target)
}

/// 把一行里的行内 Markdown 语法清理成纯文本：
/// `[文字](链接)`/`![alt](链接)` → `文字 (链接)`；去掉 `**`、`__` 加粗标记与行内反引号。
fn strip_inline_markdown(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let is_img = c == '!' && i + 1 < chars.len() && chars[i + 1] == '[';
        if c == '[' || is_img {
            let bracket = if is_img { i + 1 } else { i };
            if let Some(close) = md_find_char(&chars, bracket + 1, ']') {
                if close + 1 < chars.len() && chars[close + 1] == '(' {
                    if let Some(rparen) = md_find_char(&chars, close + 2, ')') {
                        let text: String = chars[bracket + 1..close].iter().collect();
                        let url: String = chars[close + 2..rparen].iter().collect();
                        let text = text.trim();
                        let url = url.trim();
                        if url.is_empty() {
                            out.push_str(text);
                        } else if text.is_empty() {
                            out.push_str(url);
                        } else {
                            out.push_str(text);
                            out.push_str(" (");
                            out.push_str(url);
                            out.push(')');
                        }
                        i = rparen + 1;
                        continue;
                    }
                }
            }
        }
        out.push(c);
        i += 1;
    }
    out.replace("**", "").replace("__", "").replace('`', "")
}

/// 把 Codex 回复里的 Markdown 粗略转成整洁纯文本，供「不渲染 Markdown」的平台
/// （QQ 官方机器人只能发纯文本）使用，避免 `#`、`**`、代码围栏等符号原样显示。
/// 保守处理：只去装饰性符号，不改动正文与换行结构；代码围栏内的内容原样保留。
fn markdown_to_plaintext(md: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut in_fence = false;
    for raw in md.lines() {
        let trimmed_start = raw.trim_start();
        if trimmed_start.starts_with("```") || trimmed_start.starts_with("~~~") {
            // 代码围栏标记行本身丢弃，围栏内容原样保留。
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            lines.push(raw.to_string());
            continue;
        }
        let indent_len = raw.len() - raw.trim_start().len();
        let indent = &raw[..indent_len];
        let mut body = raw[indent_len..].to_string();
        // 标题：去掉行首连续的 # 号及其后空格。
        if body.starts_with('#') {
            body = body.trim_start_matches('#').trim_start().to_string();
        }
        // 引用块：去掉行首 > 。
        if body.starts_with('>') {
            body = body[1..].trim_start().to_string();
        }
        // 无序列表符号统一成 • （有序列表的数字保留）。
        let body = if body.starts_with("- ") || body.starts_with("* ") || body.starts_with("+ ") {
            format!("• {}", &body[2..])
        } else {
            body
        };
        lines.push(format!("{indent}{}", strip_inline_markdown(&body)));
    }
    // 折叠连续空行（>=2 压成 1），并去掉首尾空白。
    let mut result: Vec<String> = Vec::new();
    let mut prev_blank = false;
    for line in lines {
        let is_blank = line.trim().is_empty();
        if is_blank && prev_blank {
            continue;
        }
        prev_blank = is_blank;
        result.push(line);
    }
    result.join("\n").trim().to_string()
}

fn send_qq_text_reply(
    binding: &MobileChannelBinding,
    message: &serde_json::Value,
    content: &str,
) -> Result<(), String> {
    let client = build_http_client(20)?;
    let token = qq_access_token(&client, binding)?;
    // QQ 官方机器人只支持纯文本渲染，把 Markdown 清理成整洁纯文本再发，
    // 否则 #、**、代码围栏等符号会原样显示（用户反馈“没格式”）。
    let content = markdown_to_plaintext(content);
    let content = content.as_str();
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
    for (_index, chunk) in split_platform_reply_text(content, 1800, 5)
        .into_iter()
        .enumerate()
    {
        let mut payload = serde_json::json!({
            "content": chunk,
            "msg_type": 0,
            "msg_seq": QQ_MSG_SEQ.fetch_add(1, Ordering::SeqCst),
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
            channel,
            status,
            error
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
    // B15: 主动推送最新快照给前端，弹窗关闭后的断线/重连也能被看到，
    //      前端不必依赖短时轮询（listen("mobile-channel-status")）
    let _ = app.emit("mobile-channel-status", build_toolbox_snapshot(app));
}

fn handle_lark_bridge_message(
    app: &tauri::AppHandle,
    payload: &serde_json::Value,
) -> Result<(), String> {
    // B9: 串行锁，确保飞书消息按接收顺序处理完一条再处理下一条，防止 Codex 乱序响应
    let _guard = LARK_MSG_LOCK
        .lock()
        .map_err(|e| format!("获取飞书消息处理锁失败: {e}"))?;
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
    // B9: message_id 去重，防止飞书重投相同消息重复触发 Codex
    if check_and_record_msg_id(&LARK_SEEN_MSG_IDS, &message_id) {
        log_info!(
            "[mobile-control][lark] duplicate message_id={}, skipped",
            message_id
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
    // B7: 解绑后通道进程虽已被停止，但若 handler 仍被调用要在此拒绝，
    // 避免以 --ephemeral 在本机启动新 Codex 会话执行任意命令
    if !binding.enabled || binding.thread_id.trim().is_empty() {
        log_info!(
            "[mobile-control][lark] rejected: channel disabled or no thread bound (enabled={}, thread_id='{}')",
            binding.enabled, binding.thread_id
        );
        return Err("飞书通道未绑定会话，消息已忽略".into());
    }
    let selected_thread = state
        .synced_codex_threads
        .iter()
        .find(|thread| thread.id == binding.thread_id)
        .cloned();
    let cwd = selected_thread
        .as_ref()
        .map(|thread| thread.cwd.as_str())
        .unwrap_or("");
    let thread_id = selected_thread
        .as_ref()
        .map(|thread| thread.id.as_str())
        .unwrap_or("");
    // B7: 飞书同 QQ，防御绑定的会话不在 synced 列表时传空 thread_id 导致 --ephemeral 兜底
    if thread_id.is_empty() {
        log_info!(
            "[mobile-control][lark] rejected: bound thread not in synced list (binding.thread_id='{}')",
            binding.thread_id
        );
        return Err("飞书通道绑定的会话不存在，消息已忽略".into());
    }
    log_info!(
        "[mobile-control][lark] dispatching to codex: thread_id={}, thread_name={}, cwd={}",
        thread_id,
        selected_thread
            .as_ref()
            .map(|thread| thread.thread_name.as_str())
            .unwrap_or(""),
        cwd
    );
    update_channel_status(app, "lark", "收到飞书消息，正在发送给 Codex", "");
    let reply = match dispatch_codex_reply(thread_id, &text, cwd) {
        Ok(reply) => reply,
        Err(error) => {
            // B9: 执行失败也回传简短状态，手机端不会无响应且不泄露本机错误细节。
            let _ = send_lark_text_reply(
                &binding,
                &message_id,
                &chat_id,
                "Codex 当前未完成本次请求，请稍后重试。",
            );
            return Err(format!("Codex 执行失败：{error}"));
        }
    };
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
        binding.thread_id,
        binding.thread_name,
        binding.app_id
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
    let mut child = cmd.spawn().map_err(|e| {
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
            log_info!(
                "[mobile-control][lark][event] type={}",
                json_string(&payload, &["type"])
            ); // B5: 不记录完整 payload，避免用户聊天原文写进日志
            match json_string(&payload, &["type"]).as_str() {
                "ready" => {
                    update_channel_status(&app, "lark", "飞书智能体已在线，等待手机消息", "")
                }
                "message" => {
                    let message_app = app.clone();
                    std::thread::spawn(move || {
                        if let Err(error) = handle_lark_bridge_message(&message_app, &payload) {
                            update_channel_status(&message_app, "lark", "飞书消息处理失败", &error);
                        }
                    });
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
        // B10: 飞书连接断开时主动上报状态，UI 显示"已断开"而非继续显示"在线"
        update_channel_status(&app, "lark", "飞书连接已断开，看门狗将尝试重连", "");
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

// B7: selected_thread_for_message 已不再使用，因为它有危险的 selected_mobile_thread_id 兜底。
// 所有 handler (lark/qq/wechat) 现在都直接按 binding.thread_id 查找，找不到则拒绝。

fn handle_qq_gateway_message(
    app: &tauri::AppHandle,
    payload: &serde_json::Value,
) -> Result<(), String> {
    // B9: 串行锁，确保 QQ 消息按顺序处理
    let _guard = QQ_MSG_LOCK
        .lock()
        .map_err(|e| format!("获取 QQ 消息处理锁失败: {e}"))?;
    let text = json_string(payload, &["content", "text"]);
    if text.is_empty() {
        return Ok(());
    }
    // B9: QQ message_id/eventId 去重，防止网关重投相同消息重复触发 Codex
    let qq_msg_key = json_string(payload, &["messageId", "eventId", "id"]);
    if check_and_record_msg_id(&QQ_SEEN_MSG_IDS, &qq_msg_key) {
        return Ok(());
    }
    let state = read_toolbox_state(app);
    let binding = state
        .mobile_channels
        .iter()
        .find(|binding| binding.channel == "qq")
        .cloned()
        .ok_or("QQ 通道不存在")?;
    // B7: 未绑定会话时拒绝执行，避免 --ephemeral 任意起新会话
    if !binding.enabled || binding.thread_id.trim().is_empty() {
        return Err("QQ 通道未绑定会话，消息已忽略".into());
    }
    let selected_thread = state
        .synced_codex_threads
        .iter()
        .find(|thread| thread.id == binding.thread_id)
        .cloned();
    let cwd = selected_thread
        .as_ref()
        .map(|thread| thread.cwd.clone())
        .unwrap_or_default();
    let thread_id = selected_thread
        .as_ref()
        .map(|thread| thread.id.as_str())
        .unwrap_or("");
    if thread_id.is_empty() {
        return Err("QQ 通道绑定的会话不存在，消息已忽略".into());
    }
    update_channel_status(app, "qq", "收到 QQ 消息，正在发送给 Codex", "");
    let reply = match dispatch_codex_reply(thread_id, &text, &cwd) {
        Ok(reply) => reply,
        Err(error) => {
            // B9: 失败时给 QQ 发送可见回执，避免用户只看到静默超时。
            let _ = send_qq_text_reply(&binding, payload, "Codex 当前未完成本次请求，请稍后重试。");
            return Err(format!("Codex 执行失败：{error}"));
        }
    };
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
    let mut child = cmd.spawn().map_err(|e| {
        QQ_GATEWAY_ACTIVE.store(false, Ordering::SeqCst);
        format!("启动 QQ 网关失败: {e}")
    })?;
    let stdout = child.stdout.take().ok_or("QQ 网关没有输出通道")?;
    // B1: 消费 stderr，避免 Node 进程 stderr 缓冲区满后 write 阻塞（静默假死）
    let stderr_opt = child.stderr.take();
    if let Ok(mut guard) = QQ_GATEWAY_CHILD.lock() {
        *guard = Some(child);
    }
    if let Some(stderr) = stderr_opt {
        std::thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().flatten() {
                log_info!("[mobile-control][qq][stderr] {}", line);
            }
        });
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
                    let message_app = app.clone();
                    std::thread::spawn(move || {
                        if let Err(error) = handle_qq_gateway_message(&message_app, &payload) {
                            update_channel_status(&message_app, "qq", "QQ 消息处理失败", &error);
                        }
                    });
                }
                "failure" => {
                    let error = json_string(&payload, &["message"]);
                    update_channel_status(&app, "qq", "QQ 网关启动失败", &error);
                }
                _ => {}
            }
        }
        // B10: QQ 网关断开时主动上报，UI 停留在"在线"而非正确显示"已断开"
        update_channel_status(&app, "qq", "QQ 连接已断开，看门狗将尝试重连", "");
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
    // B9: 串行锁，确保微信消息按顺序处理
    let _guard = WECHAT_MSG_LOCK
        .lock()
        .map_err(|e| format!("获取微信消息处理锁失败: {e}"))?;
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
    // B7: 未绑定会话时拒绝执行，避免以 --ephemeral 任意起新 Codex 会话
    if !binding.enabled || binding.thread_id.trim().is_empty() {
        return Err("微信通道未绑定会话，消息已忽略".into());
    }
    let selected_thread = state
        .synced_codex_threads
        .iter()
        .find(|thread| thread.id == binding.thread_id)
        .cloned();
    let cwd = selected_thread
        .as_ref()
        .map(|thread| thread.cwd.clone())
        .unwrap_or_default();
    let thread_id = selected_thread
        .as_ref()
        .map(|thread| thread.id.as_str())
        .unwrap_or("");
    // B7: 微信同 QQ，防御绑定的会话不在 synced 列表时传空 thread_id 导致 --ephemeral 兜底
    if thread_id.is_empty() {
        return Err("微信通道绑定的会话不存在，消息已忽略".into());
    }
    update_channel_status(app, "wechat", "收到微信消息，正在发送给 Codex", "");
    let reply = match dispatch_codex_reply(thread_id, &text, &cwd) {
        Ok(reply) => reply,
        Err(error) => {
            // B9: 失败时给微信发送可见回执，避免用户只看到静默超时。
            let _ =
                send_wechat_text_reply(&binding, payload, "Codex 当前未完成本次请求，请稍后重试。");
            return Err(format!("Codex 执行失败：{error}"));
        }
    };
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
                    update_channel_status(&app, "wechat", "微信监听启动失败", &err_msg);
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
                    let lower = error.to_lowercase();
                    // B12: token 过期不再无限重试，立即停止并通知用户重新绑定
                    if lower.contains("session timeout")
                        || lower.contains("expired")
                        || lower.contains("token")
                    {
                        update_channel_status(
                            &app,
                            "wechat",
                            "微信 token 已过期，请清除绑定后重新扫码",
                            &error,
                        );
                        break;
                    }
                    update_channel_status(&app, "wechat", "微信消息拉取失败", &error);
                    std::thread::sleep(Duration::from_secs(3));
                    continue;
                }
            };
            cursor = wechat_next_update_cursor(&result, &cursor);
            let items = wechat_update_items(&result);
            // B12: 空结果时不热循环，休眠 1 秒降低 CPU 和 API 调用频率
            if items.is_empty() {
                std::thread::sleep(Duration::from_secs(1));
            }
            for item in items {
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
                let message_app = app.clone();
                std::thread::spawn(move || {
                    if let Err(error) = handle_wechat_bot_message(&message_app, &message) {
                        update_channel_status(&message_app, "wechat", "微信消息处理失败", &error);
                    }
                });
            }
        }
        // B10: 连接断开时上报状态，让 UI 显示"已断开"而非继续显示旧状态
        // B17: 不再在此调 notifystop，stop_wechat_listener 是唯一的 notifystop 调用点，
        //      避免 stop 后 start 线程再次 notifystop 产生双重调用
        update_channel_status(&app, "wechat", "微信连接已断开，看门狗将尝试重连", "");
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
    let mut state = read_toolbox_state(app);
    let _ = refresh_smart_control_status_for_state(&mut state);
    let config_path = codex_config_path();
    let config_text = fs::read_to_string(&config_path).unwrap_or_default();
    let current_source = state.plugin_marketplace_input.clone();
    let visible_threads = visible_codex_threads(&state);
    // B5: 对发给前端的凭据字段掩码，避免 secret 通过 snapshot 暴露到 devtools/DOM
    let masked_channels: Vec<MobileChannelBinding> = state
        .mobile_channels
        .iter()
        .cloned()
        .map(|mut ch| {
            // 用布尔语义标记（非空 → "*"），前端判断 hasAppSecret 即可，无需明文
            if !ch.app_secret.is_empty() {
                ch.app_secret = "*".to_string();
            }
            if !ch.bot_token.is_empty() {
                ch.bot_token = "*".to_string();
            }
            ch
        })
        .collect();
    ToolboxSnapshot {
        plugin_marketplace_input: current_source.clone(),
        plugin_marketplaces: list_plugin_marketplaces(&config_text, &current_source),
        builtin_plugins: list_codex_builtin_plugins(&config_text),
        session_bindings: state.session_bindings.clone(),
        codex_threads: Vec::new(),
        synced_codex_threads: visible_threads.clone(),
        trashed_codex_threads: state.trashed_codex_threads.clone(),
        session_sync: CodexSessionSyncState {
            last_synced_at: state.session_sync.last_synced_at.clone(),
            total: visible_threads.len(),
        },
        mobile_channels: masked_channels,
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
        id: "cursor",
        display_name: "Cursor",
        win_appdata_dir: "Cursor",
        win_program_dirs: &["Cursor"],
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
        id: "cursor",
        display_name: "Cursor",
        mac_app_support_dir: "Cursor",
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
        id: "cursor",
        display_name: "Cursor",
        linux_config_dir: "Cursor",
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

/// 在 ~/.claude.json 中把 hasCompletedOnboarding 标记为 true。
///
/// Claude Code CLI 与 IDE 插件用这个字段判断是否需要展示首次使用引导。
/// 切换 API Key / Base URL 之后如果它不是 true，用户下次打开就会被引导页拦住，
/// 所以每次切换 Claude 配置时都顺带补齐。已经是 true 时不重写文件。
fn mark_claude_onboarding_completed() -> Result<(), String> {
    let path = claude_mcp_path(); // ~/.claude.json

    let mut config = if path.exists() {
        read_json(&path).map_err(|e| format!("读取 ~/.claude.json 失败: {e}"))?
    } else {
        serde_json::json!({})
    };
    if !config.is_object() {
        config = serde_json::json!({});
    }

    if config
        .get("hasCompletedOnboarding")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return Ok(());
    }

    config["hasCompletedOnboarding"] = serde_json::json!(true);
    write_json(&path, &config).map_err(|e| format!("写入 ~/.claude.json 失败: {e}"))?;
    log_info!("[claude] 已将 ~/.claude.json 的 hasCompletedOnboarding 置为 true");
    Ok(())
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
            total: 6,
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

fn models_endpoint_candidates(base_url: &str) -> Result<Vec<String>, String> {
    let normalized = normalize_endpoint_url(base_url)?;
    if normalized.ends_with("/models") {
        return Ok(vec![normalized]);
    }
    if normalized.ends_with("/v1") {
        return Ok(vec![format!("{normalized}/models")]);
    }
    Ok(vec![
        format!("{normalized}/v1/models"),
        format!("{normalized}/models"),
    ])
}

fn extract_model_ids(value: &serde_json::Value) -> Vec<String> {
    let data = value
        .get("data")
        .and_then(|data| data.as_array())
        .or_else(|| value.get("models").and_then(|models| models.as_array()));

    let mut ids = Vec::new();
    let mut seen = HashSet::new();
    if let Some(items) = data {
        for item in items {
            let id = item
                .as_str()
                .or_else(|| item.get("id").and_then(|id| id.as_str()))
                .or_else(|| item.get("name").and_then(|name| name.as_str()))
                .unwrap_or("")
                .trim();
            if !id.is_empty() && seen.insert(id.to_string()) {
                ids.push(id.to_string());
            }
        }
    }
    ids.sort();
    ids
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
fn fetch_available_models(
    base_url: String,
    api_key: String,
    timeout_secs: Option<u64>,
    protocol: Option<String>,
) -> Result<Vec<AvailableModel>, String> {
    let api_key = api_key.trim();
    if api_key.is_empty() {
        return Err("API Key 不能为空".into());
    }

    let timeout = Duration::from_secs(sanitize_endpoint_timeout(timeout_secs));
    let client = reqwest::blocking::Client::builder()
        .user_agent("VarSwitch model fetch")
        .timeout(timeout)
        .build()
        .map_err(|e| e.to_string())?;

    let is_gemini = protocol.as_deref() == Some("gemini");
    let urls = if is_gemini {
        let normalized = normalize_endpoint_url(&base_url)?;
        if normalized.ends_with("/models") {
            vec![normalized]
        } else if normalized.ends_with("/v1beta") {
            vec![format!("{normalized}/models")]
        } else {
            vec![format!("{normalized}/v1beta/models")]
        }
    } else {
        models_endpoint_candidates(&base_url)?
    };

    let is_anthropic = matches!(protocol.as_deref(), Some("claude") | Some("anthropic"));
    let mut last_error = String::new();
    for url in urls {
        let mut request = client
            .get(&url)
            .header("Accept", "application/json")
            .timeout(timeout);
        request = if is_gemini {
            request.header("x-goog-api-key", api_key)
        } else if is_anthropic {
            // Anthropic 官方要求 x-api-key + anthropic-version；
            // 同时附带 Bearer 以兼容仅支持 OpenAI 风格鉴权的中转网关。
            request
                .header("x-api-key", api_key)
                .header("anthropic-version", "2023-06-01")
                .bearer_auth(api_key)
        } else {
            request.bearer_auth(api_key)
        };
        match request.send() {
            Ok(resp) => {
                let status = resp.status();
                let body = resp.text().map_err(|e| e.to_string())?;
                if !status.is_success() {
                    last_error = format!("{url} 返回 HTTP {}", status.as_u16());
                    continue;
                }
                let value: serde_json::Value = serde_json::from_str(&body)
                    .map_err(|e| format!("模型列表 JSON 解析失败: {e}"))?;
                let models: Vec<AvailableModel> = extract_model_ids(&value)
                    .into_iter()
                    .map(|id| AvailableModel {
                        id: if is_gemini {
                            id.strip_prefix("models/").unwrap_or(&id).to_string()
                        } else {
                            id
                        },
                    })
                    .collect();
                if models.is_empty() {
                    last_error = format!("{url} 没有返回可识别的模型 ID");
                    continue;
                }
                return Ok(models);
            }
            Err(e) => {
                last_error = if e.is_timeout() {
                    format!("{url} 请求超时")
                } else if e.is_connect() {
                    format!("{url} 连接失败")
                } else {
                    format!("{url} 请求失败: {e}")
                };
            }
        }
    }

    Err(if last_error.is_empty() {
        "未能获取模型列表".into()
    } else {
        last_error
    })
}

#[tauri::command]
fn add_profile(
    app: tauri::AppHandle,
    name: String,
    api_key: String,
    base_url: String,
    model_id: Option<String>,
    api_format: Option<String>,
) -> Result<Profile, String> {
    if name.trim().is_empty() || api_key.trim().is_empty() {
        return Err("配置名称和 API Key 都必须填写".into());
    }
    let api_format = normalize_claude_api_format(&api_format.unwrap_or_default());
    if api_format == "openai_chat" && base_url.trim().is_empty() {
        return Err("OpenAI 格式必须填写上游 Base URL".into());
    }
    let mut data = read_profiles(&app);
    let profile = Profile {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.trim().to_string(),
        api_key: api_key.trim().to_string(),
        base_url: resolve_base_url_or_default(&base_url, DEFAULT_ANTHROPIC_BASE_URL),
        model_id: model_id.unwrap_or_default().trim().to_string(),
        api_format,
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
    api_format: Option<String>,
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
    p.base_url = resolve_base_url_or_default(&base_url, DEFAULT_ANTHROPIC_BASE_URL);
    p.model_id = model_id.map(|m| m.trim().to_string()).unwrap_or_default();
    p.api_format = normalize_claude_api_format(&api_format.unwrap_or_default());
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ClaudeProxyStatus {
    running: bool,
    port: u16,
    upstream_base_url: String,
}

#[tauri::command]
fn get_claude_proxy_status() -> ClaudeProxyStatus {
    ClaudeProxyStatus {
        running: claude_proxy::is_running(),
        port: claude_proxy::CLAUDE_PROXY_PORT,
        upstream_base_url: claude_proxy::current_upstream()
            .map(|u| u.base_url)
            .unwrap_or_default(),
    }
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
// 切换配置前自动快照各应用的 profiles 文件，误操作可回滚。

/// 返回给前端的备份信息。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConfigBackupInfo {
    name: String,  // 备份文件名
    kind: String,  // "claude" | "codex" | "grok" | "gemini"
    stamp: String, // 紧凑时间戳，如 20260624-143025（前端格式化展示）
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CodexConfigDiagnostics {
    config_path: String,
    auth_path: String,
    config_exists: bool,
    auth_exists: bool,
    has_model_provider: bool,
    has_model: bool,
    has_base_url: bool,
    has_api_key: bool,
    provider_name: String,
    model: String,
    base_url: String,
    auth_mode: String,
    active_profile_name: String,
    plugin_marketplaces: Vec<String>,
    issues: Vec<String>,
    suggestions: Vec<String>,
    last_checked_at: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CodexConfigBackupResult {
    config_backup: Option<String>,
    auth_backup: Option<String>,
    created_at: String,
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

fn backup_one_file_with_ext(
    dir: &PathBuf,
    src: &PathBuf,
    prefix: &str,
    stamp: &str,
    ext: &str,
) -> Option<String> {
    if !src.exists() {
        return None;
    }
    let safe_ext = ext.trim_start_matches('.');
    let dst = dir.join(format!("{prefix}-{stamp}.{safe_ext}"));
    match fs::copy(src, &dst) {
        Ok(_) => Some(dst.to_string_lossy().to_string()),
        Err(err) => {
            log_warn!("[backup] 备份文件失败 src={} err={err}", src.display());
            None
        }
    }
}

fn backup_codex_runtime_files(app: &tauri::AppHandle) -> CodexConfigBackupResult {
    let dir = backups_dir(app).join("codex-runtime");
    let _ = fs::create_dir_all(&dir);
    let stamp = format_compact_time(chrono_timestamp_millis());
    let config_backup =
        backup_one_file_with_ext(&dir, &codex_config_path(), "config", &stamp, "toml");
    let auth_backup = backup_one_file_with_ext(&dir, &codex_auth_path(), "auth", &stamp, "json");
    CodexConfigBackupResult {
        config_backup,
        auth_backup,
        created_at: chrono_now(),
    }
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
                .map(|n| n.starts_with(&format!("{prefix}-")))
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
    backup_one_config(&dir, &grok_profiles_path(app), "grok", &stamp);
    backup_one_config(&dir, &gemini_profiles_path(app), "gemini", &stamp);
    let runtime_backup = backup_codex_runtime_files(app);
    // 同时备份 Grok CLI 运行时配置 ~/.grok/config.toml
    let grok_runtime_dir = dir.join("grok-runtime");
    let _ = fs::create_dir_all(&grok_runtime_dir);
    let grok_config_backup = backup_one_file_with_ext(
        &grok_runtime_dir,
        &grok_config_path(),
        "config",
        &stamp,
        "toml",
    );
    prune_backups(&dir, "profiles", 20);
    prune_backups(&dir, "codex", 20);
    prune_backups(&dir, "grok", 20);
    prune_backups(&dir, "gemini", 20);
    let runtime_dir = dir.join("codex-runtime");
    prune_backups(&runtime_dir, "config", 20);
    prune_backups(&runtime_dir, "auth", 20);
    prune_backups(&grok_runtime_dir, "config", 20);
    log_info!(
        "[backup] 已自动备份配置 stamp={stamp} codex_config={:?} codex_auth={:?} grok_config={:?}",
        runtime_backup.config_backup,
        runtime_backup.auth_backup,
        grok_config_backup
    );
}

fn toml_line_value(config: &str, key: &str) -> String {
    let prefix = format!("{key} =");
    config
        .lines()
        .find(|line| line.trim().starts_with(&prefix))
        .and_then(|line| line.split_once('=').map(|(_, value)| value))
        .map(|value| value.trim().trim_matches('"').to_string())
        .unwrap_or_default()
}

fn detect_codex_plugin_marketplaces(config: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_marketplace = false;
    let mut current = String::new();
    for line in config.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("[marketplaces.") {
            in_marketplace = true;
            current.clear();
            continue;
        }
        if trimmed.starts_with('[') {
            if in_marketplace && !current.is_empty() {
                out.push(current.clone());
            }
            in_marketplace = false;
            current.clear();
            continue;
        }
        if in_marketplace && trimmed.starts_with("source") {
            if let Some((_, value)) = trimmed.split_once('=') {
                current = value.trim().trim_matches('"').to_string();
            }
        }
    }
    if in_marketplace && !current.is_empty() {
        out.push(current);
    }
    out.sort();
    out.dedup();
    out
}

fn read_codex_config_diagnostics(app: &tauri::AppHandle) -> CodexConfigDiagnostics {
    let config_path = codex_config_path();
    let auth_path = codex_auth_path();
    let config = fs::read_to_string(&config_path).unwrap_or_default();
    let status = read_codex_status().unwrap_or(LocationStatus {
        api_key: String::new(),
        base_url: String::new(),
        image_api_key: String::new(),
        image_base_url: String::new(),
        image_skill_installed: false,
    });
    let model_provider = toml_line_value(&config, "model_provider");
    let model = toml_line_value(&config, "model");
    let provider_name = if model_provider.is_empty() {
        toml_line_value(&config, "name")
    } else {
        model_provider.clone()
    };
    let auth_mode = if config
        .lines()
        .any(|l| l.trim().starts_with("experimental_bearer_token"))
    {
        "official_account_api_quota".to_string()
    } else if auth_path.exists() {
        "auth_json".to_string()
    } else {
        "unknown".to_string()
    };
    let active_profile_name = read_codex_profiles(app)
        .profiles
        .into_iter()
        .find(|profile| profile.is_active)
        .map(|profile| profile.name)
        .unwrap_or_default();
    let plugin_marketplaces = detect_codex_plugin_marketplaces(&config);
    let config_exists = config_path.exists();
    let auth_exists = auth_path.exists();
    let has_model_provider = !model_provider.is_empty() || config.contains("[model_providers.");
    let has_model = !model.is_empty();
    let has_base_url = !status.base_url.is_empty();
    let has_api_key = !status.api_key.is_empty();
    let mut issues = Vec::new();
    let mut suggestions = Vec::new();
    if !config_exists {
        issues.push("未找到 ~/.codex/config.toml".into());
        suggestions.push("在 Codex CLI 页面添加并切换一个配置".into());
    }
    if !has_model_provider {
        issues.push("config.toml 缺少 model_provider 或 model_providers 配置".into());
        suggestions.push("重新切换当前 Codex 配置，让 VarSwitch 写入完整 provider".into());
    }
    if !has_model {
        issues.push("config.toml 缺少 model 字段".into());
        suggestions.push("在 Codex 配置中填写模型名后重新保存".into());
    }
    if !has_base_url {
        issues.push("未检测到 Codex Base URL".into());
        suggestions.push("检查 provider 的 base_url 是否存在".into());
    }
    if !has_api_key {
        issues.push("未检测到 Codex API Key".into());
        suggestions.push(
            "检查 auth.json 的 OPENAI_API_KEY 或 config.toml 的 experimental_bearer_token".into(),
        );
    }
    if auth_mode == "auth_json" && !auth_exists {
        issues.push("当前看起来需要 auth.json，但文件不存在".into());
    }
    if !status.image_api_key.is_empty() && !status.image_skill_installed {
        issues.push("图片 API 已配置，但 Codex 图片生成 Skill 尚未安装".into());
        suggestions.push("重新同步当前 Codex 配置，然后重启 Codex".into());
    }
    if plugin_marketplaces.is_empty() {
        suggestions.push("可在 Toolbox 安装 Codex 插件市场".into());
    }
    issues.sort();
    issues.dedup();
    suggestions.sort();
    suggestions.dedup();
    CodexConfigDiagnostics {
        config_path: config_path.to_string_lossy().to_string(),
        auth_path: auth_path.to_string_lossy().to_string(),
        config_exists,
        auth_exists,
        has_model_provider,
        has_model,
        has_base_url,
        has_api_key,
        provider_name,
        model,
        base_url: status.base_url,
        auth_mode,
        active_profile_name,
        plugin_marketplaces,
        issues,
        suggestions,
        last_checked_at: chrono_now(),
    }
}

#[tauri::command]
fn get_codex_diagnostics(app: tauri::AppHandle) -> CodexConfigDiagnostics {
    read_codex_config_diagnostics(&app)
}

#[tauri::command]
fn backup_codex_runtime(app: tauri::AppHandle) -> CodexConfigBackupResult {
    backup_codex_runtime_files(&app)
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
        } else if let Some(rest) = fname.strip_prefix("grok-") {
            ("grok", rest.trim_end_matches(".json"))
        } else if let Some(rest) = fname.strip_prefix("gemini-") {
            ("gemini", rest.trim_end_matches(".json"))
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
    } else if name.starts_with("grok-") {
        grok_profiles_path(&app)
    } else if name.starts_with("gemini-") {
        gemini_profiles_path(&app)
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

    let mut errors: Vec<String> = Vec::new();
    let mut details = SwitchDetails {
        env_vars: false,
        editors: HashMap::new(),
        claude: false,
    };

    emit_switch_progress(&app, 1, "prepare");

    // 切换前只备份配置文件；会话同步不属于切换热路径。
    auto_backup_configs(&app);

    // OpenAI 格式的配置经本地代理转换协议：环境变量指向 127.0.0.1，
    // 真实上游（base_url / key / 模型）写入代理状态。代理起不来时中止切换，
    // 避免写入一个连不上的地址。
    let effective_base_url = if profile.api_format == "openai_chat" {
        claude_proxy::set_upstream(Some(claude_proxy::ProxyUpstream {
            base_url: profile.base_url.clone(),
            api_key: profile.api_key.clone(),
            model: profile.model_id.clone(),
        }));
        claude_proxy::ensure_server()?;
        log_info!(
            "[claude-proxy] 配置 {} 走本地代理，转发至 {}",
            profile.name,
            profile.base_url
        );
        claude_proxy::local_base_url()
    } else {
        // 直连配置生效后清空上游，代理拒绝后续请求，避免旧配置被误用
        claude_proxy::set_upstream(None);
        profile.base_url.clone()
    };

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
    match apply_auth_to_system_env(&profile.api_key, &effective_base_url) {
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
            apply_auth_to_env_array(arr, &profile.api_key, &effective_base_url);
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
        apply_auth_to_env_object(env, &profile.api_key, &effective_base_url);
    }
    // 处理 model: 仅当 profile.model_id 非空时才写入，逻辑与编辑器一致
    if !profile.model_id.is_empty() {
        settings["model"] = serde_json::json!(profile.model_id);
    }
    match write_json(&cp, &settings) {
        Ok(_) => details.claude = true,
        Err(e) => errors.push(format!("Claude: {}", e)),
    }

    // 切换配置后顺带标记引导已完成，避免 Claude Code CLI / IDE 插件
    // 每次换 Key 都重新弹一遍 onboarding 向导。
    if let Err(e) = mark_claude_onboarding_completed() {
        errors.push(format!("Claude onboarding: {}", e));
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

fn claude_model_from_settings(settings: &serde_json::Value) -> String {
    settings
        .get("model")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .trim()
        .to_string()
}

#[tauri::command]
fn get_status(app: tauri::AppHandle) -> StatusResult {
    let settings = read_app_settings(&app);
    let env_vars = Some(LocationStatus {
        api_key: read_auth_from_system_env(),
        base_url: reg_get_env(BASE_URL_ENV),
        image_api_key: String::new(),
        image_base_url: String::new(),
        image_skill_installed: false,
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
                image_api_key: String::new(),
                image_base_url: String::new(),
                image_skill_installed: false,
            })
        })() {
            editors.insert(editor.id.to_string(), status);
        }
    }

    let claude_settings = read_json(&claude_settings_path()).ok();
    let claude_model = claude_settings
        .as_ref()
        .map(claude_model_from_settings)
        .unwrap_or_default();
    let claude = claude_settings.as_ref().map(|s| {
        let env = s.get("env").and_then(|v| v.as_object());
        LocationStatus {
            api_key: env.map(read_auth_from_env_object).unwrap_or_default(),
            base_url: env
                .and_then(|e| e.get(BASE_URL_ENV))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            image_api_key: String::new(),
            image_base_url: String::new(),
            image_skill_installed: false,
        }
    });

    StatusResult {
        env_vars,
        editors,
        claude,
        claude_model,
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
        // 导入的是本机直连环境变量，一律按 Anthropic 直连处理
        api_format: default_claude_api_format(),
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
    wire_api: Option<String>,
    model: Option<String>,
    provider_name: Option<String>,
    image_api_key: Option<String>,
    image_base_url: Option<String>,
) -> Result<CodexProfile, String> {
    if name.trim().is_empty() || api_key.trim().is_empty() {
        return Err("配置名称和 API Key 都必须填写".into());
    }
    let mut data = read_codex_profiles(&app);
    let profile = CodexProfile {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.trim().to_string(),
        api_key: api_key.trim().to_string(),
        base_url: resolve_base_url_or_default(&base_url, DEFAULT_OPENAI_BASE_URL),
        auth_mode: auth_mode.unwrap_or_else(default_codex_auth_mode),
        wire_api: normalize_codex_wire_api(&wire_api.unwrap_or_default()),
        model: model.unwrap_or_default().trim().to_string(),
        provider_name: provider_name.unwrap_or_default().trim().to_string(),
        image_api_key: image_api_key.unwrap_or_default().trim().to_string(),
        image_base_url: image_base_url
            .unwrap_or_default()
            .trim()
            .trim_end_matches('/')
            .to_string(),
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
    wire_api: Option<String>,
    model: Option<String>,
    provider_name: Option<String>,
    image_api_key: Option<String>,
    image_base_url: Option<String>,
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
    p.base_url = resolve_base_url_or_default(&base_url, DEFAULT_OPENAI_BASE_URL);
    if let Some(mode) = auth_mode {
        if !mode.trim().is_empty() {
            p.auth_mode = mode.trim().to_string();
        }
    }
    if let Some(wire) = wire_api {
        p.wire_api = normalize_codex_wire_api(&wire);
    }
    p.model = model.unwrap_or_default().trim().to_string();
    p.provider_name = provider_name.unwrap_or_default().trim().to_string();
    p.image_api_key = image_api_key.unwrap_or_default().trim().to_string();
    p.image_base_url = image_base_url
        .unwrap_or_default()
        .trim()
        .trim_end_matches('/')
        .to_string();
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
    configure_codex_image_skill(&profile)?;

    for p in data.profiles.iter_mut() {
        p.is_active = p.id == profile.id;
    }
    write_codex_profiles(&app, &data)?;
    Ok(())
}

fn build_imported_codex_profile(
    name: String,
    status: LocationStatus,
    auth_mode: String,
) -> CodexProfile {
    CodexProfile {
        id: uuid::Uuid::new_v4().to_string(),
        name: if name.is_empty() {
            "导入的 Codex 配置".into()
        } else {
            name
        },
        api_key: status.api_key,
        base_url: status.base_url,
        auth_mode,
        wire_api: default_codex_wire_api(),
        model: String::new(),
        provider_name: String::new(),
        image_api_key: status.image_api_key,
        image_base_url: status.image_base_url,
        is_active: true,
        created_at: chrono_now(),
    }
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

    let profile = build_imported_codex_profile(name, status, auth_mode);

    configure_codex_image_skill(&profile)?;

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

// ── Grok Profile Commands ───────────────────────────

#[tauri::command]
fn get_grok_profiles(app: tauri::AppHandle) -> GrokProfilesData {
    read_grok_profiles(&app)
}

#[tauri::command]
fn add_grok_profile(
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
fn update_grok_profile(
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
fn delete_grok_profile(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let mut data = read_grok_profiles(&app);
    data.profiles.retain(|x| x.id != id);
    write_grok_profiles(&app, &data)
}

#[tauri::command]
fn switch_grok_profile(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let mut data = read_grok_profiles(&app);
    let profile = data
        .profiles
        .iter()
        .find(|x| x.id == id)
        .ok_or("配置未找到")?
        .clone();

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
fn import_grok_current(app: tauri::AppHandle, name: String) -> Result<GrokProfile, String> {
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
fn get_grok_status() -> Option<GrokRuntimeStatus> {
    let status = read_grok_runtime_status();
    if status.api_key.is_empty() && !status.config_exists {
        return None;
    }
    Some(status)
}

#[tauri::command]
fn get_grok_diagnostics(app: tauri::AppHandle) -> GrokConfigDiagnostics {
    read_grok_config_diagnostics(&app)
}

#[tauri::command]
fn backup_grok_runtime(app: tauri::AppHandle) -> Result<String, String> {
    let dir = backups_dir(&app).join("grok-runtime");
    fs::create_dir_all(&dir).map_err(|e| format!("创建备份目录失败: {e}"))?;
    let stamp = format_compact_time(chrono_timestamp_millis());
    match backup_one_file_with_ext(&dir, &grok_config_path(), "config", &stamp, "toml") {
        Some(path) => Ok(path),
        None => Err("没有可备份的 ~/.grok/config.toml".into()),
    }
}

#[tauri::command]
fn open_grok_config_folder() -> Result<(), String> {
    let dir = grok_config_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("创建 ~/.grok 失败: {e}"))?;
    open_folder(dir.to_string_lossy().to_string())
}

// ── Gemini Profile Commands ─────────────────────────

#[tauri::command]
fn get_gemini_profiles(app: tauri::AppHandle) -> GeminiProfilesData {
    read_gemini_profiles(&app)
}

#[tauri::command]
fn add_gemini_profile(
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
fn update_gemini_profile(
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
fn delete_gemini_profile(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let mut data = read_gemini_profiles(&app);
    data.profiles.retain(|profile| profile.id != id);
    write_gemini_profiles(&app, &data)
}

#[tauri::command]
fn switch_gemini_profile(app: tauri::AppHandle, id: String) -> Result<(), String> {
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
fn import_gemini_current(app: tauri::AppHandle, name: String) -> Result<GeminiProfile, String> {
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
fn get_gemini_status() -> Option<GeminiRuntimeStatus> {
    let status = read_gemini_runtime_status();
    if status.api_key.is_empty() && !status.settings_exists {
        None
    } else {
        Some(status)
    }
}

#[tauri::command]
fn get_codex_toolbox(app: tauri::AppHandle) -> Result<ToolboxSnapshot, String> {
    Ok(build_toolbox_snapshot(&app))
}

#[tauri::command]
fn start_smart_control(app: tauri::AppHandle) -> Result<ToolboxSnapshot, String> {
    let state = read_toolbox_state(&app);
    start_smart_control_server(
        app.clone(),
        state.mobile_remote.remote_control_backend_url.clone(),
    );
    std::thread::sleep(Duration::from_millis(120));
    Ok(build_toolbox_snapshot(&app))
}

fn emit_update_download_progress(app: &tauri::AppHandle, step: u32, total: u32, label: &str) {
    let _ = app.emit(
        "update-download-progress",
        ProgressEvent {
            step,
            total,
            label: label.into(),
        },
    );
}

#[tauri::command]
fn get_smart_control_debug() -> SmartControlDebugSnapshot {
    smart_control_debug_snapshot()
}

#[tauri::command]
fn submit_smart_control_approval(request_id: String, decision: String) -> Result<(), String> {
    let request_id = request_id.trim().to_string();
    if request_id.is_empty() {
        return Err("审批请求 ID 为空".into());
    }
    let id_value = request_id
        .parse::<u64>()
        .map(serde_json::Value::from)
        .unwrap_or_else(|_| serde_json::Value::String(request_id.clone()));
    let message = serde_json::json!({
        "id": id_value,
        "result": {
            "decision": decision.trim(),
            "approved": matches!(decision.trim().to_ascii_lowercase().as_str(), "approve" | "approved" | "allow" | "yes" | "accept"),
        }
    });
    let envelope = smart_control_wrap_client_message(message);
    smart_control_send_json(&envelope)?;
    if let Ok(mut approvals) = smart_control_approval_store().lock() {
        approvals.retain(|item| item.request_id != request_id);
    }
    Ok(())
}

#[tauri::command]
fn repair_openai_bundled_plugins(app: tauri::AppHandle) -> Result<ToolboxSnapshot, String> {
    auto_backup_configs(&app);
    let config_path = codex_config_path();
    let existing = fs::read_to_string(&config_path).unwrap_or_default();
    let next = remove_invalid_local_plugin_marketplace_sections(&existing);
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&config_path, next).map_err(|e| format!("写入 openai-bundled 插件市场失败: {e}"))?;
    Ok(build_toolbox_snapshot(&app))
}

#[tauri::command]
fn enable_codex_builtin_plugin(
    app: tauri::AppHandle,
    plugin_id: String,
) -> Result<ToolboxSnapshot, String> {
    let normalized = plugin_id.trim().to_string();
    if normalized.is_empty() || normalized.contains('\n') || normalized.contains('\r') {
        return Err("非法插件 ID".into());
    }
    auto_backup_configs(&app);
    let config_path = codex_config_path();
    let existing = fs::read_to_string(&config_path).unwrap_or_default();
    let with_marketplaces = ensure_discovered_plugin_marketplaces_config(&existing)?;
    let next = write_enabled_codex_plugin_config(&with_marketplaces, &normalized);
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&config_path, next).map_err(|e| format!("启用 Codex 内置插件失败: {e}"))?;
    Ok(build_toolbox_snapshot(&app))
}

#[tauri::command]
fn enable_important_codex_builtin_plugins(
    app: tauri::AppHandle,
) -> Result<ToolboxSnapshot, String> {
    auto_backup_configs(&app);
    let config_path = codex_config_path();
    let existing = fs::read_to_string(&config_path).unwrap_or_default();
    let mut next = ensure_discovered_plugin_marketplaces_config(&existing)?;
    let status = list_codex_builtin_plugins(&next);
    if !status.available {
        return Err(status.last_error);
    }
    let important_ids = status
        .plugins
        .iter()
        .filter(|plugin| plugin.important)
        .map(|plugin| plugin.id.clone())
        .collect::<Vec<_>>();
    if important_ids.is_empty() {
        return Err("未发现 Computer Use / Chrome / Browser / Fast Speed 等关键内置插件".into());
    }
    for id in important_ids {
        next = write_enabled_codex_plugin_config(&next, &id);
    }
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&config_path, next).map_err(|e| format!("启用关键 Codex 内置插件失败: {e}"))?;
    Ok(build_toolbox_snapshot(&app))
}

#[tauri::command]
async fn apply_plugin_marketplace(
    app: tauri::AppHandle,
    source: String,
) -> Result<ToolboxSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || apply_plugin_marketplace_blocking(app, source))
        .await
        .map_err(|e| format!("安装插件市场后台任务失败: {e}"))?
}

fn apply_plugin_marketplace_blocking(
    app: tauri::AppHandle,
    source: String,
) -> Result<ToolboxSnapshot, String> {
    let trimmed = supported_plugin_marketplace_source(&source);
    emit_plugin_marketplace_progress(&app, 1, "prepare");
    let config_path = codex_config_path();
    let existing = fs::read_to_string(&config_path).unwrap_or_default();

    if config_has_plugin_marketplace_source(&existing, trimmed)
        && plugin_marketplace_snapshot_exists(default_plugin_marketplace_name())
    {
        let mut state = read_toolbox_state(&app);
        state.plugin_marketplace_input = trimmed.to_string();
        write_toolbox_state(&app, &state)?;
        emit_plugin_marketplace_progress(&app, 6, "done");
        return Ok(build_toolbox_snapshot(&app));
    }

    let cleaned = remove_all_plugin_marketplace_sections(&existing);
    let parent = config_path
        .parent()
        .ok_or("Codex 配置目录不存在")?
        .to_path_buf();
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    fs::write(&config_path, cleaned).map_err(|e| e.to_string())?;

    emit_plugin_marketplace_progress(&app, 2, "install");
    if let Err(error) = repair_and_add_codex_plugin_marketplace(trimmed) {
        let fallback_existing = fs::read_to_string(&config_path).unwrap_or_default();
        let fallback_next = ensure_plugin_marketplace_section(
            &fallback_existing,
            default_plugin_marketplace_name(),
            trimmed,
            plugin_marketplace_source_type(trimmed),
        );
        let _ = fs::write(&config_path, fallback_next);
        return Err(format!(
            "{error}\n已尝试自动修复 varswitch-plugins 来源冲突并写入 config.toml 作为兜底；请确认 Codex CLI 和 Git 可用后重试。"
        ));
    }

    emit_plugin_marketplace_progress(&app, 3, "install");
    emit_plugin_marketplace_progress(&app, 4, "verify");

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
    emit_plugin_marketplace_progress(&app, 6, "done");
    Ok(build_toolbox_snapshot(&app))
}

#[tauri::command]
fn sync_codex_sessions(app: tauri::AppHandle) -> Result<ToolboxSnapshot, String> {
    let threads = read_codex_threads(200);
    let mut state = read_toolbox_state(&app);
    state.synced_codex_threads = threads;
    refresh_selected_mobile_thread_after_session_change(&mut state);
    state.session_sync = CodexSessionSyncState {
        last_synced_at: chrono_now(),
        total: visible_codex_threads(&state).len(),
    };
    write_toolbox_state(&app, &state)?;
    Ok(build_toolbox_snapshot(&app))
}

#[tauri::command]
fn trash_codex_sessions(
    app: tauri::AppHandle,
    thread_ids: Vec<String>,
) -> Result<ToolboxSnapshot, String> {
    let requested: HashSet<String> = thread_ids
        .into_iter()
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .collect();
    if requested.is_empty() {
        return Err("请至少选择一条会话".into());
    }

    let mut state = read_toolbox_state(&app);
    let mut available = HashMap::new();
    for thread in state
        .synced_codex_threads
        .iter()
        .cloned()
        .chain(read_codex_threads(500).into_iter())
    {
        available.entry(thread.id.clone()).or_insert(thread);
    }

    let deleted_at = chrono_now();
    let mut added = 0usize;
    let mut existing_trash = trashed_codex_thread_ids(&state);
    for id in requested {
        if existing_trash.contains(&id) {
            continue;
        }
        let Some(thread) = available.get(&id).cloned() else {
            continue;
        };
        state
            .trashed_codex_threads
            .push(codex_thread_to_trash_record(thread, &deleted_at));
        existing_trash.insert(id);
        added += 1;
    }

    if added == 0 {
        return Err("没有找到可移入回收站的会话".into());
    }

    refresh_selected_mobile_thread_after_session_change(&mut state);
    state.session_sync.total = visible_codex_threads(&state).len();
    write_toolbox_state(&app, &state)?;
    Ok(build_toolbox_snapshot(&app))
}

#[tauri::command]
fn restore_codex_sessions(
    app: tauri::AppHandle,
    thread_ids: Vec<String>,
) -> Result<ToolboxSnapshot, String> {
    let requested: HashSet<String> = thread_ids
        .into_iter()
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .collect();
    if requested.is_empty() {
        return Err("请至少选择一条待恢复会话".into());
    }

    let mut state = read_toolbox_state(&app);
    let mut restored = Vec::new();
    state.trashed_codex_threads.retain(|thread| {
        if requested.contains(&thread.id) {
            restored.push(trash_record_to_codex_thread(thread));
            false
        } else {
            true
        }
    });

    if restored.is_empty() {
        return Err("没有找到可恢复的会话".into());
    }

    let mut known_ids: HashSet<String> = state
        .synced_codex_threads
        .iter()
        .map(|thread| thread.id.clone())
        .collect();
    for thread in restored {
        if known_ids.insert(thread.id.clone()) {
            state.synced_codex_threads.push(thread);
        }
    }

    refresh_selected_mobile_thread_after_session_change(&mut state);
    state.session_sync.total = visible_codex_threads(&state).len();
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
    // B11: 保存旧凭据，用于判断是否需要重启运行中的通道
    let old_creds: (String, String, String) = state
        .mobile_channels
        .iter()
        .find(|b| b.channel == normalized_channel)
        .map(|b| (b.app_id.clone(), b.app_secret.clone(), b.bot_token.clone()))
        .unwrap_or_default();
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
    // B11: 如果凭据变化且通道正在运行，停止旧连接（看门狗会用新凭据重启）
    let new_creds: (String, String, String) = state
        .mobile_channels
        .iter()
        .find(|b| b.channel == normalized_channel)
        .map(|b| (b.app_id.clone(), b.app_secret.clone(), b.bot_token.clone()))
        .unwrap_or_default();
    if old_creds != new_creds {
        match normalized_channel.as_str() {
            "lark" if LARK_BRIDGE_ACTIVE.load(Ordering::SeqCst) => {
                log_info!("[mobile-control][configure] 飞书凭据变更，停止旧连接");
                stop_lark_bridge();
            }
            "qq" if QQ_GATEWAY_ACTIVE.load(Ordering::SeqCst) => {
                log_info!("[mobile-control][configure] QQ 凭据变更，停止旧连接");
                stop_qq_gateway();
            }
            "wechat" if WECHAT_LISTENER_ACTIVE.load(Ordering::SeqCst) => {
                log_info!("[mobile-control][configure] 微信凭据变更，停止旧连接");
                stop_wechat_listener(&app);
            }
            _ => {}
        }
    }
    Ok(build_toolbox_snapshot(&app))
}

// B4: 改 async + spawn_blocking，注册请求含网络往返，同步执行会卡 UI
#[tauri::command]
async fn start_lark_bot_registration(
    app: tauri::AppHandle,
    create_only: Option<bool>,
) -> Result<ToolboxSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        start_lark_bot_registration_blocking(app, create_only)
    })
    .await
    .map_err(|e| format!("飞书注册后台任务失败: {e}"))?
}

fn start_lark_bot_registration_blocking(
    app: tauri::AppHandle,
    create_only: Option<bool>,
) -> Result<ToolboxSnapshot, String> {
    // 新点击直接取代旧流程：先递增代际，旧轮询 worker 在下一轮检查时自行退出。
    // 此前这里用 ACTIVE 标志静默早退——旧 worker 可存活十来分钟，期间“创建/换绑”
    // 按钮点了没任何反应（前端还提示成功），表现为按钮大面积“失败”。
    let my_generation = LARK_REGISTRATION_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    LARK_REGISTRATION_ACTIVE.store(true, Ordering::SeqCst);
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
            "已打开飞书创建页面，建议命名为 VarSwitch 智能体，创建完成后会自动填充 AppID/AppSecret"
                .into()
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
        binding.qr_started_at = chrono_timestamp_millis().to_string();
        binding.updated_at = chrono_now();
    }
    write_toolbox_state(&app, &state)?;
    start_lark_registration_poll_worker(app.clone(), device_code, interval, my_generation);
    Ok(build_toolbox_snapshot(&app))
}

// B4: 命令改 async + spawn_blocking，避免 15 秒阻塞 POST 卡死主线程（前端每秒调一次）
#[tauri::command]
async fn poll_lark_bot_registration(app: tauri::AppHandle) -> Result<ToolboxSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || poll_lark_bot_registration_blocking(app))
        .await
        .map_err(|e| format!("飞书注册轮询后台任务失败: {e}"))?
}

fn poll_lark_bot_registration_blocking(app: tauri::AppHandle) -> Result<ToolboxSnapshot, String> {
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
async fn open_lark_bot_launcher(app: tauri::AppHandle) -> Result<ToolboxSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || open_lark_bot_launcher_blocking(app))
        .await
        .map_err(|e| format!("飞书启动器后台任务失败: {e}"))?
}

fn open_lark_bot_launcher_blocking(app: tauri::AppHandle) -> Result<ToolboxSnapshot, String> {
    // B4: start_lark_bot_registration 已改 async，这里调用同步的 blocking 版本
    match start_lark_bot_registration_blocking(app.clone(), Some(true)) {
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
async fn clear_mobile_channel_binding(
    app: tauri::AppHandle,
    channel: String,
) -> Result<ToolboxSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        clear_mobile_channel_binding_blocking(app, channel)
    })
    .await
    .map_err(|e| format!("清除手机通道绑定后台任务失败: {e}"))?
}

fn clear_mobile_channel_binding_blocking(
    app: tauri::AppHandle,
    channel: String,
) -> Result<ToolboxSnapshot, String> {
    let normalized_channel = normalize_mobile_channel(&channel);
    match normalized_channel.as_str() {
        "lark" => {
            // B13: 清除绑定后使正在运行的注册轮询立刻失效，避免旧 worker 回写凭据。
            LARK_REGISTRATION_GENERATION.fetch_add(1, Ordering::SeqCst);
            LARK_REGISTRATION_ACTIVE.store(false, Ordering::SeqCst);
            stop_lark_bridge();
        }
        "qq" => {
            stop_qq_gateway();
            cancel_qq_qr_binding_process();
        }
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

fn cancel_qq_qr_binding_process() {
    let _state_guard = match QQ_QR_STATE_LOCK.lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };
    QQ_QR_GENERATION.fetch_add(1, Ordering::SeqCst);
    QQ_QR_ACTIVE.store(false, Ordering::SeqCst);
    if let Ok(mut guard) = QQ_QR_CHILD.lock() {
        if let Some(mut child) = guard.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn write_qq_qr_state_for_generation(
    app: &tauri::AppHandle,
    generation: u64,
    status: &str,
    qr_url: &str,
    qr_data_url: &str,
    error: &str,
) -> bool {
    let Ok(_state_guard) = QQ_QR_STATE_LOCK.lock() else {
        return false;
    };
    if QQ_QR_GENERATION.load(Ordering::SeqCst) != generation {
        return false;
    }
    let _ = write_mobile_channel_qr_state(app, "qq", status, qr_url, qr_data_url, error);
    true
}

fn save_qq_qr_credentials_for_generation(
    app: &tauri::AppHandle,
    generation: u64,
    app_id: &str,
    app_secret: &str,
) -> bool {
    let Ok(_state_guard) = QQ_QR_STATE_LOCK.lock() else {
        return false;
    };
    if QQ_QR_GENERATION.load(Ordering::SeqCst) != generation {
        return false;
    }
    let _ =
        update_mobile_channel_credentials_from_qr(app, "qq", app_id, app_secret, "", "", "", "");
    true
}

fn finish_qq_qr_generation(generation: u64) {
    let Ok(_state_guard) = QQ_QR_STATE_LOCK.lock() else {
        return;
    };
    if QQ_QR_GENERATION.load(Ordering::SeqCst) != generation {
        return;
    }
    if let Ok(mut guard) = QQ_QR_CHILD.lock() {
        if let Some(mut child) = guard.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
    QQ_QR_ACTIVE.store(false, Ordering::SeqCst);
}

#[tauri::command]
async fn cancel_qq_qr_binding(app: tauri::AppHandle) -> Result<ToolboxSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        cancel_qq_qr_binding_process();
        let _ = write_mobile_channel_qr_state(&app, "qq", "已取消 QQ 扫码绑定", "", "", "");
        Ok(build_toolbox_snapshot(&app))
    })
    .await
    .map_err(|e| format!("取消 QQ 扫码后台任务失败: {e}"))?
}

#[tauri::command]
async fn start_qq_qr_binding(app: tauri::AppHandle) -> Result<ToolboxSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || start_qq_qr_binding_blocking(app))
        .await
        .map_err(|e| format!("启动 QQ 扫码后台任务失败: {e}"))?
}

fn start_qq_qr_binding_blocking(app: tauri::AppHandle) -> Result<ToolboxSnapshot, String> {
    if QQ_QR_ACTIVE.swap(true, Ordering::SeqCst) {
        // 保留当前二维码和状态；重复点击不能把已经生成的二维码清空。
        return Ok(build_toolbox_snapshot(&app));
    }

    let generation = QQ_QR_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let _ = write_mobile_channel_qr_state(&app, "qq", "正在准备 QQ 扫码绑定服务...", "", "", "");
    let app_for_thread = app.clone();
    std::thread::spawn(move || {
        let runner = match ensure_qq_qr_connector(&app_for_thread) {
            Ok(path) => path,
            Err(error) => {
                write_qq_qr_state_for_generation(
                    &app_for_thread,
                    generation,
                    "QQ 扫码绑定失败",
                    "",
                    "",
                    &error,
                );
                finish_qq_qr_generation(generation);
                return;
            }
        };
        let node = match node_command_name() {
            Ok(command) => command,
            Err(error) => {
                write_qq_qr_state_for_generation(
                    &app_for_thread,
                    generation,
                    "QQ 扫码绑定失败",
                    "",
                    "",
                    &error,
                );
                finish_qq_qr_generation(generation);
                return;
            }
        };
        let connector_dir = match runner.parent() {
            Some(parent) => parent.to_path_buf(),
            None => {
                write_qq_qr_state_for_generation(
                    &app_for_thread,
                    generation,
                    "QQ 扫码绑定失败",
                    "",
                    "",
                    "QQ 扫码运行目录不存在",
                );
                finish_qq_qr_generation(generation);
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
        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(error) => {
                write_qq_qr_state_for_generation(
                    &app_for_thread,
                    generation,
                    "QQ 扫码绑定失败",
                    "",
                    "",
                    &format!("启动 QQ 扫码服务失败: {error}"),
                );
                finish_qq_qr_generation(generation);
                return;
            }
        };
        let stdout = match child.stdout.take() {
            Some(stdout) => stdout,
            None => {
                write_qq_qr_state_for_generation(
                    &app_for_thread,
                    generation,
                    "QQ 扫码绑定失败",
                    "",
                    "",
                    "QQ 扫码服务没有输出通道",
                );
                let _ = child.kill();
                let _ = child.wait();
                finish_qq_qr_generation(generation);
                return;
            }
        };
        // B1: 消费 stderr，防止缓冲区满后 write 阻塞造成进程假死
        if let Some(stderr) = child.stderr.take() {
            std::thread::spawn(move || {
                let reader = BufReader::new(stderr);
                for line in reader.lines().flatten() {
                    log_info!("[mobile-control][qq-qr][stderr] {}", line);
                }
            });
        }
        // 取消可能发生在进程启动和保存句柄之间。过期轮次自行回收，不能覆盖新轮次句柄。
        // B2: 保存子进程句柄，方便外部取消；同时加 5 分钟总超时。
        // 与取消操作共用状态锁，防止取消刚发生又写回新的子进程句柄。
        let mut child = Some(child);
        let keep_child = if let Ok(_state_guard) = QQ_QR_STATE_LOCK.lock() {
            if QQ_QR_GENERATION.load(Ordering::SeqCst) == generation {
                if let Ok(mut guard) = QQ_QR_CHILD.lock() {
                    *guard = child.take();
                    true
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        };
        if !keep_child {
            if let Some(mut child) = child {
                let _ = child.kill();
                let _ = child.wait();
            }
            return;
        }
        let (line_tx, line_rx) = mpsc::channel();
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines().flatten() {
                if line_tx.send(line).is_err() {
                    break;
                }
            }
        });
        let deadline = Instant::now() + Duration::from_secs(300);
        loop {
            if QQ_QR_GENERATION.load(Ordering::SeqCst) != generation {
                break;
            }
            if std::time::Instant::now() > deadline {
                write_qq_qr_state_for_generation(
                    &app_for_thread,
                    generation,
                    "QQ 扫码已超时（5 分钟），请重新尝试",
                    "",
                    "",
                    "扫码总超时",
                );
                break;
            }
            let line = match line_rx.recv_timeout(Duration::from_secs(1)) {
                Ok(line) => line,
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            };
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
                        write_qq_qr_state_for_generation(
                            &app_for_thread,
                            generation,
                            "QQ 扫码绑定失败",
                            "",
                            "",
                            "QQ connector 未返回可用二维码且本地生成失败",
                        );
                        break;
                    }
                    write_qq_qr_state_for_generation(
                        &app_for_thread,
                        generation,
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
                        credentials_raw
                            .get(0)
                            .cloned()
                            .unwrap_or_else(|| serde_json::json!({}))
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
                    save_qq_qr_credentials_for_generation(
                        &app_for_thread,
                        generation,
                        &app_id,
                        &app_secret,
                    );
                    break;
                }
                "expired" => {
                    write_qq_qr_state_for_generation(
                        &app_for_thread,
                        generation,
                        "二维码已过期，请重新绑定",
                        "",
                        "",
                        "QQ 扫码二维码已过期",
                    );
                    break;
                }
                "failure" => {
                    let message = json_string(&payload, &["message"]);
                    write_qq_qr_state_for_generation(
                        &app_for_thread,
                        generation,
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
        // B2: 无论正常结束/超时/expired，都先 kill 子进程，再清 ACTIVE
        // 这样 child.wait() 不会永久阻塞（runner 过期后本身不退出）
        finish_qq_qr_generation(generation);
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

// B4: 命令改 async + spawn_blocking，避免 35 秒阻塞 GET 卡死主线程（前端每秒调一次）
#[tauri::command]
async fn poll_wechat_qr_binding(
    app: tauri::AppHandle,
    verify_code: Option<String>,
) -> Result<ToolboxSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || poll_wechat_qr_binding_blocking(app, verify_code))
        .await
        .map_err(|e| format!("微信扫码轮询后台任务失败: {e}"))?
}

fn poll_wechat_qr_binding_blocking(
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
        normalized_channel,
        state.mobile_remote.active_thread_id
    );
    Ok(build_toolbox_snapshot(&app))
}

#[tauri::command]
async fn unbind_codex_thread(
    app: tauri::AppHandle,
    channel: String,
) -> Result<ToolboxSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || unbind_codex_thread_blocking(app, channel))
        .await
        .map_err(|e| format!("解绑手机会话后台任务失败: {e}"))?
}

fn unbind_codex_thread_blocking(
    app: tauri::AppHandle,
    channel: String,
) -> Result<ToolboxSnapshot, String> {
    let normalized_channel = normalize_mobile_channel(&channel);
    log_info!(
        "[mobile-control][unbind] request: channel={}, normalized={}",
        channel,
        normalized_channel
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
    // B7: 解绑时停止对应平台通道，确保手机侧消息不再驱动本机 Codex
    match normalized_channel.as_str() {
        "lark" => stop_lark_bridge(),
        "qq" => stop_qq_gateway(),
        "wechat" => stop_wechat_listener(&app),
        _ => {}
    }
    log_info!(
        "[mobile-control][unbind] stopped channel and saved: channel={}",
        normalized_channel
    );
    Ok(build_toolbox_snapshot(&app))
}

#[tauri::command]
async fn start_mobile_remote(
    app: tauri::AppHandle,
    _state: State<'_, AppState>,
    port: Option<u16>,
) -> Result<ToolboxSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || start_mobile_remote_blocking(app, port))
        .await
        .map_err(|e| format!("启动手机连接后台任务失败: {e}"))?
}

fn start_mobile_remote_blocking(
    app: tauri::AppHandle,
    port: Option<u16>,
) -> Result<ToolboxSnapshot, String> {
    let _ = port;
    // B4: 通道 probe 等重活已在 run_mobile_remote_start 的后台线程里执行，
    //     这里只做 state 写入和 120ms 探测等待，不阻塞长时间网络往返
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
    start_smart_control_server(
        app.clone(),
        toolbox_state
            .mobile_remote
            .remote_control_backend_url
            .clone(),
    );
    std::thread::sleep(Duration::from_millis(120));
    let smart_status = refresh_smart_control_status_for_state(&mut toolbox_state);
    log_info!(
        "[mobile-control][smart-control] start probe: connected={}, status={}, detail={}",
        smart_status.connected,
        smart_status.status,
        smart_status.detail
    );
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
                    channel.channel,
                    channel.last_error
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
                        channel.channel,
                        error
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
        toolbox_state.mobile_remote.enabled,
        connected_count
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
        // B3: 保证看门狗单例——start→stop→start 快速切换时旧看门狗已退出才能起新的
        if WATCHDOG_ACTIVE.swap(true, Ordering::SeqCst) {
            log_info!("[mobile-control][watchdog] 已有看门狗在运行，跳过重复启动");
            return;
        }
        let watchdog_app = app.clone();
        std::thread::spawn(move || {
            // B3: 辅助函数：分片睡眠，每 1 秒检查一次取消标志，避免停止后长时间不响应
            fn interruptible_sleep(secs: u64) {
                for _ in 0..secs {
                    if MOBILE_REMOTE_CANCEL_REQUESTED.load(Ordering::SeqCst) {
                        return;
                    }
                    std::thread::sleep(Duration::from_secs(1));
                }
            }

            let mut fail_counts: std::collections::HashMap<&str, u32> =
                std::collections::HashMap::new();
            loop {
                // 每 30 秒轮询一次（分片睡眠，可随时响应取消）
                interruptible_sleep(30);
                if MOBILE_REMOTE_CANCEL_REQUESTED.load(Ordering::SeqCst) {
                    log_info!("[mobile-control][watchdog] 已停止（用户取消）");
                    break;
                }
                let state = read_toolbox_state(&watchdog_app);
                // B3: 重读 state 后再次复查 enabled + cancel 标志
                if !state.mobile_remote.enabled
                    || MOBILE_REMOTE_CANCEL_REQUESTED.load(Ordering::SeqCst)
                {
                    log_info!("[mobile-control][watchdog] remote 已禁用或已取消，看门狗退出");
                    break;
                }
                // 检查飞书
                if !LARK_BRIDGE_ACTIVE.load(Ordering::SeqCst) {
                    if let Some(binding) = state
                        .mobile_channels
                        .iter()
                        .find(|b| b.channel == "lark" && mobile_channel_has_credentials(b))
                        .cloned()
                    {
                        let cnt = fail_counts.entry("lark").or_insert(0);
                        let delay = (30u64 * (1 << (*cnt).min(4))).min(300);
                        log_info!(
                            "[mobile-control][watchdog] 飞书断线，{}s 后重启（第{}次）",
                            delay,
                            *cnt + 1
                        );
                        interruptible_sleep(delay);
                        // B3: 退避后重读 state + cancel 标志，避免停止后继续拉起桥接
                        if MOBILE_REMOTE_CANCEL_REQUESTED.load(Ordering::SeqCst) {
                            break;
                        }
                        let fresh = read_toolbox_state(&watchdog_app);
                        if !fresh.mobile_remote.enabled {
                            break;
                        }
                        match start_lark_bridge(watchdog_app.clone(), binding) {
                            Ok(_) => {
                                *cnt = 0;
                                update_channel_status(&watchdog_app, "lark", "飞书已自动重连", "");
                            }
                            Err(e) => {
                                *cnt += 1;
                                update_channel_status(
                                    &watchdog_app,
                                    "lark",
                                    "飞书自动重连失败",
                                    &e,
                                );
                            }
                        }
                    }
                } else {
                    fail_counts.remove("lark");
                }
                // 检查 QQ
                if !QQ_GATEWAY_ACTIVE.load(Ordering::SeqCst) {
                    if let Some(binding) = state
                        .mobile_channels
                        .iter()
                        .find(|b| b.channel == "qq" && mobile_channel_has_credentials(b))
                        .cloned()
                    {
                        let cnt = fail_counts.entry("qq").or_insert(0);
                        let delay = (30u64 * (1 << (*cnt).min(4))).min(300);
                        log_info!(
                            "[mobile-control][watchdog] QQ 断线，{}s 后重启（第{}次）",
                            delay,
                            *cnt + 1
                        );
                        interruptible_sleep(delay);
                        if MOBILE_REMOTE_CANCEL_REQUESTED.load(Ordering::SeqCst) {
                            break;
                        }
                        let fresh = read_toolbox_state(&watchdog_app);
                        if !fresh.mobile_remote.enabled {
                            break;
                        }
                        match start_qq_gateway(watchdog_app.clone(), binding) {
                            Ok(_) => {
                                *cnt = 0;
                                update_channel_status(&watchdog_app, "qq", "QQ 已自动重连", "");
                            }
                            Err(e) => {
                                *cnt += 1;
                                update_channel_status(&watchdog_app, "qq", "QQ 自动重连失败", &e);
                            }
                        }
                    }
                } else {
                    fail_counts.remove("qq");
                }
                // 检查微信
                if !WECHAT_LISTENER_ACTIVE.load(Ordering::SeqCst) {
                    if let Some(binding) = state
                        .mobile_channels
                        .iter()
                        .find(|b| b.channel == "wechat" && mobile_channel_has_credentials(b))
                        .cloned()
                    {
                        let cnt = fail_counts.entry("wechat").or_insert(0);
                        let delay = (30u64 * (1 << (*cnt).min(4))).min(300);
                        log_info!(
                            "[mobile-control][watchdog] 微信断线，{}s 后重启（第{}次）",
                            delay,
                            *cnt + 1
                        );
                        interruptible_sleep(delay);
                        if MOBILE_REMOTE_CANCEL_REQUESTED.load(Ordering::SeqCst) {
                            break;
                        }
                        let fresh = read_toolbox_state(&watchdog_app);
                        if !fresh.mobile_remote.enabled {
                            break;
                        }
                        match start_wechat_listener(watchdog_app.clone(), binding) {
                            Ok(_) => {
                                *cnt = 0;
                                update_channel_status(
                                    &watchdog_app,
                                    "wechat",
                                    "微信已自动重连",
                                    "",
                                );
                            }
                            Err(e) => {
                                *cnt += 1;
                                update_channel_status(
                                    &watchdog_app,
                                    "wechat",
                                    "微信自动重连失败",
                                    &e,
                                );
                            }
                        }
                    }
                } else {
                    fail_counts.remove("wechat");
                }
            }
            WATCHDOG_ACTIVE.store(false, Ordering::SeqCst);
            log_info!("[mobile-control][watchdog] 看门狗线程退出");
        });
    }
}

#[tauri::command]
async fn stop_mobile_remote(
    app: tauri::AppHandle,
    _state: State<'_, AppState>,
) -> Result<ToolboxSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || stop_mobile_remote_blocking(app))
        .await
        .map_err(|e| format!("停止手机连接后台任务失败: {e}"))?
}

fn stop_mobile_remote_blocking(app: tauri::AppHandle) -> Result<ToolboxSnapshot, String> {
    log_info!("[mobile-control][stop] stop_mobile_remote requested");
    MOBILE_REMOTE_CANCEL_REQUESTED.store(true, Ordering::SeqCst);
    MOBILE_REMOTE_START_ACTIVE.store(false, Ordering::SeqCst);
    stop_smart_control_server();
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
        codex_settings: codex_config_path().to_string_lossy().to_string(),
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
        cmd.spawn().map_err(|e| e.to_string())?;
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
    // 安全：该目标来自前端，最终会交给系统启动器。拒绝控制字符与引号，
    // 避免任何形式的参数/命令注入（合法的 URL 或自定义 scheme 不含这些字符）。
    if trimmed
        .chars()
        .any(|c| c == '"' || c == '\'' || c == '\0' || c.is_control())
    {
        return Err("目标包含非法字符".into());
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
        let release = fetch_latest_download_page_release()?;
        let latest_version = release.version.clone();

        Ok(UpdateCheckResult {
            current_version: current_version.clone(),
            latest_version: latest_version.clone(),
            has_update: is_remote_version_newer(&latest_version, &current_version),
            release_url: APP_DOWNLOAD_PAGE_URL.into(),
            release_notes: String::new(),
            published_at: String::new(),
            asset_name: Some(release.file_name),
            can_auto_update: true,
        })
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?
}

#[tauri::command]
async fn install_app_update(app: tauri::AppHandle) -> Result<UpdateDownloadResult, String> {
    emit_update_download_progress(&app, 5, 100, "prepare");

    let update = app
        .updater()
        .map_err(|e| format!("初始化自动更新失败: {e}"))?
        .check()
        .await
        .map_err(|e| format!("检查自动更新失败: {e}"))?
        .ok_or_else(|| "当前已经是最新版本".to_string())?;

    let latest_version = update.version.clone();
    let file_name = update
        .download_url
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("VarSwitch-update")
        .to_string();
    let release_url = update.download_url.to_string();
    let app_for_progress = app.clone();
    let mut downloaded: u64 = 0;

    emit_update_download_progress(&app, 10, 100, "download");
    update
        .download_and_install(
            move |chunk_len, content_len| {
                downloaded = downloaded.saturating_add(chunk_len as u64);
                let pct = content_len
                    .filter(|total| *total > 0)
                    .map(|total| 10 + ((downloaded.saturating_mul(80) / total).min(80) as u32))
                    .unwrap_or(35);
                emit_update_download_progress(&app_for_progress, pct, 100, "download");
            },
            {
                let app_for_install = app.clone();
                move || {
                    emit_update_download_progress(&app_for_install, 92, 100, "install");
                    // 安装器会重启应用，先释放单实例锁，避免新进程被误判为重复启动
                    tauri_plugin_single_instance::destroy(&app_for_install);
                }
            },
        )
        .await
        .map_err(|e| format!("自动安装更新失败: {e}"))?;

    emit_update_download_progress(&app, 100, 100, "done");

    Ok(UpdateDownloadResult {
        latest_version,
        file_name,
        file_path: String::new(),
        release_url,
    })
}

#[tauri::command]
async fn download_and_open_update(app: tauri::AppHandle) -> Result<UpdateDownloadResult, String> {
    let current_version = app.package_info().version.to_string();
    let app_handle = app.clone();

    tauri::async_runtime::spawn_blocking(move || {
        emit_update_download_progress(&app_handle, 5, 100, "prepare");
        let release = fetch_latest_download_page_release()?;
        if !is_remote_version_newer(&release.version, &current_version) {
            return Err("Already on the latest version".into());
        }

        let client = build_http_client(120)?;
        emit_update_download_progress(&app_handle, 10, 100, "download");
        let mut resp = client
            .get(&release.download_url)
            .send()
            .map_err(|e| format!("更新包下载失败: {}", e))?;

        if !resp.status().is_success() {
            return Err(format!("更新包下载返回 {}", resp.status()));
        }

        let update_dir = data_dir(&app_handle).join("updates");
        fs::create_dir_all(&update_dir).map_err(|e| e.to_string())?;
        let file_path = update_dir.join(&release.file_name);
        let mut file = fs::File::create(&file_path).map_err(|e| e.to_string())?;
        let total_size = resp.content_length().unwrap_or(0);
        let mut downloaded: u64 = 0;
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let read = resp
                .read(&mut buffer)
                .map_err(|e| format!("读取更新包失败: {}", e))?;
            if read == 0 {
                break;
            }
            file.write_all(&buffer[..read])
                .map_err(|e| format!("写入更新包失败: {}", e))?;
            downloaded += read as u64;
            if total_size > 0 {
                let pct = 10 + ((downloaded.saturating_mul(75) / total_size).min(75) as u32);
                emit_update_download_progress(&app_handle, pct, 100, "download");
            }
        }
        file.flush().map_err(|e| e.to_string())?;

        let file_path_str = file_path.to_string_lossy().to_string();
        emit_update_download_progress(&app_handle, 90, 100, "open");
        open_installer_file(&file_path)?;
        emit_update_download_progress(&app_handle, 100, 100, "done");

        Ok(UpdateDownloadResult {
            latest_version: release.version,
            file_name: release.file_name,
            file_path: file_path_str,
            release_url: APP_DOWNLOAD_PAGE_URL.into(),
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

/// 校验技能名称，拒绝任何可能造成路径穿越的输入。
/// 名称中唯一允许的层级分隔符是命令使用的冒号（`:`），
/// 每个片段都不允许为空、为 `.`/`..`，也不允许包含路径分隔符或 NUL。
fn validate_skill_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("技能名称不能为空".into());
    }
    for segment in name.split(':') {
        if segment.is_empty()
            || segment == "."
            || segment == ".."
            || segment.contains('/')
            || segment.contains('\\')
            || segment.contains('\0')
        {
            return Err("技能名称包含非法字符".into());
        }
    }
    Ok(())
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
    validate_skill_name(&name)?;
    let st = source_type.as_deref().unwrap_or("command");
    let path = skill_path_by_type(&name, st);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(path, content).map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_skill(name: String, source_type: Option<String>) -> Result<(), String> {
    validate_skill_name(&name)?;
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

fn normalize_version_tag(version: &str) -> String {
    let trimmed = version.trim();
    if trimmed.starts_with(['v', 'V']) {
        format!("v{}", &trimmed[1..])
    } else {
        format!("v{trimmed}")
    }
}

#[cfg(test)]
fn extract_latest_version_from_download_page(html: &str) -> Option<String> {
    extract_latest_download_page_release(html).map(|release| release.version)
}

fn installer_name_matches_target(name_lower: &str, target_os: &str) -> bool {
    match target_os {
        "windows" => name_lower.ends_with(".exe") || name_lower.ends_with(".msi"),
        "macos" => name_lower.ends_with(".dmg"),
        "linux" => {
            name_lower.ends_with(".appimage")
                || name_lower.ends_with(".deb")
                || name_lower.ends_with(".rpm")
        }
        _ => false,
    }
}

fn extract_download_page_releases(html: &str) -> Vec<DownloadPageRelease> {
    let marker = "VarSwitch_";
    html.match_indices(marker)
        .filter_map(|(idx, _)| {
            let tail = html.get(idx..)?;
            let file_name = tail
                .chars()
                .take_while(|ch| {
                    ch.is_ascii_alphanumeric() || matches!(*ch, '_' | '-' | '.' | '+' | '(' | ')')
                })
                .collect::<String>();
            if file_name.is_empty() {
                return None;
            }
            let version_tail = file_name.strip_prefix(marker)?;
            let version = version_tail
                .chars()
                .take_while(|ch| ch.is_ascii_digit() || *ch == '.')
                .collect::<String>();
            if version.is_empty() {
                return None;
            }
            let href_before = html[..idx].rfind("href=\"");
            let download_url = href_before
                .and_then(|href_idx| {
                    let start = href_idx + "href=\"".len();
                    let end = html[start..].find('"')? + start;
                    let href = &html[start..end];
                    if href.contains(&file_name) {
                        reqwest::Url::parse(APP_DOWNLOAD_PAGE_URL)
                            .ok()
                            .and_then(|base| base.join(href).ok())
                            .map(|url| url.to_string())
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| format!("{}releases/{}", APP_DOWNLOAD_PAGE_URL, file_name));
            Some(DownloadPageRelease {
                version: normalize_version_tag(&version),
                file_name,
                download_url,
            })
        })
        .collect()
}

fn extract_latest_download_page_release(html: &str) -> Option<DownloadPageRelease> {
    extract_download_page_releases(html)
        .into_iter()
        .filter(|release| {
            installer_name_matches_target(
                &release.file_name.to_ascii_lowercase(),
                std::env::consts::OS,
            )
        })
        .max_by(|left, right| compare_versions(&left.version, &right.version))
}

fn fetch_latest_download_page_release() -> Result<DownloadPageRelease, String> {
    let client = build_http_client(20)?;
    let resp = client
        .get(APP_DOWNLOAD_PAGE_URL)
        .send()
        .map_err(|e| format!("下载页请求失败: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("下载页返回 {}", resp.status()));
    }

    let body = resp.text().map_err(|e| format!("下载页读取失败: {}", e))?;

    extract_latest_download_page_release(&body).ok_or_else(|| "下载页未找到可用安装包".into())
}

fn open_with_system(target: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        // 用 rundll32 url.dll,FileProtocolHandler 打开目标：内部直接调用 ShellExecute，
        // 不经过 cmd/start 的 shell 解析（避免 & 截断与元字符注入），也不用 explorer.exe
        // （explorer 对带查询串/百分号编码的 URL 经常解析失败，退化成打开“文档”文件夹，
        // 表现为点“创建飞书机器人”时莫名弹出资源管理器窗口，授权页面根本没打开）。
        // FileProtocolHandler 是 ANSI 入口，这里把空格、控制字符与非 ASCII 字节按
        // RFC3986 百分号编码后再传入；已编码的 URL 不受影响（% 本身原样保留）。
        let mut encoded = String::with_capacity(target.len());
        for byte in target.as_bytes() {
            match *byte {
                0x21..=0x7E => encoded.push(*byte as char),
                _ => encoded.push_str(&format!("%{byte:02X}")),
            }
        }
        let system_root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
        let mut cmd = std::process::Command::new(format!("{system_root}\\System32\\rundll32.exe"));
        cmd.arg("url.dll,FileProtocolHandler");
        cmd.arg(&encoded);
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd.spawn().map_err(|e| e.to_string())?;
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

fn open_installer_file(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Err(format!("安装包不存在: {}", path.to_string_lossy()));
    }

    #[cfg(target_os = "windows")]
    {
        let path_str = path.to_string_lossy().to_string();
        let escaped = path_str.replace('\'', "''");
        let script = format!(
            "Unblock-File -LiteralPath '{}' -ErrorAction SilentlyContinue; Start-Process -FilePath '{}' -Verb Open -WorkingDirectory '{}'",
            escaped,
            escaped,
            path.parent()
                .map(|dir| dir.to_string_lossy().replace('\'', "''"))
                .unwrap_or_default()
        );
        let mut powershell = std::process::Command::new("powershell");
        powershell.args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ]);
        powershell.creation_flags(CREATE_NO_WINDOW);
        match powershell.status() {
            Ok(status) if status.success() => return Ok(()),
            Ok(status) => {
                log_error!(
                    "[update] PowerShell Start-Process failed with status: {:?}",
                    status.code()
                );
            }
            Err(error) => {
                log_error!("[update] PowerShell Start-Process failed: {}", error);
            }
        }

        let mut cmd = std::process::Command::new("cmd");
        cmd.args(["/C", "start", "", &path_str]);
        cmd.creation_flags(CREATE_NO_WINDOW);
        match cmd.spawn() {
            Ok(_) => Ok(()),
            Err(error) => {
                let folder = path
                    .parent()
                    .map(|dir| dir.to_string_lossy().to_string())
                    .unwrap_or_else(|| path_str.clone());
                let _ = open_folder(folder);
                Err(format!(
                    "安装包已下载，但启动安装程序失败：{error}。已打开安装包所在文件夹。"
                ))
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        open_with_system(&path.to_string_lossy())
    }
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
    fn claude_model_is_read_from_the_root_settings_field() {
        let settings = json!({ "model": "  claude-sonnet-4-5  ", "env": {} });

        assert_eq!(claude_model_from_settings(&settings), "claude-sonnet-4-5");
        assert_eq!(claude_model_from_settings(&json!({ "env": {} })), "");
    }

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
    fn invalid_local_plugin_marketplaces_are_removed_without_touching_git_sources() {
        let config = r#"model = "gpt-5"

[marketplaces.invalid-cache]
source = "C:\\definitely-missing-varswitch-marketplace"
source_type = "local"

[marketplaces.community]
source = "https://github.com/example/community-plugins.git"
source_type = "git"

[plugins]
"browser@openai-bundled" = { enabled = true }
"#;

        let cleaned = remove_invalid_local_plugin_marketplace_sections(config);

        assert!(!cleaned.contains("[marketplaces.invalid-cache]"));
        assert!(cleaned.contains("[marketplaces.community]"));
        assert!(cleaned.contains("https://github.com/example/community-plugins.git"));
        assert!(cleaned.contains("browser@openai-bundled"));
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
        assert_eq!(default_plugin_marketplace_name(), "varswitch-plugins");
        assert_eq!(
            default_plugin_marketplace_url(),
            "https://gitcode.com/2301_79703673/codex-plugins.git"
        );
    }

    #[test]
    fn enabling_plugin_updates_existing_plugin_table_without_duplicate_inline_key() {
        let config = r#"[plugins."chrome-devtools@varswitch-plugins"]
enabled = false

[plugins]
"browser@openai-bundled" = { enabled = true }
"#;

        let updated =
            write_enabled_codex_plugin_config(config, "chrome-devtools@varswitch-plugins");

        assert!(updated.contains("[plugins.\"chrome-devtools@varswitch-plugins\"]"));
        assert!(updated.contains("enabled = true"));
        assert!(!updated.contains("\"chrome-devtools@varswitch-plugins\" = { enabled = true }"));
        assert!(updated.contains("\"browser@openai-bundled\" = { enabled = true }"));
    }

    #[test]
    fn enabling_plugin_deduplicates_inline_key_when_plugin_table_exists() {
        let config = r#"[plugins."chrome-devtools@varswitch-plugins"]
enabled = false

[plugins]
"chrome-devtools@varswitch-plugins" = { enabled = true }
"browser@openai-bundled" = { enabled = true }
"#;

        let updated =
            write_enabled_codex_plugin_config(config, "chrome-devtools@varswitch-plugins");

        assert_eq!(
            updated.matches("chrome-devtools@varswitch-plugins").count(),
            1
        );
        assert!(updated.contains("[plugins.\"chrome-devtools@varswitch-plugins\"]"));
        assert!(updated.contains("enabled = true"));
        assert!(updated.contains("\"browser@openai-bundled\" = { enabled = true }"));
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
        qq.qr_data_url = String::new();
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
    fn normalize_toolbox_state_keeps_fresh_qq_image_qr_even_without_openable_url() {
        let mut state = ToolboxState::default();
        let mut qq = default_mobile_channel("qq");
        qq.qr_url = "internal-connector-token".into();
        qq.qr_data_url = "data:image/png;base64,abc".into();
        qq.qr_started_at = chrono_timestamp_millis().to_string();
        state.mobile_channels.push(qq);

        let state = normalize_toolbox_state(state);
        let qq = state
            .mobile_channels
            .iter()
            .find(|item| item.channel == "qq")
            .expect("qq channel");

        assert_eq!(qq.qr_data_url, "data:image/png;base64,abc");
        assert_eq!(qq.qr_url, "internal-connector-token");
    }

    #[test]
    fn normalize_toolbox_state_clears_invalid_qr_started_at() {
        let mut state = ToolboxState::default();
        let mut qq = default_mobile_channel("qq");
        qq.qr_url = "https://bots.qq.com/connect/abc".into();
        qq.qr_data_url = "data:image/png;base64,abc".into();
        qq.qr_started_at = "2026-06-28 12:00:00".into();
        state.mobile_channels.push(qq);

        let state = normalize_toolbox_state(state);
        let qq = state
            .mobile_channels
            .iter()
            .find(|item| item.channel == "qq")
            .expect("qq channel");

        assert!(qq.qr_data_url.is_empty());
        assert!(qq.qr_url.is_empty());
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
    fn normalize_smart_control_backend_url_adds_scheme_and_trims_slashes() {
        assert_eq!(
            normalize_smart_control_backend_url("127.0.0.1:3847/"),
            "http://127.0.0.1:3847"
        );
        assert_eq!(
            normalize_smart_control_backend_url("https://localhost:3847/backend-api/"),
            "https://localhost:3847/backend-api"
        );
        assert_eq!(
            normalize_smart_control_backend_url(""),
            default_smart_control_backend_url()
        );
    }

    #[test]
    fn smart_control_json_response_contains_cors_and_content_length() {
        let response = smart_control_json_response(json!({ "ok": true }));
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.contains("Access-Control-Allow-Origin: *"));
        assert!(response.contains("Content-Type: application/json; charset=utf-8"));
        assert!(response.ends_with(r#"{"ok":true}"#));
    }

    #[test]
    fn codex_config_toml_content_points_chatgpt_base_url_to_local_control() {
        let content = codex_config_toml_content(
            "custom",
            "gpt-test",
            "https://api.example.com",
            "sk-test",
            false,
        );
        assert!(content.contains(r#"chatgpt_base_url = "http://127.0.0.1:3847/backend-api""#));
        assert!(content.contains(r#"model_provider = "custom""#));
        assert!(content.contains(r#"model = "gpt-test""#));

        let official = codex_config_toml_content(
            "customer",
            "gpt-5.5",
            "https://api.example.com",
            "sk-test",
            true,
        );
        assert!(official.contains(r#"chatgpt_base_url = "http://127.0.0.1:3847/backend-api""#));
        assert!(official.contains(r#"preferred_auth_method = "apikey""#));
        assert!(official.contains(r#"experimental_bearer_token = "sk-test""#));
    }

    #[test]
    fn codex_config_merge_preserves_unmanaged_sections() {
        let existing = r#"
model_provider = "old"
approval_policy = "never"

[model_providers.old]
name = "old"
base_url = "https://old.example.com"

[projects."H:/variable-switching"]
trust_level = "trusted"

[marketplaces.varswitch-plugins]
source = "https://example.com/plugins.git"
source_type = "git"

[plugins]
"chrome@openai-bundled".enabled = true
"#;
        let generated = codex_config_toml_content(
            "custom",
            "gpt-5-codex",
            "https://api.example.com/v1",
            "sk-test",
            false,
        );

        let merged = merge_codex_config_with_preserved_sections(&generated, existing);

        assert!(merged.contains(r#"model_provider = "custom""#));
        assert!(merged.contains(r#"approval_policy = "never""#));
        assert!(merged.contains(r#"[projects."H:/variable-switching"]"#));
        assert!(merged.contains("[marketplaces.varswitch-plugins]"));
        assert!(merged.contains("[plugins]"));
        assert!(!merged.contains("[model_providers.old]"));
    }

    #[test]
    fn codex_config_does_not_write_unknown_gpt_image_2_section() {
        let generated = codex_config_toml_content_with_image(
            "custom",
            "gpt-5-codex",
            "https://api.example.com/v1",
            "sk-chat",
            "responses",
            false,
            "sk-image",
            "",
        );

        assert!(!generated.contains("[gpt_image_2]"));
        assert!(!generated.contains("sk-image"));
    }

    #[test]
    fn codex_profile_image_base_url_defaults_to_empty() {
        let profile: CodexProfile = serde_json::from_value(json!({
            "id": "profile-1",
            "name": "Test",
            "apiKey": "sk-test",
            "baseUrl": "https://api.example.test/v1",
            "isActive": false,
            "createdAt": "1"
        }))
        .expect("profile JSON should deserialize");

        assert!(profile.image_base_url.is_empty());
    }

    #[test]
    fn existing_codex_profiles_clear_all_image_base_urls() {
        let mut data = CodexProfilesData {
            profiles: vec![CodexProfile {
                id: "profile-1".into(),
                name: "Test".into(),
                api_key: "sk-test".into(),
                base_url: "https://api.example.test/v1".into(),
                auth_mode: default_codex_auth_mode(),
                wire_api: default_codex_wire_api(),
                model: String::new(),
                provider_name: String::new(),
                image_api_key: "image-key".into(),
                image_base_url: "https://image.example.test/v1".into(),
                is_active: false,
                created_at: "1".into(),
            }],
            image_base_url_migrated: true,
        };

        assert!(clear_codex_image_base_urls(&mut data));
        assert!(data.profiles[0].image_base_url.is_empty());
        assert_eq!(data.profiles[0].image_api_key, "image-key");
        assert!(!clear_codex_image_base_urls(&mut data));
    }

    #[test]
    fn log_redaction_hides_url_queries_and_key_values() {
        let redacted = redact_log_message(
            "gateway=https://gateway.example.test/ws?access_key=secret-key&ticket=temporary appSecret=app-secret bot_token=bot-secret",
        );

        assert!(redacted.contains("https://gateway.example.test/ws?[query-redacted]"));
        assert!(redacted.contains("appSecret=***"));
        assert!(redacted.contains("bot_token=***"));
        for secret in ["secret-key", "temporary", "app-secret", "bot-secret"] {
            assert!(!redacted.contains(secret));
        }
    }

    #[test]
    fn codex_config_merge_removes_legacy_gpt_image_2_section() {
        let existing = r#"
model_provider = "old"

[gpt_image_2]
api_key = "old-image"
base_url = "https://old.example.com/v1"

[projects."H:/variable-switching"]
trust_level = "trusted"
"#;
        let generated = codex_config_toml_content_with_image(
            "custom",
            "gpt-5-codex",
            "https://api.example.com/v1",
            "sk-chat",
            "responses",
            false,
            "sk-new-image",
            "https://image.example.com/v1/",
        );

        let merged = merge_codex_config_with_preserved_sections(&generated, existing);

        assert!(!merged.contains("[gpt_image_2]"));
        assert!(!merged.contains("old-image"));
        assert!(!merged.contains("sk-new-image"));
        assert!(merged.contains(r#"[projects."H:/variable-switching"]"#));
    }

    #[test]
    fn codex_image_skill_routes_generation_requests_to_the_bundled_script() {
        let script_path =
            Path::new(r"C:\Users\Test\.codex\skills\varswitch-imagegen\scripts\generate-image.ps1");
        let manifest = codex_image_skill_manifest(script_path);
        let script = codex_image_skill_script();

        assert!(manifest.contains("name: varswitch-imagegen"));
        assert!(manifest.contains("preferred image-generation Skill"));
        assert!(manifest.contains("built-in `imagegen`"));
        assert!(manifest.contains("missing, not configured, or fails"));
        assert!(manifest.contains("generate-image.ps1"));
        assert!(manifest.contains("powershell.exe"));
        assert!(script.contains("VARSWITCH_IMAGE_API_KEY"));
        assert!(script.contains("VARSWITCH_IMAGE_BASE_URL"));
        assert!(script.contains("VARSWITCH_IMAGE_MODEL"));
        assert!(script.contains("/images/generations"));
        assert!(script.contains("b64_json"));
        assert!(script.contains("Invoke-WebRequest"));
    }

    #[test]
    fn codex_image_priority_instructions_are_idempotent_and_preserve_user_content() {
        let existing = "Keep my own global instruction.\n";

        let enabled_once = merge_codex_image_priority_instructions(existing, true);
        let enabled_twice = merge_codex_image_priority_instructions(&enabled_once, true);

        assert_eq!(enabled_once, enabled_twice);
        assert!(enabled_once.contains("Keep my own global instruction."));
        assert!(enabled_once.contains(CODEX_IMAGE_PRIORITY_START));
        assert!(enabled_once.contains("prefer `varswitch-imagegen`"));
        assert!(enabled_once.contains("built-in `imagegen`"));
        assert_eq!(enabled_once.matches(CODEX_IMAGE_PRIORITY_START).count(), 1);

        let disabled = merge_codex_image_priority_instructions(&enabled_once, false);
        assert_eq!(disabled, existing);
        assert!(!disabled.contains(CODEX_IMAGE_PRIORITY_START));
    }

    #[test]
    fn codex_image_priority_instructions_preserve_windows_line_endings() {
        let existing = "First instruction.\r\nSecond instruction.\r\n";

        let enabled = merge_codex_image_priority_instructions(existing, true);
        assert!(!enabled.replace("\r\n", "").contains('\n'));

        let disabled = merge_codex_image_priority_instructions(&enabled, false);
        assert_eq!(disabled, existing);
    }

    #[test]
    fn codex_image_priority_instructions_can_follow_non_ascii_content() {
        let existing = "保留用户指令。";

        let enabled = merge_codex_image_priority_instructions(existing, true);
        let disabled = merge_codex_image_priority_instructions(&enabled, false);

        assert_eq!(disabled, existing);
    }

    #[test]
    fn imported_codex_profile_preserves_detected_image_configuration() {
        let status = LocationStatus {
            api_key: "sk-chat".into(),
            base_url: "https://chat.example.test/v1".into(),
            image_api_key: "sk-image".into(),
            image_base_url: "https://image.example.test/v1".into(),
            image_skill_installed: true,
        };

        let profile = build_imported_codex_profile("Imported".into(), status, "auth_json".into());

        assert_eq!(profile.image_api_key, "sk-image");
        assert_eq!(profile.image_base_url, "https://image.example.test/v1");
        assert!(profile.is_active);
    }

    #[test]
    fn codex_image_skill_installer_writes_a_complete_skill_directory() {
        let skill_dir = std::env::temp_dir().join(format!(
            "varswitch-image-skill-test-{}",
            uuid::Uuid::new_v4()
        ));

        install_codex_image_skill_at(&skill_dir).expect("skill install should succeed");
        let manifest = fs::read_to_string(skill_dir.join("SKILL.md")).unwrap();
        let script = fs::read_to_string(skill_dir.join("scripts/generate-image.ps1")).unwrap();

        assert!(manifest.contains("varswitch-imagegen"));
        assert!(manifest.contains("generate-image.ps1"));
        assert!(script.contains("/images/generations"));
        assert!(!script.contains("sk-image"));

        fs::remove_dir_all(skill_dir).unwrap();
    }

    #[test]
    fn legacy_gpt_image_2_values_remain_readable_for_migration() {
        let config = r#"
model_provider = "custom"

[model_providers.custom]
base_url = "https://chat.example.com/v1"

[gpt_image_2]
api_key = "sk-image"
base_url = "https://image.example.com/v1"
model = "gpt-image-2"
"#;

        assert_eq!(
            toml_section_value(config, "gpt_image_2", "api_key"),
            "sk-image"
        );
        assert_eq!(
            toml_section_value(config, "gpt_image_2", "base_url"),
            "https://image.example.com/v1"
        );
    }

    #[test]
    fn toml_section_value_reads_provider_base_without_image_base_conflict() {
        let config = r#"
model_provider = "custom"

[gpt_image_2]
base_url = "https://image.example.com/v1"

[model_providers.custom]
base_url = "https://chat.example.com/v1"
"#;

        let provider_name = toml_line_value(config, "model_provider");
        assert_eq!(
            toml_section_value(
                config,
                &format!("model_providers.{provider_name}"),
                "base_url"
            ),
            "https://chat.example.com/v1"
        );
    }

    #[test]
    fn visible_codex_threads_filters_toolbox_trash_only() {
        let mut state = ToolboxState::default();
        state.synced_codex_threads = vec![
            CodexThreadRecord {
                id: "keep".into(),
                thread_name: "Keep".into(),
                ..CodexThreadRecord::default()
            },
            CodexThreadRecord {
                id: "hidden".into(),
                thread_name: "Hidden".into(),
                ..CodexThreadRecord::default()
            },
        ];
        state
            .trashed_codex_threads
            .push(codex_thread_to_trash_record(
                state.synced_codex_threads[1].clone(),
                "2026-07-03 00:00:00",
            ));

        let visible = visible_codex_threads(&state);

        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].id, "keep");
        assert_eq!(state.synced_codex_threads.len(), 2);
    }

    #[test]
    fn websocket_accept_key_matches_rfc_example() {
        assert_eq!(
            websocket_accept_key("dGhlIHNhbXBsZSBub25jZQ=="),
            "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
        );
    }

    #[test]
    fn smart_control_preview_truncates_long_text() {
        assert_eq!(smart_control_preview("abcdef", 10), "abcdef");
        assert_eq!(smart_control_preview("abcdef", 3), "abc…");
    }

    #[test]
    fn try_smart_control_dispatch_returns_none_when_not_connected() {
        SMART_CONTROL_REMOTE_CONNECTED.store(false, Ordering::SeqCst);
        let result =
            try_smart_control_dispatch("thread", "hello").expect("dispatch should not fail");
        assert!(result.is_none());
    }

    #[test]
    fn smart_control_turn_start_message_has_expected_shape() {
        let message = smart_control_turn_start_message(42, "thread-1", " hello ");
        assert_eq!(message.get("id").and_then(|v| v.as_u64()), Some(42));
        assert_eq!(
            message.get("method").and_then(|v| v.as_str()),
            Some("turn/start")
        );
        assert_eq!(
            message
                .get("params")
                .and_then(|v| v.get("threadId"))
                .and_then(|v| v.as_str()),
            Some("thread-1")
        );
        assert_eq!(
            message
                .get("params")
                .and_then(|v| v.get("input"))
                .and_then(|v| v.as_array())
                .and_then(|items| items.first())
                .and_then(|v| v.get("text"))
                .and_then(|v| v.as_str()),
            Some("hello")
        );
    }

    #[test]
    fn smart_control_wraps_client_message_with_envelope_identity() {
        smart_control_reset_client_state();
        let message = smart_control_initialize_message(7);
        let envelope = smart_control_wrap_client_message(message);
        assert_eq!(
            envelope.get("type").and_then(|v| v.as_str()),
            Some("client_message")
        );
        assert!(envelope.get("client_id").and_then(|v| v.as_str()).is_some());
        assert!(envelope.get("stream_id").and_then(|v| v.as_str()).is_some());
        assert_eq!(envelope.get("seq_id").and_then(|v| v.as_u64()), Some(1));
        assert_eq!(
            envelope
                .get("message")
                .and_then(|v| v.get("method"))
                .and_then(|v| v.as_str()),
            Some("initialize")
        );
    }

    #[test]
    fn smart_control_reassembles_server_message_chunk() {
        smart_control_reset_client_state();
        let message = json!({"id": 9, "result": {"text": "chunked"}});
        let raw = serde_json::to_vec(&message).unwrap();
        let mid = raw.len() / 2;
        let first = base64::engine::general_purpose::STANDARD.encode(&raw[..mid]);
        let second = base64::engine::general_purpose::STANDARD.encode(&raw[mid..]);
        let base = json!({
            "type": "server_message_chunk",
            "client_id": "c",
            "stream_id": "s",
            "seq_id": 3,
            "segment_count": 2,
            "message_size_bytes": raw.len(),
        });
        let mut chunk1 = base.clone();
        chunk1["segment_id"] = json!(0);
        chunk1["message_chunk_base64"] = json!(first);
        assert!(smart_control_reassemble_server_chunk(&chunk1).is_none());
        let mut chunk2 = base;
        chunk2["segment_id"] = json!(1);
        chunk2["message_chunk_base64"] = json!(second);
        assert_eq!(
            smart_control_reassemble_server_chunk(&chunk2)
                .and_then(|v| v.get("result").and_then(|r| r.get("text")).cloned())
                .and_then(|v| v.as_str().map(str::to_string)),
            Some("chunked".into())
        );
    }

    #[test]
    fn smart_control_extracts_approval_request() {
        let value = json!({
            "type": "server_message",
            "client_id": "c",
            "stream_id": "s",
            "seq_id": 1,
            "message": {
                "id": "approval-1",
                "method": "approval/request",
                "params": {
                    "title": "Run command",
                    "body": "Allow command?",
                    "options": [{"label": "approve"}, {"label": "deny"}]
                }
            }
        });
        let approval = smart_control_extract_approval(&value).expect("approval");
        assert_eq!(approval.request_id, "approval-1");
        assert_eq!(approval.title, "Run command");
        assert_eq!(approval.options, vec!["approve", "deny"]);
    }

    #[test]
    fn smart_control_turn_accumulator_completes_pending() {
        let request_id = 777;
        smart_control_register_pending(request_id);
        smart_control_observe_turn_item(&json!({
            "id": request_id,
            "delta": {"text": "hello "}
        }));
        smart_control_observe_turn_item(&json!({
            "id": request_id,
            "delta": {"text": "world"},
            "status": "completed"
        }));
        let result = smart_control_wait_pending(request_id, Duration::from_millis(50))
            .expect("pending result");
        assert_eq!(result.text, "hello world");
    }

    #[test]
    fn smart_control_delta_does_not_complete_pending_before_done() {
        let request_id = 778;
        smart_control_register_pending(request_id);
        smart_control_maybe_complete_pending(&json!({
            "id": request_id,
            "delta": {"text": "partial "}
        }));
        let pending_done = smart_control_pending_store()
            .0
            .lock()
            .ok()
            .and_then(|pending| pending.get(&request_id).map(|result| result.done))
            .unwrap_or(true);
        assert!(!pending_done);
        smart_control_maybe_complete_pending(&json!({
            "id": request_id,
            "delta": {"text": "done"},
            "status": "completed"
        }));
        let result = smart_control_wait_pending(request_id, Duration::from_millis(50))
            .expect("pending result");
        assert_eq!(result.text, "partial done");
    }

    #[test]
    fn codex_inject_script_does_not_force_scroll_layout() {
        let script = codex_inject_send_script("\"hello\"");
        assert!(
            !script.contains("scrollIntoView"),
            "CDP compatibility injection must not force-scroll the Codex window"
        );
    }

    #[test]
    fn codex_debug_port_probe_relaunches_app_when_port_is_missing() {
        let source = include_str!("lib.rs");
        let marker = "fn codex_debug_port_or_relaunch()";
        let start = source.find(marker).expect("function exists");
        let tail = &source[start..];
        let open = tail.find('{').expect("function body starts");
        let mut depth = 0i32;
        let mut end = tail.len();
        for (idx, ch) in tail[open..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = open + idx + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        let body = &tail[..end];
        assert!(body.contains("relaunch_codex_with_debug_port"));
        assert!(body.contains("CODEX_PREFERRED_DEBUG_PORT"));
    }

    #[test]
    fn packaged_codex_activation_casts_com_object_inside_csharp() {
        let source = include_str!("lib.rs");
        assert!(source.contains("public static class PackagedAppActivator"));
        assert!(source.contains(
            "var manager = (IApplicationActivationManager)new ApplicationActivationManager();"
        ));
        assert!(source.contains("[VarSwitch.PackagedAppActivator]::Activate"));
    }

    #[test]
    fn smart_control_extracts_text_from_response_shapes() {
        assert_eq!(
            smart_control_extract_text(&json!({"id": 1, "result": {"text": "hello"}})),
            "hello"
        );
        assert_eq!(
            smart_control_extract_text(&json!({"id": 1, "delta": {"text": "world"}})),
            "world"
        );
        assert_eq!(
            smart_control_extract_error(&json!({"id": 1, "error": {"message": "bad"}})),
            "bad"
        );
    }

    #[test]
    fn push_smart_control_event_keeps_ring_buffer_bounded() {
        for idx in 0..90 {
            push_smart_control_event(SmartControlEvent {
                received_at: idx.to_string(),
                event_type: "test".into(),
                message_id: idx.to_string(),
                method: "noop".into(),
                raw_preview: "x".into(),
            });
        }
        let snapshot = smart_control_debug_snapshot();
        assert!(snapshot.events.len() <= 80);
        assert_eq!(
            snapshot
                .last_event
                .as_ref()
                .map(|event| event.message_id.as_str()),
            Some("89")
        );
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
    fn models_endpoint_candidates_support_v1_and_plain_base_urls() {
        assert_eq!(
            models_endpoint_candidates("https://api.example.com/v1").unwrap(),
            vec!["https://api.example.com/v1/models"]
        );
        assert_eq!(
            models_endpoint_candidates("https://api.example.com").unwrap(),
            vec![
                "https://api.example.com/v1/models",
                "https://api.example.com/models"
            ]
        );
    }

    #[test]
    fn extract_model_ids_handles_openai_and_custom_shapes() {
        let ids = extract_model_ids(&json!({
            "data": [
                {"id": "gpt-5.5"},
                {"id": "gpt-5.5"},
                {"name": "claude-opus-4.8"},
                ""
            ]
        }));

        assert_eq!(ids, vec!["claude-opus-4.8", "gpt-5.5"]);
        assert_eq!(
            extract_model_ids(&json!({"models": ["kimi-k2.7", {"id": "glm-5.1"}]})),
            vec!["glm-5.1", "kimi-k2.7"]
        );
    }

    #[test]
    fn known_editors_only_include_primary_supported_editors() {
        let editor_ids: Vec<&str> = KNOWN_EDITORS.iter().map(|editor| editor.id).collect();
        assert!(
            editor_ids.contains(&"vscode") && editor_ids.contains(&"cursor"),
            "primary supported editors should be present"
        );
        assert!(
            !editor_ids.contains(&"vscode-insiders")
                && !editor_ids.contains(&"windsurf")
                && !editor_ids.contains(&"trae")
                && !editor_ids.contains(&"vscodium"),
            "VS Code Insiders, Windsurf, Trae, and VSCodium should not be shown in settings editor paths"
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
    fn extract_latest_version_from_download_page_uses_newest_installer_version() {
        let html = r#"
            <a href="./releases/VarSwitch_1.0.0_x64-setup.exe">VarSwitch_1.0.0_x64-setup.exe</a>
            <a href="./releases/VarSwitch_1.2.3_x64-setup.exe">VarSwitch_1.2.3_x64-setup.exe</a>
        "#;

        assert_eq!(
            extract_latest_version_from_download_page(html),
            Some("v1.2.3".into())
        );
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
            "discard-style loopback proxy should be ignored instead of breaking update requests"
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

/// 启动时把主窗口收进当前显示器的可视范围。
///
/// tauri.conf.json 里的默认尺寸是按大屏设计的；在 1080p 屏幕、或者系统缩放
/// 125% / 150% 的机器上，窗口会比屏幕还高，底部直接跑到屏幕外面。
/// 表现就是：弹窗（新增配置向导等）右下角的「保存 / 保存并启用」按钮看不见，
/// 只有最大化 / 全屏时才出现。这里统一按显示器工作区做一次收缩 + 重新居中。
fn fit_window_to_work_area(window: &tauri::WebviewWindow) {
    let monitor = match window.current_monitor() {
        Ok(Some(m)) => Some(m),
        _ => match window.primary_monitor() {
            Ok(Some(m)) => Some(m),
            _ => None,
        },
    };
    let monitor = match monitor {
        Some(m) => m,
        None => {
            log_info!("[window] 未能获取显示器信息，跳过窗口尺寸收缩");
            return;
        }
    };

    // work_area 已经排除了任务栏 / Dock 占用的区域
    let work_area = monitor.work_area();
    let area_size = work_area.size;
    let area_pos = work_area.position;
    if area_size.width == 0 || area_size.height == 0 {
        return;
    }

    // 再留一点余量给窗口阴影和边框
    let max_width = area_size.width.saturating_sub(16).max(640);
    let max_height = area_size.height.saturating_sub(16).max(480);

    let current = match window.outer_size() {
        Ok(size) => size,
        Err(error) => {
            log_error!("[window] 读取窗口尺寸失败: {error}");
            return;
        }
    };

    let target_width = current.width.min(max_width).max(1);
    let target_height = current.height.min(max_height).max(1);

    if target_width != current.width || target_height != current.height {
        if let Err(error) = window.set_size(tauri::PhysicalSize::new(target_width, target_height)) {
            log_error!("[window] 调整窗口尺寸失败: {error}");
            return;
        }
        log_info!(
            "[window] 窗口尺寸超出屏幕，已从 {}x{} 收缩到 {}x{}",
            current.width,
            current.height,
            target_width,
            target_height
        );
    }

    // 收缩后重新居中，避免窗口标题栏或底部仍然在屏幕外
    let offset_x = area_size.width.saturating_sub(target_width) / 2;
    let offset_y = area_size.height.saturating_sub(target_height) / 2;
    let x = area_pos.x + offset_x as i32;
    let y = area_pos.y + offset_y as i32;
    if let Err(error) = window.set_position(tauri::PhysicalPosition::new(x, y)) {
        log_error!("[window] 重新居中窗口失败: {error}");
    }
}

/// 把主窗口从托盘/最小化状态恢复并聚焦。
/// 托盘菜单、托盘点击、单实例二次启动都复用这一段逻辑。
fn focus_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    } else {
        log_error!("[window] 未找到主窗口 main，无法聚焦");
    }
}

pub fn run() {
    tauri::Builder::default()
        // 单实例插件必须最先注册：第二个进程启动时会把参数转发给已运行的实例并自行退出
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            log_info!("[single-instance] 检测到重复启动，聚焦已有窗口");
            focus_main_window(app);
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(AppState {
            cancel_flag: AtomicBool::new(false),
        })
        .setup(|app| {
            // 初始化日志系统（写入 app_data_dir/logs/varswitch.log）
            init_logging(&app.handle());

            // 当前激活的 Claude 配置若走 OpenAI 协议转换，重启后恢复本地代理，
            // 否则系统环境变量里指向 127.0.0.1 的地址会连不上。
            {
                let profiles = read_profiles(&app.handle());
                if let Some(active) = profiles
                    .profiles
                    .iter()
                    .find(|p| p.is_active && p.api_format == "openai_chat")
                {
                    claude_proxy::set_upstream(Some(claude_proxy::ProxyUpstream {
                        base_url: active.base_url.clone(),
                        api_key: active.api_key.clone(),
                        model: active.model_id.clone(),
                    }));
                    match claude_proxy::ensure_server() {
                        Ok(_) => log_info!(
                            "[claude-proxy] 启动恢复：{} → {}",
                            active.name,
                            active.base_url
                        ),
                        Err(e) => log_error!("[claude-proxy] 启动恢复失败：{e}"),
                    }
                }
            }

            // 读取应用设置
            let settings = read_app_settings(&app.handle());
            let silent_startup = settings.silent_startup;

            // B10: 上次退出时手机远程是开启状态，本次启动自动恢复连接。
            // 延迟 1.5 秒等窗口和前端就绪，再在后台线程拉起各通道，不阻塞启动。
            let startup_state = read_toolbox_state(&app.handle());
            if startup_state.mobile_remote.enabled {
                log_info!("[startup] 检测到 mobile_remote.enabled，自动恢复手机通道连接");
                let app_for_restore = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(1500));
                    MOBILE_REMOTE_CANCEL_REQUESTED.store(false, Ordering::SeqCst);
                    if MOBILE_REMOTE_START_ACTIVE.swap(true, Ordering::SeqCst) {
                        log_info!("[startup] 已有启动流程在跑，跳过自动恢复");
                        return;
                    }
                    run_mobile_remote_start(app_for_restore);
                });
            }

            // Build tray menu
            let show_item = MenuItemBuilder::with_id("show", "显示主窗口").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "退出").build(app)?;
            let menu = MenuBuilder::new(app)
                .items(&[&show_item, &quit_item])
                .build()?;

            // Build tray icon
            let tray_builder = TrayIconBuilder::new().tooltip("VarSwitch").menu(&menu);
            // 图标不可用时不阻断启动，仅无图标显示
            let tray_builder = if let Some(icon) = app.default_window_icon() {
                tray_builder.icon(icon.clone())
            } else {
                tray_builder
            };
            tray_builder
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" => {
                        focus_main_window(app);
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
                        focus_main_window(tray.app_handle());
                    }
                })
                .build(app)?;

            // 把窗口收缩到当前显示器的可用工作区内。
            // 配置里的默认尺寸偏大，在 1080p 或开了缩放的屏幕上会超出可视范围，
            // 导致弹窗底部（保存 / 保存并启用按钮）落到屏幕外，只有最大化才看得见。
            if let Some(window) = app.get_webview_window("main") {
                fit_window_to_work_area(&window);
            }

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
            fetch_available_models,
            add_profile,
            update_profile,
            delete_profile,
            get_claude_proxy_status,
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
            get_grok_profiles,
            add_grok_profile,
            update_grok_profile,
            delete_grok_profile,
            switch_grok_profile,
            import_grok_current,
            get_grok_status,
            get_grok_diagnostics,
            backup_grok_runtime,
            open_grok_config_folder,
            get_gemini_profiles,
            add_gemini_profile,
            update_gemini_profile,
            delete_gemini_profile,
            switch_gemini_profile,
            import_gemini_current,
            get_gemini_status,
            get_codex_diagnostics,
            backup_codex_runtime,
            get_codex_toolbox,
            start_smart_control,
            get_smart_control_debug,
            submit_smart_control_approval,
            repair_openai_bundled_plugins,
            enable_codex_builtin_plugin,
            enable_important_codex_builtin_plugins,
            apply_plugin_marketplace,
            sync_codex_sessions,
            trash_codex_sessions,
            restore_codex_sessions,
            select_mobile_thread,
            configure_mobile_channel,
            start_lark_bot_registration,
            poll_lark_bot_registration,
            open_lark_bot_launcher,
            clear_mobile_channel_binding,
            start_qq_qr_binding,
            cancel_qq_qr_binding,
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
            install_app_update,
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
            usage_stats::get_usage_dashboard,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
