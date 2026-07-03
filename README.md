# VarSwitch

VarSwitch 是一个面向 Claude Code、Codex CLI 和 API 用户的桌面配置管理工具。它通过可视化界面集中管理 API Key / Token、Base URL、模型、编辑器设置、Claude 配置、Codex 配置、Skills、Prompts、MCP Server、Codex 插件市场与移动端控制能力。

当前应用版本：`2.3.5`。

## 主要功能

### 1. Claude Code 配置管理

- **多配置管理**：创建、编辑、删除多套 Claude Code/API 配置。
- **配置字段**：支持配置名称、Token/API Key、Base URL、可选模型 ID。
- **导入当前配置**：从当前系统环境变量、Claude 设置或编辑器设置中导入已有配置。
- **一键切换**：点击配置卡片的切换按钮后，同步写入：
  - 系统环境变量：`ANTHROPIC_AUTH_TOKEN` / `ANTHROPIC_AUTH_KEY`、`ANTHROPIC_BASE_URL`
  - 兼容旧变量：读取并清理/兼容 `ANTHROPIC_API_KEY`
  - Claude 配置：`~/.claude/settings.json` 的 `env`
  - 支持的编辑器 `settings.json` 中的 `claudeCode.environmentVariables`
- **实时状态检查**：首页展示系统环境变量、Claude 设置、各编辑器设置是否与当前配置同步。
- **切换进度与取消**：切换过程中显示系统环境变量、编辑器、Claude 等步骤进度，并支持取消切换。
- **手动同步**：当前有活动配置时，可使用 `Sync Now` 重新写入当前配置。
- **接口测速**：添加/编辑配置时可对 Base URL 做连通性和延迟测试。

### 2. 编辑器同步

VarSwitch 会自动检测并同步下列编辑器的 Claude Code 设置：

- VS Code
- VS Code Insiders
- Cursor
- Windsurf
- Trae
- VSCodium

在「Settings / 设置」中可以为每个编辑器手动指定 `settings.json` 路径；也可以填写用户配置目录，应用会自动补全为 `settings.json`。

### 3. Codex CLI 配置管理

- **Codex 独立配置列表**：在 `Codex CLI` 页面维护 Codex 专用配置。
- **配置字段**：配置名称、API Key、Base URL、模型、Provider、写入方式。
- **Provider 预设**：内置常用 Codex Provider 预设，也支持自定义。
- **两种写入方式**：
  - 默认写入：写入 `~/.codex/auth.json` 和 `~/.codex/config.toml`
  - 官方账号/API 额度模式：只写入 `~/.codex/config.toml`，不改动 `auth.json`
- **Codex 当前状态**：读取并展示 Codex 当前 API Key、Base URL、模型、Provider 等状态。
- **Codex 诊断**：检查 `~/.codex/config.toml`、`~/.codex/auth.json` 是否存在，是否包含模型、Provider、Base URL、API Key，并给出修复建议。
- **Codex 运行时备份**：可一键备份 `config.toml` 和 `auth.json`。
- **手动同步**：当前有活动 Codex 配置时，可使用 `Sync Now` 重新写入。

### 4. Codex Toolbox

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

### 5. Skills 管理

顶部工具栏的 Skills 入口用于管理 Claude Code 扩展能力：

- 查看已安装的 slash commands 与 skills。
- 新增、编辑、删除本地 Skill/Command。
- 读取 `~/.claude/commands` 与 `~/.claude/skills`。
- 从 `SKILL.md` frontmatter 中解析 description。
- Discover 页浏览技能目录。
- 支持管理技能仓库来源。
- 支持从 GitHub 搜索 skills。
- 支持从 URL 安装 skill。

### 6. Prompts 编辑

顶部工具栏的 Prompts 入口用于维护 Claude Code 提示词：

- 直接读取和保存 `~/.claude/CLAUDE.md`。
- 提供模板库，可快速插入/替换常用提示词模板。
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

### 7. MCP Server 管理

顶部工具栏的 MCP 入口用于管理 Claude MCP Server：

- 读取 `~/.claude.json` 中的 `mcpServers`。
- 新增、编辑、删除 MCP Server。
- 使用 JSON 方式编辑单个 MCP Server 配置。
- 提供 MCP Server 预设列表。
- 支持在 GitHub 搜索 MCP Server。
- 支持打开相关 GitHub 页面。

### 8. 设置、备份与恢复

在 Settings 中可以管理应用行为与路径：

- **Language**：中文 / English。
- **Theme**：Light / Dark。
- **Launch at startup**：开机自启。
- **Silent startup**：启动后静默最小化到系统托盘。
- **Minimize to tray**：关闭窗口时隐藏到托盘而不是退出。
- **Config directory**：打开 VarSwitch 应用配置目录。
- **Claude settings**：查看 `~/.claude/settings.json` 路径。
- **Runtime logs**：打开运行日志目录。
- **编辑器路径**：查看自动检测状态，或手动设置各编辑器 `settings.json` 路径。
- **Export Profiles**：导出 Claude 配置列表。
- **Import Profiles**：导入 Claude 配置列表。
- **Auto backup**：切换前自动备份配置。
- **Roll back**：查看并恢复配置备份。

自动备份包含：

- Claude 配置列表备份
- Codex 配置列表备份
- Codex 运行时文件备份：`~/.codex/config.toml`、`~/.codex/auth.json`

### 9. 更新、下载站与仓库入口

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

### 10. 系统托盘

- 启动后创建系统托盘图标。
- 左键点击托盘图标可恢复主窗口。
- 托盘菜单包含：
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
4. 在 `Discover` 中浏览仓库技能目录。
5. 可使用 GitHub 搜索或 Repos 管理技能来源。

### 编辑 CLAUDE.md

1. 点击顶部工具栏的 Prompts 图标。
2. 在 Editor 中编辑 `~/.claude/CLAUDE.md`。
3. 可切换到 Templates 选择内置模板。
4. 点击 Save 保存。

### 管理 MCP Server

1. 点击顶部工具栏的 MCP 图标。
2. 在 Installed 中查看当前 `mcpServers`。
3. 点击 `+ Add Server` 添加服务。
4. 填写 Server Name 和 JSON 配置。
5. 保存后写入 `~/.claude.json`。
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
| 系统环境变量 | `ANTHROPIC_AUTH_TOKEN` / `ANTHROPIC_AUTH_KEY`、`ANTHROPIC_BASE_URL` |
| Claude 设置 | `~/.claude/settings.json` 的 `env` |
| 编辑器设置 | `claudeCode.environmentVariables` |
| VarSwitch 配置 | 应用配置目录中的 profiles 数据 |

### Codex CLI

| 位置 | 写入内容 |
| --- | --- |
| `~/.codex/auth.json` | `OPENAI_API_KEY`，仅默认写入模式 |
| `~/.codex/config.toml` | `model_provider`、`model`、`chatgpt_base_url`、`model_providers`、`base_url` 等 |
| VarSwitch 配置 | 应用配置目录中的 Codex profiles 数据 |

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

Windows 通常生成 `.msi` 和 `.exe` 安装包，macOS 通常生成 `.dmg`。

## CI/CD

项目配置了 GitHub Actions 自动构建，推送 `v*` 标签时触发，支持：

- macOS aarch64 / x86_64
- Windows x86_64

### macOS 发布签名

macOS 从浏览器下载的 `.dmg` 如果没有做 Apple Developer 签名和 notarization，Gatekeeper 可能拦截。发布工作流要求在 GitHub Actions 中配置以下 secrets：

- `APPLE_CERTIFICATE`：`Developer ID Application` 证书导出的 `.p12` 文件内容，需先转成 base64
- `APPLE_CERTIFICATE_PASSWORD`：导出 `.p12` 时设置的密码
- `APPLE_ID`：用于 notarization 的 Apple ID 邮箱
- `APPLE_PASSWORD`：上述 Apple ID 的 app-specific password
- `APPLE_TEAM_ID`：Apple Developer Team ID
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
│   ├── src/lib.rs       # 核心逻辑：配置读写、环境变量同步、Codex Toolbox、Skills、Prompts、MCP
│   ├── src/main.rs      # Tauri 入口
│   ├── Cargo.toml       # Rust 依赖
│   ├── tauri.conf.json  # Tauri 配置
│   └── capabilities/    # Tauri 权限配置
├── .github/workflows/   # CI 构建配置
├── dev-server.js        # 前端开发静态服务器
├── dev.bat              # Windows 开发脚本
├── build.bat            # Windows 构建脚本
└── README.md            # 使用说明
```

## 许可证

MIT
