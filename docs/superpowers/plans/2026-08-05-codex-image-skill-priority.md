# Codex 图片 Skill 优先路由实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 保留 Codex 内置 `imagegen`，同时使完整配置的 `varswitch-imagegen` 成为图片生成请求的默认首选。

**Architecture:** 自定义 Skill 负责实际图片 API 调用；`~/.codex/AGENTS.md` 中的 VarSwitch 托管块负责跨工作区路由偏好。所有写入均使用固定标记进行局部、可逆更新。

**Tech Stack:** Rust、Tauri、Codex Skills、Codex `AGENTS.md`

## Global Constraints

- 保留内置 `imagegen`。
- 保留用户已有的 `~/.codex/AGENTS.md` 内容和 `config.toml` 非托管内容。
- 图片凭据仅保存在现有环境变量机制中，不写入 Skill 或指令文件。
- 不新增依赖。

---

### Task 1: 路由规则与 Skill 触发语义

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/src/lib.rs`

**Interfaces:**
- Produces: `merge_codex_image_priority_instructions(existing: &str, enabled: bool) -> String`
- Produces: `configure_codex_image_priority_instructions(enabled: bool) -> Result<(), String>`

- [ ] **Step 1: 写入失败测试**，覆盖添加、幂等更新、移除和用户内容保留。
- [ ] **Step 2: 运行目标 Rust 测试并确认按预期失败。**
- [ ] **Step 3: 实现最小的标记块合并和文件同步逻辑，并强化 Skill 描述。**
- [ ] **Step 4: 在图片 Skill 安装/移除流程中同步路由规则。**
- [ ] **Step 5: 运行目标测试并确认通过。**

### Task 2: 导入字段与真实 Codex 提示验证

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `LocationStatus.image_api_key`、`LocationStatus.image_base_url`
- Produces: 导入后字段完整的 `CodexProfile`

- [ ] **Step 1: 添加导入映射的回归测试或抽取可测试的构造函数。**
- [ ] **Step 2: 运行测试并确认失败。**
- [ ] **Step 3: 保留检测到的图片 Base URL，并通过最小实现使测试通过。**
- [ ] **Step 4: 运行 `cargo fmt --check`、目标 Rust 测试、完整 Rust 测试和前端测试。**
- [ ] **Step 5: 使用临时 `CODEX_HOME` 执行 `codex debug prompt-input`，确认两个 Skill 都可见且优先规则已注入。**
