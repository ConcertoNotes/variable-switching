# CC Switch v1 Provider 导入兼容设计

## 背景与目标

VarSwitch 已注册 `varswitch://` 自定义协议，并支持以下旧格式：

- `varswitch://import/profile?app=<claude|codex|gemini|grok>&payload=<base64url(JSON)>`
- `varswitch://import/mcp?payload=<base64url(JSON)>`

本次在不破坏旧格式的前提下，新增与 CC Switch v1 Provider Import 查询参数兼容的格式：

```text
varswitch://v1/import?resource=provider&app=<claude|codex|gemini>&name=...&endpoint=...&apiKey=...&model=...&homepage=...&enabled=true
```

新格式只负责导入 Provider，不扩展 MCP 或 Grok 协议合同。旧格式的 Profile、MCP 和 Grok 行为保持不变。

## 协议识别与解析

后端继续作为深链解析与安全校验边界。`parse_deep_link_url` 根据 URL 的 hostname/path 分派到旧格式解析器或新 v1 Provider 解析器，最后都转换成现有 `DeepLinkImport { kind, app, data }` 结构，复用前端确认弹窗和后端写入命令。

新协议必须同时满足：

```text
scheme   = varswitch
hostname = v1
pathname = /import
resource = provider
app      = claude | codex | gemini
enabled  = true
```

`name`、`endpoint`、`apiKey`、`model`、`homepage` 和 `enabled` 必须存在；字符串字段去除首尾空白后不得为空。Endpoint 必须是绝对 HTTP 或 HTTPS URL。未知参数忽略，以保留向前兼容空间；未知版本、action、resource 和 app 必须拒绝。

查询参数采用标准 `application/x-www-form-urlencoded` 语义解析：百分号编码解码一次，`+` 转为空格，支持 UTF-8 中文。不得对解析结果再次执行 percent decode。

## 数据映射

新格式转换为现有 Profile 导入数据：

| 新协议字段 | 内部字段 | 说明 |
| --- | --- | --- |
| `app` | `DeepLinkImport.app` | 仅 `claude`、`codex`、`gemini` |
| `name` | `data.name` | Provider 名称 |
| `endpoint` | `data.baseUrl` | 复用现有 Provider Base URL 字段 |
| `apiKey` | `data.apiKey` | 仅在内存中传递，落盘使用现有加密路径 |
| `model` | `data.model` | 主模型 |
| `haikuModel` | `data.haikuModel` | Claude 可选角色模型 |
| `sonnetModel` | `data.sonnetModel` | Claude 可选角色模型 |
| `opusModel` | `data.opusModel` | Claude 可选角色模型 |
| `homepage` | `data.homepage` | 用于确认界面展示，当前不写入 Profile |
| `enabled` | `data.enabled` | 必须为 `true`，但不自动激活或切换 |

Codex 的 `endpoint` 直接采用网页传入值。协议发送方负责按合同补充 `/v1`，VarSwitch 不再重复追加，避免产生 `/v1/v1`。Claude 和 Gemini 同样保存规范化后的传入 Endpoint。

## 确认弹窗与安全行为

收到合法深链后必须显示确认弹窗，不得静默导入。弹窗展示：

- 目标应用；
- Provider 名称；
- Endpoint；
- 主模型；
- Claude Haiku、Sonnet、Opus 可选模型；
- Homepage；
- 脱敏 API Key。

API Key 沿用现有前端脱敏逻辑，不展示完整值。日志只记录问号前的协议位置，绝不记录查询参数。取消、关闭或导入完成后清空前端暂存对象；后端不缓存原始 URL。

`enabled=true` 只表示该导入数据有效，不触发 Provider 激活或 CLI 环境切换。导入后的配置沿用现有 `is_active=false` 新增语义；覆盖已有配置时保留原有激活状态。

## 同名冲突

后端在发出确认事件前检查目标应用现有 Provider 名称，并把冲突状态与建议的唯一名称交给前端。

无冲突时，用户确认后直接新增。存在同名配置时，弹窗提供：

1. `重命名后导入`：默认选项，使用后端生成的 `名称 (2)`、`名称 (3)` 等唯一名称新增；
2. `覆盖现有配置`：保留既有 Profile ID、创建时间和激活状态，只更新新协议提供的名称、Endpoint、API Key 与模型字段；
3. `取消`：不执行任何写入。

旧格式继续沿用原有自动重命名新增行为，不新增覆盖入口，从而避免改变旧链接的既有语义。

覆盖操作必须在后端重新校验数据和冲突目标，不能信任前端传入的 ID 或冲突状态。如果目标在用户确认前已被删除或改名，后端返回可读错误，要求重新发起导入，避免覆盖错误配置。

## 组件与接口调整

### Rust 后端

`src-tauri/src/deeplink.rs` 负责：

- 标准查询参数解析；
- 新旧协议路由；
- v1 Provider 参数校验和数据归一化；
- 检查同名冲突并生成建议名称；
- 根据 `rename` 或 `overwrite` 决策执行写入；
- 保持旧 Profile/MCP 导入入口兼容。

`DeepLinkImport` 增加仅供确认流程使用的协议来源和冲突元数据，但不包含完整原始 URL。`apply_deep_link_import` 增加冲突决策参数；旧格式未提供决策时仍采用现有自动重命名行为。

### 前端

`public/index.html` 为确认弹窗增加模型、Homepage 和同名冲突选项。

`public/app.js` 负责：

- 渲染新字段并隐藏不适用的 Claude 可选模型；
- 仅在 v1 同名冲突时显示重命名/覆盖选择；
- 将用户选择传给后端；
- 在取消、关闭、成功和失败后的适当时机清理敏感暂存数据；
- 导入后刷新对应 Provider 列表。

`public/style.css` 仅补充新字段与冲突选项所需的局部样式，不调整其他页面布局。

### 文档

`README.md` 在现有 Deep Link 章节中增加 v1 Provider 查询参数格式，并明确旧格式仍受支持。

## 错误处理

解析错误通过现有 `deeplink-import-error` 事件显示可读提示。错误文本可以指出缺失或非法字段，但不得包含 API Key、完整查询参数或原始 URL。

写入失败时保留确认弹窗和当前选择，方便用户重试；取消和关闭才立即清空暂存数据。成功写入后关闭弹窗、清空暂存并刷新列表。

## 测试与验收

Rust 单元测试覆盖：

- Claude、Codex、Gemini 新格式正确映射；
- 原有 Profile、MCP、Grok 深链继续解析；
- `hostname=v1` 与 `pathname=/import` 的正确识别；
- `+`、UTF-8 中文、百分号编码和单次解码；
- 未知版本、action、resource、app；
- 缺失或空白的必填字段；
- 非 HTTP(S) Endpoint；
- `enabled` 缺失或不为 `true`；
- 未知参数被忽略；
- 同名建议名称、重命名导入和覆盖前的目标复核。

前端自动化测试覆盖：

- v1 Provider 的展示字段；
- API Key 只显示脱敏值；
- Claude 可选模型的显示与其他应用隐藏；
- 仅冲突时显示重命名/覆盖选项；
- 默认选择重命名；
- 确认时传递正确决策；
- 取消、关闭和成功后清除暂存数据；
- 旧 Profile 与 MCP 弹窗行为不回归。

最终验证包括：

- 运行相关 Rust 单元测试和完整 `cargo test`；
- 运行前端测试；
- 完成 Tauri debug 构建；
- 在 Windows 实际启动应用，用假密钥分别验证冷启动和运行中唤醒；
- 验证只存在一个 VarSwitch 实例、确认弹窗字段正确、取消不写入、重命名和覆盖符合选择；
- 检查应用控制台没有新增错误，并检查日志中不存在测试 API Key 或完整深链。

## 非目标

- 不移除或改变旧 Deep Link 合同；
- 不给 v1 协议增加 Grok 或 MCP 导入；
- 不新增依赖、数据库或远端一次性导入码服务；
- 不因 `enabled=true` 自动激活或切换 Provider；
- 不重构与深链导入无关的 Provider 管理逻辑或页面。
