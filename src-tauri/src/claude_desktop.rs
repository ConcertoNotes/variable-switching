//! Claude Desktop 的 MCP 配置接入。
//!
//! Claude Desktop（Anthropic 官方桌面客户端）把 MCP 服务器配置存在独立的
//! claude_desktop_config.json 里，格式与 ~/.claude.json 相同：
//! { "mcpServers": { "<name>": { command/args/env } } }。
//! 文件里可能还有 Desktop 自己的其他键（如 globalShortcut 等），写回时必须原样保留。
//!
//! 本模块只做三件事：读取（聚合展示用）、按名写入 / 更新、按名删除。
//! 所有落盘都走 crate::write_file_atomic，避免写坏用户配置。

use std::fs;
use std::path::{Path, PathBuf};

/// 用户主目录（macOS / Linux 分支拼路径用）。
/// 与 lib.rs 的 home_dir 逻辑一致，这里独立实现一份以减少跨文件耦合。
#[cfg(target_os = "windows")]
fn desktop_home_dir() -> PathBuf {
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(PathBuf::from)
        .unwrap_or_default()
}

#[cfg(not(target_os = "windows"))]
fn desktop_home_dir() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    PathBuf::from(home)
}

/// Claude Desktop 配置文件的平台路径：
/// - Windows: %APPDATA%\Claude\claude_desktop_config.json
/// - macOS:   ~/Library/Application Support/Claude/claude_desktop_config.json
/// - Linux:   ~/.config/Claude/claude_desktop_config.json
pub fn claude_desktop_config_path() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        let local_app_data = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| desktop_home_dir().join("AppData").join("Local"));
        let app_data = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| desktop_home_dir().join("AppData").join("Roaming"));
        claude_desktop_config_path_for_roots(&local_app_data, &app_data)
    }
    #[cfg(target_os = "macos")]
    {
        desktop_home_dir()
            .join("Library")
            .join("Application Support")
            .join("Claude")
            .join("claude_desktop_config.json")
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        desktop_home_dir()
            .join(".config")
            .join("Claude")
            .join("claude_desktop_config.json")
    }
}

fn claude_config_path_in(root: &Path) -> PathBuf {
    root.join("Claude").join("claude_desktop_config.json")
}

/// 在 Windows 的 MSIX 与传统安装之间选择 Claude Desktop 配置路径。
///
/// MSIX 当前把配置放在 LOCALAPPDATA；若只发现传统 APPDATA/Claude，则继续兼容旧安装。
/// 两者都不存在时默认使用 LOCALAPPDATA，确保首次启动 MSIX 时写入正确目录。
fn claude_desktop_config_path_for_roots(local_app_data: &Path, app_data: &Path) -> PathBuf {
    let local_dir = local_app_data.join("Claude");
    let app_dir = app_data.join("Claude");
    if local_dir.exists() || !app_dir.exists() {
        claude_config_path_in(local_app_data)
    } else {
        claude_config_path_in(app_data)
    }
}

fn claude_package_data_exists(local_app_data: &Path) -> bool {
    let packages = local_app_data.join("Packages");
    let Ok(entries) = fs::read_dir(packages) else {
        return false;
    };
    entries.flatten().any(|entry| {
        entry.file_type().is_ok_and(|file_type| file_type.is_dir())
            && entry
                .file_name()
                .to_string_lossy()
                .to_ascii_lowercase()
                .starts_with("claude_")
    })
}

fn claude_desktop_installation_evidence_for_roots(
    local_app_data: &Path,
    app_data: &Path,
    appx_installed: bool,
) -> bool {
    appx_installed
        || claude_package_data_exists(local_app_data)
        || local_app_data.join("Claude").exists()
        || app_data.join("Claude").exists()
}

/// 是否检测到 Claude Desktop：配置文件或其父目录（%APPDATA%\Claude 等）
/// 存在即视为已安装。这是 best-effort 判定，Desktop 未装时该目录一般不存在。
pub fn claude_desktop_installed() -> bool {
    #[cfg(target_os = "windows")]
    {
        let local_app_data = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| desktop_home_dir().join("AppData").join("Local"));
        let app_data = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| desktop_home_dir().join("AppData").join("Roaming"));
        return claude_desktop_installation_evidence_for_roots(
            &local_app_data,
            &app_data,
            claude_package_data_exists(&local_app_data),
        );
    }

    #[cfg(not(target_os = "windows"))]
    {
        let path = claude_desktop_config_path();
        path.exists() || path.parent().is_some_and(|dir| dir.exists())
    }
}

/// 读取指定配置文件里的 mcpServers 对象。
/// 文件不存在 / 解析失败 / mcpServers 不是对象时都返回空表（读取按尽力处理，不报错）。
fn read_desktop_servers_at(path: &Path) -> serde_json::Map<String, serde_json::Value> {
    let Ok(text) = fs::read_to_string(path) else {
        return serde_json::Map::new();
    };
    serde_json::from_str::<serde_json::Value>(&text)
        .ok()
        .and_then(|root| {
            root.get("mcpServers")
                .and_then(|v| v.as_object())
                .cloned()
        })
        .unwrap_or_default()
}

/// 把整份配置写回文件：自动创建父目录，pretty JSON + 原子写。
fn write_desktop_config_at(path: &Path, root: &serde_json::Value) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建 Claude Desktop 配置目录失败: {e}"))?;
    }
    let text = serde_json::to_string_pretty(root)
        .map_err(|e| format!("序列化 Claude Desktop 配置失败: {e}"))?;
    crate::write_file_atomic(path, &text)
}

/// 写入 / 更新 mcpServers[name]，保留文件里其他所有键。
/// 文件不存在时新建；文件存在但 JSON 损坏时报错（避免覆盖丢失用户原有内容）。
fn set_desktop_server_at(
    path: &Path,
    name: &str,
    config: &serde_json::Value,
) -> Result<(), String> {
    let mut root = if path.exists() {
        let text = fs::read_to_string(path)
            .map_err(|e| format!("读取 Claude Desktop 配置失败: {e}"))?;
        serde_json::from_str::<serde_json::Value>(&text)
            .map_err(|e| format!("解析 Claude Desktop 配置失败（不覆盖损坏文件）: {e}"))?
    } else {
        serde_json::json!({})
    };
    if !root.is_object() {
        // 极端情况：文件是合法 JSON 但顶层不是对象（如数组）。视为损坏，拒绝覆盖。
        return Err("Claude Desktop 配置顶层不是 JSON 对象".into());
    }
    if !root.get("mcpServers").is_some_and(|v| v.is_object()) {
        root["mcpServers"] = serde_json::json!({});
    }
    root["mcpServers"][name] = config.clone();
    write_desktop_config_at(path, &root)
}

/// 删除 mcpServers[name]。文件不存在、解析失败或条目不存在时静默成功（不动文件）。
fn remove_desktop_server_at(path: &Path, name: &str) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let Ok(text) = fs::read_to_string(path) else {
        return Ok(());
    };
    let Ok(mut root) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Ok(());
    };
    let removed = root
        .get_mut("mcpServers")
        .and_then(|v| v.as_object_mut())
        .is_some_and(|servers| servers.remove(name).is_some());
    if removed {
        write_desktop_config_at(path, &root)
    } else {
        Ok(())
    }
}

/// 读取 Claude Desktop 的 MCP 状态，供统一面板聚合展示。
/// 返回 { "installed": bool, "configPath": str, "servers": { name: config, ... } }；
/// 配置文件不存在时 servers 为空对象，不报错。
#[tauri::command]
pub fn get_claude_desktop_mcp() -> Result<serde_json::Value, String> {
    let path = claude_desktop_config_path();
    let servers = read_desktop_servers_at(&path);
    Ok(serde_json::json!({
        "installed": claude_desktop_installed(),
        "configPath": path.to_string_lossy(),
        "servers": serde_json::Value::Object(servers),
    }))
}

/// 写入 / 更新 Claude Desktop 的一个 MCP 服务器条目（保留文件其他键）。
#[tauri::command]
pub fn set_claude_desktop_mcp_server(
    name: String,
    config: serde_json::Value,
) -> Result<(), String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("服务器名称不能为空".into());
    }
    set_desktop_server_at(&claude_desktop_config_path(), &name, &config)
}

/// 从 Claude Desktop 配置里删除一个 MCP 服务器条目（不存在则静默成功）。
#[tauri::command]
pub fn remove_claude_desktop_mcp_server(name: String) -> Result<(), String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("服务器名称不能为空".into());
    }
    remove_desktop_server_at(&claude_desktop_config_path(), &name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_path_prefers_local_appdata_msix_directory() {
        let root = std::env::temp_dir().join(format!(
            "varswitch-claude-desktop-path-priority-{}",
            uuid::Uuid::new_v4()
        ));
        let local = root.join("local");
        let appdata = root.join("roaming");
        let local_config = local.join("Claude").join("claude_desktop_config.json");
        fs::create_dir_all(local_config.parent().unwrap()).unwrap();
        fs::write(&local_config, b"{}").unwrap();

        assert_eq!(
            claude_desktop_config_path_for_roots(&local, &appdata),
            local_config
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn config_path_falls_back_to_roaming_appdata_for_legacy_install() {
        let root = std::env::temp_dir().join(format!(
            "varswitch-claude-desktop-path-legacy-{}",
            uuid::Uuid::new_v4()
        ));
        let local = root.join("local");
        let appdata = root.join("roaming");
        let roaming_config = appdata.join("Claude").join("claude_desktop_config.json");
        fs::create_dir_all(roaming_config.parent().unwrap()).unwrap();
        fs::write(&roaming_config, b"{}").unwrap();

        assert_eq!(
            claude_desktop_config_path_for_roots(&local, &appdata),
            roaming_config
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn claude_3p_directory_alone_is_not_installation_evidence() {
        let root = std::env::temp_dir().join(format!(
            "varswitch-claude-desktop-install-evidence-{}",
            uuid::Uuid::new_v4()
        ));
        let local = root.join("local");
        let appdata = root.join("roaming");
        fs::create_dir_all(local.join("Claude-3p")).unwrap();

        assert!(!claude_desktop_installation_evidence_for_roots(
            &local, &appdata, false
        ));
        let _ = fs::remove_dir_all(root);
    }

    /// 每个测试用独立的临时目录，避免并行测试互相干扰
    fn temp_config_path(tag: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!(
                "varswitch-claude-desktop-{tag}-{}",
                uuid::Uuid::new_v4()
            ))
            .join("Claude")
            .join("claude_desktop_config.json")
    }

    fn cleanup(path: &Path) {
        // 删掉临时目录树（best-effort）
        if let Some(dir) = path.parent().and_then(|p| p.parent()) {
            let _ = fs::remove_dir_all(dir);
        }
    }

    #[test]
    fn set_preserves_unrelated_keys() {
        let path = temp_config_path("preserve");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        // 预置带无关键的现有配置（模拟 Desktop 自己写入的字段）
        fs::write(
            &path,
            r#"{
  "globalShortcut": "Ctrl+Space",
  "theme": { "mode": "dark" },
  "mcpServers": {
    "existing": { "command": "old-cmd" }
  }
}"#,
        )
        .unwrap();

        let config = serde_json::json!({ "command": "npx", "args": ["-y", "demo-mcp"] });
        set_desktop_server_at(&path, "demo", &config).unwrap();

        let root: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        // 无关键必须原样保留
        assert_eq!(root["globalShortcut"], "Ctrl+Space");
        assert_eq!(root["theme"]["mode"], "dark");
        // 原有条目不受影响，新条目写入成功
        assert_eq!(root["mcpServers"]["existing"]["command"], "old-cmd");
        assert_eq!(root["mcpServers"]["demo"], config);
        cleanup(&path);
    }

    #[test]
    fn set_creates_file_and_parent_dirs() {
        let path = temp_config_path("create");
        assert!(!path.exists());

        let config = serde_json::json!({ "command": "uvx", "args": ["mcp-server-git"] });
        set_desktop_server_at(&path, "git", &config).unwrap();

        assert!(path.exists());
        let root: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(root["mcpServers"]["git"], config);
        cleanup(&path);
    }

    #[test]
    fn remove_missing_entry_or_file_is_silent_ok() {
        let path = temp_config_path("remove");
        // 文件不存在：静默成功
        remove_desktop_server_at(&path, "nope").unwrap();
        assert!(!path.exists());

        // 文件存在但条目不存在：静默成功且不改动文件内容
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let original = r#"{ "mcpServers": { "keep": { "command": "x" } }, "other": 1 }"#;
        fs::write(&path, original).unwrap();
        remove_desktop_server_at(&path, "nope").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), original);

        // 条目存在：删除后其他键保留
        remove_desktop_server_at(&path, "keep").unwrap();
        let root: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert!(root["mcpServers"].as_object().unwrap().is_empty());
        assert_eq!(root["other"], 1);
        cleanup(&path);
    }

    #[test]
    fn read_missing_file_returns_empty_servers() {
        let path = temp_config_path("read");
        assert!(read_desktop_servers_at(&path).is_empty());

        // 损坏 JSON 同样按空表处理（读取按尽力，不报错）
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "{ not valid json").unwrap();
        assert!(read_desktop_servers_at(&path).is_empty());
        cleanup(&path);
    }
}
