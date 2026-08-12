//! MCP 域：旧版 MCP 命令、统一 MCP 读写与迷你 TOML 解析器（从 lib.rs 拆分，逻辑未改动）。

use crate::*;

pub(crate) fn claude_mcp_path() -> PathBuf {
    home_dir().join(".claude.json")
}

/// Search GitHub for MCP server repositories
#[tauri::command]
pub(crate) async fn search_github_mcp(query: String) -> Result<Vec<serde_json::Value>, String> {
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

// ── MCP Server Commands ─────────────────────────────

#[tauri::command]
pub(crate) fn get_mcp_servers_list() -> Result<serde_json::Value, String> {
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
pub(crate) fn save_mcp_server(name: String, config: serde_json::Value) -> Result<(), String> {
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
pub(crate) fn delete_mcp_server_entry(name: String) -> Result<(), String> {
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

// ── 统一 MCP 管理（Claude / Codex / Gemini）─────────────
//
// 一个面板同时管理三个应用的 MCP 服务器：
// - Claude：~/.claude.json 的 mcpServers 对象
// - Codex：~/.codex/config.toml 的 [mcp_servers.<name>] 段
// - Gemini：~/.gemini/settings.json 的 mcpServers 对象
//
// 项目约定不引入 toml crate，Codex 侧用受限的字符串级解析器处理：
// 只解析我们关心的 command / args / env / url 字段（env 同时支持
// `env = { K = "v" }` 内联表与 `[mcp_servers.<name>.env]` 子表两种形式），
// 写回时保留 config.toml 中所有无关内容，所有写入走 write_file_atomic。

/// 受限 TOML 取值游标：支持基本/字面量字符串（含三引号多行形式）、
/// 数组、内联表、布尔与数字，足够覆盖 MCP server 段落里出现的取值形态。
pub(crate) struct MiniTomlCursor {
    pub(crate) chars: Vec<char>,
    pub(crate) pos: usize,
}

impl MiniTomlCursor {
    fn new(text: &str) -> Self {
        Self {
            chars: text.chars().collect(),
            pos: 0,
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += 1;
        Some(c)
    }

    fn starts_with(&self, prefix: &str) -> bool {
        prefix
            .chars()
            .enumerate()
            .all(|(offset, c)| self.chars.get(self.pos + offset) == Some(&c))
    }

    /// 跳过空白与注释；cross_lines 为 true 时连换行一起跳过
    fn skip_trivia(&mut self, cross_lines: bool) {
        loop {
            match self.peek() {
                Some(' ') | Some('\t') | Some('\r') => {
                    self.pos += 1;
                }
                Some('\n') if cross_lines => {
                    self.pos += 1;
                }
                Some('#') => {
                    while let Some(c) = self.peek() {
                        if c == '\n' {
                            break;
                        }
                        self.pos += 1;
                    }
                }
                _ => break,
            }
        }
    }

    /// 跳到下一行行首（解析失败时的恢复手段，尽力继续解析后续键值）
    fn skip_to_next_line(&mut self) {
        while let Some(c) = self.bump() {
            if c == '\n' {
                break;
            }
        }
    }

    /// 解析点分键路径，键段允许裸键或引号键（如 env.TOKEN、"my.key"）
    fn parse_dotted_key(&mut self) -> Option<Vec<String>> {
        let mut segments = Vec::new();
        loop {
            self.skip_trivia(false);
            let segment = match self.peek()? {
                '"' | '\'' => self.parse_string()?,
                _ => {
                    let mut out = String::new();
                    while let Some(c) = self.peek() {
                        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                            out.push(c);
                            self.pos += 1;
                        } else {
                            break;
                        }
                    }
                    if out.is_empty() {
                        return None;
                    }
                    out
                }
            };
            segments.push(segment);
            self.skip_trivia(false);
            if self.peek() == Some('.') {
                self.pos += 1;
                continue;
            }
            break;
        }
        Some(segments)
    }

    /// 解析 TOML 字符串（"basic" / 'literal'，含 """ / ''' 三引号形式）
    fn parse_string(&mut self) -> Option<String> {
        let quote = self.peek()?;
        if quote != '"' && quote != '\'' {
            return None;
        }
        let delim: String = std::iter::repeat(quote).take(3).collect();
        let triple = self.starts_with(&delim);
        self.pos += if triple { 3 } else { 1 };
        let mut out = String::new();
        loop {
            if triple && self.starts_with(&delim) {
                self.pos += 3;
                return Some(out);
            }
            let c = self.bump()?; // 未闭合的字符串 → 解析失败
            if !triple && c == quote {
                return Some(out);
            }
            if quote == '"' && c == '\\' {
                match self.bump()? {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    'b' => out.push('\u{0008}'),
                    'f' => out.push('\u{000C}'),
                    '"' => out.push('"'),
                    '\\' => out.push('\\'),
                    esc @ ('u' | 'U') => {
                        let len = if esc == 'u' { 4 } else { 8 };
                        let mut hex = String::new();
                        for _ in 0..len {
                            if let Some(h) = self.bump() {
                                hex.push(h);
                            }
                        }
                        if let Some(ch) =
                            u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32)
                        {
                            out.push(ch);
                        }
                    }
                    other => {
                        // 未知转义按原样保留，尽力不丢内容
                        out.push('\\');
                        out.push(other);
                    }
                }
                continue;
            }
            out.push(c);
        }
    }

    /// 解析一个 TOML 值并转成 JSON 值（字符串 / 数组 / 内联表 / 布尔 / 数字）
    fn parse_value(&mut self) -> Option<serde_json::Value> {
        self.skip_trivia(true);
        match self.peek()? {
            '"' | '\'' => self.parse_string().map(serde_json::Value::String),
            '[' => {
                self.pos += 1;
                let mut items = Vec::new();
                loop {
                    self.skip_trivia(true);
                    if self.peek() == Some(']') {
                        self.pos += 1;
                        break;
                    }
                    items.push(self.parse_value()?);
                    self.skip_trivia(true);
                    match self.peek() {
                        Some(',') => {
                            self.pos += 1;
                        }
                        Some(']') => {
                            self.pos += 1;
                            break;
                        }
                        _ => return None, // 非法数组，放弃本值
                    }
                }
                Some(serde_json::Value::Array(items))
            }
            '{' => {
                self.pos += 1;
                let mut table = serde_json::Map::new();
                loop {
                    self.skip_trivia(true);
                    if self.peek() == Some('}') {
                        self.pos += 1;
                        break;
                    }
                    let key = self.parse_dotted_key()?;
                    self.skip_trivia(false);
                    if self.peek() != Some('=') {
                        return None;
                    }
                    self.pos += 1;
                    let value = self.parse_value()?;
                    insert_json_at_path(&mut table, &key, value);
                    self.skip_trivia(true);
                    match self.peek() {
                        Some(',') => {
                            self.pos += 1;
                        }
                        Some('}') => {
                            self.pos += 1;
                            break;
                        }
                        _ => return None,
                    }
                }
                Some(serde_json::Value::Object(table))
            }
            _ => {
                // 裸值：读取到行尾或结构分隔符为止，再按布尔 / 数字 / 字符串归类
                let mut out = String::new();
                while let Some(c) = self.peek() {
                    if matches!(c, '\n' | ',' | ']' | '}' | '#') {
                        break;
                    }
                    out.push(c);
                    self.pos += 1;
                }
                let token = out.trim();
                if token.is_empty() {
                    return None;
                }
                if token == "true" {
                    return Some(serde_json::Value::Bool(true));
                }
                if token == "false" {
                    return Some(serde_json::Value::Bool(false));
                }
                if let Ok(n) = token.parse::<i64>() {
                    return Some(serde_json::Value::from(n));
                }
                if let Ok(f) = token.parse::<f64>() {
                    return Some(serde_json::Value::from(f));
                }
                Some(serde_json::Value::String(token.to_string()))
            }
        }
    }
}

/// 按点分路径把值写入嵌套 JSON 对象（路径中间节点不是对象时覆盖为对象）
pub(crate) fn insert_json_at_path(
    root: &mut serde_json::Map<String, serde_json::Value>,
    path: &[String],
    value: serde_json::Value,
) {
    let Some((head, rest)) = path.split_first() else {
        return;
    };
    if rest.is_empty() {
        root.insert(head.clone(), value);
        return;
    }
    let entry = root
        .entry(head.clone())
        .or_insert_with(|| serde_json::json!({}));
    if !entry.is_object() {
        *entry = serde_json::json!({});
    }
    if let Some(map) = entry.as_object_mut() {
        insert_json_at_path(map, rest, value);
    }
}

/// 解析 TOML 表头行（[a.b] / [[a.b]]，支持引号键与行尾注释），返回键路径；
/// 不是表头则返回 None
pub(crate) fn parse_toml_header_path(line: &str) -> Option<Vec<String>> {
    let trimmed = line.trim();
    if !trimmed.starts_with('[') {
        return None;
    }
    let start = if trimmed.starts_with("[[") { 2 } else { 1 };
    let chars: Vec<char> = trimmed.chars().collect();
    let mut index = start;
    let mut quote: Option<char> = None;
    let mut end = None;
    while index < chars.len() {
        let c = chars[index];
        match quote {
            Some(q) => {
                if q == '"' && c == '\\' {
                    index += 1; // 跳过被转义的字符
                } else if c == q {
                    quote = None;
                }
            }
            None => {
                if c == '"' || c == '\'' {
                    quote = Some(c);
                } else if c == ']' {
                    end = Some(index);
                    break;
                }
            }
        }
        index += 1;
    }
    let end = end?;
    // 闭合括号后只允许出现 ]]（数组表）、空白或注释
    let rest: String = chars[end + 1..].iter().collect();
    let rest = rest.trim_start_matches(']');
    let rest = rest.trim();
    if !rest.is_empty() && !rest.starts_with('#') {
        return None;
    }
    let inner: String = chars[start..end].iter().collect();
    let mut cursor = MiniTomlCursor::new(&inner);
    let path = cursor.parse_dotted_key()?;
    cursor.skip_trivia(false);
    if cursor.peek().is_some() {
        return None; // 键路径后还有多余内容，不当作合法表头
    }
    Some(path)
}

/// 解析一个 TOML 段落体内的所有键值对（解析失败的行跳过，尽力继续）
pub(crate) fn parse_toml_section_body(body: &str) -> serde_json::Map<String, serde_json::Value> {
    let mut table = serde_json::Map::new();
    let mut cursor = MiniTomlCursor::new(body);
    loop {
        cursor.skip_trivia(true);
        if cursor.peek().is_none() {
            break;
        }
        let Some(key) = cursor.parse_dotted_key() else {
            cursor.skip_to_next_line();
            continue;
        };
        cursor.skip_trivia(false);
        if cursor.peek() != Some('=') {
            cursor.skip_to_next_line();
            continue;
        }
        cursor.pos += 1;
        let Some(value) = cursor.parse_value() else {
            cursor.skip_to_next_line();
            continue;
        };
        insert_json_at_path(&mut table, &key, value);
    }
    table
}

/// 把解析出来的 Codex mcp_servers 表转成 Claude 风格 JSON 配置，
/// 只保留 command / args / env / url 四个统一面板关心的字段
pub(crate) fn codex_mcp_table_to_claude_config(
    table: &serde_json::Map<String, serde_json::Value>,
) -> serde_json::Value {
    let mut config = serde_json::Map::new();
    if let Some(command) = table.get("command").and_then(|v| v.as_str()) {
        config.insert(
            "command".into(),
            serde_json::Value::String(command.to_string()),
        );
    }
    if let Some(args) = table.get("args").and_then(|v| v.as_array()) {
        let items: Vec<serde_json::Value> = args
            .iter()
            .map(|item| match item {
                serde_json::Value::String(_) => item.clone(),
                other => serde_json::Value::String(other.to_string()),
            })
            .collect();
        config.insert("args".into(), serde_json::Value::Array(items));
    }
    if let Some(url) = table.get("url").and_then(|v| v.as_str()) {
        config.insert("url".into(), serde_json::Value::String(url.to_string()));
    }
    if let Some(env) = table.get("env").and_then(|v| v.as_object()) {
        if !env.is_empty() {
            let map: serde_json::Map<String, serde_json::Value> = env
                .iter()
                .map(|(key, value)| {
                    let value = match value {
                        serde_json::Value::String(_) => value.clone(),
                        other => serde_json::Value::String(other.to_string()),
                    };
                    (key.clone(), value)
                })
                .collect();
            config.insert("env".into(), serde_json::Value::Object(map));
        }
    }
    serde_json::Value::Object(config)
}

/// 解析 config.toml 中所有 [mcp_servers.<name>] 段（含 .env 等子表），
/// 返回 (name, Claude 风格 JSON 配置) 列表，保持文件中出现的顺序
pub(crate) fn parse_codex_mcp_servers(existing: &str) -> Vec<(String, serde_json::Value)> {
    // 先按表头把文件切成段落，记录每段的键路径与正文
    let mut sections: Vec<(Vec<String>, String)> = Vec::new();
    let mut current: Option<(Vec<String>, Vec<&str>)> = None;
    for line in existing.lines() {
        if let Some(path) = parse_toml_header_path(line) {
            if let Some((prev_path, body)) = current.take() {
                sections.push((prev_path, body.join("\n")));
            }
            current = Some((path, Vec::new()));
            continue;
        }
        if let Some((_, body)) = current.as_mut() {
            body.push(line);
        }
    }
    if let Some((prev_path, body)) = current.take() {
        sections.push((prev_path, body.join("\n")));
    }

    // 聚合 mcp_servers.<name>；[mcp_servers.<name>.env] 之类的子表挂到对应键下
    let mut order: Vec<String> = Vec::new();
    let mut tables: HashMap<String, serde_json::Map<String, serde_json::Value>> = HashMap::new();
    for (path, body) in sections {
        if path.len() < 2 || path[0] != "mcp_servers" {
            continue;
        }
        let name = path[1].clone();
        if !tables.contains_key(&name) {
            order.push(name.clone());
        }
        let table = tables.entry(name).or_default();
        let parsed = parse_toml_section_body(&body);
        if path.len() == 2 {
            for (key, value) in parsed {
                table.insert(key, value);
            }
        } else {
            insert_json_at_path(table, &path[2..], serde_json::Value::Object(parsed));
        }
    }

    order
        .into_iter()
        .map(|name| {
            let table = tables.remove(&name).unwrap_or_default();
            let config = codex_mcp_table_to_claude_config(&table);
            (name, config)
        })
        .collect()
}

/// 把字符串编码成 TOML 基本字符串（含引号），转义反斜杠 / 引号 / 控制字符
pub(crate) fn toml_basic_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// TOML 键：能用裸键就用裸键，否则加引号转义
pub(crate) fn toml_key(key: &str) -> String {
    let bare = !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if bare {
        key.to_string()
    } else {
        toml_basic_string(key)
    }
}

/// 把 Claude 风格 JSON 配置序列化成 [mcp_servers.<name>] TOML 段文本
/// （command 字符串、args 字符串数组、url 字符串、env 内联表）
pub(crate) fn codex_mcp_section_text(name: &str, config: &serde_json::Value) -> String {
    let mut out = format!("[mcp_servers.{}]\n", toml_key(name));
    if let Some(command) = config.get("command").and_then(|v| v.as_str()) {
        out.push_str(&format!("command = {}\n", toml_basic_string(command)));
    }
    if let Some(args) = config.get("args").and_then(|v| v.as_array()) {
        let items = args
            .iter()
            .map(|item| match item {
                serde_json::Value::String(s) => toml_basic_string(s),
                other => toml_basic_string(&other.to_string()),
            })
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!("args = [{items}]\n"));
    }
    if let Some(url) = config.get("url").and_then(|v| v.as_str()) {
        out.push_str(&format!("url = {}\n", toml_basic_string(url)));
    }
    if let Some(env) = config.get("env").and_then(|v| v.as_object()) {
        if !env.is_empty() {
            let pairs = env
                .iter()
                .map(|(key, value)| {
                    let rendered = match value {
                        serde_json::Value::String(s) => toml_basic_string(s),
                        serde_json::Value::Bool(_) | serde_json::Value::Number(_) => {
                            value.to_string()
                        }
                        other => toml_basic_string(&other.to_string()),
                    };
                    format!("{} = {}", toml_key(key), rendered)
                })
                .collect::<Vec<_>>()
                .join(", ");
            out.push_str(&format!("env = {{ {pairs} }}\n"));
        }
    }
    out
}

/// 从 config.toml 文本中剔除 [mcp_servers.<name>] 段及其所有子表，
/// 返回（剩余行，首个被删段落所在的行号）
pub(crate) fn strip_codex_mcp_server_section(existing: &str, name: &str) -> (Vec<String>, Option<usize>) {
    let mut output: Vec<String> = Vec::new();
    let mut removed_at: Option<usize> = None;
    let mut skipping = false;
    for line in existing.lines() {
        if let Some(path) = parse_toml_header_path(line) {
            let matches = path.len() >= 2 && path[0] == "mcp_servers" && path[1] == name;
            if matches {
                skipping = true;
                if removed_at.is_none() {
                    removed_at = Some(output.len());
                }
                continue;
            }
            skipping = false;
        }
        if !skipping {
            output.push(line.to_string());
        }
    }
    (output, removed_at)
}

/// 折叠连续空行、去掉首尾空行，非空内容以单个换行结尾
pub(crate) fn normalize_toml_output(lines: Vec<String>) -> String {
    let mut compact: Vec<String> = Vec::new();
    let mut last_blank = true; // 起始视为空行，顺带去掉开头的空行
    for line in lines {
        let blank = line.trim().is_empty();
        if blank && last_blank {
            continue;
        }
        compact.push(line);
        last_blank = blank;
    }
    while compact.last().is_some_and(|l| l.trim().is_empty()) {
        compact.pop();
    }
    if compact.is_empty() {
        return String::new();
    }
    let mut text = compact.join("\n");
    text.push('\n');
    text
}

/// 在 config.toml 文本中写入 / 替换 [mcp_servers.<name>] 段，保留其他所有内容；
/// 已存在时原位替换（连同 .env 等子表一起换掉），不存在时追加到文件末尾
pub(crate) fn upsert_codex_mcp_server_section(
    existing: &str,
    name: &str,
    config: &serde_json::Value,
) -> String {
    let (mut lines, removed_at) = strip_codex_mcp_server_section(existing, name);
    let mut section_lines: Vec<String> = codex_mcp_section_text(name, config)
        .lines()
        .map(str::to_string)
        .collect();
    match removed_at {
        Some(index) => {
            // 原位替换：与上下相邻的非空行之间补一个空行分隔
            if index < lines.len() && !lines[index].trim().is_empty() {
                section_lines.push(String::new());
            }
            if index > 0 && !lines[index - 1].trim().is_empty() {
                section_lines.insert(0, String::new());
            }
            for (offset, line) in section_lines.into_iter().enumerate() {
                lines.insert(index + offset, line);
            }
        }
        None => {
            if lines.iter().any(|l| !l.trim().is_empty()) {
                if lines.last().is_some_and(|l| !l.trim().is_empty()) {
                    lines.push(String::new());
                }
            } else {
                lines.clear();
            }
            lines.extend(section_lines);
        }
    }
    normalize_toml_output(lines)
}

/// 从 config.toml 文本中移除 [mcp_servers.<name>] 段；没有该段时原样返回
pub(crate) fn remove_codex_mcp_server_section(existing: &str, name: &str) -> String {
    let (lines, removed_at) = strip_codex_mcp_server_section(existing, name);
    if removed_at.is_none() {
        return existing.to_string();
    }
    normalize_toml_output(lines)
}

/// 读取 JSON 配置文件（~/.claude.json / ~/.gemini/settings.json）里的 mcpServers 对象；
/// 文件不存在或解析失败时返回空表（聚合读取按“尽力”处理，不因单个文件损坏而失败）
pub(crate) fn read_json_mcp_map(path: &PathBuf) -> serde_json::Map<String, serde_json::Value> {
    if !path.exists() {
        return serde_json::Map::new();
    }
    read_json_or_default(path, serde_json::json!({}))
        .get("mcpServers")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default()
}

/// 对 JSON 配置文件的 mcpServers 做写入或移除：
/// config 为 Some 时新增 / 覆盖条目；为 None 时移除条目（文件或条目不存在则不动文件）
pub(crate) fn apply_json_mcp_change(
    path: &PathBuf,
    name: &str,
    config: Option<&serde_json::Value>,
) -> Result<(), String> {
    match config {
        Some(config) => {
            let mut settings = if path.exists() {
                read_json(path)?
            } else {
                serde_json::json!({})
            };
            if !settings.is_object() {
                settings = serde_json::json!({});
            }
            if !settings.get("mcpServers").is_some_and(|v| v.is_object()) {
                settings["mcpServers"] = serde_json::json!({});
            }
            settings["mcpServers"][name] = config.clone();
            write_json(path, &settings)
        }
        None => {
            if !path.exists() {
                return Ok(());
            }
            let mut settings = read_json(path)?;
            let removed = settings
                .get_mut("mcpServers")
                .and_then(|v| v.as_object_mut())
                .is_some_and(|servers| servers.remove(name).is_some());
            if removed {
                write_json(path, &settings)
            } else {
                Ok(())
            }
        }
    }
}

/// 对 ~/.codex/config.toml 的 [mcp_servers.<name>] 段做写入或移除，
/// 内容没有变化时不落盘，避免无谓地重排用户文件
pub(crate) fn apply_codex_mcp_change(name: &str, config: Option<&serde_json::Value>) -> Result<(), String> {
    let path = codex_config_path();
    let existing = fs::read_to_string(&path).unwrap_or_default();
    let next = match config {
        Some(config) => upsert_codex_mcp_server_section(&existing, name, config),
        None => {
            if !path.exists() {
                return Ok(());
            }
            remove_codex_mcp_server_section(&existing, name)
        }
    };
    if next == existing {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    write_file_atomic(&path, &next)
}

/// 聚合三个应用的 MCP 服务器列表。
/// 返回 { "servers": [ { "name", "config", "apps": { claude, codex, gemini } } ] }，
/// config 取值优先级 claude > codex > gemini
#[tauri::command]
pub(crate) fn get_unified_mcp_servers() -> Result<serde_json::Value, String> {
    let claude = read_json_mcp_map(&claude_mcp_path());
    let codex = {
        let path = codex_config_path();
        fs::read_to_string(&path)
            .map(|text| parse_codex_mcp_servers(&text))
            .unwrap_or_default()
    };
    let gemini = read_json_mcp_map(&gemini_settings_path());

    // 按 Claude → Codex → Gemini 的顺序收集名称，保证列表顺序稳定
    let mut order: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for name in claude.keys() {
        if seen.insert(name.clone()) {
            order.push(name.clone());
        }
    }
    for (name, _) in &codex {
        if seen.insert(name.clone()) {
            order.push(name.clone());
        }
    }
    for name in gemini.keys() {
        if seen.insert(name.clone()) {
            order.push(name.clone());
        }
    }

    let servers: Vec<serde_json::Value> = order
        .into_iter()
        .map(|name| {
            let claude_config = claude.get(&name);
            let codex_config = codex
                .iter()
                .find(|(codex_name, _)| codex_name == &name)
                .map(|(_, config)| config);
            let gemini_config = gemini.get(&name);
            let config = claude_config
                .or(codex_config)
                .or(gemini_config)
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            serde_json::json!({
                "name": name,
                "config": config,
                "apps": {
                    "claude": claude_config.is_some(),
                    "codex": codex_config.is_some(),
                    "gemini": gemini_config.is_some(),
                }
            })
        })
        .collect();

    Ok(serde_json::json!({ "servers": servers }))
}

/// 按应用启停保存一个 MCP 服务器：
/// apps 形如 { claude: bool, codex: bool, gemini: bool }，
/// 启用的应用写入该 server 配置，停用的应用里若存在同名条目则移除
#[tauri::command]
pub(crate) fn save_unified_mcp_server(
    name: String,
    config: serde_json::Value,
    apps: serde_json::Value,
) -> Result<(), String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("服务器名称不能为空".into());
    }
    let enabled = |key: &str| apps.get(key).and_then(|v| v.as_bool()).unwrap_or(false);

    apply_json_mcp_change(
        &claude_mcp_path(),
        &name,
        enabled("claude").then_some(&config),
    )?;
    apply_codex_mcp_change(&name, enabled("codex").then_some(&config))?;
    apply_json_mcp_change(
        &gemini_settings_path(),
        &name,
        enabled("gemini").then_some(&config),
    )?;
    Ok(())
}

/// 从 Claude / Codex / Gemini 三处删除同名 MCP 服务器（不存在则跳过）
#[tauri::command]
pub(crate) fn delete_unified_mcp_server(name: String) -> Result<(), String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("服务器名称不能为空".into());
    }
    apply_json_mcp_change(&claude_mcp_path(), &name, None)?;
    apply_codex_mcp_change(&name, None)?;
    apply_json_mcp_change(&gemini_settings_path(), &name, None)?;
    Ok(())
}

/// Get preset MCP server configurations
#[tauri::command]
pub(crate) fn get_mcp_presets() -> Vec<serde_json::Value> {
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
