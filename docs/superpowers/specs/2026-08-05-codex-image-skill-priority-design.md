# Codex 图片 Skill 优先路由设计

## 目标

当启用的 Codex 配置包含完整的图片 API Key 与 Base URL 时，Codex 对图片生成请求优先读取并执行 `varswitch-imagegen`。Codex 内置 `imagegen` 保持可用，仅在自定义 Skill 未配置、缺失或执行失败时兜底。

## 设计

1. 强化 `varswitch-imagegen/SKILL.md` 的触发描述，明确它是 VarSwitch 图片配置启用时的首选图片生成路径。
2. 在 `~/.codex/AGENTS.md` 中维护一段带固定边界标记的路由规则。启用图片配置时写入或更新该段；切换到无图片配置时仅移除该段，保留用户原有内容。
3. 保留 Codex 内置 `imagegen` 的安装与启用状态，不写 `[[skills.config]]` 禁用项。
4. 导入当前 Codex 配置时保留已经检测到的图片 Base URL，避免导入后的活动配置缺少安装自定义 Skill 所需字段。

## 验收

- 路由规则可重复写入且不会产生重复段落。
- 移除路由规则不会改动用户自己的全局指令。
- 自定义 Skill 的描述和正文都明确首选关系与兜底条件。
- 导入数据保留 `image_api_key` 与 `image_base_url`。
- `codex debug prompt-input` 能同时看到自定义 Skill、优先规则和仍保留的内置 `imagegen`。
