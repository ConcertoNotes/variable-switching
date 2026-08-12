//! 提示词域：CLAUDE.md、提示词模板、预设库与回填（从 lib.rs 拆分，逻辑未改动）。

use crate::*;

pub(crate) fn claude_md_path() -> PathBuf {
    home_dir().join(".claude").join("CLAUDE.md")
}

// ── Claude Prompts Commands ─────────────────────────

#[tauri::command]
pub(crate) fn get_claude_md() -> Result<String, String> {
    let path = claude_md_path();
    if !path.exists() {
        return Ok(String::new());
    }
    fs::read_to_string(path).map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) fn save_claude_md(content: String) -> Result<(), String> {
    let path = claude_md_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    write_file_atomic(&path, &content)
}

/// Get built-in prompt templates
#[tauri::command]
pub(crate) fn get_prompt_templates() -> Vec<serde_json::Value> {
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

// ── Prompt Presets Commands（预设库 + 跨应用同步 + 回填保护）─────────
// 数据文件 data_dir/prompt_presets.json，结构：
// { presets: [{ id, name, content, apps: {claude, codex, gemini}, updatedAt }],
//   activeId: uuid | null,
//   lastWritten: { claude?: "上次激活写入的完整内容", codex?: ..., gemini?: ... } }
// live 文件：~/.claude/CLAUDE.md、~/.codex/AGENTS.md、~/.gemini/GEMINI.md。

/// 预设支持的应用列表，数组顺序即回填检测优先级（claude > codex > gemini）。
pub(crate) const PROMPT_PRESET_APPS: [&str; 3] = ["claude", "codex", "gemini"];

/// Gemini 全局记忆文件路径（~/.gemini/GEMINI.md）。
pub(crate) fn gemini_global_md_path() -> PathBuf {
    home_dir().join(".gemini").join("GEMINI.md")
}

/// 按应用名取对应的 live 提示词文件路径。
pub(crate) fn prompt_live_path(app_name: &str) -> Option<PathBuf> {
    match app_name {
        "claude" => Some(claude_md_path()),
        "codex" => Some(codex_global_agents_path()),
        "gemini" => Some(gemini_global_md_path()),
        _ => None,
    }
}

/// 预设数据文件路径。
pub(crate) fn prompt_presets_path(app: &tauri::AppHandle) -> PathBuf {
    data_dir(app).join("prompt_presets.json")
}

/// 当前 UTC 时间的 ISO8601 字符串（如 2026-08-12T07:00:00.000Z）。
/// 复用 civil_from_days 做日期换算，避免引入新依赖。
pub(crate) fn prompt_iso8601_now() -> String {
    let millis = chrono_timestamp_millis();
    let total_secs = (millis / 1000) as i64;
    let ms = (millis % 1000) as u64;
    let days = total_secs.div_euclid(86400);
    let secs_of_day = total_secs.rem_euclid(86400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{ms:03}Z",
        secs_of_day / 3600,
        (secs_of_day % 3600) / 60,
        secs_of_day % 60
    )
}

/// 读取预设数据文件；文件不存在或损坏时返回字段齐全的空结构，
/// 保证后续代码可以直接索引 presets / activeId / lastWritten。
pub(crate) fn read_prompt_presets_data(app: &tauri::AppHandle) -> serde_json::Value {
    let path = prompt_presets_path(app);
    let mut data = fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .unwrap_or(serde_json::Value::Null);
    if !data.is_object() {
        data = serde_json::json!({});
    }
    if !data.get("presets").map(|v| v.is_array()).unwrap_or(false) {
        data["presets"] = serde_json::json!([]);
    }
    if data.get("activeId").is_none() {
        data["activeId"] = serde_json::Value::Null;
    }
    if !data
        .get("lastWritten")
        .map(|v| v.is_object())
        .unwrap_or(false)
    {
        data["lastWritten"] = serde_json::json!({});
    }
    data
}

/// 持久化预设数据文件（原子写入，防止写坏）。
pub(crate) fn write_prompt_presets_data(
    app: &tauri::AppHandle,
    data: &serde_json::Value,
) -> Result<(), String> {
    let json = serde_json::to_string_pretty(data).map_err(|e| e.to_string())?;
    write_file_atomic(&prompt_presets_path(app), &json)
}

/// 读取某应用 live 文件的状态（路径 / 是否存在 / 内容），供前端展示。
pub(crate) fn prompt_live_state(app_name: &str) -> serde_json::Value {
    match prompt_live_path(app_name) {
        Some(path) => {
            let exists = path.is_file();
            let content = if exists {
                fs::read_to_string(&path).unwrap_or_default()
            } else {
                String::new()
            };
            serde_json::json!({
                "path": path.display().to_string(),
                "exists": exists,
                "content": content,
            })
        }
        None => serde_json::json!({ "path": "", "exists": false, "content": "" }),
    }
}

/// 组装返回给前端的完整数据：presets + activeId + 各应用 live 状态。
pub(crate) fn prompt_presets_response(data: &serde_json::Value) -> serde_json::Value {
    let mut live = serde_json::Map::new();
    for name in PROMPT_PRESET_APPS {
        live.insert(name.to_string(), prompt_live_state(name));
    }
    serde_json::json!({
        "presets": data.get("presets").cloned().unwrap_or_else(|| serde_json::json!([])),
        "activeId": data.get("activeId").cloned().unwrap_or(serde_json::Value::Null),
        "live": serde_json::Value::Object(live),
    })
}

/// 回填判定（纯函数，便于单测）。
/// entries 按优先级排好序（claude > codex > gemini），每项为
/// (应用名, live 文件内容, 上次激活写入的内容)：
/// - live 为 None（文件不存在 / 读取失败）→ 跳过，避免误把预设内容清空；
/// - lastWritten 为 None（该应用没有写入基准）→ 无从比较，跳过；
/// - live 与 lastWritten 不一致 → 命中，返回 (应用名, live 内容) 供回填。
pub(crate) fn detect_prompt_backfill(
    entries: &[(String, Option<String>, Option<String>)],
) -> Option<(String, String)> {
    for (app_name, live, last) in entries {
        let (Some(live), Some(last)) = (live.as_ref(), last.as_ref()) else {
            continue;
        };
        if live != last {
            return Some((app_name.clone(), live.clone()));
        }
    }
    None
}

/// 判断预设是否勾选了某个应用。
pub(crate) fn preset_app_enabled(preset: &serde_json::Value, app_name: &str) -> bool {
    preset
        .get("apps")
        .and_then(|apps| apps.get(app_name))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// 按 id 在 presets 数组中查找预设。
pub(crate) fn find_prompt_preset<'a>(data: &'a serde_json::Value, id: &str) -> Option<&'a serde_json::Value> {
    data.get("presets")?
        .as_array()?
        .iter()
        .find(|p| p.get("id").and_then(|v| v.as_str()) == Some(id))
}

/// 获取全部预设与各应用 live 文件状态。
#[tauri::command]
pub(crate) fn get_prompt_presets(app: tauri::AppHandle) -> Result<serde_json::Value, String> {
    let data = read_prompt_presets_data(&app);
    Ok(prompt_presets_response(&data))
}

/// 新建（id 为空）或更新（id 非空）一个预设；只改数据文件，不动 live 文件。
#[tauri::command]
pub(crate) fn save_prompt_preset(
    app: tauri::AppHandle,
    id: Option<String>,
    name: String,
    content: String,
    apps: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("预设名称不能为空".into());
    }
    // 归一化 apps：只保留支持的应用，统一为 bool，未提供的按未勾选处理
    let mut normalized = serde_json::Map::new();
    for app_name in PROMPT_PRESET_APPS {
        let enabled = apps
            .get(app_name)
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        normalized.insert(app_name.to_string(), serde_json::Value::Bool(enabled));
    }
    let apps_value = serde_json::Value::Object(normalized);

    let mut data = read_prompt_presets_data(&app);
    let now = prompt_iso8601_now();
    match id.as_deref() {
        Some(id) if !id.is_empty() => {
            // 更新已有预设
            let list = data["presets"]
                .as_array_mut()
                .ok_or("预设数据损坏：presets 不是数组")?;
            let preset = list
                .iter_mut()
                .find(|p| p.get("id").and_then(|v| v.as_str()) == Some(id))
                .ok_or_else(|| format!("预设不存在: {id}"))?;
            preset["name"] = serde_json::Value::String(name);
            preset["content"] = serde_json::Value::String(content);
            preset["apps"] = apps_value;
            preset["updatedAt"] = serde_json::Value::String(now);
        }
        _ => {
            // 新建预设，生成 uuid
            let preset = serde_json::json!({
                "id": uuid::Uuid::new_v4().to_string(),
                "name": name,
                "content": content,
                "apps": apps_value,
                "updatedAt": now,
            });
            data["presets"]
                .as_array_mut()
                .ok_or("预设数据损坏：presets 不是数组")?
                .push(preset);
        }
    }
    write_prompt_presets_data(&app, &data)?;
    Ok(prompt_presets_response(&data))
}

/// 删除预设；若删除的是当前激活预设则把 activeId 置空（live 文件保持不动）。
#[tauri::command]
pub(crate) fn delete_prompt_preset(app: tauri::AppHandle, id: String) -> Result<serde_json::Value, String> {
    let mut data = read_prompt_presets_data(&app);
    if let Some(list) = data["presets"].as_array_mut() {
        list.retain(|p| p.get("id").and_then(|v| v.as_str()) != Some(id.as_str()));
    }
    if data["activeId"].as_str() == Some(id.as_str()) {
        data["activeId"] = serde_json::Value::Null;
    }
    write_prompt_presets_data(&app, &data)?;
    Ok(prompt_presets_response(&data))
}

/// 激活预设：
/// a. 回填保护——旧激活预设启用的应用中，若 live 内容与上次写入（lastWritten）
///    不一致（按 claude > codex > gemini 优先级取第一个差异），把该 live 内容
///    写回旧预设 content 并更新其 updatedAt，避免用户手工修改丢失；
/// b. 把新预设 content 原子写入其勾选的各 live 文件（父目录不存在则创建）；
/// c. 更新 lastWritten（写了哪个应用记哪个）与 activeId。
#[tauri::command]
pub(crate) fn activate_prompt_preset(app: tauri::AppHandle, id: String) -> Result<serde_json::Value, String> {
    let mut data = read_prompt_presets_data(&app);
    let target = find_prompt_preset(&data, &id)
        .cloned()
        .ok_or_else(|| format!("预设不存在: {id}"))?;

    // ── a. 回填保护 ──
    let mut backfilled = false;
    let mut backfilled_app: Option<String> = None;
    let old_active_id = data["activeId"].as_str().map(|s| s.to_string());
    if let Some(old_id) = old_active_id {
        // 旧激活预设可能已被删除，此时没有回填对象；
        // 先收集只读信息（entries），再对 data 做可变修改，避免借用冲突
        let entries: Option<Vec<(String, Option<String>, Option<String>)>> =
            find_prompt_preset(&data, &old_id).map(|old_preset| {
                PROMPT_PRESET_APPS
                    .into_iter()
                    .filter(|&name| preset_app_enabled(old_preset, name))
                    .map(|name| {
                        let live = prompt_live_path(name).and_then(|p| fs::read_to_string(p).ok());
                        let last = data["lastWritten"]
                            .get(name)
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        (name.to_string(), live, last)
                    })
                    .collect()
            });
        if let Some(entries) = entries {
            if let Some((hit_app, live_content)) = detect_prompt_backfill(&entries) {
                if let Some(preset) = data["presets"]
                    .as_array_mut()
                    .and_then(|list| {
                        list.iter_mut().find(|p| {
                            p.get("id").and_then(|v| v.as_str()) == Some(old_id.as_str())
                        })
                    })
                {
                    preset["content"] = serde_json::Value::String(live_content);
                    preset["updatedAt"] = serde_json::Value::String(prompt_iso8601_now());
                    backfilled = true;
                    backfilled_app = Some(hit_app);
                    // 回填结果先落盘：即使后续写 live 文件失败，用户手工修改也已保住
                    write_prompt_presets_data(&app, &data)?;
                }
            }
        }
    }

    // ── b. 写入新预设内容到勾选的各 live 文件 ──
    let content = target
        .get("content")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    for name in PROMPT_PRESET_APPS {
        if !preset_app_enabled(&target, name) {
            continue;
        }
        let path = prompt_live_path(name).ok_or_else(|| format!("未知应用: {name}"))?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("创建目录 {} 失败: {e}", parent.display()))?;
        }
        write_file_atomic(&path, &content)?;
        // ── c. 写了哪个应用就记录哪个的 lastWritten ──
        data["lastWritten"][name] = serde_json::Value::String(content.clone());
    }
    data["activeId"] = serde_json::Value::String(id);
    write_prompt_presets_data(&app, &data)?;

    Ok(serde_json::json!({
        "data": prompt_presets_response(&data),
        "backfilled": backfilled,
        "backfilledApp": backfilled_app,
    }))
}

#[cfg(test)]
mod prompt_preset_tests {
    use super::detect_prompt_backfill;

    /// 便捷构造一条回填判定输入。
    fn entry(
        app: &str,
        live: Option<&str>,
        last: Option<&str>,
    ) -> (String, Option<String>, Option<String>) {
        (
            app.to_string(),
            live.map(|s| s.to_string()),
            last.map(|s| s.to_string()),
        )
    }

    #[test]
    fn no_backfill_when_live_matches_last_written() {
        // 所有应用 live 与 lastWritten 一致 → 无需回填
        let entries = vec![
            entry("claude", Some("A"), Some("A")),
            entry("codex", Some("B"), Some("B")),
            entry("gemini", Some("C"), Some("C")),
        ];
        assert_eq!(detect_prompt_backfill(&entries), None);
    }

    #[test]
    fn backfill_picks_first_diff_by_priority() {
        // claude 与 codex 均有差异 → 按优先级取 claude
        let entries = vec![
            entry("claude", Some("edited-claude"), Some("A")),
            entry("codex", Some("edited-codex"), Some("B")),
        ];
        assert_eq!(
            detect_prompt_backfill(&entries),
            Some(("claude".to_string(), "edited-claude".to_string()))
        );
    }

    #[test]
    fn backfill_falls_through_when_higher_priority_matches() {
        // claude 无差异、codex 有差异 → 命中 codex
        let entries = vec![
            entry("claude", Some("A"), Some("A")),
            entry("codex", Some("edited"), Some("B")),
            entry("gemini", Some("also-edited"), Some("C")),
        ];
        assert_eq!(
            detect_prompt_backfill(&entries),
            Some(("codex".to_string(), "edited".to_string()))
        );
    }

    #[test]
    fn missing_live_file_is_skipped() {
        // live 文件被用户删除（None）→ 跳过该应用，不误清空预设
        let entries = vec![
            entry("claude", None, Some("A")),
            entry("codex", Some("edited"), Some("B")),
        ];
        assert_eq!(
            detect_prompt_backfill(&entries),
            Some(("codex".to_string(), "edited".to_string()))
        );
    }

    #[test]
    fn missing_last_written_baseline_is_skipped() {
        // 没有写入基准（lastWritten 缺失）→ 无从比较，不回填
        let entries = vec![entry("claude", Some("manual content"), None)];
        assert_eq!(detect_prompt_backfill(&entries), None);
    }

    #[test]
    fn empty_entries_no_backfill() {
        // 旧预设未启用任何应用 → 无需回填
        assert_eq!(detect_prompt_backfill(&[]), None);
    }
}
