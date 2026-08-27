# Claude Desktop 动态模型与汉化入口 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复 Windows MSIX 安装检测，让 Claude Desktop Gateway 动态展示并精确转发第三方 API 可用模型，并提供可还原的 Claude Desktop 汉化入口。

**Architecture:** 保留现有 Claude Desktop Profile、独立 Gateway 和 `fetch_available_models` 合同。Gateway 增加按 Profile 凭据哈希索引的短时模型目录缓存：优先返回上游安全 Claude 模型，失败时回退配置模型和旧角色别名；消息请求对真实模型 ID 直通，对旧三角色路由继续映射。汉化功能独立为 Tauri 后端模块，使用打包的第三方 PowerShell 脚本和翻译库，首次复制到可写应用数据目录后以管理员权限执行。

**Tech Stack:** Rust/Tauri 2, `reqwest` blocking client, `tiny_http`, serde JSON, vanilla JavaScript, Node `node:test`, PowerShell, Tauri bundle resources。

## Global Constraints

- 使用简体中文回复；代码标识符、命令、日志和报错保持原文。
- 保留当前工作区无关修改、换行差异和未跟踪 `VARSWITCH_PROTOCOL.md`，只提交本次明确新增/修改文件。
- 不把非 Claude 模型改名伪装成 Claude 模型；不改变 Claude Code、Codex、Gemini、Grok、OpenCode 的配置合同。
- API Key 只在后端解密后用于请求，不写入日志、前端错误、Profile JSON 或 PowerShell 参数输出。
- 汉化资源保留 `claude-desktop-zh-simple` 的 Apache-2.0 `LICENSE` 和来源链接；项目归档及新版可能失效必须在 UI 和错误中说明。
- 每个生产代码变更先有对应失败测试并观察失败，再写最小实现；每个任务完成后运行针对性测试。

---

### Task 1: 修复 Claude Desktop Windows MSIX 路径与安装检测

**Files:**
- Modify: `src-tauri/src/claude_desktop.rs`
- Test: `src-tauri/src/claude_desktop.rs` 内 `#[cfg(test)]` 模块
- Modify: `src-tauri/src/claude_desktop_provider.rs`（仅在调用新路径选择辅助函数时）

**Interfaces:**
- Produce `pub fn claude_desktop_config_path() -> PathBuf`：Windows 优先 `%LOCALAPPDATA%/Claude`，传统路径作为回退。
- Produce `pub fn claude_desktop_installation_evidence() -> ClaudeDesktopInstallationEvidence` 或等价内部结构，供状态命令返回最终路径和 evidence 字段。

- [ ] **Step 1: 写失败测试**

增加纯路径辅助函数测试，固定临时根目录并断言：LocalAppData 下存在 `Claude/claude_desktop_config.json` 时优先它；LocalAppData 缺失而 AppData 存在时回退传统路径；仅存在 `Claude-3p` 时不判定已安装；包数据目录证据可判定安装但不改变配置写入路径。

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml claude_desktop -- --nocapture`

Expected: 新增路径优先级测试 FAIL，当前实现仍固定返回 `%APPDATA%/Claude` 或无法区分 `Claude-3p`。

- [ ] **Step 3: 最小实现**

抽出接受显式 `local_app_data`、`app_data` 和包证据的纯函数；Windows 运行时读取 `LOCALAPPDATA`/`APPDATA`，检测 `Claude` 配置文件、父目录和 `Packages/Claude_pzs8sxrjxfjjc`，不把 VarSwitch 自己的 3P 目录当唯一证据。状态命令返回所选 `configPath`，写入命令沿用同一路径。

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test --manifest-path src-tauri/Cargo.toml claude_desktop -- --nocapture`

Expected: 路径和安装检测测试 PASS，已有 MCP 配置读写测试保持 PASS。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/claude_desktop.rs src-tauri/src/claude_desktop_provider.rs
git commit -m "fix: 检测 Windows MSIX Claude Desktop"
```

### Task 2: 增加动态模型目录缓存与过滤

**Files:**
- Modify: `src-tauri/src/claude.rs`（公开复用阻塞模型拉取辅助函数）
- Modify: `src-tauri/src/claude_desktop_gateway.rs`
- Test: `src-tauri/src/claude_desktop_gateway.rs` 内 Gateway 测试

**Interfaces:**
- Consume `fetch_available_models_blocking(base_url, api_key, timeout_secs, protocol)`。
- Produce `model_catalog_response(runtime: &ClaudeDesktopRuntime) -> Value`，返回 Claude Desktop 可识别的真实模型 ID。
- Preserve `model_list_response(runtime)`、`request_targets(requested_model, runtime)` 现有调用点。

- [ ] **Step 1: 写失败测试**

新增测试覆盖：本地 tiny-http 上游返回 `claude-opus-5`、`claude-sonnet-4-6`、`gpt-5` 时 `/models` 只返回前两项；第二次请求命中短时缓存且不重复请求；缓存过期或上游失败时回退 Profile 中安全模型；真实 `claude-opus-5` 请求体原样改为该模型而不是 `upstream-default`。

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml claude_desktop_gateway -- --nocapture`

Expected: 动态目录、缓存和精确转发测试 FAIL，当前只生成三个固定角色别名并把 `claude-opus-5` 当成 Opus 角色映射。

- [ ] **Step 3: 最小实现**

在 Gateway 内增加 `OnceLock<Mutex<HashMap<String, ModelCatalogCache>>>`，key 使用 Profile URL、API 格式和 Key 的 SHA-1，不保存明文 Key；缓存 TTL 固定 300 秒。调用现有模型拉取逻辑，过滤 `claude-sonnet-`、`claude-opus-`、`claude-haiku-` 前缀，记录最多 200 字符错误摘要。只把固定 `SONNET_ROUTE_ID`、`OPUS_ROUTE_ID`、`HAIKU_ROUTE_ID` 作为旧别名；其他安全 Claude ID 作为精确模型直通。失败时从 Profile 的默认/角色字段收集安全模型，最后才回退旧别名。

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test --manifest-path src-tauri/Cargo.toml claude_desktop_gateway -- --nocapture`

Expected: 动态目录、缓存、过滤、精确模型和旧别名测试 PASS。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/claude.rs src-tauri/src/claude_desktop_gateway.rs
git commit -m "feat: 动态发现 Claude Desktop Gateway 模型"
```

### Task 3: 配置弹窗增加模型获取与选择控件

**Files:**
- Modify: `public/index.html`
- Modify: `public/app.js`
- Test: `public/app.test.js`

**Interfaces:**
- Consume existing Tauri command `fetch_available_models` with `protocol: "claude"`/`"anthropic"`。
- Produce DOM controls bound to `claudeDesktopProfileModelId`, `claudeDesktopProfileSonnetModel`, `claudeDesktopProfileOpusModel`, `claudeDesktopProfileHaikuModel`。

- [ ] **Step 1: 写失败测试**

在 `public/app.test.js` 增加静态行为断言：Claude Desktop 表单有 `claudeDesktopProfileModelFetchBtn`、模型结果容器和四个 `select`/可选输入；`app.js` 包含调用 `fetch_available_models`、保留当前值、填充选项和过滤提示的逻辑。

- [ ] **Step 2: 运行测试确认失败**

Run: `node --test public/app.test.js`

Expected: 新增 DOM/绑定断言 FAIL，因为当前表单只有纯文本输入且没有 Desktop 专用拉取逻辑。

- [ ] **Step 3: 最小实现**

在 Base URL/API Key 行增加“获取模型”；为四个字段使用带可编辑回退的 `select` 或 datalist，避免无法识别的旧模型丢失；实现 `fetchClaudeDesktopModels()`，调用现有命令、按 ID 去重、只提示非 Claude ID 被过滤，不成功时不覆盖原值；保存仍发送字符串字段并保留现有后端校验。

- [ ] **Step 4: 运行测试确认通过**

Run: `node --test public/app.test.js`

Expected: 前端全量测试 PASS，新增模型获取控件断言 PASS。

- [ ] **Step 5: 提交**

```bash
git add public/index.html public/app.js public/app.test.js
git commit -m "feat: 为 Claude Desktop 配置增加模型选择"
```

### Task 4: 集成 claude-desktop-zh-simple 汉化脚本资源与后端命令

**Files:**
- Create: `src-tauri/src/claude_desktop_localization.rs`
- Create: `src-tauri/resources/claude-desktop-zh-simple/scripts/claude-desktop-zh-simple.ps1`
- Create: `src-tauri/resources/claude-desktop-zh-simple/translation_memory.json`
- Create: `src-tauri/resources/claude-desktop-zh-simple/version.json`
- Create: `src-tauri/resources/claude-desktop-zh-simple/LICENSE`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/tauri.conf.json`
- Test: `src-tauri/src/claude_desktop_localization.rs` 内单元测试

**Interfaces:**
- Produce `get_claude_desktop_localization_status(app: AppHandle) -> Result<Value, String>`。
- Produce `run_claude_desktop_localization(app: AppHandle, action: String) -> Result<Value, String>`，action 白名单 `status|patch|restore`。
- Produce `open_claude_desktop_localization_project() -> Result<(), String>`。

- [ ] **Step 1: 写失败测试**

为 action 白名单、PowerShell 参数构造、非 Windows 拒绝、资源目录初始化和输出脱敏增加纯函数测试。断言 `patch` 包含 `-Action patch -Yes -SkipUpdateCheck`，不包含 API Key 或任意用户输入；未知 action 返回错误。

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml claude_desktop_localization -- --nocapture`

Expected: 新模块测试 FAIL，因为模块、命令和资源尚不存在。

- [ ] **Step 3: 最小实现**

从 `app.path().resource_dir()/claude-desktop-zh-simple` 复制四个静态资源到 `app.path().app_data_dir()/claude-desktop-zh-simple`（只在目标静态文件缺失时复制）；Windows 使用 `powershell.exe -NoProfile -ExecutionPolicy Bypass -File <script> -Action <action> -Yes -SkipUpdateCheck`，通过 `Start-Process -Verb RunAs -Wait -PassThru` 获取管理员退出码；捕获 stdout/stderr 时移除疑似 Key、`enc:v1:` 和长 token，仅返回最后 12000 字符摘要。非 Windows 返回“不支持”。固定 GitHub URL 通过现有安全打开器打开。

- [ ] **Step 4: 配置资源并注册命令**

在 `tauri.conf.json` 增加资源目录映射；`lib.rs` 注册三个命令。保留 Apache-2.0 许可证和来源说明。资源文件从已核对的仓库提交 `ec004b9` 原样复制，不修改其脚本逻辑。

- [ ] **Step 5: 运行测试确认通过**

Run: `cargo test --manifest-path src-tauri/Cargo.toml claude_desktop_localization -- --nocapture`

Expected: 参数白名单、脱敏和跨平台测试 PASS。

- [ ] **Step 6: 提交**

```bash
git add src-tauri/src/claude_desktop_localization.rs src-tauri/src/lib.rs src-tauri/tauri.conf.json src-tauri/resources/claude-desktop-zh-simple
git commit -m "feat: 集成 Claude Desktop 汉化入口"
```

### Task 5: Claude Desktop 页面增加汉化操作与本地化文案

**Files:**
- Modify: `public/index.html`
- Modify: `public/app.js`
- Modify: `public/app.test.js`
- Modify: `public/style.css`（仅增加操作区状态样式）

**Interfaces:**
- Consume `get_claude_desktop_localization_status`、`run_claude_desktop_localization`、`open_claude_desktop_localization_project`。
- Produce handlers `loadClaudeDesktopLocalizationStatus()`、`runClaudeDesktopLocalization(action)` and button bindings。

- [ ] **Step 1: 写失败测试**

增加静态断言：页面包含状态、汉化、还原、打开项目四个按钮及结果容器；`app.js` 绑定三个 Tauri 命令、动作白名单、确认退出提示和执行中禁用；中英文词条同时存在。

- [ ] **Step 2: 运行测试确认失败**

Run: `node --test public/app.test.js`

Expected: 汉化按钮和命令绑定断言 FAIL。

- [ ] **Step 3: 最小实现**

在 Desktop 页面操作区增加按钮，调用统一异步 handler；`patch`/`restore` 先用 `appConfirm` 提示完全退出 Claude Desktop，状态动作不需要确认；按钮执行时全部禁用并显示进度，成功显示脚本摘要，失败显示脱敏错误与“项目已归档/新版可能失效”提示；加载页面时读取状态并展示版本、资源路径、当前状态和缺失翻译计数。

- [ ] **Step 4: 运行测试确认通过**

Run: `node --test public/app.test.js`

Expected: 前端全量测试 PASS。

- [ ] **Step 5: 提交**

```bash
git add public/index.html public/app.js public/app.test.js public/style.css
git commit -m "feat: 增加 Claude Desktop 汉化操作按钮"
```

### Task 6: 集成验证、构建与发布前检查

**Files:**
- Modify only if verification exposes a regression; otherwise no source edits.
- Inspect: `build.bat`, `sync-mac-release.mjs`, `deploy-download-site.bat`, generated bundle resources。

- [ ] **Step 1: 运行格式、语法和全量测试**

Run:

```bash
node --check public/app.js
node --test public/app.test.js
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo test --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml
git diff --check
```

Expected: 所有命令退出码 0；记录 Node/Rust 测试总数。

- [ ] **Step 2: 运行 Tauri debug 构建**

Run: `npm exec tauri build -- --debug --no-bundle`

Expected: debug 构建成功，Rust 资源路径和命令注册无错误。

- [ ] **Step 3: 验证动态 Gateway HTTP 行为**

使用测试上游执行已授权 `GET /claude-desktop/v1/models`、真实模型 `POST /claude-desktop/v1/messages`、未授权请求和未知模型请求；检查请求体模型、Authorization 替换、过滤结果和错误状态，不输出密钥。

- [ ] **Step 4: 验证实际 Windows 桌面 UI**

启动新构建，确认状态卡识别当前 MSIX、模型获取按钮填充选择、Claude Desktop 菜单出现真实模型；确认汉化按钮可显示状态并在退出/管理员确认后返回真实脚本结果。若资源结构不兼容，保存失败报告和备份证据，不声明汉化成功。

- [ ] **Step 5: 按用户后续明确要求执行发布脚本**

只有在上述验证全部通过且用户仍要求发布时，才按当前 `build.bat`、GitHub macOS workflow、`sync-mac-release.mjs` 和 `deploy-download-site.bat` 的实际参数执行；发布前核对版本、远程 tag、Windows 安装包、macOS 双架构资源和下载站 HTTP 状态，禁止覆盖无关工作区修改。

