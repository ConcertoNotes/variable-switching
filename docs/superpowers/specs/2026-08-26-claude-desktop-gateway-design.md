# Claude Desktop 第三方供应商与 Gateway 设计

## 背景与目标

VarSwitch 当前把 Claude Code 作为独立配置域管理，已有 `127.0.0.1:25789` 本地代理，可对 Anthropic Messages 请求执行透传、OpenAI Chat Completions 双向转换、模型重写、故障转移和熔断。现有 Claude Desktop 支持只覆盖 MCP 配置，没有第三方推理供应商、3P Profile 或 Gateway 请求路由。

本次把 Claude Desktop 提升为独立的一等配置域，一次性提供：

- 独立供应商列表及增删改查、排序和启用；
- 从 Claude Code 一键导入供应商；
- Gateway、直连和官方登录三种使用状态；
- Sonnet、Opus、Haiku 角色到真实上游模型的映射；
- Claude Desktop 3P Profile 的安全写入、状态检查和失败回滚；
- 独立本地 Gateway 鉴权、模型目录和消息转发；
- 本地路由状态、重启提示及可诊断错误。

Claude Code 和 Claude Desktop 必须能够选择不同供应商、不同模型映射和不同故障转移池，任何一方切换都不得覆盖另一方的运行状态。

## 方案选择

采用独立 Claude Desktop 管理域，而不是把 Desktop 绑定到 Claude Code 当前配置，也不只暴露裸代理地址。

独立管理域的代价是新增一套供应商持久化、页面和切换命令，但能够正确表达 Desktop 的 3P Profile 生命周期、官方模式、直连/Gateway 差异和独立模型映射。Claude Code 导入只在用户触发时复制当前数据，导入后的 Desktop 配置不再与源配置联动。

## 产品界面

左侧应用导航新增 `Claude Desktop`，使用现有 Claude、Codex 页面相同的状态卡、配置卡和操作按钮样式，不改变其他信息架构。

页面包含：

- 当前状态：是否检测到 Claude Desktop、官方/直连/Gateway 模式、当前供应商、Profile 路径、本地 Gateway 地址和运行状态；
- 页面操作：立即同步、从 Claude Code 导入、添加 Claude Desktop 配置；
- 内置不可删除的 `Claude Desktop Official` 配置；
- 第三方供应商卡片：启用、编辑、删除及拖拽排序；
- Gateway 模式时显示本地路由依赖与“保持 VarSwitch 运行、完全退出并重启 Claude Desktop”的提示。

供应商表单字段：

| 字段 | 说明 |
| --- | --- |
| 配置名称 | Claude Desktop 域内唯一显示名称 |
| 连接模式 | `gateway` 或 `direct` |
| API 格式 | `anthropic` 或 `openai_chat`；直连只允许 `anthropic` |
| Base URL | 第三方上游根地址 |
| API Key | 第三方真实密钥；Gateway 模式只由 VarSwitch 使用，直连模式会写入 Desktop 3P Profile |
| 默认模型 | 角色映射未填写时的回退模型 |
| Sonnet 模型 | Desktop Sonnet 角色对应的真实上游模型 |
| Opus 模型 | Desktop Opus 角色对应的真实上游模型 |
| Haiku 模型 | Desktop Haiku 角色对应的真实上游模型 |
| 故障转移 | Gateway 模式下是否加入 Desktop 独立备用池 |

新增配置默认使用 Gateway 模式。导入 Claude Code 配置时保留名称、Base URL、API Key、API 格式、默认模型、三角色模型和故障转移意图；若名称冲突，沿用现有唯一名称生成规则，不覆盖已有 Desktop 配置。

## 数据模型与持久化

新增独立 `claude_desktop_profiles.json`，沿用当前应用数据目录、序列化方式、原子写入和密钥保护路径。数据结构与 Claude Profile 保持必要字段同构，但使用独立类型，避免未来两种客户端配置语义互相污染。

每个第三方配置至少保存：

```text
id, name, apiKey, baseUrl, connectionMode, apiFormat,
modelId, sonnetModel, opusModel, haikuModel,
proxyFailover, isActive, createdAt
```

官方配置由稳定内置 ID 表示，不保存真实 API Key。Gateway Token 单独保存在应用设置文件中，第一次启用 Gateway 时生成 `vsd-<uuid>`，后续稳定复用；它不是第三方 Key，也不是固定的 `PROXY_MANAGED`。

同一时刻只允许一个 Claude Desktop 配置处于激活状态。删除激活配置前必须先切换到其他配置或官方模式，避免留下无来源的 3P Profile。

## 3P Profile 写入

### 平台路径

Windows 写入：

```text
%LOCALAPPDATA%/Claude/claude_desktop_config.json
%LOCALAPPDATA%/Claude-3p/claude_desktop_config.json
%LOCALAPPDATA%/Claude-3p/configLibrary/_meta.json
%LOCALAPPDATA%/Claude-3p/configLibrary/<VarSwitch Profile ID>.json
```

macOS 写入对应的：

```text
~/Library/Application Support/Claude/
~/Library/Application Support/Claude-3p/
```

Linux 不写入 3P Profile，后端返回明确的“不支持”错误，前端保留供应商数据但禁止启用。

### Gateway 模式

Profile 指向：

```json
{
  "inferenceProvider": "gateway",
  "inferenceGatewayBaseUrl": "http://127.0.0.1:25789/claude-desktop",
  "inferenceGatewayAuthScheme": "bearer",
  "inferenceGatewayApiKey": "vsd-<uuid>",
  "disableDeploymentModeChooser": true,
  "coworkEgressAllowedHosts": ["*"],
  "inferenceModels": []
}
```

`inferenceModels` 只暴露 Claude Desktop 能识别的安全角色 ID。Sonnet、Opus、Haiku 各暴露一个稳定的 `claude-*` 路由名，真实上游模型仅保存在 VarSwitch 供应商配置中。

### 直连模式

直连仅接受 Anthropic Messages 兼容上游。Profile 直接写入供应商 Base URL 和真实 API Key，并写入安全的 Claude 角色模型列表。直连模式生效后不要求 VarSwitch 保持运行。

### 官方模式

启用 `Claude Desktop Official` 时，把正常目录和 3P 目录的 deployment mode 恢复为 `1p`，只移除 VarSwitch 自己管理的 Profile 和 `_meta.json` 引用，不删除其他应用或用户创建的 Profile，也不修改 MCP、快捷键等无关配置。

### 原子性与回滚

切换前读取所有目标文件的存在状态与原始字节。所有写入使用现有原子写文件工具。任一步失败时按快照恢复已修改文件；原本不存在的文件恢复为不存在。现有文件 JSON 损坏或顶层类型不符合预期时拒绝覆盖，并给出具体路径。

## Gateway 服务

复用现有 `127.0.0.1:25789` 监听进程，新增独立路由：

```text
GET  /claude-desktop/v1/models
POST /claude-desktop/v1/messages
```

Gateway 运行状态与 Claude Code 共用监听进程，但运行配置分离：

- Claude Code 保留现有主上游和故障转移池；
- Claude Desktop 新增独立主上游、角色映射和故障转移池；
- 路由根据 URL namespace 选择状态，不使用同一个全局 `UPSTREAM` 覆盖两端。

每个 Desktop 请求必须携带：

```http
Authorization: Bearer vsd-<uuid>
```

缺失或不匹配返回 401。监听地址始终为回环地址，不允许把第三方 API Key 当作本地 Gateway Token，也不接受固定占位符作为有效认证。

`GET /v1/models` 根据当前配置生成安全角色模型目录。`POST /v1/messages` 先把 Desktop 请求中的角色模型映射到真实上游模型，再进入现有转发链：

- `anthropic`：替换认证头和模型后透传 Anthropic Messages；
- `openai_chat`：复用现有 Anthropic → OpenAI Chat 请求转换以及普通/流式响应转换；
- 请求发送到上游时丢弃 Desktop Gateway Token，改为当前供应商真实 API Key。

Gateway 模式复用现有请求超时、429/5xx 故障转移、熔断及“流式响应一旦开始便不切换”的语义，但健康状态按 Claude Desktop 独立记录。

## 模型映射

Desktop 可见角色固定为 Sonnet、Opus、Haiku。每个角色的真实上游模型按以下优先级解析：

1. 对应角色字段；
2. 默认模型；
3. 第一个非空角色模型，顺序为 Sonnet、Opus、Haiku。

保存 Gateway 配置时必须至少存在一个可解析模型。请求携带未知的非 Claude 角色名时返回 400，不静默使用默认模型；带发布日期后缀的同角色官方名称可按 `sonnet`、`opus`、`haiku` 关键词回落到对应映射。

## 后端接口

新增或扩展 Tauri 命令：

- 获取、添加、更新、删除和排序 Claude Desktop 配置；
- 从 Claude Code 导入配置；
- 启用配置、恢复官方模式和立即同步；
- 获取 Claude Desktop 安装、Profile、当前模式及 Gateway 状态；
- 获取 Desktop Gateway 健康状态和重置其熔断器。

所有命令在后端重新校验名称、URL、模式、API 格式、API Key 和模型映射，不信任前端隐藏/禁用状态。返回对象中的 API Key 沿用现有脱敏/密钥保护合同，不在日志、错误或状态接口中暴露明文。

## 错误处理与用户提示

- 未安装或未初始化 Claude Desktop：允许保存配置，启用时提示需要先启动一次 Desktop；
- 不支持的平台：禁止启用并展示平台限制；
- Gateway 未运行：启用 Gateway 配置时启动监听；启动失败则不写 3P Profile；
- 端口占用：报告 `127.0.0.1:25789` 及原始监听错误；
- 无模型映射、非法 Base URL、缺少 Key、直连选择 OpenAI 格式：保存前阻止；
- Profile 写入失败：回滚文件和激活状态；
- 上游 401/403：保留上游错误语义但脱敏 Key；
- 上游 429/5xx/连接错误：按 Desktop 独立池故障转移；
- 切换成功：明确提示完全退出并重启 Claude Desktop；Gateway 模式额外提示保持 VarSwitch 运行。

## 测试与验收

Rust 单元测试覆盖：

- Desktop 配置 CRUD、排序、同名导入和激活唯一性；
- Gateway Token 首次生成、稳定复用和错误鉴权；
- Windows/macOS 路径构造及 Linux 拒绝；
- Gateway、直连、官方三种 Profile 内容；
- 写入中途失败时逐文件回滚，损坏 JSON 不覆盖；
- `/models` 安全角色目录；
- 精确模型、发布日期后缀、默认模型和未知模型映射；
- Anthropic 透传与 OpenAI Chat 转换；
- Desktop 与 Claude Code 上游状态互不覆盖；
- Desktop 独立故障转移与熔断。

前端测试覆盖：

- 侧栏路由、页面标题、状态区、空状态和操作按钮；
- 表单模式切换和字段显隐；
- 导入、添加、编辑、删除、排序和启用命令参数；
- 官方配置不可编辑/删除；
- Gateway 运行、重启及错误提示；
- 中英文文案和相邻 Claude/Codex 页面不回归。

最终验证：

1. `node --check public/app.js`；
2. `node --test public/app.test.js`；
3. 在 `src-tauri` 运行 `cargo fmt --check`、相关测试、完整 `cargo test` 和 `cargo check`；
4. Tauri debug 构建；
5. 在临时 `LOCALAPPDATA`/HOME 下执行 Gateway、直连、官方切换并检查落盘文件；
6. 启动实际页面，验证桌面与窄宽度布局、交互和控制台；
7. 对本地 `/models`、未授权 `/messages` 及带有效 Token 的转发执行 HTTP 验证；
8. 若本机没有可用 Claude Desktop 或第三方测试 Key，明确报告未执行真实外部对话，不用模拟结果冒充端到端成功。

## 非目标

- 不复制 CC-Switch 的数据库、OAuth 账号体系、用量数据库或全部协议适配器；
- 不新增 OpenAI Responses、Gemini Native 等 VarSwitch 当前 Claude 配置尚未支持的上游格式；
- 不把 Claude Desktop MCP 管理与本次推理供应商配置混为同一数据结构；
- 不改变 Claude Code、Codex、Gemini、Grok 或 OpenCode 的现有配置合同；
- 不修改与本功能无关的发布文件、下载站或版本号。
