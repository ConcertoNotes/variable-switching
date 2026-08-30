const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

// 后端按领域拆成了多个模块文件，断言「某段 Rust 代码存在」时要看整个 src 目录，
// 否则每次搬移函数都会误报失败。
function readRustSources() {
  const dir = path.join(__dirname, "..", "src-tauri", "src");
  return fs
    .readdirSync(dir)
    .filter((name) => name.endsWith(".rs"))
    .map((name) => fs.readFileSync(path.join(dir, name), "utf8"))
    .join("\n");
}

// 命令注册行在拆分后带模块前缀（如 `gemini::get_gemini_profiles,`）
function commandRegistrationPattern(command) {
  return new RegExp(`\\n\\s*(?:\\w+::)?${command},`);
}

// 样式同样按层叠顺序拆成了 styles/*.css，断言「某条规则存在」时要看全部文件，
// 否则规则在文件之间搬一次就会误报失败。拼接顺序与 index.html 的 link 顺序一致。
function readStylesheets() {
  const dir = path.join(__dirname, "styles");
  return fs
    .readdirSync(dir)
    .filter((name) => name.endsWith(".css"))
    .sort()
    .map((name) => fs.readFileSync(path.join(dir, name), "utf8"))
    .join("\n");
}

// 各应用的运行时状态网格共用同一套横向布局规则。新增应用时只改这个数组，
// 断言集中在 assertStatusGridsShareRule，不必逐个测试改选择器串。
const STATUS_GRID_IDS = [
  "#statusGrid",
  "#codexStatusGrid",
  "#grokStatusGrid",
  "#geminiStatusGrid",
  "#opencodeStatusGrid",
];

// 代理健康 / Desktop 状态与网关健康各自只渲染一张卡，也必须是单列，
// 否则会继承 .status-grid 的 3 列、把唯一那张卡压到 1/3 宽、右边空掉 2/3。
// 它们的卡内布局与上面几个不同，所以只共用「单列」这一条规则。
const SINGLE_COLUMN_GRID_IDS = [
  ...STATUS_GRID_IDS,
  "#proxyHealthGrid",
  "#claudeDesktopStatusGrid",
  "#claudeDesktopGatewayHealthGrid",
];

function assertStatusGridsShareRule(css, suffix, declaration, { ids = STATUS_GRID_IDS } = {}) {
  const selectors = ids
    .map((id) => `${id}${suffix}`.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"))
    .join(",\\s*");
  assert.match(css, new RegExp(`${selectors}\\s*\\{[^}]*${declaration}`, "s"));
}

const {
  shouldAutoOpenUsageGuide,
  getUpdateActionMode,
  formatVersionTag,
  getEditorPathMode,
  validateEditorPathInput,
  resolveUniversalAppBaseUrl,
  normalizeFetchedModels,
  normalizeClaudeDesktopModels,
  getCodexToolboxLayout,
  shouldRenderChannelQr,
  getMobileBindingUiState,
  getSwitchOverlayState,
  getCodexSessionMetrics,
  getGlobalConfigDisplay,
  sanitizeMobileLogValue,
  resolveUsageRange,
  isDeepseekCodexConfig,
  maskDeepLinkSecret,
  getDeepLinkImportView,
  buildDeepLinkApplyRequest,
} = require("./app-helpers.js");

function getDirectConsolePages(html) {
  const mainStart = html.indexOf('<main class="workspace-main"');
  const mainEnd = html.indexOf("</main>", mainStart);
  assert.notEqual(mainStart, -1, "workspaceMain should exist");
  assert.notEqual(mainEnd, -1, "workspaceMain should be closed");

  const fragment = html.slice(mainStart, mainEnd);
  const stack = [];
  const pages = [];
  const voidTags = new Set(["img", "input", "br", "hr", "meta", "link"]);
  for (const match of fragment.matchAll(/<\/?([a-z][a-z0-9-]*)([^>]*)>/gi)) {
    const [token, rawTag, attributes] = match;
    const tag = rawTag.toLowerCase();
    if (token.startsWith("</")) {
      stack.pop();
      continue;
    }
    if (/data-console-page-panel=/.test(attributes) && stack.length === 1 && stack[0] === "main") {
      const page = attributes.match(/data-console-page-panel="([^"]+)"/)?.[1];
      if (page) pages.push(page);
    }
    if (!voidTags.has(tag) && !token.endsWith("/>")) stack.push(tag);
  }
  return pages;
}

test("all console pages are direct children of workspaceMain", () => {
  const html = fs.readFileSync(require.resolve("./index.html"), "utf8");
  assert.deepEqual(getDirectConsolePages(html), [
    "claude",
    "claude-desktop",
    "codex",
    "grok",
    "gemini",
    "opencode",
    "add-provider",
    "toolbox",
    "developer-tools",
    "usage",
    "cli-sessions",
    "settings",
  ]);
});

test("overview page and navigation entry stay removed", () => {
  const html = fs.readFileSync(require.resolve("./index.html"), "utf8");
  assert.doesNotMatch(html, /data-console-page(?:-panel)?="overview"/);
  assert.doesNotMatch(html, /id="pageOverview"/);
  assert.match(html, /class="sidebar-item active" data-console-page="add-provider"/);
});

test("document ids and local asset references stay valid", () => {
  const htmlPath = require.resolve("./index.html");
  const html = fs.readFileSync(htmlPath, "utf8");
  const ids = [...html.matchAll(/\sid="([^"]+)"/g)].map((match) => match[1]);
  const duplicates = [...new Set(ids.filter((id, index) => ids.indexOf(id) !== index))];
  const localReferences = [...html.matchAll(/(?:src|href)="([^"#][^"]*)"/g)]
    .map((match) => match[1])
    .filter((reference) => !reference.includes("://") && !reference.startsWith("data:"));
  const missing = [...new Set(localReferences.filter((reference) => {
    const target = require("node:path").join(require("node:path").dirname(htmlPath), reference);
    return !fs.existsSync(target);
  }))];

  assert.deepEqual(duplicates, []);
  assert.deepEqual(missing, []);
});

test("stylesheet braces stay balanced", () => {
  // 逐个文件校验：拆分后每个文件都必须自洽，一个文件缺 `}` 不能靠下一个文件补上，
  // 否则规则会静默串到相邻文件里。
  const dir = path.join(__dirname, "styles");
  const names = fs.readdirSync(dir).filter((name) => name.endsWith(".css"));
  assert.ok(names.length > 0, "styles/ 下应有拆分后的样式文件");

  for (const name of names) {
    const css = fs.readFileSync(path.join(dir, name), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
    let depth = 0;
    let minimumDepth = 0;
    for (const character of css) {
      if (character === "{") depth += 1;
      if (character === "}") depth -= 1;
      minimumDepth = Math.min(minimumDepth, depth);
    }
    assert.equal(minimumDepth, 0, `${name} 出现了多余的 }`);
    assert.equal(depth, 0, `${name} 有未闭合的 {`);
  }
});

test("index.html loads every stylesheet in cascade order", () => {
  // 层叠顺序即优先级：link 的顺序必须与 styles/ 下的文件名排序一致，
  // 漏掉一个文件或调换顺序都会让后面几代重构层失效。
  const html = fs.readFileSync(require.resolve("./index.html"), "utf8");
  const linked = [...html.matchAll(/<link rel="stylesheet" href="styles\/([^"]+)"/g)].map((m) => m[1]);
  const onDisk = fs.readdirSync(path.join(__dirname, "styles")).filter((n) => n.endsWith(".css")).sort();

  assert.deepEqual(linked, onDisk);
  assert.doesNotMatch(html, /href="style\.css"/, "旧的单文件 style.css 不应再被引用");
});

test("settings groups stay independent and keep path actions compact", () => {
  const html = fs.readFileSync(require.resolve("./index.html"), "utf8");
  const css = readStylesheets();
  const app = fs.readFileSync(require.resolve("./app.js"), "utf8");

  // 4 组：通用 / 目录 / 备份 / 数据目录（多设备同步）
  assert.equal((html.match(/class="settings-group-items"/g) || []).length, 4);
  assert.equal((html.match(/data-settings-copy-path=/g) || []).length, 3);
  assert.match(html, /id="settingsManualBackupLabel"[\s\S]*id="settingsExportBtn"[\s\S]*id="settingsImportBtn"/);
  assert.doesNotMatch(html.slice(html.indexOf("<!-- Settings Panel -->"), html.indexOf('id="usageGuideOverlay"')), />Open</);
  assert.doesNotMatch(html, /class="settings-backup-actions"/);
  assert.match(app, /setText\("settingsOpenLogsDir", t\("settingsOpen"\)\)/);
  assert.match(app, /setText\("settingsOpenBackupsBtn", t\("settingsOpen"\)\)/);
  assert.match(app, /function copySettingsPath\(pathKey\)/);
  assert.match(css, /\.settings-page-host\s*>\s*\.settings-body\s*\{[^}]*background:\s*transparent;/s);
  assert.match(css, /\.toggle-slider\s*\{[^}]*border:\s*1px solid/s);
});

test("settings language and theme controls keep their selected state in sync", () => {
  const app = fs.readFileSync(require.resolve("./app.js"), "utf8");
  const syncStart = app.indexOf("function updateSettingsSegControls()");
  const syncEnd = app.indexOf("function updateThemeSegControl()", syncStart);
  const syncSettingsControls = app.slice(syncStart, syncEnd);

  assert.notEqual(syncStart, -1);
  assert.match(syncSettingsControls, /settingsLangZh/);
  assert.match(syncSettingsControls, /settingsLangEn/);
  assert.match(syncSettingsControls, /settingsThemeLight/);
  assert.match(syncSettingsControls, /settingsThemeDark/);
  assert.match(syncSettingsControls, /classList\.toggle\("active", active\)/);
  assert.match(syncSettingsControls, /setAttribute\("aria-pressed", String\(active\)\)/);
  assert.match(app, /function updateThemeSegControl\(\)[\s\S]*?updateSettingsSegControls\(\);/);
  assert.match(app, /function updateLangSegControl\(\)[\s\S]*?updateSettingsSegControls\(\);/);
});

test("new installations default to Simplified Chinese", () => {
  const app = fs.readFileSync(require.resolve("./app.js"), "utf8");

  assert.match(app, /let currentLang = localStorage\.getItem\(LANG_STORAGE_KEY\) \|\| "zh";/);
  assert.match(app, /if \(!I18N\[currentLang\]\) \{\s*currentLang = "zh";/);
});

test("console navigation exposes the active page to assistive technology", () => {
  const app = fs.readFileSync(require.resolve("./app.js"), "utf8");
  assert.match(app, /item\.setAttribute\("aria-current",\s*"page"\)/);
  assert.match(app, /item\.removeAttribute\("aria-current"\)/);
});

test("Codex wizard keeps long forms scrollable inside the viewport", () => {
  const css = readStylesheets();
  assert.match(
    css,
    /\.codex-wizard-modal\s+#codexProfileForm\s*\{[^}]*overflow-y:\s*auto;/s
  );
  assert.match(
    css,
    /\.codex-wizard-modal\s*\{[^}]*max-height:\s*calc\(100vh\s*-\s*48px\);/s
  );
});

test("DeepSeek Codex preset uses the native Responses API", () => {
  const app = fs.readFileSync(require.resolve("./app.js"), "utf8");
  const presetsStart = app.indexOf("const CODEX_PRESETS = [");
  const deepseekStart = app.indexOf('id: "deepseek"', presetsStart);
  const nextPreset = app.indexOf("\n  {", deepseekStart);
  const deepseekPreset = app.slice(deepseekStart, nextPreset);

  assert.notEqual(deepseekStart, -1);
  assert.match(deepseekPreset, /providerName:\s*"deepseek"/);
  assert.match(deepseekPreset, /wire:\s*"responses"/);
  assert.match(deepseekPreset, /model:\s*"deepseek-v4-pro"/);
  assert.match(deepseekPreset, /models:\s*\["deepseek-v4-flash",\s*"deepseek-v4-pro"\]/);
  assert.doesNotMatch(deepseekPreset, /wire:\s*"chat"/);
});

test("DeepSeek official Codex settings lock the protocol without matching spoofed hosts", () => {
  assert.equal(typeof isDeepseekCodexConfig, "function");
  assert.equal(isDeepseekCodexConfig("deepseek", "https://api.deepseek.com"), true);
  assert.equal(isDeepseekCodexConfig("deepseek", "https://api.deepseek.com/v1"), true);
  assert.equal(isDeepseekCodexConfig("deepseek", "https://api.deepseek.com.evil.test"), false);
  assert.equal(isDeepseekCodexConfig("custom", "https://api.deepseek.com"), false);
});

test("DeepSeek preset locks Responses and exposes its built-in model choices", () => {
  const app = fs.readFileSync(require.resolve("./app.js"), "utf8");
  const html = fs.readFileSync(require.resolve("./index.html"), "utf8");

  assert.match(app, /function syncCodexWireApiControl\(preset/);
  assert.match(app, /wireSelect\.disabled\s*=\s*locked/);
  assert.match(app, /renderModelResults\("codex",\s*preset\.models\)/);
  assert.doesNotMatch(html, /Chat Completions（DeepSeek、Kimi 等）/);
});

test("Codex runtime status uses a full-width horizontal layout", () => {
  const css = readStylesheets();
  assertStatusGridsShareRule(css, "", "grid-template-columns:\\s*1fr;", { ids: SINGLE_COLUMN_GRID_IDS });
  // 卡内字段用 flex 换行 + grow：字段数固定而 auto-fit 列数会变，
  // 用 grid 时末行会留空格子，所以这里锁定 flex 方案。
  assertStatusGridsShareRule(css, " .status-card", "flex-wrap:\\s*wrap;");
  assertStatusGridsShareRule(css, " .status-item", "flex:\\s*1 1 200px;");
  assertStatusGridsShareRule(css, " .status-value", "text-overflow:\\s*ellipsis;");
});

test("Grok and Codex runtime status share the same responsive layout", () => {
  const css = readStylesheets();
  assertStatusGridsShareRule(css, "", "grid-template-columns:\\s*1fr;", { ids: SINGLE_COLUMN_GRID_IDS });
  assertStatusGridsShareRule(css, " .status-card", "flex-wrap:\\s*wrap;");
});

test("Codex image generation is presented as a Skill instead of an unknown config table", () => {
  const html = fs.readFileSync(require.resolve("./index.html"), "utf8");
  const app = fs.readFileSync(require.resolve("./app.js"), "utf8");
  const previewStart = app.indexOf("function buildCodexOfficialConfig");
  const previewEnd = app.indexOf("function updateCodexOfficialConfigPreview", previewStart);
  const previewBuilder = app.slice(previewStart, previewEnd);

  assert.match(html, /id="codexImageSectionTitle">Codex Image Skill</);
  assert.match(html, /id="codexImageSectionHint"[^>]*>[^<]*Skill/i);
  assert.doesNotMatch(previewBuilder, /\[gpt_image_2\]/);
});

test("editing the active Codex profile reapplies image Skill configuration after save", () => {
  const app = fs.readFileSync(require.resolve("./app.js"), "utf8");
  const submitStart = app.indexOf("async function handleCodexSubmit");
  const submitEnd = app.indexOf("async function handleCodexSwitch", submitStart);
  const submit = app.slice(submitStart, submitEnd);

  assert.match(submit, /editingActiveProfile\s*=\s*codexProfiles\.some/);
  assert.match(submit, /codexEnableAfterSave[\s\S]{0,120}editingActiveProfile/);
  assert.match(submit, /if \(enableAfter && savedId\) \{\s*await handleCodexSwitch\(savedId\);/s);
});

test("Codex image Base URL uses a generic placeholder without a provider default", () => {
  const html = fs.readFileSync(require.resolve("./index.html"), "utf8");
  const app = fs.readFileSync(require.resolve("./app.js"), "utf8");
  assert.match(html, /id="codexImageBaseUrl"[^>]*placeholder="URL"/);
  assert.doesNotMatch(html, /hk\.getelucid\.com/);
  assert.doesNotMatch(app, /hk\.getelucid\.com/);
});

test("configuration management uses the cc-switch universal provider workflow", () => {
  const html = fs.readFileSync(require.resolve("./index.html"), "utf8");
  const app = fs.readFileSync(require.resolve("./app.js"), "utf8");
  assert.doesNotMatch(html, /data-console-page(?:-panel)?="configurations"|>配置管理</);
  assert.match(html, /data-console-page="add-provider"/);
  assert.match(html, /data-console-page-panel="add-provider"/);
  assert.doesNotMatch(html, /id="providerProtocolPicker"|id="providerContinueBtn"/);
  assert.match(html, /id="universalProviderForm"/);
  assert.match(html, /id="universalProviderPresetGrid"/);
  assert.match(html, /id="universalProviderName"/);
  assert.match(html, /id="universalProviderBaseUrl"/);
  assert.match(html, /id="universalProviderApiKey"/);
  assert.match(html, /id="universalProviderApps"/);
  assert.equal((html.match(/data-universal-app=/g) || []).length, 4);
  assert.match(html, /data-universal-app="gemini"/);
  assert.match(html, /id="universalGeminiModel"/);
  assert.match(app, /const UNIVERSAL_PROVIDER_PRESETS\s*=/);
  assert.match(app, /async function handleUniversalProviderSubmit\(/);
  assert.match(app, /rollbackUniversalProviderProfiles/);
  assert.match(app, /add_gemini_profile/);
  assert.match(app, /delete_gemini_profile/);
});

test("universal provider form exposes per-app protocol and connectivity checks", () => {
  const html = fs.readFileSync(require.resolve("./index.html"), "utf8");
  const app = fs.readFileSync(require.resolve("./app.js"), "utf8");

  // 连通性验证与模型自动补全
  assert.match(html, /id="universalEndpointTestBtn"/);
  assert.match(html, /id="universalEndpointResults"/);
  assert.match(html, /id="universalModelOptions"/);
  assert.equal((html.match(/list="universalModelOptions"/g) || []).length, 4);

  // 按应用参数卡片与协议选择（与单独添加弹窗能力对齐）
  assert.equal((html.match(/data-universal-app-card=/g) || []).length, 4);
  assert.match(html, /id="universalCodexWireApi"/);
  assert.match(html, /id="universalCodexWireApi"[\s\S]{0,240}value="responses"[\s\S]{0,240}value="chat"/);
  assert.match(html, /id="universalGrokApiBackend"/);
  assert.match(html, /id="universalGrokApiBackend"[\s\S]{0,320}value="chat_completions"[\s\S]{0,320}value="messages"/);
  assert.equal((html.match(/data-universal-url-preview=/g) || []).length, 4);

  // 提交时协议与按应用地址生效，不再写死
  assert.match(app, /async function handleUniversalEndpointTest\(/);
  assert.match(app, /wireApi:\s*\$\("universalCodexWireApi"\)\?\.value \|\| "responses"/);
  assert.match(app, /apiBackend:\s*\$\("universalGrokApiBackend"\)\?\.value \|\| "chat_completions"/);
  assert.match(app, /resolveUniversalAppBaseUrl/);
  assert.doesNotMatch(app, /wireApi:\s*"responses",/);
  assert.doesNotMatch(app, /apiBackend:\s*"chat_completions",/);
});

test("provider dialogs expose API key actions, inline status, and name validation hooks", () => {
  const html = fs.readFileSync(require.resolve("./index.html"), "utf8");
  const app = fs.readFileSync(require.resolve("./app.js"), "utf8");
  for (const id of ["profileApiKey", "codexApiKey", "grokApiKey", "geminiApiKey"]) {
    assert.match(html, new RegExp(`id="${id}"`));
  }

  // 两个按钮组由 JS 生成（见 renderApiKeyActions / renderBaseUrlActions）：
  // 之前六个弹窗各写一份内联标记，两个 SVG 重复 12 次，其中一份的路径还抄错了。
  // 这里断言占位符齐全 + 生成器存在，而不是数内联标记的份数。
  assert.ok((html.match(/data-api-key-actions="/g) || []).length >= 6, "API Key 按钮组占位符不足");
  assert.ok((html.match(/data-base-url-actions="/g) || []).length >= 6, "Base URL 按钮组占位符不足");
  assert.doesNotMatch(html, /data-api-key-action="toggle"/, "按钮应由 JS 生成，不再内联");
  assert.match(app, /function renderApiKeyActions\(/);
  assert.match(app, /function renderBaseUrlActions\(/);
  // 生成必须早于顶层的 addEventListener，否则拿不到按钮元素
  assert.ok(
    app.indexOf("\nrenderBaseUrlActions();") < app.indexOf('on("profileModelFetchBtn"'),
    "按钮组要在顶层事件绑定之前生成"
  );

  assert.ok((html.match(/class="field-error"/g) || []).length >= 4);
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

test("provider name validation rejects blanks and duplicates while allowing the edited record", () => {
  const app = fs.readFileSync(require.resolve("./app.js"), "utf8");
  assert.match(app, /if \(!normalized\) return \{ valid: false/);
  assert.match(app, /String\(profile\?\.id\) !== String\(currentId/);
  assert.match(app, /配置名称已存在|Config name already exists/);
  assert.match(app, /codexSaveEnableBtn/);
  const css = readStylesheets();
  assert.match(css, /\.field-error[^}]*color:\s*#f43f5e/);
});

test("Claude Desktop dialog uses the requested API key and inline feedback layout", () => {
  const html = fs.readFileSync(require.resolve("./index.html"), "utf8");
  const app = fs.readFileSync(require.resolve("./app.js"), "utf8");
  assert.match(html, /id="claudeDesktopProfileOverlay"[\s\S]*provider-config-modal/);
  // 按钮组改由 JS 生成，这里断言这个弹窗挂了两个占位符（target / 前缀都对）
  assert.match(html, /data-api-key-actions="claudeDesktopProfileApiKey"/);
  assert.match(html, /data-base-url-actions="claude-desktop" data-id-prefix="claudeDesktopProfile"/);
  assert.match(html, /id="claudeDesktopProfileNameError"/);
  assert.match(app, /claudeDesktopProfileModelFetchResults/);
  assert.match(app, /claude-desktop.*nameError/);
});

test("provider dialogs follow the two-column modal form layout", () => {
  const html = fs.readFileSync(require.resolve("./index.html"), "utf8");
  ["modalOverlay", "claudeDesktopProfileOverlay", "codexModalOverlay", "grokModalOverlay", "geminiModalOverlay", "opencodeModalOverlay"].forEach((overlayId) => {
    const overlay = html.match(new RegExp(`<div class="modal-overlay" id="${overlayId}">[\\s\\S]*?<div class="modal\\s[^>]*>`));
    assert.ok(overlay, `${overlayId} should contain a modal`);
    assert.match(overlay[0], /provider-config-modal/);
  });
  assert.match(html, /class="modal-form-grid(?:\s|\")/);
  assert.match(html, /class="[^"]*modal-form-full/);
  assert.match(html, /class="[^"]*modal-footer-option/);
  assert.match(html, /保存并立即启用|Save and activate/);
  for (const id of ["profileBaseUrl", "claudeDesktopProfileBaseUrl", "codexBaseUrl", "grokBaseUrl", "geminiBaseUrl", "opencodeBaseUrl"]) {
    const field = html.match(new RegExp(`id="${id}"[\\s\\S]{0,260}base-url-actions`));
    assert.ok(field, `${id} should use action-in-input layout`);
  }
});

test("provider modal controls keep inline alignment and stable action columns", () => {
  const html = fs.readFileSync(require.resolve("./index.html"), "utf8");
  const css = readStylesheets();

  assert.match(css, /\.provider-config-modal \.proxy-failover-checkbox\s*\{[^}]*display:\s*inline-flex[^}]*align-items:\s*center/s);
  assert.match(css, /\.provider-config-modal \.base-url-actions \.link-button\s*\{[^}]*display:\s*inline-flex[^}]*justify-content:\s*center[^}]*gap:\s*5px/s);
  assert.match(css, /\.provider-config-modal \.claude-desktop-model-mapping-row\s*\{[^}]*grid-template-columns:\s*minmax\(0,\s*1fr\)\s+minmax\(0,\s*1fr\)\s+52px/s);
  assert.match(css, /\.provider-config-modal \.claude-desktop-model-mapping-head\s*\{[^}]*grid-template-columns:\s*minmax\(0,\s*1fr\)\s+minmax\(0,\s*1fr\)\s+52px/s);
  assert.match(css, /\.provider-config-modal \.claude-desktop-model-mapping-row input\s*\{[^}]*border:\s*1px solid #e2e8f0[^}]*border-radius:\s*8px[^}]*min-height:\s*40px/s);

  const codexForm = html.match(/<form id="codexProfileForm">([\s\S]*?)<\/form>/)?.[1] || "";
  assert.ok(codexForm.indexOf('id="codexProfileName"') < codexForm.indexOf('id="codexBaseUrl"'));
  assert.ok(codexForm.indexOf('id="codexBaseUrl"') < codexForm.indexOf('id="codexWireApi"'));
  assert.ok(codexForm.indexOf('id="codexWireApi"') < codexForm.indexOf('id="codexModel"'));
});

test("Claude supports OpenAI-format upstreams via the local conversion proxy", () => {
  const html = fs.readFileSync(require.resolve("./index.html"), "utf8");
  const app = fs.readFileSync(require.resolve("./app.js"), "utf8");
  const rust = readRustSources();
  const proxy = fs.readFileSync(require.resolve("../src-tauri/src/claude_proxy.rs"), "utf8");
  const cargo = fs.readFileSync(require.resolve("../src-tauri/Cargo.toml"), "utf8");

  // Claude 弹窗与统一供应商页都能选 API 格式
  assert.match(html, /id="profileApiFormat"/);
  assert.match(html, /id="profileApiFormat"[\s\S]{0,240}value="anthropic"[\s\S]{0,240}value="openai_chat"/);
  assert.match(html, /id="universalClaudeApiFormat"/);

  // 前端把格式传给后端，并按格式选择验证协议
  assert.match(app, /apiFormat:\s*claudeApiFormat/);
  assert.match(app, /invoke\("update_profile", \{ id: editingId, name, apiKey, baseUrl, modelId, apiFormat, sonnetModel, opusModel, haikuModel, proxyFailover, proxyTakeover \}\)/);
  assert.match(app, /invoke\("add_profile", \{ name, apiKey, baseUrl, modelId: modelId \|\| null, apiFormat, sonnetModel, opusModel, haikuModel, proxyFailover, proxyTakeover \}\)/);
  assert.match(app, /function productProtocol\(kind\)/);

  // Rust：Profile 结构、代理模块、切换链路、状态命令
  assert.match(cargo, /tiny_http\s*=/);
  assert.match(rust, /mod claude_proxy;/);
  assert.match(rust, /fn normalize_claude_api_format/);
  assert.match(rust, /api_format:\s*String,/);
  assert.match(rust, /claude_proxy::ensure_server\(\)\?/);
  assert.match(rust, /apply_auth_to_system_env\(&profile\.api_key, &effective_base_url\)/);
  assert.match(rust, /get_claude_proxy_status,/);
  // 启动恢复：激活的、经代理的配置在重启后重建代理
  assert.match(rust, /p\.is_active && profile_uses_proxy\(p\)/);

  // 代理模块：双向转换 + 流式状态机
  assert.match(proxy, /pub const CLAUDE_PROXY_PORT: u16 = 25789;/);
  assert.match(proxy, /pub fn anthropic_request_to_openai/);
  assert.match(proxy, /pub fn openai_response_to_anthropic/);
  assert.match(proxy, /\/v1\/messages/);
  assert.match(proxy, /chat\/completions/);
  assert.match(proxy, /message_start/);
  assert.match(proxy, /input_json_delta/);
  assert.match(proxy, /#\[cfg\(test\)\]/);
});

test("every Prompts tab is wired to a button and a content panel", () => {
  const html = fs.readFileSync(require.resolve("./index.html"), "utf8");
  const app = fs.readFileSync(require.resolve("./app.js"), "utf8");

  // 表驱动的 tab 定义：漏掉任一 tab 会导致面板内容叠加显示
  const table = app.match(/const PROMPT_TABS = \[([\s\S]*?)\];/);
  assert.ok(table, "PROMPT_TABS table should exist");
  const entries = [...table[1].matchAll(/\{\s*id:\s*"(\w+)",\s*button:\s*"(\w+)",\s*content:\s*"(\w+)"\s*\}/g)];
  assert.equal(entries.length, 3);

  for (const [, id, button, content] of entries) {
    assert.match(html, new RegExp(`id="${button}"`), `${id} tab button missing in HTML`);
    assert.match(html, new RegExp(`id="${content}"`), `${id} tab content missing in HTML`);
    assert.match(app, new RegExp(`switchPromptTab\\("${id}"\\)`), `${id} tab never activated`);
  }
});

test("Anthropic upstreams can be taken over by the proxy for failover", () => {
  const html = fs.readFileSync(require.resolve("./index.html"), "utf8");
  const app = fs.readFileSync(require.resolve("./app.js"), "utf8");
  const rust = readRustSources();
  const proxy = fs.readFileSync(require.resolve("../src-tauri/src/claude_proxy.rs"), "utf8");

  // 表单提供接管开关，仅对 anthropic 直连显示（openai_chat 本就必经代理）
  assert.match(html, /id="profileProxyTakeover"/);
  assert.match(app, /takeoverGroup\.hidden = isOpenAi/);
  assert.match(app, /apiFormat !== "openai_chat" && Boolean\(\$\("profileProxyTakeover"\)\?\.checked\)/);

  // 后端：接管字段 + 统一的「是否经代理」判定驱动切换与启动恢复
  assert.match(rust, /pub\(crate\) proxy_takeover: bool,/);
  assert.match(rust, /fn profile_uses_proxy\(profile: &Profile\) -> bool/);
  assert.match(rust, /profile\.api_format == "openai_chat" \|\| profile\.proxy_takeover/);
  assert.match(rust, /if profile_uses_proxy\(&profile\)/);
  assert.match(rust, /set_upstream_with_mode/);

  // 代理引擎：透传模式直发 Anthropic 端点，流式按字节搬运不做翻译
  assert.match(proxy, /pub enum UpstreamMode/);
  assert.match(proxy, /UpstreamMode::Anthropic =>[\s\S]{0,200}x-api-key/);
  assert.match(proxy, /fn passthrough_stream/);
  // 备用池每项自带协议，两种协议可混用
  assert.match(proxy, /pub fn set_failover_targets/);
  assert.match(proxy, /pub struct ProxyTarget/);
});

test("resolveUniversalAppBaseUrl adapts one gateway URL per app", () => {
  // Codex / Grok 遵循 OpenAI 兼容惯例，缺版本段时自动补 /v1
  assert.equal(resolveUniversalAppBaseUrl("codex", "https://api.example.com"), "https://api.example.com/v1");
  assert.equal(resolveUniversalAppBaseUrl("codex", "https://api.example.com/"), "https://api.example.com/v1");
  assert.equal(resolveUniversalAppBaseUrl("grok", "https://api.example.com/openai"), "https://api.example.com/openai/v1");
  // 已带版本段（/v1、/v2、/v1beta）时保持原样
  assert.equal(resolveUniversalAppBaseUrl("codex", "https://api.example.com/v1"), "https://api.example.com/v1");
  assert.equal(resolveUniversalAppBaseUrl("grok", "https://api.example.com/v1/"), "https://api.example.com/v1");
  assert.equal(resolveUniversalAppBaseUrl("codex", "https://api.example.com/v1beta"), "https://api.example.com/v1beta");
  // Claude / Gemini 使用根地址，只去掉末尾斜杠
  assert.equal(resolveUniversalAppBaseUrl("claude", "https://api.example.com/"), "https://api.example.com");
  assert.equal(resolveUniversalAppBaseUrl("gemini", "https://api.example.com"), "https://api.example.com");
  // 空值安全
  assert.equal(resolveUniversalAppBaseUrl("codex", ""), "");
  assert.equal(resolveUniversalAppBaseUrl("claude", null), "");
});

test("provider navigation follows saved configuration availability", () => {
  const html = fs.readFileSync(require.resolve("./index.html"), "utf8");
  const app = fs.readFileSync(require.resolve("./app.js"), "utf8");
  const css = readStylesheets();
  assert.equal((html.match(/data-provider-nav=/g) || []).length, 4);
  assert.match(app, /function renderProviderNavigation\(\)/);
  assert.match(app, /claude:\s*profiles\.length\s*>\s*0/);
  assert.match(app, /codex:\s*codexProfiles\.length\s*>\s*0/);
  assert.match(app, /grok:\s*grokProfiles\.length\s*>\s*0/);
  assert.match(app, /gemini:\s*true/);
  assert.match(app, /item\.hidden\s*=\s*!available/);
  assert.match(css, /\.sidebar-item\[hidden\]\s*\{[^}]*display:\s*none\s*!important;/s);
  assert.match(css, /@media\s*\(max-width:\s*760px\)[\s\S]*?\.provider-builder-footer\s*\{[^}]*position:\s*fixed;[^}]*bottom:\s*70px;/s);
});

test("Gemini is a complete configurable provider", () => {
  const html = fs.readFileSync(require.resolve("./index.html"), "utf8");
  const app = fs.readFileSync(require.resolve("./app.js"), "utf8");
  const rust = readRustSources();

  assert.match(html, /data-provider-nav="gemini"[\s\S]*gemini-color\.svg/);
  assert.match(html, /id="pageGemini"[^>]*data-console-page-panel="gemini"/);
  assert.match(html, /id="geminiProfilesGrid"/);
  assert.match(html, /id="geminiProfileForm"/);
  assert.match(app, /let geminiProfiles\s*=\s*\[\]/);
  assert.match(app, /async function loadGeminiProfiles\(/);
  assert.match(app, /async function handleGeminiSwitch\(/);
  assert.match(app, /async function handleGeminiDelete\(/);
  assert.match(app, /async function handleGeminiImport\(/);
  for (const command of [
    "get_gemini_profiles",
    "add_gemini_profile",
    "update_gemini_profile",
    "delete_gemini_profile",
    "switch_gemini_profile",
    "import_gemini_current",
    "get_gemini_status",
  ]) {
    assert.match(rust, new RegExp(`fn ${command}\\b`));
    assert.match(rust, commandRegistrationPattern(command));
  }
  assert.match(rust, /\.join\("\.gemini"\)\.join\("settings\.json"\)/);
  assert.match(rust, /GEMINI_API_KEY/);
  assert.match(rust, /GOOGLE_GEMINI_BASE_URL/);
  assert.match(rust, /GEMINI_MODEL/);
  assert.match(rust, /\["selectedType"\]\s*=\s*serde_json::json!\("gemini-api-key"\)/);
});

test("plugin marketplace page and toolbox controls stay removed", () => {
  const html = fs.readFileSync(require.resolve("./index.html"), "utf8");
  assert.doesNotMatch(html, /data-console-page(?:-panel)?="plugins"/);
  assert.doesNotMatch(html, /id="pagePlugins"|id="pluginPageHost"/);
  assert.doesNotMatch(html, /id="toolboxTabMarket"|id="toolboxMarketContent"/);
  assert.doesNotMatch(html, /id="toolboxMarketApplyBtn"|id="builtinPluginPanel"/);
});

test("Codex Toolbox switches session sync and mobile control inside one page", () => {
  const html = fs.readFileSync(require.resolve("./index.html"), "utf8");
  const app = fs.readFileSync(require.resolve("./app.js"), "utf8");
  const css = readStylesheets();
  const openToolboxStart = app.indexOf("function openCodexToolbox()");
  const openToolboxEnd = app.indexOf("function closeCodexToolbox()", openToolboxStart);
  const openToolbox = app.slice(openToolboxStart, openToolboxEnd);
  const switchTabStart = app.indexOf("function switchToolboxTab(tab)");
  const switchTabEnd = app.indexOf("function openCodexToolbox()", switchTabStart);
  const switchTab = app.slice(switchTabStart, switchTabEnd);

  assert.match(html, /id="toolboxSectionLabel"[^>]*>工具箱</);
  assert.match(
    html,
    /id="codexToolboxNav"[^>]*data-console-page="toolbox"[^>]*>\s*<img src="OpenAI-black-monoblossom\.svg" alt="">\s*<span>Codex Toolbox<\/span>/
  );
  assert.doesNotMatch(html, /data-console-page="sessions"|data-console-page="mobile"/);
  assert.match(html, /id="pageCodexToolbox"[^>]*data-console-page-panel="toolbox"/);
  assert.match(html, /id="toolboxPageTabs"[^>]*role="tablist"/);
  assert.match(html, /id="toolboxTabSession"[^>]*role="tab"/);
  assert.match(html, /id="toolboxTabRemote"[^>]*role="tab"/);
  assert.match(openToolbox, /switchConsolePage\("toolbox"\)/);
  assert.match(openToolbox, /switchToolboxTab\("session"\)/);
  assert.doesNotMatch(openToolbox, /switchConsolePage\("mobile"\)/);
  assert.match(switchTab, /setAttribute\("aria-selected"/);
  assert.match(switchTab, /session\.style\.display\s*=\s*tab\s*===\s*"session"/);
  assert.match(switchTab, /remote\.style\.display\s*=\s*tab\s*===\s*"remote"/);
  assert.match(css, /\.sidebar-item img\[src\*="OpenAI"\]\s*\{[^}]*transform:\s*scale\(1\.12\);/s);
});

test("Claude Toolbox appears above Codex Toolbox and defaults to Skills", () => {
  const html = fs.readFileSync(require.resolve("./index.html"), "utf8");
  const app = fs.readFileSync(require.resolve("./app.js"), "utf8");
  const settingsStart = html.indexOf('id="pageSettings"');
  const settingsEnd = html.indexOf("</section>", settingsStart);
  const settingsPage = html.slice(settingsStart, settingsEnd);
  const claudeToolboxNav = html.indexOf('id="developerToolsNav"');
  const codexToolboxNav = html.indexOf('id="codexToolboxNav"');

  assert.ok(claudeToolboxNav > -1 && claudeToolboxNav < codexToolboxNav);
  assert.match(html, /class="sidebar-item sidebar-toolbox-entry"[^>]*id="developerToolsNav"[^>]*data-console-page="developer-tools"[^>]*>[\s\S]*?anthropic-color\.svg[\s\S]*?<span>Claude Toolbox<\/span>/);
  assert.doesNotMatch(html, /class="sidebar-item sidebar-subitem"[^>]*data-console-page="developer-tools"/);
  assert.match(html, /data-console-page-panel="developer-tools"[^>]*>[\s\S]*?<h1>Claude Toolbox<\/h1>/);
  assert.match(html, /class="developer-tools-tabs"[^>]*aria-label="Claude Toolbox"/);
  assert.match(html, /class="developer-tools-tab active"[^>]*id="developerToolsSkillsTab"/);
  assert.match(html, /id="developerToolsPromptsTab"/);
  assert.match(html, /id="developerToolsMcpTab"/);
  assert.doesNotMatch(settingsPage, /settingsSkillsBtn|settingsPromptsBtn|settingsMcpBtn|settings-tools-card/);
  assert.match(app, /let activeDeveloperTool = "skills"/);
  assert.match(app, /function switchDeveloperTool\(tool\)/);
  assert.match(app, /openDeveloperTools\("skills"\)/);

  // 三个工具面板静态内联在页面里，不再由 JS 从隐藏 overlay 搬运过来。
  // 断言渲染结果而不是搬运函数，避免面板挂在 DOM 之外还能「通过」。
  assert.doesNotMatch(app, /mountDeveloperToolsPage/);
  assert.doesNotMatch(html, /id="skillsOverlay"|id="promptsOverlay"|id="mcpOverlay"/);
  for (const hostId of ["developerToolSkillsContent", "developerToolPromptsContent", "developerToolMcpContent"]) {
    assert.match(
      html,
      new RegExp(`id="${hostId}"[^>]*>\\s*<div class="mgmt-panel[^"]*developer-tool-panel"`),
      `${hostId} 应直接内联 mgmt-panel`
    );
  }
});

test("compact navigation accounts for every primary destination", () => {
  const css = readStylesheets();
  assert.match(
    css,
    /@media\s*\(max-width:\s*760px\)[\s\S]*?\.sidebar-nav\s*\{[^}]*grid-auto-columns:\s*minmax\(76px,\s*1fr\);/s
  );
});

test("static copy is translated through data-i18n, not hardcoded", () => {
  const html = fs.readFileSync(require.resolve("./index.html"), "utf8");
  const app = fs.readFileSync(require.resolve("./app.js"), "utf8");

  // 属性驱动的翻译入口
  assert.match(app, /function applyStaticI18n\(/);
  assert.match(app, /root\.querySelectorAll\("\[data-i18n\]"\)/);
  assert.match(app, /document\.title = t\("appTitle"\);\s*\n\s*applyStaticI18n\(\);/);

  // 每个 data-i18n 的 key 都必须在中英两套字典里存在，否则 t() 会退化成回显 key
  const keys = [...html.matchAll(/data-i18n(?:-placeholder|-title)?="([^"]+)"/g)].map((m) => m[1]);
  assert.ok(keys.length > 40, `data-i18n 标记过少（${keys.length}），静态文案可能又被写死了`);

  const enBlock = app.slice(app.indexOf("  en: {"), app.indexOf("  zh: {"));
  const zhBlock = app.slice(app.indexOf("  zh: {"));
  for (const key of new Set(keys)) {
    assert.ok(new RegExp(`^\\s+${key}:`, "m").test(enBlock), `英文字典缺少 ${key}`);
    assert.ok(new RegExp(`^\\s+${key}:`, "m").test(zhBlock), `中文字典缺少 ${key}`);
  }

  // 供应商预设的名称/描述存 key，不再把中文写死在表里
  assert.match(app, /nameKey:\s*"universalPreset/);
  assert.match(app, /descriptionKey:\s*"universalPreset/);
  assert.doesNotMatch(
    app.slice(app.indexOf("UNIVERSAL_PROVIDER_PRESETS"), app.indexOf("const I18N")),
    /[一-龥]/,
    "预设表里不应再有中文字面量"
  );
});

test("switching language re-renders every page, not just the active one", () => {
  const app = fs.readFileSync(require.resolve("./app.js"), "utf8");
  const fn = app.slice(app.indexOf("function setLanguage(lang)"), app.indexOf("function setTheme("));

  // 所有页面常驻 DOM：只重渲染当前页会让其他页停在旧语言，切回去才发现没变。
  for (const renderer of [
    "renderProfiles", "loadStatus",
    "renderCodexProfiles", "loadCodexStatus",
    "renderGrokProfiles", "loadGrokStatus",
    "renderGeminiProfiles", "loadGeminiStatus",
    "renderOpenCodeProfiles", "loadOpenCodeStatus",
    "renderClaudeDesktopProfiles", "renderUniversalProviderForm",
    "renderSessionStatusCard",
  ]) {
    assert.ok(fn.includes(renderer), `setLanguage 漏了 ${renderer}`);
  }
  // 旧写法按 currentPage 分支，只有停留在该页时才重渲染
  assert.doesNotMatch(fn, /if \(currentPage === "(codex|grok|gemini)"\)/);
});

test("glass effects stay on the two surfaces that need them", () => {
  const css = readStylesheets();

  // 打开配置弹窗时曾有 13 层 backdrop-filter 同时生效：顶栏、侧栏、遮罩、弹窗，
  // 外加表单里每一个输入框。输入框和弹窗都是不透明的，那层模糊算了也看不见。
  assert.doesNotMatch(
    css,
    /\.form-group input,\s*\n\.form-select\s*\{[^}]*backdrop-filter/s,
    "输入框不应再做毛玻璃"
  );
  assert.match(
    css,
    /\.modal,[\s\S]{0,120}\.guide-modal,[\s\S]{0,80}\{[^}]*backdrop-filter:\s*none;/s,
    "弹窗面板本身应关掉毛玻璃，背景虚化交给遮罩"
  );
});

test("status sections stay flat so the config list reaches the first screen", () => {
  const css = readStylesheets();

  // 状态区曾是「带内边距的卡片里再套一张状态卡」，两层内边距加上一行重复的产品名，
  // 把配置列表推到首屏之外。这里锁定扁平化后的结构。
  assert.match(css, /\.console-page \.status-section\s*\{[^}]*padding:\s*0;[^}]*background:\s*none;/s);
  assert.match(css, /\.console-page \.status-section \.status-card\s*\{[^}]*radial-gradient/s);

  // 每个状态网格的卡内标题都要收起，包括 Claude Desktop 那个单独的网格。
  // 代理健康与网关健康的卡片没有标题元素，不在此列。
  const titled = SINGLE_COLUMN_GRID_IDS.filter((id) => !id.includes("Health"));
  assertStatusGridsShareRule(css, " .status-card-title", "display:\\s*none;", { ids: titled });
});

test("sidebar collapses to icons on mid-width windows", () => {
  const css = readStylesheets();
  const app = fs.readFileSync(require.resolve("./app.js"), "utf8");

  // 1200px 以下侧栏只留图标，把横向空间让给正文；760px 以下另有底部导航栏接管。
  const collapsed = css.match(/@media\s*\(max-width:\s*1200px\)\s*and\s*\(min-width:\s*761px\)\s*\{[\s\S]*?\n\}/);
  assert.ok(collapsed, "应存在中等宽度的侧栏折叠断点");
  assert.match(collapsed[0], /--sidebar-width:\s*62px;/);
  assert.match(collapsed[0], /\.sidebar-item\s*\{[^}]*grid-template-columns:\s*20px;/s);

  // 文字收起后靠原生 tooltip 说明每一项，title 由 JS 从标签同步，切语言时要一起更新
  assert.match(app, /function syncSidebarItemTitles\(\)/);
  assert.match(app, /updateThemeSegControl\(\);[\s\S]{0,200}syncSidebarItemTitles\(\);/);
});

test("water-tight left alignment between toolbar and sidebar", () => {
  const css = readStylesheets();
  // 顶栏 logo 与侧栏项的左边距同源，避免两条基线各自漂移
  assert.match(css, /--gutter:\s*calc\(var\(--sidebar-inset\)\s*\+\s*var\(--sidebar-item-inset\)\);/);
  assert.match(css, /\.toolbar-inner\s*\{\s*padding:\s*0 var\(--gutter\);\s*\}/);
  assert.match(css, /\.sidebar-item\s*\{[^}]*padding:\s*8px var\(--sidebar-item-inset\);/s);
});

test("console provides keyboard focus and reduced-motion affordances", () => {
  const css = readStylesheets();
  assert.match(css, /:where\([^)]*button[^)]*\):focus-visible\s*\{/s);
  assert.match(css, /@media\s*\(prefers-reduced-motion:\s*reduce\)\s*\{/s);
});

test("global toolbar and page sections do not duplicate primary config actions", () => {
  const html = fs.readFileSync(require.resolve("./index.html"), "utf8");
  assert.doesNotMatch(html, /id="globalStatusCluster"|id="globalConfigPill"|id="globalStatus"/);
  assert.doesNotMatch(html, /id="(?:importBtn|addBtn|settingsBtn)"/);
  assert.doesNotMatch(
    html,
    /id="(?:syncNowBtn|codexSyncNowBtn|toolboxSessionSyncBtn)"/
  );
  assert.match(html, /id="(?:claudeSyncBtn|codexSyncBtn|sessionPageSyncBtn)"/);
});

test("Claude status and profiles reuse the Codex page structure", () => {
  const html = fs.readFileSync(require.resolve("./index.html"), "utf8");
  const app = fs.readFileSync(require.resolve("./app.js"), "utf8");
  const loadStatusStart = app.indexOf("function renderClaudeStatus(status)");
  const loadStatusEnd = app.indexOf("async function loadProfiles()", loadStatusStart);
  const loadStatus = app.slice(loadStatusStart, loadStatusEnd);

  assert.doesNotMatch(html, /id="claudeRefreshBtn"/);
  assert.doesNotMatch(html, /id="claudeEnvMatrix"/);
  assert.doesNotMatch(html, /id="activeConfigSection"/);
  const pageStart = html.indexOf('id="pageClaudeCode"');
  const pageEnd = html.indexOf('<!-- end pageClaudeCode -->', pageStart);
  const page = html.slice(pageStart, pageEnd);
  assert.match(page, /class="section-header-row"[\s\S]*id="statusSectionTitle"[\s\S]*id="statusGrid"/);
  assert.match(page, /id="profilesSectionTitle">Claude Config List</);
  assert.match(loadStatus, /productIcon\("anthropic"\)\}Claude<\/span>/);
  assert.match(loadStatus, /class="status-card-title"/);
  assert.equal((loadStatus.match(/class="status-item"/g) || []).length, 3);
  assert.match(loadStatus, /Claude API Key/);
  assert.match(loadStatus, /Claude Base URL/);
  assert.match(loadStatus, /Claude 模型/);
  assert.match(loadStatus, /status\?\.claudeModel/);
  assert.doesNotMatch(loadStatus, /claude-runtime-card|claude-status-fields|claude-status-field/);
  assert.doesNotMatch(loadStatus, /statusSystemEnv|claude-runtime-group|status-badge/);
  assert.doesNotMatch(loadStatus, /editor-carousel|editorLocations|status\.editors/);

  const css = readStylesheets();
  assertStatusGridsShareRule(css, "", "grid-template-columns:\\s*1fr;", { ids: SINGLE_COLUMN_GRID_IDS });
  assert.match(css, /\.product-icon svg,\s*\.product-icon img\s*\{[^}]*width:\s*16px;[^}]*height:\s*16px;/s);
});

test("Codex page mirrors the compact Claude action and status layout", () => {
  const html = fs.readFileSync(require.resolve("./index.html"), "utf8");
  const app = fs.readFileSync(require.resolve("./app.js"), "utf8");
  const pageStart = html.indexOf('id="pageCodex"');
  const pageEnd = html.indexOf('<!-- end pageCodex -->', pageStart);
  const page = html.slice(pageStart, pageEnd);

  assert.match(
    page,
    /id="codexSyncBtn"[\s\S]*id="codexPageImportBtn"[\s\S]*id="codexPageAddBtn"/
  );
  assert.doesNotMatch(page, /codexRefreshDiagnosticsBtn|codexBackupRuntimeBtn|codexCurrentStatusCard/);
  assert.doesNotMatch(page, /codexActiveConfigSection|codexDiagnosticsPanel/);
  assert.match(page, /id="codexStatusSectionTitle"[\s\S]*id="codexStatusGrid"/);
  assert.doesNotMatch(page, /Codex CLI/);
  assert.match(app, /productIcon\("codex"\)\}Codex<\/span>/);
  const productIconStart = app.indexOf("function productIcon(kind)");
  const productIconEnd = app.indexOf("function setBadge", productIconStart);
  const productIcon = app.slice(productIconStart, productIconEnd);
  assert.match(productIcon, /product-icon-codex[\s\S]*OpenAI-black-monoblossom\.svg/);
  assert.doesNotMatch(productIcon, /<svg viewBox="0 0 64 64"/);
  assert.match(app, /当前: \$\{active\.name\}/);
  assert.match(app, /\$\("codexSyncBtn"\)\?\.addEventListener\("click"/);
});

test("Claude profile actions use the same equal-width buttons as Codex", () => {
  const css = readStylesheets();
  const app = fs.readFileSync(require.resolve("./app.js"), "utf8");
  const renderStart = app.indexOf("function renderProfiles()");
  const renderEnd = app.indexOf("function updateActiveConfigBar()", renderStart);
  const renderProfiles = app.slice(renderStart, renderEnd);

  assert.match(renderProfiles, /class="btn btn-danger btn-sm"/);
  assert.doesNotMatch(renderProfiles, /profile-delete-btn/);
  assert.match(css, /\.profile-actions\s+\.btn\s*\{[^}]*flex:\s*1;/s);
  assert.doesNotMatch(css, /#pageClaudeCode\s+\.profile-actions/);
});

test("Claude Desktop has a first-class provider page and gateway form", () => {
  const html = fs.readFileSync(require.resolve("./index.html"), "utf8");
  assert.match(html, /id="claudeDesktopNav"[^>]*data-console-page="claude-desktop"/);
  assert.match(html, /id="pageClaudeDesktop"[^>]*data-console-page-panel="claude-desktop"/);
  assert.match(html, /id="claudeDesktopStatusGrid"/);
  assert.match(html, /id="claudeDesktopProfilesList"/);
  assert.match(html, /id="claudeDesktopImportBtn"/);
  assert.match(html, /id="claudeDesktopProfileConnectionMode"/);
  assert.match(html, /value="gateway"/);
  assert.match(html, /value="direct"/);
  assert.match(html, /id="claudeDesktopProfileSonnetModel"/);
  assert.match(html, /id="claudeDesktopProfileOpusModel"/);
  assert.match(html, /id="claudeDesktopProfileHaikuModel"/);
  assert.match(html, /id="claudeDesktopModelMappings"/);
  assert.match(html, /id="claudeDesktopProfileAddMappingBtn"/);
});

test("Claude Desktop gateway form exposes model discovery controls", () => {
  const html = fs.readFileSync(require.resolve("./index.html"), "utf8");
  const app = fs.readFileSync(require.resolve("./app.js"), "utf8");
  // 「拉取模型」按钮由 renderBaseUrlActions 依据 data-id-prefix 生成 id
  assert.match(html, /data-base-url-actions="claude-desktop" data-id-prefix="claudeDesktopProfile"/);
  assert.match(app, /\$\{esc\(prefix\)\}ModelFetchBtn/);
  assert.match(html, /id="claudeDesktopProfileModelFetchResults"/);
  assert.match(html, /list="claudeDesktopModelOptions"/);
  assert.match(app, /function fetchClaudeDesktopModels/);
  assert.match(app, /fetch_available_models/);
  assert.match(app, /claudeDesktopProfileModelId/);
});

test("Claude Desktop mappings are explicit source-to-upstream pairs", () => {
  const html = fs.readFileSync(require.resolve("./index.html"), "utf8");
  const app = fs.readFileSync(require.resolve("./app.js"), "utf8");
  assert.match(html, /Claude Code 模型/);
  assert.match(html, /上游实际模型/);
  assert.match(app, /modelMappings:/);
  assert.match(app, /function addClaudeDesktopModelMapping/);
});

test("Claude Desktop Gateway keeps third-party model ids from discovery", () => {
  assert.deepEqual(
    normalizeClaudeDesktopModels([
      { id: "gpt-5" },
      { id: "qwen3.7-max" },
      { id: "claude-sonnet-4-6" },
      { id: "" },
      { id: "gpt-5" },
    ]),
    ["claude-sonnet-4-6", "gpt-5", "qwen3.7-max"]
  );
  const app = fs.readFileSync(require.resolve("./app.js"), "utf8");
  assert.doesNotMatch(app, /isClaudeDesktopModelId/);
});

test("Claude Desktop does not embed a localization workflow", () => {
  const html = fs.readFileSync(require.resolve("./index.html"), "utf8");
  const app = fs.readFileSync(require.resolve("./app.js"), "utf8");
  assert.doesNotMatch(html, /claudeDesktopLocalization/);
  assert.doesNotMatch(app, /claudeDesktopLocalization|run_claude_desktop_localization/);
});

test("Claude Desktop frontend wires independent provider commands", () => {
  const app = fs.readFileSync(require.resolve("./app.js"), "utf8");
  for (const command of [
    "get_claude_desktop_profiles",
    "get_claude_desktop_provider_status",
    "add_claude_desktop_profile",
    "update_claude_desktop_profile",
    "delete_claude_desktop_profile",
    "reorder_claude_desktop_profiles",
    "switch_claude_desktop_profile",
    "sync_claude_desktop_profile",
    "import_claude_profiles_to_desktop",
    "get_claude_desktop_gateway_health",
    "claude_desktop_gateway_reset_breaker",
  ]) assert.match(app, new RegExp(`invoke\\(["']${command}["']`));
  assert.match(app, /function loadClaudeDesktopPage/);
  assert.match(app, /function renderClaudeDesktopStatus/);
  assert.match(app, /function renderClaudeDesktopProfiles/);
  assert.match(app, /function updateClaudeDesktopProfileFormState/);
});

test("Claude Desktop confirmations use the in-app async dialog", () => {
  const app = fs.readFileSync(require.resolve("./app.js"), "utf8");
  const deleteFlow = app.match(/async function deleteClaudeDesktopProfile[\s\S]*?\n}/)?.[0] || "";
  const importFlow = app.match(/async function importClaudeProfilesToDesktop[\s\S]*?\n}/)?.[0] || "";
  assert.match(deleteFlow, /await appConfirm\(/);
  assert.match(importFlow, /await appConfirm\(/);
  assert.doesNotMatch(deleteFlow, /\bconfirm\(/);
  assert.doesNotMatch(importFlow, /\bconfirm\(/);
});

test("download page exposes a shared download counter and clickable sparkle", () => {
  const html = fs.readFileSync(path.join(__dirname, "..", "varswitch-download-site", "index.html"), "utf8");
  const script = fs.readFileSync(path.join(__dirname, "..", "varswitch-download-site", "download-site.js"), "utf8");
  assert.match(html, /id="downloadCount"/);
  assert.match(html, /id="downloadCountButton"/);
  assert.match(html, /data-download-track/);
  assert.match(html, /download-site\.js/);
  assert.match(script, /const endpoint = ["']\/api\/download-count["']/);
});

test("download counter helper formats counts and posts the selected metric", async () => {
  const counter = require(path.join(__dirname, "..", "varswitch-download-site", "download-site.js"));
  assert.equal(counter.formatCount(1234), "1.2K");
  let request;
  const result = await counter.requestCount(async (url, options) => {
    request = { url, options };
    return { ok: true, json: async () => ({ metric: "sparkles", count: 7 }) };
  }, "sparkles", "POST");
  assert.deepEqual(result, { metric: "sparkles", count: 7 });
  assert.equal(request.url, "/api/download-count?metric=sparkles");
  assert.equal(request.options.method, "POST");
});

test("getCodexSessionMetrics reads the current toolbox response fields", () => {
  assert.deepEqual(
    getCodexSessionMetrics({
      sessionSync: { total: 137, lastSyncedAt: "1783855833904" },
      syncedCodexThreads: [{ id: "one" }, { id: "two" }],
      trashedCodexThreads: [{ id: "trash" }],
    }),
    {
      count: 137,
      trashCount: 1,
      lastSyncedAt: "1783855833904",
    }
  );
});

test("getGlobalConfigDisplay follows the active console page", () => {
  const state = {
    claude: "闪速ai",
    codex: "elucid",
    grok: "xAI Official",
    total: 17,
  };
  assert.deepEqual(getGlobalConfigDisplay("claude", state, "zh"), {
    label: "Claude",
    name: "闪速ai",
    title: "Claude: 闪速ai · Codex: elucid · Grok: xAI Official",
  });
  assert.equal(getGlobalConfigDisplay("codex", state, "zh").name, "elucid");
  assert.equal(getGlobalConfigDisplay("sessions", state, "zh").label, "Codex");
  assert.equal(getGlobalConfigDisplay("grok", state, "zh").name, "xAI Official");
  assert.deepEqual(getGlobalConfigDisplay("overview", state, "zh"), {
    label: "环境",
    name: "3 个已启用",
    title: "Claude: 闪速ai · Codex: elucid · Grok: xAI Official",
  });
  assert.equal(getGlobalConfigDisplay("add-provider", state, "zh").name, "17 套配置");
});

test("getSwitchOverlayState differentiates Claude progress from Codex/Grok busy state", () => {
  assert.deepEqual(getSwitchOverlayState("claude"), {
    cancellable: true,
    indeterminate: false,
    showSteps: true,
  });
  assert.deepEqual(getSwitchOverlayState("codex"), {
    cancellable: false,
    indeterminate: true,
    showSteps: false,
  });
  assert.deepEqual(getSwitchOverlayState("grok"), {
    cancellable: false,
    indeterminate: true,
    showSteps: false,
  });
});

test("shouldAutoOpenUsageGuide defaults to showing the guide", () => {
  assert.equal(shouldAutoOpenUsageGuide(null), true);
  assert.equal(shouldAutoOpenUsageGuide({}), true);
  assert.equal(
    shouldAutoOpenUsageGuide({ neverShowUsageGuide: false }),
    true
  );
});

test("getMobileBindingUiState prefers bound credentials over stale QR status", () => {
  const now = 1_700_000_000_000;
  assert.deepEqual(
    getMobileBindingUiState(
      {
        channel: "wechat",
        botToken: "token",
        status: "微信 iLink 已在线，等待手机消息",
        qrStatus: "微信二维码已失效，请重新生成",
        qrDeviceCode: "device-code",
        qrDataUrl: "data:image/png;base64,abc",
        qrStartedAt: String(now - 11 * 60 * 1000),
      },
      now
    ),
    { kind: "bound", hasCredential: true, showQr: false, busy: false }
  );
});

test("mobile debug sanitizer redacts nested credentials and URL query tickets", () => {
  assert.deepEqual(
    sanitizeMobileLogValue({
      binding: { appSecret: "secret", nested: [{ botToken: "token" }] },
      gateway: "https://gateway.example.test/ws?access_key=key&ticket=ticket",
    }),
    {
      binding: { appSecret: "***", nested: [{ botToken: "***" }] },
      gateway: "https://gateway.example.test/ws?access_key=***&ticket=***",
    }
  );
});

test("shouldAutoOpenUsageGuide respects never remind again", () => {
  assert.equal(
    shouldAutoOpenUsageGuide({ neverShowUsageGuide: true }),
    false
  );
});

test("getUpdateActionMode stays in check mode before or after no-update results", () => {
  assert.equal(getUpdateActionMode(null, false), "check");
  assert.equal(getUpdateActionMode({ hasUpdate: false }, false), "check");
});

test("getUpdateActionMode switches to release mode when a new version is available", () => {
  assert.equal(
    getUpdateActionMode({ hasUpdate: true, canAutoUpdate: true }, false),
    "release"
  );
});

test("getUpdateActionMode exposes busy state while checking updates", () => {
  assert.equal(getUpdateActionMode(null, true), "busy");
  assert.equal(
    getUpdateActionMode({ hasUpdate: true, canAutoUpdate: true }, true),
    "busy"
  );
});

test("formatVersionTag adds a v prefix only when needed", () => {
  assert.equal(formatVersionTag("1.2.3"), "v1.2.3");
  assert.equal(formatVersionTag("v1.2.3"), "v1.2.3");
  assert.equal(formatVersionTag(""), "");
});

test("getEditorPathMode distinguishes manual, detected, and default rows", () => {
  assert.equal(
    getEditorPathMode({ customized: true, detected: true }),
    "custom"
  );
  assert.equal(
    getEditorPathMode({ customized: false, detected: true }),
    "detected"
  );
  assert.equal(
    getEditorPathMode({ customized: false, detected: false }),
    "default"
  );
});

test("validateEditorPathInput rejects empty path drafts", () => {
  assert.deepEqual(validateEditorPathInput("   "), {
    valid: false,
    reason: "empty",
  });
  assert.deepEqual(validateEditorPathInput(" C:/Users/test/AppData/Code/User "), {
    valid: true,
    value: "C:/Users/test/AppData/Code/User",
  });
});

test("normalizeFetchedModels accepts strings and model objects", () => {
  assert.deepEqual(
    normalizeFetchedModels([
      { id: "gpt-5.5" },
      " claude-opus-4.8 ",
      { id: "gpt-5.5" },
      { name: "ignored" },
      "",
    ]),
    ["claude-opus-4.8", "gpt-5.5"]
  );
});

test("getCodexToolboxLayout keeps channel binding out of session sync", () => {
  assert.deepEqual(getCodexToolboxLayout("session"), {
    showSessionBindings: false,
    showRemoteBindings: false,
  });
  assert.equal(getCodexToolboxLayout("remote").showRemoteBindings, true);
});

test("CC Switch v1 deep link view masks secrets and exposes provider fields", () => {
  const view = getDeepLinkImportView({
    kind: "profile",
    app: "claude",
    source: "cc_switch_v1",
    conflict: { existingName: "Team", suggestedName: "Team (2)" },
    data: {
      name: "Team",
      apiKey: "sk-1234567890abcd",
      baseUrl: "https://api.example.com/",
      model: "claude-sonnet-4",
      haikuModel: "claude-haiku-4",
      sonnetModel: "claude-sonnet-4",
      opusModel: "claude-opus-4",
      homepage: "https://api.example.com",
    },
  });
  assert.equal(view.apiKeyMasked, "sk-123...abcd");
  assert.equal(view.showProviderDetails, true);
  assert.equal(view.showClaudeModels, true);
  assert.equal(view.showConflict, true);
  assert.equal(view.suggestedName, "Team (2)");
  assert.equal(view.defaultConflictAction, "rename");
});

test("deep link secret mask fully hides short API keys", () => {
  assert.equal(maskDeepLinkSecret("a"), "****");
  assert.equal(maskDeepLinkSecret("ab"), "****");
  assert.equal(maskDeepLinkSecret("sk-12345678"), "****");
});

test("legacy MCP deep link view keeps provider-only controls hidden", () => {
  const view = getDeepLinkImportView({
    kind: "mcp", app: "", source: "legacy",
    data: { name: "context7", config: { command: "npx" } },
  });
  assert.equal(view.showProviderDetails, false);
  assert.equal(view.showClaudeModels, false);
  assert.equal(view.showConflict, false);
});

test("deep link apply request permits only the visible v1 conflict choice", () => {
  const payload = {
    kind: "profile", app: "codex", source: "cc_switch_v1",
    conflict: {
      existingName: "Team",
      suggestedName: "Team (2)",
      confirmationToken: "server-token",
    },
    data: { name: "Team" },
  };
  assert.deepEqual(buildDeepLinkApplyRequest(payload, "overwrite"), {
    kind: "profile", app: "codex", data: { name: "Team" },
    source: "cc_switch_v1", conflictAction: "overwrite", conflictToken: "server-token",
  });
  const rename = buildDeepLinkApplyRequest(payload, "invalid");
  assert.equal(rename.conflictAction, "rename");
  assert.equal(Object.hasOwn(rename, "conflictToken"), false);
});

test("deep link confirmation dialog exposes v1 fields and an explicit conflict choice", () => {
  const html = fs.readFileSync(require.resolve("./index.html"), "utf8");
  for (const id of [
    "deeplinkModelRow", "deeplinkHaikuModelRow", "deeplinkSonnetModelRow",
    "deeplinkOpusModelRow", "deeplinkHomepageRow", "deeplinkConflictGroup",
  ]) {
    assert.match(html, new RegExp(`id="${id}"`));
  }
  assert.match(html, /name="deeplinkConflictAction"[^>]*value="rename"[^>]*checked/);
  assert.match(html, /name="deeplinkConflictAction"[^>]*value="overwrite"/);
});

test("initially hidden deep link conflict fieldset is not rendered by project CSS", () => {
  const html = fs.readFileSync(require.resolve("./index.html"), "utf8");
  const css = readStylesheets();
  const fieldset = html.match(/<fieldset\b[^>]*id="deeplinkConflictGroup"[^>]*>/)?.[0] || "";
  assert.match(fieldset, /\bhidden\b/);

  const hiddenRule = css.match(/\.deeplink-conflict\[hidden\]\s*\{([^}]*)\}/)?.[1] || "";
  assert.match(hiddenRule, /\bdisplay\s*:\s*none\s*;/);
});

test("shouldRenderChannelQr rejects stale QQ text QR cache", () => {
  const now = 1_700_000_000_000;
  assert.equal(
    shouldRenderChannelQr(
      {
        channel: "qq",
        qrUrl: "raw-internal-token",
        qrDataUrl: "data:image/png;base64,abc",
        qrStartedAt: String(now),
      },
      now
    ),
    false
  );
});

test("shouldRenderChannelQr accepts fresh platform QR images only", () => {
  const now = 1_700_000_000_000;
  assert.equal(
    shouldRenderChannelQr(
      {
        channel: "qq",
        qrUrl: "https://bots.qq.com/connect/abc",
        qrDataUrl: "data:image/png;base64,abc",
        qrStartedAt: String(now),
      },
      now
    ),
    true
  );
  assert.equal(
    shouldRenderChannelQr(
      {
        channel: "wechat",
        qrDeviceCode: "device-code",
        qrDataUrl: "data:image/svg+xml;base64,abc",
        qrStartedAt: String(now),
      },
      now
    ),
    false
  );
  assert.equal(
    shouldRenderChannelQr(
      {
        channel: "wechat",
        qrDeviceCode: "device-code",
        qrDataUrl: "data:image/png;base64,abc",
        qrStartedAt: String(now),
      },
      now
    ),
    true
  );
});

// 2026-08-12 09:07:00（本地时间，测试与运行环境时区无关）
const USAGE_NOW_MS = new Date(2026, 7, 12, 9, 7, 0).getTime();
const localTs = (y, m, d, h = 0, mi = 0, s = 0) =>
  Math.floor(new Date(y, m, d, h, mi, s).getTime() / 1000);

test("resolveUsageRange today covers the local calendar day, not a rolling window", () => {
  const range = resolveUsageRange("today", { nowMs: USAGE_NOW_MS });
  // 凌晨 00:00:00 起
  assert.equal(range.startTs, localTs(2026, 7, 12));
  // 当天 23:59:59 止（次日零点前一秒）
  assert.equal(range.endTs, localTs(2026, 7, 13) - 1);
});

test("resolveUsageRange 1d is a rolling 24h window ending now", () => {
  const nowSec = Math.floor(USAGE_NOW_MS / 1000);
  const range = resolveUsageRange("1d", { nowMs: USAGE_NOW_MS });
  assert.equal(range.startTs, nowSec - 86400);
  assert.equal(range.endTs, nowSec);
  // 与“今天”不是同一概念：起点应为昨天 09:07，早于今天零点
  const today = resolveUsageRange("today", { nowMs: USAGE_NOW_MS });
  assert.ok(range.startTs < today.startTs);
});

test("resolveUsageRange 7d starts at local midnight 6 days ago and ends now", () => {
  const range = resolveUsageRange("7d", { nowMs: USAGE_NOW_MS });
  assert.equal(range.startTs, localTs(2026, 7, 6));
  assert.equal(range.endTs, Math.floor(USAGE_NOW_MS / 1000));
});

test("resolveUsageRange custom defaults to last 24h and honors live end", () => {
  const nowSec = Math.floor(USAGE_NOW_MS / 1000);
  assert.deepEqual(
    resolveUsageRange("custom", {
      nowMs: USAGE_NOW_MS,
      customStartTs: null,
      customEndTs: null,
      customLiveEnd: true,
    }),
    { startTs: nowSec - 86400, endTs: nowSec }
  );
  assert.deepEqual(
    resolveUsageRange("custom", {
      nowMs: USAGE_NOW_MS,
      customStartTs: 100,
      customEndTs: 200,
      customLiveEnd: false,
    }),
    { startTs: 100, endTs: 200 }
  );
  assert.deepEqual(resolveUsageRange("all", { nowMs: USAGE_NOW_MS }), {
    startTs: null,
    endTs: null,
  });
});

// 手机控制的桥接脚本以 Rust 原始字符串内嵌在 codex_toolbox.rs 里，rustc 不会检查其中的
// JS 语法。曾有一次批量替换把模板里的 `const x =` 一并改成 `pub(crate) const x =`，
// 编译照样通过，运行时飞书/QQ 桥接进程却启动即崩溃，看门狗每 30 秒重启一次。
test("embedded mobile bridge JS templates stay valid JavaScript", () => {
  const os = require("node:os");
  const { execFileSync } = require("node:child_process");
  const source = fs.readFileSync(
    path.join(__dirname, "..", "src-tauri", "src", "codex_toolbox.rs"),
    "utf8"
  );
  const templates = ["qq_qr_runner_text", "qq_gateway_runner_text", "lark_bridge_runner_text"];
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "varswitch-runner-"));
  try {
    for (const name of templates) {
      const start = source.indexOf(`fn ${name}()`);
      assert.ok(start >= 0, `${name} 未找到`);
      const open = source.indexOf('r#"', start);
      const close = source.indexOf('"#', open + 3);
      assert.ok(open >= 0 && close > open, `${name} 的原始字符串边界解析失败`);
      const script = source.slice(open + 3, close);
      assert.ok(!script.includes("pub(crate)"), `${name} 里混入了 Rust 语法`);
      const file = path.join(tmpDir, `${name}.mjs`);
      fs.writeFileSync(file, script, "utf8");
      execFileSync(process.execPath, ["--check", file], { stdio: "pipe" });
    }
  } finally {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
});
