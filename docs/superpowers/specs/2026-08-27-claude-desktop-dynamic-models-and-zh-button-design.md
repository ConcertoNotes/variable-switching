# Claude Desktop 动态模型与汉化按钮设计

## 背景与目标

修复 Claude Desktop 页面显示“未检测到”的 Windows MSIX 路径问题，并让 Claude Desktop Gateway 使用当前第三方 API Key 获取真实模型目录。用户在 Claude Desktop 中选择哪个模型，Gateway 就把该模型 ID 转发给上游，不再把所有角色静默映射到默认模型。

同时在 Claude Desktop 页面增加“汉化 Claude Desktop”入口，复用 `GMYXDS/claude-desktop-zh-simple` 的 Apache-2.0 脚本和翻译记忆库，提供状态、汉化、还原三个动作。脚本已归档且 README 声明新版资源结构可能失效，因此按钮必须展示真实执行结果，不能承诺所有 Claude 版本均可汉化。

## 方案与边界

### 动态模型目录

采用 Gateway 动态发现方案：

1. 激活 Gateway 配置后，VarSwitch 使用该配置解密后的 Base URL、API Key 和 API 格式请求上游模型列表。
2. Gateway 的 `/claude-desktop/v1/models` 以短时缓存提供模型目录；首次无缓存时即时拉取，失败时回退到已配置的安全模型，并在健康状态中记录错误。
3. 只公布 Claude Desktop 能识别的 Claude 风格模型 ID。非 Claude 风格模型不伪造成 Claude 名称，前端刷新模型时给出过滤提示。
4. `/claude-desktop/v1/messages` 对目录中的真实模型 ID 直接转发；旧版 `claude-sonnet-*`、`claude-opus-*`、`claude-haiku-*` 路由继续走角色映射，保持已有 Profile 兼容。
5. 主配置使用用户选定的精确模型；故障转移候选沿用现有角色映射和熔断语义，不改变 Claude Code 的全局上游状态。

配置弹窗复用现有 `fetch_available_models` 命令，增加“获取模型”操作和四个可选模型控件（默认、Sonnet、Opus、Haiku）。获取失败不覆盖原值；保存和后端切换仍重新校验 URL、Key、模式、格式和模型。

### MSIX 安装检测

Windows 路径按以下顺序选择：

- `%LOCALAPPDATA%/Claude/claude_desktop_config.json`（MSIX 当前配置）；
- `%APPDATA%/Claude/claude_desktop_config.json`（传统安装兼容）；
- 当配置文件尚未生成时，检查 Claude AppX 包数据目录或已安装包证据。

不把 VarSwitch 自己创建的 `Claude-3p` 目录单独当作安装证据。状态接口返回最终使用的配置路径及检测依据，避免再次出现路径与状态不一致。

### 汉化按钮

在 Claude Desktop 页面操作区增加：

- `查看汉化状态`：读取脚本状态并展示 Claude 版本、资源文件、备份状态和预计命中数；
- `一键汉化`：确认退出 Claude Desktop 后，以管理员权限执行 `patch -Yes -SkipUpdateCheck`；脚本先备份三个资源文件，再替换翻译内容；
- `还原英文`：执行 `restore -Yes -SkipUpdateCheck`，只使用脚本生成的备份；
- `打开项目`：打开原项目 GitHub 页面，保留来源与归属信息。

脚本、`translation_memory.json`、`version.json` 和 `LICENSE` 放入 Tauri 资源包。首次执行时复制到应用数据目录的可写子目录，再从该目录运行，避免安装目录不可写；只在目标文件不存在时初始化静态资源，不覆盖用户可能维护的翻译记忆库。PowerShell 子进程由 Rust 后端等待并返回退出码、标准输出和标准错误，前端只显示脱敏后的结果。非 Windows 平台返回明确“不支持”，资源结构不匹配时保留备份并报告失败。

## 数据流与接口

新增/扩展 Tauri 命令：

- `fetch_claude_desktop_models`：按 Profile 解密后的凭据获取模型，返回 `{ id }` 列表及过滤统计；
- `get_claude_desktop_localization_status`：返回脚本状态摘要；
- `run_claude_desktop_localization`：参数为 `status|patch|restore`，返回退出码、输出摘要和状态文件路径；
- `open_claude_desktop_localization_project`：打开固定 GitHub URL，不拼接用户输入。

Gateway 内部新增模型目录缓存和精确模型解析函数；缓存只保存模型 ID、时间戳和错误摘要，不保存 API Key。模型请求的认证仍使用本地 Gateway Token，上游请求始终替换为真实 API Key。

## 错误处理与安全

- API Key 只在后端解密后用于请求，不写入 Profile JSON、日志、前端错误或脚本参数输出。
- 上游模型接口的 401/403/超时分别保留可诊断错误；缓存过期且拉取失败时回退安全配置模型。
- 汉化动作执行前检测 Claude 进程并提示退出；脚本自身的备份/权限/资源结构错误原样归类到用户可读提示。
- 所有汉化写入沿用脚本的逐文件备份和报告机制；VarSwitch 不删除其他 Claude 配置、MCP 或用户 Profile。
- 第三方脚本与翻译库保留 Apache-2.0 `LICENSE` 和来源链接；界面标注“基于 claude-desktop-zh-simple，项目已归档”。

## 测试与验收

Rust：

- Windows MSIX、传统路径优先级和无配置文件时的安装证据；
- 动态模型缓存命中、过期刷新、过滤非 Claude ID、上游失败回退；
- 真实模型 ID 精确转发、旧角色别名兼容、未知模型返回 400；
- 汉化资源目录初始化、命令参数白名单、非 Windows 拒绝和输出脱敏。

前端：

- 模型获取按钮填充四个控件且失败不覆盖；
- 汉化状态/汉化/还原/打开项目按钮绑定正确命令；
- 执行中禁用重复点击、成功/失败提示和中英文文案；
- Claude Desktop 页面现有 Gateway/直连流程不回归。

最终验证：

1. `node --check public/app.js`；
2. `node --test public/app.test.js`；
3. `cargo fmt --check`、相关 Rust 测试、完整 `cargo test` 和 `cargo check`；
4. Tauri debug 构建，确认资源包包含脚本、翻译库和许可证；
5. 在临时配置目录执行动态 `/models`、精确 `/messages`、未授权请求和汉化命令参数验证；
6. 在实际 Windows Claude Desktop 页面验证 MSIX 检测、模型菜单及汉化按钮反馈。若当前 Claude 版本资源结构已不兼容，报告可复现失败证据，不将其表述为成功。

## 非目标

- 不复制 CC-Switch 的账号、数据库或 OAuth 体系；
- 不把非 Claude 模型改名伪装成 Claude 模型；
- 不在本次改动中修改 Claude Code、Codex、Gemini、Grok 或 OpenCode 的模型合同；
- 不删除用户已有 Claude Desktop 配置、MCP 服务器或第三方 Profile；
- 不绕过 Claude Desktop 或 Windows 的权限确认，不静默下载并执行未经打包的远程脚本。

## 第三方来源

- 项目：<https://github.com/GMYXDS/claude-desktop-zh-simple>
- 许可：Apache License 2.0，随资源包一并分发。
