# VarSwitch

VarSwitch 是一个面向 Claude Code、Codex CLI、Grok CLI、Gemini CLI 和 API 用户的桌面配置管理工具。它通过可视化界面集中管理 API Key / Token、Base URL、模型、编辑器设置、四个应用的多套配置、Skills、Prompts 预设、MCP Server、本机历史会话、Codex 插件市场与移动端控制能力，并提供托盘快速切换、本地代理故障转移、`varswitch://` 深链导入与多设备数据同步。

当前应用版本：`2.3.5`。

## 主要功能

### 1. Claude Code 配置管理

- **多配置管理**：创建、编辑、删除多套 Claude Code/API 配置。
- **供应商预设**：内置 11 个供应商预设（Anthropic 官方、DeepSeek、Kimi、智谱 GLM、MiniMax、阿里云百炼、火山方舟 Coding Plan、百度千帆、美团 LongCat、SiliconFlow、OpenRouter），也支持自定义。
- **配置字段**：支持配置名称、Token/API Key、Base URL、可选模型 ID。
- **角色模型映射**：每套配置可选填 Sonnet / Opus / Haiku 三个角色模型，分别写入 `ANTHROPIC_DEFAULT_SONNET_MODEL` / `ANTHROPIC_DEFAULT_OPUS_MODEL` / `ANTHROPIC_DEFAULT_HAIKU_MODEL`，留空则不设置并清理旧值。
- **OpenAI 兼容供应商**：「API 格式」选择 OpenAI Chat Completions 时，`ANTHROPIC_BASE_URL` 指向本地协议转换代理（`127.0.0.1:25789`），由代理在 Anthropic 与 OpenAI 协议之间双向翻译（含流式响应与工具调用），可直接接入仅提供 OpenAI 接口的上游；该模式需保持 VarSwitch 运行。
- **导入当前配置**：从当前系统环境变量、Claude 设置或编辑器设置中导入已有配置。
- **一键切换**：点击配置卡片的切换按钮后，同步写入：
  - 系统环境变量：`ANTHROPIC_AUTH_TOKEN` / `ANTHROPIC_AUTH_KEY`、`ANTHROPIC_BASE_URL`，以及角色模型映射变量
  - 兼容旧变量：读取并清理/兼容 `ANTHROPIC_API_KEY`
  - Claude 配置：`~/.claude/settings.json` 的 `env`
  - 支持的编辑器 `settings.json` 中的 `claudeCode.environmentVariables`
- **实时状态检查**：首页展示系统环境变量、Claude 设置、各编辑器设置是否与当前配置同步。
- **切换进度与取消**：切换过程中显示系统环境变量、编辑器、Claude 等步骤进度，并支持取消切换。
- **手动同步**：当前有活动配置时，可使用 `Sync Now` 重新写入当前配置。
- **接口测速**：添加/编辑配置时可对 Base URL 做连通性和延迟测试。
- **拖拽排序**：配置卡片提供拖拽手柄，可调整列表顺序并自动保存；Codex、Grok、Gemini 配置列表同样支持。

### 2. 本地代理故障转移

针对经本地协议转换代理的 OpenAI 兼容配置，提供高可用能力：

- **备用上游池**：在其他 OpenAI 兼容格式的配置中勾选「加入代理故障转移池」，即可作为备用上游。
- **自动故障转移**：请求候选顺序为当前激活配置加池内配置（按列表顺序）。在尚未向客户端返回任何数据前，连接失败/超时、5xx、429 会自动换下一个候选重试；4xx 视为配置问题直接返回；流式响应一旦开始转发不再切换。
- **熔断器**：单个上游连续失败 3 次后熔断 60 秒，期间跳过该上游；到期进入半开状态放行一个探测请求，成功则恢复，失败则重新熔断。
- **代理健康面板**：激活配置走本地代理时，Claude 页显示代理运行状态、主/备上游、失败与总请求数、自动转移次数、最近错误，并支持一键重置熔断器。

### 3. 编辑器同步

VarSwitch 会自动检测并同步下列编辑器的 Claude Code 设置：

- VS Code
- VS Code Insiders
- Cursor
- Windsurf
- Trae
- VSCodium

在「Settings / 设置」中可以为每个编辑器手动指定 `settings.json` 路径；也可以填写用户配置目录，应用会自动补全为 `settings.json`。

### 4. Codex CLI 配置管理

- **Codex 独立配置列表**：在 `Codex CLI` 页面维护 Codex 专用配置。
- **配置字段**：配置名称、API Key、Base URL、模型、Provider、写入方式。
- **Provider 预设**：内置 13 个 Provider 预设（OpenAI 官方、DeepSeek、Kimi、智谱 GLM、MiniMax、SiliconFlow、OpenRouter、阿里云百炼、火山方舟 Coding Plan、百度千帆、美团 LongCat、Groq、心流 iFlow），也支持自定义。
- **两种写入方式**：
  - 默认写入：写入 `~/.codex/auth.json` 和 `~/.codex/config.toml`
  - 官方账号/API 额度模式：只写入 `~/.codex/config.toml`，不改动 `auth.json`
- **Codex 当前状态**：读取并展示 Codex 当前 API Key、Base URL、模型、Provider 等状态。
- **Codex 诊断**：检查 `~/.codex/config.toml`、`~/.codex/auth.json` 是否存在，是否包含模型、Provider、Base URL、API Key，并给出修复建议。
- **Codex 运行时备份**：可一键备份 `config.toml` 和 `auth.json`。
- **手动同步**：当前有活动 Codex 配置时，可使用 `Sync Now` 重新写入。

### 5. Grok CLI 与 Gemini CLI 配置管理

- **Grok 独立配置列表**：内置 4 个 xAI 预设（Official、Grok Fast、Grok 4.5、Grok Build），也支持自定义。切换时写入 `~/.grok/config.toml`（把默认模型指向 VarSwitch 托管的模型段，保留文件中的其他内容），并同步 `XAI_API_KEY` / `XAI_BASE_URL`、`GROK_API_KEY` / `GROK_BASE_URL` 等系统环境变量。
- **Gemini 独立配置列表**：切换时写入 `~/.gemini/settings.json`（认证方式与模型），并同步 `GEMINI_API_KEY`、`GOOGLE_GEMINI_BASE_URL`、`GEMINI_MODEL` 系统环境变量。
- **当前状态**：读取并展示 Grok / Gemini 当前生效的配置来源、Base URL 与模型。

### 6. Codex Toolbox

Codex 页面提供 `Codex Toolbox`，包含三个主要模块。

#### Plugin Market

- 查看当前 Codex 插件市场配置。
- 安装/应用插件市场到 Codex。
- 支持多个来源类型，包括 GitHub、GitCode、本地路径等。
- 保留已有插件市场片段并写回 `~/.codex/config.toml`。
- 检测 OpenAI Codex 内置插件市场。
- 修复 OpenAI bundled 插件市场配置。
- 查看内置插件状态、已启用数量、关键插件启用情况。
- 一键启用关键插件，例如 Computer Use / Chrome 相关插件。

#### Session Sync

- 扫描本机 Codex 会话记录。
- 将本地 Codex 历史会话同步到 VarSwitch。
- 展示已同步会话数量、最后同步时间和会话预览。
- 为移动端控制选择可操作的 Codex 会话。

#### Mobile Control

- 将移动平台消息转发到指定 Codex 会话，实现手机端控制。
- 支持绑定/配置：
  - 飞书 / Lark
  - QQ
  - 微信
- 支持配置平台凭据，如 App ID、App Secret、Bot Token、Account ID、Base URL、User ID、Bot Open ID、Gateway URL。
- 支持扫码绑定 QQ、微信。
- 支持飞书/Lark 机器人创建、注册轮询和已有机器人绑定。
- 支持选择/绑定/解绑 Codex 会话。
- 支持启动/停止平台监听。
- 支持高级控制状态检测、协议事件日志和待审批请求提交。

### 7. 会话管理器

「会话（Sessions）」页面用于管理本机 CLI 历史会话：

- 聚合扫描 Claude Code（`~/.claude/projects`）与 Codex（`~/.codex/sessions`、`~/.codex/archived_sessions`）的会话记录。
- 列表展示会话标题、工作目录与更新时间，按更新时间降序排列。
- 支持按标题、目录或会话 ID 搜索，并按来源（全部 / Claude / Codex）筛选。
- **一键恢复**：Claude 会话在新终端窗口执行 `claude -r <会话ID>`（原工作目录仍存在时先切换过去）；Codex 会话通过 `codex://threads/<ID>` 深链在桌面应用中打开。

### 8. Skills 管理

顶部工具栏的 Skills 入口用于管理 Claude Code 扩展能力：

- 查看已安装的 slash commands 与 skills。
- 新增、编辑、删除本地 Skill/Command。
- 读取 `~/.claude/commands` 与 `~/.claude/skills`。
- 从 `SKILL.md` frontmatter 中解析 description。
- Discover 页浏览技能目录。
- 支持管理技能仓库来源。
- 支持从 GitHub 搜索 skills。
- 支持从 URL 安装 skill。
- **从 ZIP 安装**：选择本地 ZIP 一键安装 Skill，自动定位包内 `SKILL.md`（支持位于根目录、唯一顶层目录或子目录），内置 zip-slip 路径穿越防护；可同时安装到 Claude（`~/.claude/skills/`）与 Codex（`~/.codex/skills/`），同名技能已存在时确认后覆盖。

### 9. Prompts 预设库与编辑

顶部工具栏的 Prompts 入口用于维护多套提示词并跨应用同步：

- **预设库（Presets）**：维护多套提示词预设，每套可勾选生效应用（Claude / Codex / Gemini）。
- **激活即写入**：激活预设时，把内容写入所勾选应用的 live 文件：`~/.claude/CLAUDE.md`、`~/.codex/AGENTS.md`、`~/.gemini/GEMINI.md`。
- **回填保护**：激活新预设前，若检测到 live 文件在上次写入后被手工修改过，会先把这些修改回填到原激活预设，避免手工改动丢失。
- **编辑器（Editor）**：直接读取和保存 `~/.claude/CLAUDE.md`。
- **模板库（Templates）**：可快速插入/替换常用提示词模板，也可将模板一键存为预设。
- 内置模板类别包括：
  - 中文开发者
  - 简洁模式
  - 代码质量
  - 安全优先
  - Full-Stack TypeScript
  - Python
  - Rust
  - React & Next.js
  - 软件架构
  - 数据库设计
  - TDD
  - Claude Code 最佳实践
  - Git 工作流
  - 错误处理
  - Code Review
  - 项目脚手架
  - 重构指南

### 10. MCP Server 跨应用管理

顶部工具栏的 MCP 入口统一管理三个应用的 MCP Server：

- **聚合展示**：汇总 Claude（`~/.claude.json` 的 `mcpServers`）、Codex（`~/.codex/config.toml` 的 `[mcp_servers.<name>]` 段）、Gemini（`~/.gemini/settings.json` 的 `mcpServers`）中的全部 Server。
- **按应用启停**：每个 Server 卡片提供 Claude / Codex / Gemini 应用徽标（chip），点击即可单独启停；在最后一个启用应用中停用会先确认，再从所有应用移除。
- **保留原文件内容**：写回 `~/.codex/config.toml` 时保留其他配置内容，兼容 `env` 内联表与子表两种写法。
- 新增、编辑、删除 MCP Server，可勾选写入的应用。
- 使用 JSON 方式编辑单个 MCP Server 配置。
- 提供 MCP Server 预设列表。
- 支持在 GitHub 搜索 MCP Server。
- 支持打开相关 GitHub 页面。

### 11. 设置、备份与恢复

在 Settings 中可以管理应用行为与路径：

- **Language**：中文 / English。
- **Theme**：Light / Dark。
- **Launch at startup**：开机自启。
- **Silent startup**：启动后静默最小化到系统托盘。
- **Minimize to tray**：关闭窗口时隐藏到托盘而不是退出。
- **Config directory**：打开 VarSwitch 应用配置目录。
- **数据目录（多设备同步）**：查看当前数据目录，切换到自定义目录或恢复默认，详见下一节。
- **Claude settings**：查看 `~/.claude/settings.json` 路径。
- **Runtime logs**：打开运行日志目录。
- **编辑器路径**：查看自动检测状态，或手动设置各编辑器 `settings.json` 路径。
- **Export Profiles**：导出 Claude 配置列表。
- **Import Profiles**：导入 Claude 配置列表。
- **Auto backup**：切换前自动备份配置。
- **Roll back**：查看并恢复配置备份。

自动备份包含：

- Claude / Codex / Grok / Gemini 四份配置列表备份
- Codex 运行时文件备份：`~/.codex/config.toml`、`~/.codex/auth.json`
- Grok 运行时文件备份：`~/.grok/config.toml`

### 12. 数据可靠性与多设备同步

- **原子写入**：所有配置类文件（应用数据、`~/.claude/settings.json`、`~/.codex/config.toml` 等）都先写同目录临时文件、再重命名替换，进程崩溃或断电也不会留下写坏的半成品配置。
- **自定义数据目录**：在 Settings 中可把数据目录指向 OneDrive、Dropbox、坚果云、NAS 等同步文件夹，多台设备共享同一套配置数据。
- **智能复制**：切换目录时把现有 `*.json` 配置与 `backups` 目录复制到目标位置，目标已存在同名文件时跳过、绝不覆盖，避免冲掉其他设备同步来的更新数据。
- **指针文件**：指向关系记录在默认数据目录下的 `data_dir_override.txt`；目标目录不可用（如网盘未挂载）时自动回落默认目录，不影响启动。

### 13. Deep Link 一键导入

应用注册 `varswitch://` 协议，支持从网页、文档或聊天中一键导入：

- `varswitch://import/profile?app=<claude|codex|gemini|grok>&payload=<base64url(JSON)>`：导入单条配置，payload 含 `name`、`apiKey`，可选 `baseUrl`、`model` 等字段。
- `varswitch://import/mcp?payload=<base64url(JSON)>`：导入 MCP Server，payload 含 `name`、`config`，可选 `apps` 指定启用的应用。
- **导入前确认**：链接只做解析与校验，随后弹出确认框展示导入内容，API Key 打码显示（仅保留前 6 位与后 4 位），并提示确认链接来源可信，防止钓鱼。
- 确认后仅新增配置，不会自动激活或切换。

### 14. 更新、下载站与仓库入口

首页快捷入口包括：

- **Usage Guide**：打开内置使用指南。
- **Check for Updates**：检查应用更新。
- **Download Site**：打开下载站。
- **GitHub Repo**：打开项目仓库。
- **Codex Toolbox**：快速进入 Codex 工具箱。

应用内更新使用 Tauri updater，更新源为：

```text
https://download.varswitch.strova.top/latest.json
```

下载页入口为：

```text
https://download.varswitch.strova.top/
```

### 15. 系统托盘

- 启动后创建系统托盘图标。
- 左键点击托盘图标可恢复主窗口。
- **托盘快速切换**：托盘菜单按应用分为 Claude / Codex / Gemini / Grok 四个子菜单，子菜单标题显示当前激活配置名（如 `Claude · 主力配置`），菜单项列出该应用全部配置并在激活项上打勾，点击即可在后台完成切换。
- 配置增删改、切换或导入后，托盘菜单自动刷新。
- 托盘菜单还包含：
  - 显示主窗口
  - 退出
- 若启用「关闭窗口时最小化到托盘」，点击窗口关闭按钮会隐藏窗口。
- 若启用「静默启动」，应用启动后直接隐藏到托盘。

## 快速使用

### Claude Code 配置切换

1. 打开 VarSwitch。
2. 在 `Claude Code` 页面点击 `+ 添加配置`。
3. 填写配置名称、令牌、Base URL，可选填写模型 ID。
4. 如需确认接口可用，点击 `Test Speed` 测试地址。
5. 保存配置。
6. 在配置列表中点击 `Switch`。
7. 等待系统环境变量、编辑器设置、Claude 设置全部同步完成。
8. 重启终端和编辑器，让新的环境变量生效。

如果本机已经有可用配置，可以点击顶部 `导入当前配置`，从现有系统/Claude/编辑器配置中导入。

### Codex CLI 配置切换

1. 切换到 `Codex CLI` 页面。
2. 点击 `+ 添加配置`。
3. 选择预设或使用自定义 Provider。
4. 填写 API Key、Base URL、模型、Provider。
5. 选择写入方式：
   - 普通 API Key：使用默认写入。
   - 已登录官方账号且希望走 API 额度：选择官方账号/API 额度模式。
6. 保存配置。
7. 在配置列表中点击 `Switch`。
8. 如有异常，点击 `刷新诊断` 查看问题和建议。

### 管理 Claude Code Skills

1. 点击顶部工具栏的 Skills 图标。
2. 在 `Installed` 中查看本地 commands/skills。
3. 点击 `+ Add Skill` 创建新项，或选择已有项编辑。
4. 点击 `从 ZIP 安装` 选择本地 ZIP，勾选安装到 Claude / Codex 后确认。
5. 在 `Discover` 中浏览仓库技能目录。
6. 可使用 GitHub 搜索或 Repos 管理技能来源。

### 管理 Prompts 预设

1. 点击顶部工具栏的 Prompts 图标。
2. 在 Presets 中新建或编辑预设，勾选要生效的应用（Claude / Codex / Gemini）。
3. 激活预设，内容即写入所勾选应用的 live 文件。
4. 在 Editor 中可直接编辑 `~/.claude/CLAUDE.md`。
5. 可切换到 Templates 选择内置模板，或将模板一键存为预设。

### 管理 MCP Server

1. 点击顶部工具栏的 MCP 图标。
2. 在 Installed 中查看聚合自 Claude、Codex、Gemini 的 MCP Server。
3. 点击 `+ Add Server` 添加服务。
4. 填写 Server Name、JSON 配置，并勾选启用到的应用。
5. 保存后写入所选应用的配置文件；也可以直接点击卡片上的应用徽标启停。
6. 可在 Presets 中查看预设或搜索 GitHub。

### 使用 Codex 移动端控制

1. 进入 `Codex CLI` 页面。
2. 点击 `打开 Codex Toolbox`。
3. 在 `Session Sync` 中点击 `Sync Now`，导入本机 Codex 会话。
4. 在 `Mobile Control` 中选择要控制的会话。
5. 选择飞书/Lark、QQ 或微信，并完成平台凭据或扫码绑定。
6. 点击 `Start Platform Link` 启动监听。
7. 在手机端对应平台发送消息，即可转发到绑定的 Codex 会话。

## 配置写入位置

### Claude Code/API

| 位置 | 写入内容 |
| --- | --- |
| 系统环境变量 | `ANTHROPIC_AUTH_TOKEN` / `ANTHROPIC_AUTH_KEY`、`ANTHROPIC_BASE_URL`，选填角色模型映射 `ANTHROPIC_DEFAULT_SONNET_MODEL` / `ANTHROPIC_DEFAULT_OPUS_MODEL` / `ANTHROPIC_DEFAULT_HAIKU_MODEL` |
| Claude 设置 | `~/.claude/settings.json` 的 `env`（含角色模型映射） |
| 编辑器设置 | `claudeCode.environmentVariables`（含角色模型映射） |
| VarSwitch 配置 | 数据目录中的 profiles 数据 |

### Codex CLI

| 位置 | 写入内容 |
| --- | --- |
| `~/.codex/auth.json` | `OPENAI_API_KEY`，仅默认写入模式 |
| `~/.codex/config.toml` | `model_provider`、`model`、`chatgpt_base_url`、`model_providers`、`base_url` 等 |
| VarSwitch 配置 | 数据目录中的 Codex profiles 数据 |

### Grok CLI

| 位置 | 写入内容 |
| --- | --- |
| `~/.grok/config.toml` | VarSwitch 托管的 `[model.*]` 段（模型、Base URL、API Key 等）并设为默认模型，保留其他内容 |
| 系统环境变量 | `XAI_API_KEY` / `XAI_BASE_URL`、`GROK_API_KEY` / `GROK_BASE_URL`，选填 `XAI_MODEL` |
| VarSwitch 配置 | 数据目录中的 Grok profiles 数据 |

### Gemini CLI

| 位置 | 写入内容 |
| --- | --- |
| `~/.gemini/settings.json` | 认证方式（`security.auth.selectedType`）与模型（`model.name`） |
| 系统环境变量 | `GEMINI_API_KEY`、`GOOGLE_GEMINI_BASE_URL`、`GEMINI_MODEL` |
| VarSwitch 配置 | 数据目录中的 Gemini profiles 数据 |

### Prompts 预设

| 位置 | 写入内容 |
| --- | --- |
| `~/.claude/CLAUDE.md` | 激活预设且勾选 Claude 时写入；Editor 直接编辑 |
| `~/.codex/AGENTS.md` | 激活预设且勾选 Codex 时写入 |
| `~/.gemini/GEMINI.md` | 激活预设且勾选 Gemini 时写入 |
| VarSwitch 配置 | 数据目录中的 `prompt_presets.json` |

### MCP Server

| 位置 | 写入内容 |
| --- | --- |
| `~/.claude.json` | `mcpServers` 对象 |
| `~/.codex/config.toml` | `[mcp_servers.<name>]` 段，保留文件其他内容 |
| `~/.gemini/settings.json` | `mcpServers` 对象 |

### Skills

| 位置 | 写入内容 |
| --- | --- |
| `~/.claude/skills/<name>/` | 新建或从 ZIP / URL 安装的 Skill |
| `~/.codex/skills/<name>/` | ZIP 安装勾选 Codex 时写入 |

### 数据目录

| 位置 | 写入内容 |
| --- | --- |
| 默认数据目录 | 应用数据（profiles、prompt_presets 等 `*.json` 与 `backups/`），未自定义时使用 |
| `data_dir_override.txt` | 默认数据目录下的指针文件，内容为自定义数据目录的绝对路径 |
| 自定义数据目录 | 指针文件生效后，上述应用数据改存于该目录 |

## 开发环境要求

- Node.js >= 18
- Rust >= 1.70
- Tauri v2 系统依赖

## 开发

```bash
npm install
npm run tauri dev
```

Windows 用户也可以使用：

```bash
dev.bat
```

## 构建

```bash
npm run tauri build
```

Windows 用户也可以使用：

```bash
build.bat
```

构建产物位于：

```text
src-tauri/target/release/bundle/
```

Windows 生成 `.exe`（NSIS）安装包，macOS 生成 `.dmg`。

> macOS 安装包无法在 Windows 上构建。Apple 的系统框架、制作 `.dmg` 用的 `hdiutil`
> 以及签名工具链都只存在于 macOS，Rust 也无法从 Windows 交叉编译到 macOS。
> 因此 macOS 版本统一由 GitHub Actions 的 macos runner 产出，见下方发布流程。

## CI/CD

推送 `v*` 标签或手动触发工作流后，会并行构建三个目标：

| 作业 | 安装包 |
| --- | --- |
| Windows x64 | `VarSwitch_<版本>_x64-setup.exe` |
| macOS Apple Silicon | `VarSwitch_<版本>_aarch64.dmg` |
| macOS Intel | `VarSwitch_<版本>_x64.dmg` |

每个作业同时产出更新包与 `.sig` 签名供自动更新使用。macOS 的更新包被显式重命名为
`VarSwitch_<版本>_<架构>.app.tar.gz`，因为 Tauri 默认命名不含架构，两个 macOS
作业会上传同名文件并互相覆盖。

需要在仓库 Settings > Secrets and variables > Actions 中配置：

- `TAURI_SIGNING_PRIVATE_KEY`：`src-tauri/updater.key` 的完整内容
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`：该私钥的密码

缺少这两项时工作流会直接失败。否则构建仍会成功，但产物没有签名，对应平台会被静默
排除在 `latest.json` 之外，表现为「CI 全绿，用户却收不到更新」。

## 发布流程

```bash
# 1. 本地构建 Windows 包并统一版本号
build.bat --version x.y.z

# 2. 推送标签，触发三平台构建
git tag vx.y.z && git push origin vx.y.z

# 3. 三个作业全绿后，把 macOS 产物同步进下载站
node sync-mac-release.mjs --token <GitHub Token>

# 4. 部署下载站
deploy-download-site.bat
```

本仓库是私有仓库，因此第 3 步必须带 Token（私有仓库对未认证请求一律返回 404）。
Token 只需要 `repo` 读权限，也可以写进环境变量 `GITHUB_TOKEN` 后省略该参数。

私有仓库的 macOS runner 按 10 倍分钟计费，GitHub Free 的 2000 分钟/月相当于约 200
分钟 macOS 构建时间。工作流已启用 Rust 编译缓存，首次构建较慢，之后会显著缩短。

Windows 与 macOS 必须发布同一版本号。若 `latest.json` 中两端版本不一致，落后的一端
会被反复提示更新却始终停留在旧版本，因此 `deploy-download-site.bat` 会主动丢弃版本
不匹配的 macOS 条目并给出提示。

`sync-mac-release.mjs` 默认把 dmg 与更新包下载到下载站，由自有域名分发，方便无法稳定
访问 GitHub 的用户；加 `--host github` 则只写入 GitHub Release 的链接，不占用部署体积。

### macOS 未公证说明

安装包目前没有做 Apple Developer 签名与公证。从浏览器下载后 Gatekeeper 会加上隔离
标记，首次打开会提示「VarSwitch 已损坏，无法打开」——文件本身是完好的。用户把 App
拖进「应用程序」后执行一次下面的命令即可正常使用：

```bash
sudo xattr -rd com.apple.quarantine /Applications/VarSwitch.app
```

通过应用内自动更新安装的版本不经过浏览器，不会带上隔离标记，无需重复执行。

若后续购买了 Apple Developer 账号，在仓库中配置以下 secrets 并在工作流里加入签名与
公证步骤，即可免去用户的这一步操作：

- `APPLE_CERTIFICATE`：`Developer ID Application` 证书导出的 `.p12`，需先转成 base64
- `APPLE_CERTIFICATE_PASSWORD`：导出 `.p12` 时设置的密码
- `APPLE_ID` / `APPLE_PASSWORD` / `APPLE_TEAM_ID`：公证用的 Apple ID、app-specific password 与 Team ID
- `KEYCHAIN_PASSWORD`：CI 临时 keychain 的密码

获取 `APPLE_CERTIFICATE` 的方式示例：

```bash
openssl base64 -A -in certificate.p12 -out certificate-base64.txt
```

## 项目结构

```text
├── public/              # 前端资源
│   ├── index.html       # 主页面
│   ├── app.js           # 应用逻辑
│   ├── app-helpers.js   # 前端辅助函数
│   ├── app.test.js      # 前端测试
│   ├── style.css        # 样式
│   └── app-icon.png     # 应用图标
├── src-tauri/           # Tauri / Rust 后端
│   ├── src/lib.rs       # 核心逻辑：配置读写、环境变量同步、托盘、会话、Codex Toolbox、Skills、Prompts、MCP、Deep Link
│   ├── src/claude_proxy.rs  # 本地协议转换代理：Anthropic/OpenAI 双向翻译、故障转移与熔断
│   ├── src/main.rs      # Tauri 入口
│   ├── Cargo.toml       # Rust 依赖
│   ├── tauri.conf.json  # Tauri 配置
│   └── capabilities/    # Tauri 权限配置
├── .github/workflows/   # CI 构建配置（Windows + macOS 双架构）
├── varswitch-download-site/  # 下载站，含 releases 与 latest.json
├── dev-server.js        # 前端开发静态服务器
├── dev.bat              # Windows 开发脚本
├── build.bat            # Windows 构建脚本
├── sync-mac-release.mjs # 把 CI 构建的 macOS 产物同步进下载站
└── README.md            # 使用说明
```

## 许可证

MIT
