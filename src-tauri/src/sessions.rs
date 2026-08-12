//! CLI 会话管理：Claude / Codex 历史会话扫描与恢复（从 lib.rs 拆分，逻辑未改动）。

use crate::*;

/// 会话管理器返回给前端的单条会话条目。
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CliSessionEntry {
    /// 会话 ID（Claude 为 jsonl 文件名 UUID，Codex 为线程 UUID）
    pub(crate) id: String,
    /// 来源应用："claude" | "codex"
    pub(crate) app: String,
    /// 标题（首条真实用户消息截断），提取失败时为空
    pub(crate) title: String,
    /// 会话工作目录，提取失败时为空
    pub(crate) cwd: String,
    /// 最近更新时间（会话文件 mtime，unix 秒）
    pub(crate) updated_at: i64,
    /// 会话文件绝对路径
    pub(crate) path: String,
}

pub(crate) fn claude_projects_root() -> PathBuf {
    home_dir().join(".claude").join("projects")
}

pub(crate) fn codex_archived_sessions_root() -> PathBuf {
    codex_config_dir().join("archived_sessions")
}

/// 文件 mtime → unix 秒（取不到时返回 0，排序时垫底）。
pub(crate) fn file_mtime_unix_secs(path: &Path) -> i64 {
    fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|dur| dur.as_secs() as i64)
        .unwrap_or(0)
}

/// 限量读取文件头部：最多 max_bytes 字节内的前 max_lines 行。
/// 会话 jsonl 可达数十 MB，绝不整读；被字节上限截断的半行 JSON 解析失败即自然跳过。
pub(crate) fn read_file_head_lines(path: &Path, max_bytes: u64, max_lines: usize) -> Vec<String> {
    let Ok(file) = fs::File::open(path) else {
        return Vec::new();
    };
    BufReader::new(file)
        .take(max_bytes)
        .lines()
        .take(max_lines)
        .map_while(|line| line.ok())
        .collect()
}

/// 从 Claude 会话行提取消息文本：message.content 可能是字符串，
/// 也可能是 [{type:"text",text:"..."}] 数组，两种形态都处理。
pub(crate) fn claude_message_text(value: &serde_json::Value) -> String {
    let content = value.pointer("/message/content").or_else(|| value.get("content"));
    match content {
        Some(serde_json::Value::String(text)) => text.clone(),
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .filter_map(|item| {
                if item.get("type").and_then(|v| v.as_str()) == Some("text") {
                    item.get("text").and_then(|v| v.as_str())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join(" "),
        _ => String::new(),
    }
}

/// Claude 会话里由 CLI 自动生成的用户行（斜杠命令回显、caveat 提示等），不适合当标题。
pub(crate) fn is_claude_synthetic_user_text(text: &str) -> bool {
    let t = text.trim_start();
    t.starts_with("<command-name>")
        || t.starts_with("<command-message>")
        || t.starts_with("<local-command-stdout>")
        || t.starts_with("<bash-input>")
        || t.starts_with("Caveat: the messages below were generated")
}

/// 从 Claude 会话 jsonl 的头部行中提取（标题, 工作目录）。
/// 标题取第一条 type=="user" 且非合成内容的消息文本（截断 80 字符）；
/// cwd 取任意行的顶层 "cwd" 字段。任何行解析失败都容忍跳过。
pub(crate) fn extract_claude_session_preview(lines: &[String]) -> (String, String) {
    let mut title = String::new();
    let mut cwd = String::new();
    for line in lines {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if cwd.is_empty() {
            if let Some(dir) = value.get("cwd").and_then(|v| v.as_str()) {
                let dir = dir.trim();
                if !dir.is_empty() {
                    cwd = dir.to_string();
                }
            }
        }
        if title.is_empty()
            && value.get("type").and_then(|v| v.as_str()) == Some("user")
            && value.get("isMeta").and_then(|v| v.as_bool()) != Some(true)
        {
            let text = claude_message_text(&value);
            if !text.trim().is_empty() && !is_claude_synthetic_user_text(&text) {
                title = shorten_preview(&text, 80);
            }
        }
        if !title.is_empty() && !cwd.is_empty() {
            break;
        }
    }
    (title, cwd)
}

/// 扫描 ~/.claude/projects/*/*.jsonl，产出 Claude Code 会话条目。
/// 性能：每个文件只读前 64KB 内的至多 40 行头部。
pub(crate) fn scan_claude_cli_sessions() -> Vec<CliSessionEntry> {
    const HEAD_BYTES: u64 = 64 * 1024;
    const HEAD_LINES: usize = 40;
    let mut out = Vec::new();
    let Ok(projects) = fs::read_dir(claude_projects_root()) else {
        return out;
    };
    for project in projects.flatten() {
        let dir = project.path();
        if !dir.is_dir() {
            continue;
        }
        let Ok(files) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in files.flatten() {
            let path = entry.path();
            if !path.is_file() || path.extension().and_then(|v| v.to_str()) != Some("jsonl") {
                continue;
            }
            let Some(id) = path.file_stem().and_then(|v| v.to_str()).map(str::to_string) else {
                continue;
            };
            let (title, cwd) =
                extract_claude_session_preview(&read_file_head_lines(&path, HEAD_BYTES, HEAD_LINES));
            out.push(CliSessionEntry {
                id,
                app: "claude".into(),
                title,
                cwd,
                updated_at: file_mtime_unix_secs(&path),
                path: path.to_string_lossy().to_string(),
            });
        }
    }
    out
}

/// 从 Codex 会话文件名（rollout-<时间戳>-<uuid>.jsonl 的 stem）提取末尾的线程 UUID。
pub(crate) fn codex_thread_id_from_stem(stem: &str) -> Option<String> {
    if stem.len() < 36 {
        return None;
    }
    let start = stem.len() - 36;
    if !stem.is_char_boundary(start) {
        return None;
    }
    let candidate = &stem[start..];
    let valid = candidate.bytes().enumerate().all(|(index, byte)| match index {
        8 | 13 | 18 | 23 => byte == b'-',
        _ => byte.is_ascii_hexdigit(),
    });
    valid.then(|| candidate.to_ascii_lowercase())
}

/// Codex 会话里由 CLI 注入的合成用户消息（AGENTS 指引、环境上下文等），不适合当标题。
pub(crate) fn is_codex_synthetic_user_text(text: &str) -> bool {
    let t = text.trim_start();
    t.starts_with("<user_instructions>")
        || t.starts_with("<environment_context>")
        || t.starts_with("<ENVIRONMENT_CONTEXT>")
        || t.starts_with("<permissions instructions>")
        || t.starts_with("<turn_context>")
        || t.starts_with("# AGENTS.md instructions")
}

/// 从 Codex 会话 jsonl 的头部行中提取（线程 ID, 标题, 工作目录）。
/// 线程 ID 与 cwd 优先取 session_meta；标题取首条真实用户消息
/// （response_item 的 role=="user"，或 event_msg 的 user_message 事件）。
pub(crate) fn extract_codex_session_preview(lines: &[String]) -> (String, String, String) {
    let mut id = String::new();
    let mut title = String::new();
    let mut cwd = String::new();
    for line in lines {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let kind = value.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if kind == "session_meta" {
            if id.is_empty() {
                id = value
                    .pointer("/payload/id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
            }
            if cwd.is_empty() {
                cwd = value
                    .pointer("/payload/cwd")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
            }
            continue;
        }
        if cwd.is_empty() {
            if let Some(dir) = value
                .pointer("/payload/cwd")
                .and_then(|v| v.as_str())
                .or_else(|| value.pointer("/payload/origin/cwd").and_then(|v| v.as_str()))
            {
                let dir = dir.trim();
                if !dir.is_empty() {
                    cwd = dir.to_string();
                }
            }
        }
        if title.is_empty() {
            let role = value
                .pointer("/payload/role")
                .and_then(|v| v.as_str())
                .or_else(|| value.pointer("/payload/message/role").and_then(|v| v.as_str()))
                .unwrap_or("");
            let text = if role == "user" {
                value
                    .pointer("/payload/content")
                    .map(extract_text_from_content_value)
                    .or_else(|| {
                        value
                            .pointer("/payload/message/content")
                            .map(extract_text_from_content_value)
                    })
                    .unwrap_or_default()
            } else if kind == "event_msg"
                && value.pointer("/payload/type").and_then(|v| v.as_str()) == Some("user_message")
            {
                value
                    .pointer("/payload/message")
                    .map(extract_text_from_content_value)
                    .unwrap_or_default()
            } else {
                String::new()
            };
            if !text.trim().is_empty() && !is_codex_synthetic_user_text(&text) {
                title = shorten_preview(&text, 80);
            }
        }
        if !id.is_empty() && !title.is_empty() && !cwd.is_empty() {
            break;
        }
    }
    (id, title, cwd)
}

/// 递归收集目录下全部 .jsonl 文件（sessions 按 YYYY/MM/DD 分层，archived_sessions 平铺）。
pub(crate) fn collect_jsonl_files(root: &Path, out: &mut Vec<PathBuf>) {
    if !root.is_dir() {
        return;
    }
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|v| v.to_str()) == Some("jsonl") {
                out.push(path);
            }
        }
    }
}

/// 扫描 ~/.codex/sessions 与 ~/.codex/archived_sessions，产出 Codex 会话条目。
/// 标题优先复用 session_index.jsonl 中 Codex 自己维护的 thread_name（与
/// read_codex_threads 同一数据源），否则从文件头部提取首条真实用户消息；
/// 性能：每个文件只读前 96KB 内的至多 60 行（Codex 头部有大段注入指令，预算比 Claude 稍大）。
pub(crate) fn scan_codex_cli_sessions() -> Vec<CliSessionEntry> {
    const HEAD_BYTES: u64 = 96 * 1024;
    const HEAD_LINES: usize = 60;
    let mut index_names: HashMap<String, String> = HashMap::new();
    for line in read_lines_if_exists(&codex_session_index_path()) {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        let id = value
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        let name = value
            .get("thread_name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if !id.is_empty() && !name.is_empty() {
            index_names.insert(id, name);
        }
    }

    let mut files = Vec::new();
    collect_jsonl_files(&codex_sessions_root(), &mut files);
    collect_jsonl_files(&codex_archived_sessions_root(), &mut files);

    let mut out = Vec::new();
    for path in files {
        let stem = path.file_stem().and_then(|v| v.to_str()).unwrap_or("");
        let filename_id = codex_thread_id_from_stem(stem);
        let (meta_id, extracted_title, cwd) =
            extract_codex_session_preview(&read_file_head_lines(&path, HEAD_BYTES, HEAD_LINES));
        // 文件名 UUID 优先（无须解析成功），否则退回 session_meta 里的 id
        let Some(id) = filename_id.or_else(|| {
            let trimmed = meta_id.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_ascii_lowercase())
        }) else {
            continue;
        };
        let title = index_names.get(&id).cloned().unwrap_or(extracted_title);
        out.push(CliSessionEntry {
            id,
            app: "codex".into(),
            title,
            cwd,
            updated_at: file_mtime_unix_secs(&path),
            path: path.to_string_lossy().to_string(),
        });
    }
    out
}

/// 会话管理器：列出本机 Claude Code / Codex 历史会话，按更新时间降序。
/// query 对 title/cwd/id 做不区分大小写子串过滤；app 过滤来源；limit 默认 200。
#[tauri::command]
pub(crate) fn list_cli_sessions(
    query: Option<String>,
    app: Option<String>,
    limit: Option<usize>,
) -> Result<serde_json::Value, String> {
    let app_filter = app
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty() && value != "all");
    let mut sessions = Vec::new();
    if app_filter.as_deref().map_or(true, |value| value == "claude") {
        sessions.extend(scan_claude_cli_sessions());
    }
    if app_filter.as_deref().map_or(true, |value| value == "codex") {
        sessions.extend(scan_codex_cli_sessions());
    }
    if let Some(needle) = query
        .map(|value| value.trim().to_lowercase())
        .filter(|value| !value.is_empty())
    {
        sessions.retain(|session| {
            session.title.to_lowercase().contains(&needle)
                || session.cwd.to_lowercase().contains(&needle)
                || session.id.to_lowercase().contains(&needle)
        });
    }
    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    // 同一线程可能有多个 rollout 文件（恢复会新写文件），排序后按 (app, id) 去重保留最新
    let mut seen: HashSet<(String, String)> = HashSet::new();
    sessions.retain(|session| seen.insert((session.app.clone(), session.id.clone())));
    sessions.truncate(limit.unwrap_or(200).max(1));
    Ok(serde_json::json!({ "sessions": sessions }))
}

/// 会话 ID 是否只含 [A-Za-z0-9_-]。ID 会拼进命令行/深链接，严格白名单防注入。
pub(crate) fn is_safe_cli_session_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// 在新终端窗口里执行 `claude -r <id>` 恢复 Claude Code 会话。
/// Windows 弹出新的 cmd 窗口；macOS 用 osascript 驱动 Terminal（best-effort）；其余平台暂不支持。
pub(crate) fn resume_claude_session_in_terminal(session_id: &str, cwd: Option<&Path>) -> Result<(), String> {
    #[cfg(windows)]
    {
        // start 打开的新 cmd 窗口继承本进程 current_dir，设置 cwd 即可回到原工作目录；
        // start 的首个引号参数是窗口标题，故留空。参数逐个传递，不经 shell 拼接。
        let mut cmd = Command::new("cmd");
        cmd.args(["/C", "start", "", "cmd", "/K", "claude", "-r", session_id])
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        }
        #[cfg(target_os = "windows")]
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd.spawn()
            .map_err(|e| format!("启动终端恢复 Claude 会话失败: {e}"))?;
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        // 先拼 shell 命令（目录用单引号安全引用），再按 AppleScript 字符串规则转义嵌入
        let shell_command = match cwd {
            Some(dir) => format!(
                "cd '{}' && claude -r {session_id}",
                dir.to_string_lossy().replace('\'', "'\\''")
            ),
            None => format!("claude -r {session_id}"),
        };
        let escaped = shell_command.replace('\\', "\\\\").replace('"', "\\\"");
        let script =
            format!("tell application \"Terminal\"\nactivate\ndo script \"{escaped}\"\nend tell");
        Command::new("osascript")
            .args(["-e", &script])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("启动 Terminal 恢复 Claude 会话失败: {e}"))?;
        Ok(())
    }
    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        let _ = (session_id, cwd);
        Err("当前平台暂不支持自动恢复 Claude 会话，请在终端手动执行 claude -r <会话ID>".into())
    }
}

/// 恢复历史会话：Codex 复用 codex://threads/<id> 深链接打开桌面应用，
/// Claude 在新终端窗口执行 `claude -r <id>`。cwd 仅在是真实存在的目录时生效。
#[tauri::command]
pub(crate) fn resume_cli_session(app: String, id: String, cwd: Option<String>) -> Result<(), String> {
    let id = id.trim().to_string();
    if !is_safe_cli_session_id(&id) {
        return Err("会话 ID 含非法字符，已拒绝执行".into());
    }
    let cwd = cwd
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.is_dir());
    match app.trim().to_ascii_lowercase().as_str() {
        "codex" => open_codex_thread_deep_link(&id),
        "claude" => resume_claude_session_in_terminal(&id, cwd.as_deref()),
        other => Err(format!("不支持的应用类型: {other}")),
    }
}

#[cfg(test)]
mod cli_session_manager_tests {
    use super::*;

    fn lines(raw: &[&str]) -> Vec<String> {
        raw.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn claude_preview_reads_string_content_and_cwd() {
        let (title, cwd) = extract_claude_session_preview(&lines(&[
            r#"{"type":"mode","mode":"normal","sessionId":"abc"}"#,
            r#"{"type":"user","cwd":"H:\\demo","message":{"role":"user","content":"帮我修一个 bug"}}"#,
        ]));
        assert_eq!(title, "帮我修一个 bug");
        assert_eq!(cwd, "H:\\demo");
    }

    #[test]
    fn claude_preview_reads_array_content() {
        let (title, cwd) = extract_claude_session_preview(&lines(&[
            r#"{"cwd":"/home/u/proj","type":"user","message":{"role":"user","content":[{"type":"text","text":"first"},{"type":"text","text":"second"}]}}"#,
        ]));
        assert_eq!(title, "first second");
        assert_eq!(cwd, "/home/u/proj");
    }

    #[test]
    fn claude_preview_skips_meta_synthetic_and_tool_result_lines() {
        let (title, _) = extract_claude_session_preview(&lines(&[
            r#"{"type":"user","isMeta":true,"message":{"role":"user","content":"Caveat: the messages below were generated..."}}"#,
            r#"{"type":"user","message":{"role":"user","content":"<command-name>/clear</command-name>"}}"#,
            r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"ok"}]}}"#,
            r#"{"type":"user","message":{"role":"user","content":"真正的问题"}}"#,
        ]));
        assert_eq!(title, "真正的问题");
    }

    #[test]
    fn claude_preview_tolerates_broken_json_and_returns_empty() {
        let (title, cwd) = extract_claude_session_preview(&lines(&[
            "{not-json",
            r#"{"type":"assistant","message":{"role":"assistant","content":"hi"}}"#,
        ]));
        assert_eq!(title, "");
        assert_eq!(cwd, "");
    }

    #[test]
    fn claude_preview_truncates_title_to_80_chars() {
        let long_text = "x".repeat(200);
        let line = format!(
            r#"{{"type":"user","message":{{"role":"user","content":"{long_text}"}}}}"#
        );
        let (title, _) = extract_claude_session_preview(&[line]);
        assert_eq!(title.chars().count(), 83); // 80 字符 + "..."
        assert!(title.ends_with("..."));
    }

    #[test]
    fn codex_preview_reads_session_meta_and_skips_synthetic_user() {
        let (id, title, cwd) = extract_codex_session_preview(&lines(&[
            r#"{"type":"session_meta","payload":{"id":"019cbe7d-4530-73a2-a5d6-13f37d0665cb","cwd":"h:\\proj"}}"#,
            r##"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"# AGENTS.md instructions for h:\\proj"}]}}"##,
            r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"请实现会话管理器"}]}}"#,
        ]));
        assert_eq!(id, "019cbe7d-4530-73a2-a5d6-13f37d0665cb");
        assert_eq!(title, "请实现会话管理器");
        assert_eq!(cwd, "h:\\proj");
    }

    #[test]
    fn codex_thread_id_parses_rollout_filename() {
        assert_eq!(
            codex_thread_id_from_stem("rollout-2026-03-05T22-53-26-019cbe7d-4530-73a2-a5d6-13f37d0665cb"),
            Some("019cbe7d-4530-73a2-a5d6-13f37d0665cb".to_string())
        );
        assert_eq!(codex_thread_id_from_stem("notes"), None);
        assert_eq!(
            codex_thread_id_from_stem("rollout-2026-03-05T22-53-26-零19cbe7d-4530-73a2-a5d6-13f37d0665c"),
            None
        );
    }

    #[test]
    fn session_id_whitelist_blocks_injection() {
        assert!(is_safe_cli_session_id("019cbe7d-4530-73a2-a5d6-13f37d0665cb"));
        assert!(is_safe_cli_session_id("abc_DEF-123"));
        assert!(!is_safe_cli_session_id(""));
        assert!(!is_safe_cli_session_id("abc def"));
        assert!(!is_safe_cli_session_id("abc&calc"));
        assert!(!is_safe_cli_session_id("a\"b"));
        assert!(!is_safe_cli_session_id("a;b|c"));
    }
}
