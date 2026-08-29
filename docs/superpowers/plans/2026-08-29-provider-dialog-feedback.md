# 独立供应商配置弹窗反馈优化 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 Claude、Codex、Grok、Gemini 独立配置弹窗补齐 API Key 操作、连接状态、预设级联和名称实时校验。

**Architecture:** 在现有 `public/app.js` 中增加 provider 字段映射驱动的共享 helper，四个弹窗通过映射接入，保留既有提交与后端命令。`public/index.html` 提供统一的操作按钮、状态挂载点和名称错误节点，`public/style.css` 提供图标按钮、状态胶囊和预设覆写高亮。

**Tech Stack:** 原生 HTML/CSS/JavaScript、Node.js 内置 `node:test`、Tauri `invoke` 前端桥接。

## Global Constraints

- 本次仅改 Claude、Codex、Grok、Gemini 四个独立配置弹窗，不改“添加统一供应商”页面及后端协议。
- 保留现有保存、导入、模型获取、切换和 DeepSeek 特殊 Responses 规则。
- 名称重复只在同一 provider 列表内判定；编辑时排除当前配置自身 ID。
- API Key 不写入日志；剪贴板异常只通过 Toast 告知用户。
- 修改前保留工作区中无关的未提交变更，不重置或覆盖它们。

---

### Task 1: 为四个弹窗补充失败测试与可查询的 DOM 契约

**Files:**
- Modify: `public/app.test.js`
- Test: `public/app.test.js`

**Interfaces:**
- Produces assertions for `data-api-key-action`, `data-inline-status`, `.field-error` and helper/event names used by later tasks.

- [ ] **Step 1: Write the failing tests**

在现有 provider 表单测试附近增加以下测试，直接读取 `public/index.html` 与 `public/app.js`：

```js
test("provider dialogs expose API key actions, inline status, and name validation hooks", () => {
  const html = fs.readFileSync(require.resolve("./index.html"), "utf8");
  const app = fs.readFileSync(require.resolve("./app.js"), "utf8");
  for (const id of ["profileApiKey", "codexApiKey", "grokApiKey", "geminiApiKey"]) {
    assert.match(html, new RegExp(`id="${id}"`));
  }
  assert.equal((html.match(/data-api-key-action="toggle"/g) || []).length, 4);
  assert.equal((html.match(/data-api-key-action="paste"/g) || []).length, 4);
  assert.equal((html.match(/data-inline-status=/g) || []).length, 4);
  assert.equal((html.match(/class="field-error"/g) || []).length, 4);
  assert.match(app, /function bindProviderDialogEnhancements\(/);
  assert.match(app, /function validateProviderName\(/);
});

test("provider preset application updates protocol fields and emits overwrite highlight", () => {
  const app = fs.readFileSync(require.resolve("./app.js"), "utf8");
  assert.match(app, /applyClaudePreset[\s\S]{0,1200}profileApiFormat/);
  assert.match(app, /applyCodexPreset[\s\S]{0,1200}codexWireApi/);
  assert.match(app, /applyGrokPreset[\s\S]{0,1200}grokApiBackend/);
  assert.match(app, /preset-overwritten/);
});

test("endpoint checks render a latency or HTTP status pill", () => {
  const app = fs.readFileSync(require.resolve("./app.js"), "utf8");
  assert.match(app, /renderProviderInlineStatus\(/);
  assert.match(app, /延迟|latency/);
  assert.match(app, /HTTP|Unauthorized|未授权/);
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `node --test public/app.test.js`

Expected: FAIL because the new data attributes, helper names and status rendering are not present yet.

- [ ] **Step 3: Commit the test-only change**

```bash
git add public/app.test.js
git commit -m "test: define provider dialog feedback contracts"
```

### Task 2: 实现 API Key 显示/隐藏与粘贴操作

**Files:**
- Modify: `public/index.html:650-655,810-817,910-915,1050-1055`
- Modify: `public/app.js:2462-2480,10180-10210`
- Modify: `public/style.css` (provider dialog input action styles)

**Interfaces:**
- Consumes existing `tryClipboardAutoFill`, `showToast`, `setText`.
- Produces `bindProviderApiKeyActions()` and `data-api-key-action` hooks for all four dialogs.

- [ ] **Step 1: Add the HTML controls**

将四个 API Key 输入改成同一结构：输入框包在 `.input-with-action` 中，右侧各放一个 `button type="button" data-api-key-action="toggle" data-api-key-target="..."` 和 `button type="button" data-api-key-action="paste" data-api-key-target="..."`，并为每个表单添加 `<small class="field-error" ...></small>` 名称错误节点与 `[data-inline-status]` 状态挂载点。按钮使用 aria-label/title，不在按钮中显示密钥。

- [ ] **Step 2: Run the focused structural tests**

Run: `node --test public/app.test.js --test-name-pattern="provider dialogs expose"`

Expected: 仍因 JS helper 缺失而失败，但 HTML 计数断言应先通过；若出现 HTML 计数错误，先修正 DOM 结构。

- [ ] **Step 3: Implement minimal shared action binding**

新增：

```js
function bindProviderApiKeyActions() {
  document.querySelectorAll("[data-api-key-action]").forEach((button) => {
    button.addEventListener("click", async () => {
      const input = $(button.dataset.apiKeyTarget);
      if (!input) return;
      if (button.dataset.apiKeyAction === "toggle") {
        input.type = input.type === "password" ? "text" : "password";
        button.setAttribute("aria-label", input.type === "password" ? "显示 API Key" : "隐藏 API Key");
        return;
      }
      try {
        input.value = (await navigator.clipboard.readText() || "").trim();
        input.dispatchEvent(new Event("input", { bubbles: true }));
      } catch (error) {
        showToast(currentLang === "zh" ? "无法读取剪贴板" : "Unable to read clipboard", "error");
      }
    });
  });
}
```

在初始化绑定阶段调用一次，并保留原有 focus 自动填充作为兼容行为。

- [ ] **Step 4: Add CSS for the action cluster**

为 `.input-with-action`、`.api-key-action` 增加相对定位、右侧间距、hover/focus-visible 样式，确保输入框右 padding 足够且不改变其他表单布局。

- [ ] **Step 5: Run tests and syntax check**

Run: `node --test public/app.test.js`; `node --check public/app.js`

Expected: API Key 结构测试通过，现有测试无新增失败。

- [ ] **Step 6: Commit**

```bash
git add public/index.html public/app.js public/style.css public/app.test.js
git commit -m "feat: add provider API key actions"
```

### Task 3: 实现连接测试 inline 状态胶囊

**Files:**
- Modify: `public/index.html` (four endpoint test button rows)
- Modify: `public/app.js` (`handleEndpointTest`, shared status helpers)
- Modify: `public/style.css` (success/error/loading pill styles)
- Modify: `public/app.test.js`

**Interfaces:**
- Consumes `productFieldMap`, `describeVerifyError`, `setButtonBusy`.
- Produces `renderProviderInlineStatus(kind, state)` and `clearProviderInlineStatus(kind)`.

- [ ] **Step 1: Write the failing status assertion**

追加测试断言 `handleEndpointTest` 调用 `renderProviderInlineStatus(kind, { tone: "success", latencyMs: elapsed })`，错误分支传递解析出的 HTTP 状态码，并在 Base URL/API Key input 事件中调用清理函数。

- [ ] **Step 2: Run the test to verify RED**

Run: `node --test public/app.test.js --test-name-pattern="endpoint checks render"`

Expected: FAIL because the helper and calls do not exist.

- [ ] **Step 3: Implement status helpers**

按 `productFieldMap(kind)` 找到 `[data-inline-status="kind"]`，成功状态文本为 `✓ 延迟 ${latencyMs}ms`，失败状态从 `HTTP (\d{3})` 提取并映射 `401` 为“未授权/Unauthorized”，没有状态码时显示摘要。状态 class 只使用 `is-success` 或 `is-error`。

- [ ] **Step 4: Integrate into `handleEndpointTest`**

开始时清除旧状态并调用 `setButtonBusy(button, true, t("endpointTesting"))`；成功和失败分别渲染状态胶囊；`finally` 调用 `setButtonBusy(button, false)`。保持现有 endpoint 结果、模型列表和 Toast。

- [ ] **Step 5: Clear stale status on input/open**

给四个 Base URL/API Key 输入绑定 `input` 事件清除 inline 状态；在 `openModal`、`openCodexModal`、`openGrokModal`、`openGeminiModal` 初始化时调用清除。

- [ ] **Step 6: Add CSS and run tests**

新增浅绿/浅红胶囊及小字号样式，运行：`node --test public/app.test.js`; `node --check public/app.js`。

- [ ] **Step 7: Commit**

```bash
git add public/index.html public/app.js public/style.css public/app.test.js
git commit -m "feat: show inline provider connection status"
```

### Task 4: 实现预设级联覆写与黄色高亮

**Files:**
- Modify: `public/app.js` (`applyClaudePreset`, `applyCodexPreset`, `applyGrokPreset` and shared highlight helper)
- Modify: `public/style.css` (overwrite animation)
- Modify: `public/app.test.js`

**Interfaces:**
- Consumes existing preset arrays and `syncCodexWireApiControl`.
- Produces `highlightPresetOverwrite(fieldIds)` used by all `apply*Preset` functions.

- [ ] **Step 1: Write and run failing preset tests**

断言三个 `apply*Preset` 函数在非空 preset 时更新对应协议字段并调用 `highlightPresetOverwrite`；自定义 preset（null）不调用覆盖逻辑。先运行 `node --test public/app.test.js --test-name-pattern="preset application"`，确认 RED。

- [ ] **Step 2: Implement field highlight helper**

新增 `highlightPresetOverwrite(fieldIds)`：为存在的元素添加 `preset-overwritten`，监听 animationend 或 1.2 秒后移除；不改变字段值之外的状态。

- [ ] **Step 3: Extend preset apply functions**

Claude preset 写入 `profileApiFormat`（预设可选 `apiFormat`，默认 `anthropic`）；Codex 写入 `codexWireApi`（读取 `preset.wire`，DeepSeek 仍调用 `syncCodexWireApiControl`）；Grok 写入 `grokApiBackend`（预设可选 `apiBackend`，默认 `chat_completions`）。每个实际写入的 ID 传给高亮 helper。自定义选择只清除 hint，不覆盖现有输入。

- [ ] **Step 4: Add CSS animation and run tests**

实现黄色背景/边框的短暂 `@keyframes preset-overwrite-pulse`，运行完整 Node 测试与语法检查。

- [ ] **Step 5: Commit**

```bash
git add public/app.js public/style.css public/app.test.js
git commit -m "feat: cascade provider presets with overwrite feedback"
```

### Task 5: 实现名称实时校验与保存防呆

**Files:**
- Modify: `public/app.js`（新增 `validateProviderName`，接入四个提交函数与输入事件）
- Modify: `public/index.html`（名称错误节点如未在 Task 2 添加则补齐）
- Modify: `public/style.css`（`.field-error` 与 invalid input）
- Modify: `public/app.test.js`

**Interfaces:**
- Consumes arrays `profiles`, `codexProfiles`, `grokProfiles`, `geminiProfiles` and editing IDs.
- Produces `validateProviderName({ kind, name, editingId }) -> { valid, message }` and `bindProviderNameValidation()`.

- [ ] **Step 1: Write failing pure validation tests**

在测试中断言源码包含空值、同 provider 重名和排除当前 ID 的分支，并检查四个保存按钮均绑定校验。运行 `node --test public/app.test.js --test-name-pattern="name validation"`，确认 RED。

- [ ] **Step 2: Implement pure validation helper**

使用 provider 配置映射取得列表和编辑 ID；名称 `trim()` 后为空返回“请填写配置名称”，与其他项 `name.trim().toLowerCase()` 相等返回“配置名称已存在”，匹配自身 ID 时跳过；返回 `{ valid: true, message: "" }`。

- [ ] **Step 3: Implement live UI binding**

新增 `bindProviderNameValidation()`，在 `input`/`blur` 时更新对应 `.field-error`，切换 `aria-invalid`，并设置 `submitBtn`、`codexSubmitBtn`、`codexSaveEnableBtn`、`grokSubmitBtn`、`geminiSubmitBtn` 的 disabled 状态。打开/编辑弹窗时立即运行一次。

- [ ] **Step 4: Add submit guards**

在 `handleSubmit`、`handleCodexSubmit`、`handleGrokSubmit`、`handleGeminiSubmit` 读取名称后调用 helper；无效时显示字段错误并直接 return，不发起 `invoke`。

- [ ] **Step 5: Add CSS and run tests**

添加 `.field-error` 的 rose 色小字号和 invalid 边框样式；运行 `node --test public/app.test.js`、`node --check public/app.js`。

- [ ] **Step 6: Commit**

```bash
git add public/index.html public/app.js public/style.css public/app.test.js
git commit -m "feat: validate provider names before save"
```

### Task 6: 浏览器验收与最终回归

**Files:**
- Modify: 无（仅在发现缺陷时回到对应任务）

- [ ] **Step 1: Run the complete automated checks**

Run: `node --test public/app.test.js`; `node --check public/app.js`; `git diff --check HEAD~5..HEAD`。

- [ ] **Step 2: Start the local frontend**

Run: `node dev-server.js`，记录实际端口并在浏览器打开。

- [ ] **Step 3: Verify each provider dialog**

逐个打开 Claude、Codex、Grok、Gemini 弹窗，检查：API Key 显示/隐藏与粘贴；测试连接按钮旋转和成功/失败胶囊；切换预设后字段黄色高亮与协议联动；名称为空/重名时错误文本和保存按钮禁用；编辑当前配置保留自身名称。

- [ ] **Step 4: Inspect console and worktree**

确认浏览器控制台无新增错误、没有 API Key 输出；运行 `git status --short`，确认只包含本任务变更及原有用户未提交文件。

