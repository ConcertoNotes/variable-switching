use serde::{Deserialize, Serialize};
use std::cmp::Ordering as CmpOrdering;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{
    Emitter, Manager, State,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

#[cfg(target_os = "windows")]
use winreg::enums::HKEY_CURRENT_USER;
#[cfg(target_os = "windows")]
use winreg::RegKey;

const AUTH_TOKEN_ENV: &str = "ANTHROPIC_AUTH_TOKEN";
const AUTH_KEY_ENV: &str = "ANTHROPIC_AUTH_KEY";
const LEGACY_AUTH_ENV: &str = "ANTHROPIC_API_KEY";
const BASE_URL_ENV: &str = "ANTHROPIC_BASE_URL";
const SWITCH_TOTAL_STEPS: u32 = 6;
const GITHUB_REPO_URL: &str = "https://github.com/ConcertoNotes/variable-switching";
const GITHUB_LATEST_RELEASE_API: &str =
    "https://api.github.com/repos/ConcertoNotes/variable-switching/releases/latest";

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
    let dir = app.path().app_data_dir().expect("no app data dir");
    fs::create_dir_all(&dir).ok();
    dir
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

fn home_dir() -> PathBuf {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_default();
    PathBuf::from(home)
}

fn claude_settings_path() -> PathBuf {
    home_dir().join(".claude").join("settings.json")
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
    EditorDef { id: "vscode", display_name: "VS Code", win_appdata_dir: "Code", win_program_dirs: &["Microsoft VS Code"] },
    EditorDef { id: "vscode-insiders", display_name: "VS Code Insiders", win_appdata_dir: "Code - Insiders", win_program_dirs: &["Microsoft VS Code Insiders"] },
    EditorDef { id: "cursor", display_name: "Cursor", win_appdata_dir: "Cursor", win_program_dirs: &["Cursor"] },
    EditorDef { id: "windsurf", display_name: "Windsurf", win_appdata_dir: "Windsurf", win_program_dirs: &["Windsurf"] },
    EditorDef { id: "trae", display_name: "Trae", win_appdata_dir: "Trae", win_program_dirs: &["Trae"] },
    EditorDef { id: "vscodium", display_name: "VSCodium", win_appdata_dir: "VSCodium", win_program_dirs: &["VSCodium"] },
];

#[cfg(target_os = "macos")]
const KNOWN_EDITORS: &[EditorDef] = &[
    EditorDef { id: "vscode", display_name: "VS Code", mac_app_support_dir: "Code" },
    EditorDef { id: "vscode-insiders", display_name: "VS Code Insiders", mac_app_support_dir: "Code - Insiders" },
    EditorDef { id: "cursor", display_name: "Cursor", mac_app_support_dir: "Cursor" },
    EditorDef { id: "windsurf", display_name: "Windsurf", mac_app_support_dir: "Windsurf" },
    EditorDef { id: "trae", display_name: "Trae", mac_app_support_dir: "Trae" },
    EditorDef { id: "vscodium", display_name: "VSCodium", mac_app_support_dir: "VSCodium" },
];

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
const KNOWN_EDITORS: &[EditorDef] = &[
    EditorDef { id: "vscode", display_name: "VS Code", linux_config_dir: "Code" },
    EditorDef { id: "vscode-insiders", display_name: "VS Code Insiders", linux_config_dir: "Code - Insiders" },
    EditorDef { id: "cursor", display_name: "Cursor", linux_config_dir: "Cursor" },
    EditorDef { id: "windsurf", display_name: "Windsurf", linux_config_dir: "Windsurf" },
    EditorDef { id: "trae", display_name: "Trae", linux_config_dir: "Trae" },
    EditorDef { id: "vscodium", display_name: "VSCodium", linux_config_dir: "VSCodium" },
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
        vec![
            home_dir()
                .join("Library")
                .join("Application Support")
                .join(editor.mac_app_support_dir),
        ]
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

    if default_path.parent().map(|parent| parent.exists()).unwrap_or(false) {
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
    let auth_name = pick_auth_name(env.contains_key(AUTH_TOKEN_ENV), env.contains_key(AUTH_KEY_ENV));
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

// ── Tauri Commands ──────────────────────────────────

#[tauri::command]
fn get_profiles(app: tauri::AppHandle) -> ProfilesData {
    read_profiles(&app)
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
    if let Some(mid) = model_id {
        p.model_id = mid.trim().to_string();
    }
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
            Ok(_) => { details.editors.insert(editor.id.to_string(), true); }
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
    let mut settings = read_json_or_default(&cp, serde_json::json!({
        "permissions": {
            "allow": [],
            "deny": []
        },
        "env": {}
    }));
    if !settings.is_object() {
        settings = serde_json::json!({});
    }
    if !settings
        .get("env")
        .map(|v| v.is_object())
        .unwrap_or(false)
    {
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
    let mut editors: HashMap<String, LocationStatus> = HashMap::new();

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
        std::process::Command::new("explorer")
            .arg(dir.to_string_lossy().to_string())
            .spawn()
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

#[tauri::command]
async fn check_app_update(app: tauri::AppHandle) -> Result<UpdateCheckResult, String> {
    let current_version = app.package_info().version.to_string();

    tauri::async_runtime::spawn_blocking(move || {
        let release = fetch_latest_release()?;
        let asset =
            select_release_asset(&release.assets, std::env::consts::OS, std::env::consts::ARCH);

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

        let asset =
            select_release_asset(&release.assets, std::env::consts::OS, std::env::consts::ARCH)
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
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
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
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
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
    let branch = if branch.trim().is_empty() { "main".to_string() } else { branch.trim().to_string() };
    let mut data = read_skill_repos(&app);
    if data.repos.iter().any(|r| r.url == url) {
        return Err("Repository already exists".into());
    }
    data.repos.push(SkillRepo { url, branch, enabled: true });
    write_skill_repos(&app, &data)
}

#[tauri::command]
fn remove_skill_repo(app: tauri::AppHandle, url: String) -> Result<(), String> {
    let mut data = read_skill_repos(&app);
    data.repos.retain(|r| r.url != url);
    write_skill_repos(&app, &data)
}

/// 通过 GitHub Tree API 查找仓库中 SKILL.md 的实际路径
fn find_skill_md_in_repo(client: &reqwest::blocking::Client, full_name: &str, branch: &str) -> Result<String, String> {
    let tree_url = format!(
        "https://api.github.com/repos/{}/git/trees/{}?recursive=1",
        full_name, branch
    );
    let resp = client.get(&tree_url).send()
        .map_err(|e| format!("GitHub Tree API error: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("GitHub Tree API returned {}", resp.status()));
    }
    let body: serde_json::Value = resp.json()
        .map_err(|e| format!("JSON parse error: {}", e))?;

    let mut skill_paths: Vec<String> = Vec::new();
    if let Some(tree) = body.get("tree").and_then(|v| v.as_array()) {
        for item in tree {
            if let Some(path) = item.get("path").and_then(|v| v.as_str()) {
                if path.ends_with("SKILL.md") && item.get("type").and_then(|v| v.as_str()) == Some("blob") {
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

/// Download a skill from a URL and install it to ~/.claude/skills/
#[tauri::command]
async fn install_skill_from_url(name: String, url: String) -> Result<(), String> {
    if name.is_empty() {
        return Err("Skill name is required".into());
    }

    let content = if url.is_empty() {
        // No URL — create a placeholder skill
        format!("---\nname: {}\ndescription: Installed from catalog\n---\n\n# {}\n\nThis skill was installed from the catalog. Edit this file to customize it.\n", name, name)
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

        let resp = client.get(&url).send()
            .map_err(|e| format!("GitHub API error: {}", e))?;

        if !resp.status().is_success() {
            return Err(format!("GitHub API returned {}", resp.status()));
        }

        let body: serde_json::Value = resp.json::<serde_json::Value>()
            .map_err(|e| format!("JSON parse error: {}", e))?;

        let mut results = Vec::new();
        if let Some(items) = body.get("items").and_then(|v| v.as_array()) {
            for item in items {
                let full_name = item.get("full_name").and_then(|v| v.as_str()).unwrap_or("");
                let desc = item.get("description").and_then(|v| v.as_str()).unwrap_or("");
                let stars = item.get("stargazers_count").and_then(|v| v.as_u64()).unwrap_or(0);
                let html_url = item.get("html_url").and_then(|v| v.as_str()).unwrap_or("");

                if full_name.is_empty() { continue; }

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

        let resp = client.get(&url).send()
            .map_err(|e| format!("GitHub API error: {}", e))?;

        if !resp.status().is_success() {
            return Err(format!("GitHub API returned {}", resp.status()));
        }

        let body: serde_json::Value = resp.json::<serde_json::Value>()
            .map_err(|e| format!("JSON parse error: {}", e))?;

        let mut skills = Vec::new();
        if let Some(items) = body.get("items").and_then(|v: &serde_json::Value| v.as_array()) {
            for item in items {
                let full_name = item.get("full_name").and_then(|v: &serde_json::Value| v.as_str()).unwrap_or("");
                let desc = item.get("description").and_then(|v: &serde_json::Value| v.as_str()).unwrap_or("");
                let stars = item.get("stargazers_count").and_then(|v: &serde_json::Value| v.as_u64()).unwrap_or(0);
                let default_branch = item.get("default_branch").and_then(|v: &serde_json::Value| v.as_str()).unwrap_or("main");

                if full_name.is_empty() { continue; }

                let html_url = item.get("html_url").and_then(|v: &serde_json::Value| v.as_str()).unwrap_or("");
                // 使用 raw.githubusercontent.com 直接下载 SKILL.md
                let raw_url = format!("https://raw.githubusercontent.com/{}/{}/SKILL.md", full_name, default_branch);
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

    builder.build().map_err(|e| format!("HTTP client error: {}", e))
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
    [
        "x64",
        "x86_64",
        "amd64",
        "arm64",
        "aarch64",
        "universal",
    ]
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
        std::process::Command::new("cmd")
            .args(["/C", "start", "", target])
            .spawn()
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
            .filter(|v| {
                v.get("name").and_then(|n| n.as_str()) == Some("ANTHROPIC_AUTH_TOKEN")
            })
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
        assert!(!has_key, "ANTHROPIC_AUTH_KEY should be removed when token is used");
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

        assert!(!has_key, "ANTHROPIC_AUTH_KEY should be removed and converted to TOKEN");
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
        settings.editor_paths.insert(
            editor.id.to_string(),
            r"C:\Custom\VSCode\User".to_string(),
        );

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

        let selected = select_release_asset(&assets, "macos", "aarch64")
            .expect("should pick a macOS dmg");

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
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).expect("valid tauri.conf.json");

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
            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("VarSwitch")
                .menu(&menu)
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
            let window = app.get_webview_window("main").unwrap();
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
            add_profile,
            update_profile,
            delete_profile,
            switch_profile,
            get_status,
            get_detected_editors,
            import_current,
            snapshot_config,
            restore_config,
            cancel_switch,
            get_app_settings,
            save_app_settings,
            get_app_paths,
            open_folder,
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
