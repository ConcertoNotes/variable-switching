//! Claude Desktop 汉化资源的可还原执行入口。

use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::Manager;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

const LOCALIZATION_DIR_NAME: &str = "claude-desktop-zh-simple";
const LOCALIZATION_SCRIPT_RELATIVE: &str = "scripts/claude-desktop-zh-simple.ps1";
const LOCALIZATION_STATIC_FILES: &[&str] = &[
    "scripts/claude-desktop-zh-simple.ps1",
    "translation_memory.json",
    "version.json",
    "LICENSE",
];

fn localization_action_args(action: &str) -> Result<Vec<String>, String> {
    let action = action.trim().to_ascii_lowercase();
    if !matches!(action.as_str(), "status" | "patch" | "restore") {
        return Err("汉化操作只允许 status、patch 或 restore".into());
    }
    Ok(vec![action])
}

fn powershell_script_args(action: &str) -> Result<Vec<String>, String> {
    let action = localization_action_args(action)?;
    Ok(vec![
        "-NoProfile".into(),
        "-ExecutionPolicy".into(),
        "Bypass".into(),
        "-File".into(),
        "<script>".into(),
        "-Action".into(),
        action[0].clone(),
        "-Yes".into(),
        "-SkipUpdateCheck".into(),
        "-NoElevate".into(),
    ])
}

fn redact_localization_output(output: &str) -> String {
    let mut redacted = Vec::new();
    let mut redact_next = false;
    for token in output.split_whitespace() {
        let lower = token.to_ascii_lowercase();
        let looks_like_secret = redact_next
            || lower.starts_with("sk-")
            || lower.starts_with("enc:v1:")
            || (token.len() >= 32
                && token
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || "+/=_-".contains(ch)));
        redacted.push(if looks_like_secret {
            "[REDACTED]"
        } else {
            token
        });
        redact_next = lower == "bearer";
    }
    redacted.join(" ")
}

fn quote_powershell(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn build_elevated_command(
    script: &Path,
    action: &str,
    stdout_path: &Path,
    stderr_path: &Path,
) -> String {
    let args = powershell_script_args(action).expect("action must be allowlisted before execution");
    let ps_args = args
        .iter()
        .enumerate()
        .map(|(index, value)| {
            if index == 4 {
                quote_powershell(&script.to_string_lossy())
            } else {
                quote_powershell(value)
            }
        })
        .collect::<Vec<_>>()
        .join(",");
    let stdout = quote_powershell(&stdout_path.to_string_lossy());
    let stderr = quote_powershell(&stderr_path.to_string_lossy());
    format!(
        "$p = Start-Process -FilePath 'powershell.exe' -ArgumentList {} -WindowStyle Hidden -Verb RunAs -Wait -PassThru -RedirectStandardOutput {} -RedirectStandardError {}; $stdoutText = if (Test-Path -LiteralPath {}) {{ Get-Content -LiteralPath {} -Raw }} else {{ '' }}; $stderrText = if (Test-Path -LiteralPath {}) {{ Get-Content -LiteralPath {} -Raw }} else {{ '' }}; Remove-Item -LiteralPath {},{} -Force -ErrorAction SilentlyContinue; [Console]::Out.Write($stdoutText); [Console]::Error.Write($stderrText); exit $p.ExitCode",
        ps_args, stdout, stderr, stdout, stdout, stderr, stderr, stdout, stderr
    )
}

fn localization_paths(
    resource_root: &Path,
    writable_root: &Path,
) -> (PathBuf, PathBuf) {
    (
        resource_root.to_path_buf(),
        writable_root.join(LOCALIZATION_DIR_NAME),
    )
}

fn initialize_localization_bundle(resource_root: &Path, writable_root: &Path) -> Result<PathBuf, String> {
    let (source_root, target_root) = localization_paths(resource_root, writable_root);
    for relative in LOCALIZATION_STATIC_FILES {
        let source = source_root.join(relative);
        let target = target_root.join(relative);
        if !source.is_file() {
            return Err(format!("汉化资源缺失: {}", source.display()));
        }
        // 脚本随 VarSwitch 发布版本更新；每次初始化都刷新脚本，避免旧版本
        // 已复制到用户数据目录后继续执行有缺陷的实现。翻译记忆库等用户
        // 可能在线更新的文件仍保持原有的“只初始化一次”行为。
        let should_refresh = *relative == LOCALIZATION_SCRIPT_RELATIVE;
        if should_refresh || !target.exists() {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| format!("创建汉化资源目录失败: {error}"))?;
            }
            fs::copy(&source, &target)
                .map_err(|error| format!("初始化汉化资源失败: {error}"))?;
        }
    }
    Ok(target_root)
}

#[cfg(target_os = "windows")]
fn execute_localization_action(
    app: &tauri::AppHandle,
    action: &str,
) -> Result<serde_json::Value, String> {
    let action = localization_action_args(action)?;
    let resource_root = app
        .path()
        .resource_dir()
        .map_err(|error| format!("读取应用资源目录失败: {error}"))?
        .join(LOCALIZATION_DIR_NAME);
    let writable_root = crate::data_dir(app);
    let project_root = initialize_localization_bundle(&resource_root, &writable_root)?;
    let script = project_root.join(LOCALIZATION_SCRIPT_RELATIVE);
    let unique = format!(
        "varswitch-claude-localization-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    );
    let stdout_path = std::env::temp_dir().join(format!("{unique}.out"));
    let stderr_path = std::env::temp_dir().join(format!("{unique}.err"));
    let command = build_elevated_command(&script, &action[0], &stdout_path, &stderr_path);
    let mut process = Command::new("powershell.exe");
    process.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &command]);
    process.creation_flags(crate::CREATE_NO_WINDOW);
    let output = process
        .output()
        .map_err(|error| format!("启动汉化脚本失败: {error}"))?;
    let _ = fs::remove_file(&stdout_path);
    let _ = fs::remove_file(&stderr_path);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    Ok(json!({
        "action": action[0],
        "exitCode": output.status.code(),
        "success": output.status.success(),
        "output": redact_localization_output(&format!("{stdout}\n{stderr}")),
        "projectPath": project_root.to_string_lossy(),
    }))
}

#[cfg(not(target_os = "windows"))]
fn execute_localization_action(
    _app: &tauri::AppHandle,
    _action: &str,
) -> Result<serde_json::Value, String> {
    Err("Claude Desktop 汉化按钮目前仅支持 Windows".into())
}

#[tauri::command]
pub(crate) fn run_claude_desktop_localization(
    app: tauri::AppHandle,
    action: String,
) -> Result<serde_json::Value, String> {
    execute_localization_action(&app, &action)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn localization_action_args_are_allowlisted() {
        assert_eq!(localization_action_args("patch").unwrap(), vec!["patch"]);
        assert_eq!(localization_action_args("restore").unwrap(), vec!["restore"]);
        assert_eq!(localization_action_args("status").unwrap(), vec!["status"]);
        assert!(localization_action_args("powershell -EncodedCommand bad").is_err());
    }

    #[test]
    fn powershell_arguments_include_safe_noninteractive_flags() {
        let args = powershell_script_args("patch").unwrap();
        assert!(args
            .windows(2)
            .any(|pair| pair == ["-Action".to_string(), "patch".to_string()]));
        assert!(args.iter().any(|arg| arg == "-Yes"));
        assert!(args.iter().any(|arg| arg == "-SkipUpdateCheck"));
    }

    #[test]
    fn elevated_command_captures_child_output() {
        let command = build_elevated_command(
            Path::new("C:/claude/script.ps1"),
            "status",
            Path::new("C:/temp/stdout.txt"),
            Path::new("C:/temp/stderr.txt"),
        );
        assert!(command.contains("-WindowStyle Hidden"));
        assert!(command.contains("-RedirectStandardOutput"));
        assert!(command.contains("-RedirectStandardError"));
        assert!(command.contains("Get-Content -LiteralPath"));
        assert!(command.contains("-Raw"));
    }

    #[test]
    fn localization_output_redacts_secrets_and_ciphertexts() {
        let output = redact_localization_output(
            "ok sk-super-secret enc:v1:abcdef Bearer abcdefghijklmnopqrstuvwxyz",
        );
        assert!(!output.contains("sk-super-secret"));
        assert!(!output.contains("enc:v1:abcdef"));
        assert!(!output.contains("abcdefghijklmnopqrstuvwxyz"));
        assert!(output.contains("[REDACTED]"));
    }
}
