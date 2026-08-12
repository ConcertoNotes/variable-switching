const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");

const {
  shouldAutoOpenUsageGuide,
  getUpdateActionMode,
  formatVersionTag,
  getEditorPathMode,
  validateEditorPathInput,
  resolveUniversalAppBaseUrl,
  normalizeFetchedModels,
  getCodexToolboxLayout,
  shouldRenderChannelQr,
  getMobileBindingUiState,
  getSwitchOverlayState,
  getCodexSessionMetrics,
  getGlobalConfigDisplay,
  sanitizeMobileLogValue,
  resolveUsageRange,
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
    "codex",
    "grok",
    "gemini",
    "add-provider",
    "toolbox",
    "developer-tools",
    "usage",
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
  const css = fs
    .readFileSync(require.resolve("./style.css"), "utf8")
    .replace(/\/\*[\s\S]*?\*\//g, "");
  let depth = 0;
  let minimumDepth = 0;
  for (const character of css) {
    if (character === "{") depth += 1;
    if (character === "}") depth -= 1;
    minimumDepth = Math.min(minimumDepth, depth);
  }
  assert.equal(minimumDepth, 0);
  assert.equal(depth, 0);
});

test("settings groups stay independent and keep path actions compact", () => {
  const html = fs.readFileSync(require.resolve("./index.html"), "utf8");
  const css = fs.readFileSync(require.resolve("./style.css"), "utf8");
  const app = fs.readFileSync(require.resolve("./app.js"), "utf8");

  assert.equal((html.match(/class="settings-group-items"/g) || []).length, 3);
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
  const css = fs.readFileSync(require.resolve("./style.css"), "utf8");
  assert.match(
    css,
    /\.codex-wizard-modal\s+#codexProfileForm\s*\{[^}]*overflow-y:\s*auto;/s
  );
  assert.match(
    css,
    /\.codex-wizard-modal\s*\{[^}]*max-height:\s*calc\(100vh\s*-\s*48px\);/s
  );
});

test("Codex runtime status uses a full-width horizontal layout", () => {
  const css = fs.readFileSync(require.resolve("./style.css"), "utf8");
  assert.match(css, /#codexStatusGrid,\s*#grokStatusGrid,\s*#geminiStatusGrid\s*\{[^}]*grid-template-columns:\s*1fr;/s);
  assert.match(
    css,
    /#codexStatusGrid\s+\.status-card,\s*#grokStatusGrid\s+\.status-card,\s*#geminiStatusGrid\s+\.status-card\s*\{[^}]*repeat\(auto-fit,\s*minmax\(/s
  );
  assert.match(
    css,
    /#codexStatusGrid\s+\.status-value,\s*#grokStatusGrid\s+\.status-value,\s*#geminiStatusGrid\s+\.status-value\s*\{[^}]*text-overflow:\s*ellipsis;/s
  );
});

test("Grok and Codex runtime status share the same responsive layout", () => {
  const css = fs.readFileSync(require.resolve("./style.css"), "utf8");
  assert.match(css, /#codexStatusGrid,\s*#grokStatusGrid,\s*#geminiStatusGrid\s*\{[^}]*grid-template-columns:\s*1fr;/s);
  assert.match(
    css,
    /#codexStatusGrid\s+\.status-card,\s*#grokStatusGrid\s+\.status-card,\s*#geminiStatusGrid\s+\.status-card\s*\{[^}]*repeat\(auto-fit,\s*minmax\(/s
  );
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
  assert.match(submit, /codexEnableAfterSave\s*\|\|\s*editingActiveProfile/);
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

test("Claude supports OpenAI-format upstreams via the local conversion proxy", () => {
  const html = fs.readFileSync(require.resolve("./index.html"), "utf8");
  const app = fs.readFileSync(require.resolve("./app.js"), "utf8");
  const rust = fs.readFileSync(require.resolve("../src-tauri/src/lib.rs"), "utf8");
  const proxy = fs.readFileSync(require.resolve("../src-tauri/src/claude_proxy.rs"), "utf8");
  const cargo = fs.readFileSync(require.resolve("../src-tauri/Cargo.toml"), "utf8");

  // Claude 弹窗与统一供应商页都能选 API 格式
  assert.match(html, /id="profileApiFormat"/);
  assert.match(html, /id="profileApiFormat"[\s\S]{0,240}value="anthropic"[\s\S]{0,240}value="openai_chat"/);
  assert.match(html, /id="universalClaudeApiFormat"/);

  // 前端把格式传给后端，并按格式选择验证协议
  assert.match(app, /apiFormat:\s*claudeApiFormat/);
  assert.match(app, /invoke\("update_profile", \{ id: editingId, name, apiKey, baseUrl, modelId, apiFormat \}\)/);
  assert.match(app, /invoke\("add_profile", \{ name, apiKey, baseUrl, modelId: modelId \|\| null, apiFormat \}\)/);
  assert.match(app, /function productProtocol\(kind\)/);

  // Rust：Profile 结构、代理模块、切换链路、状态命令
  assert.match(cargo, /tiny_http\s*=/);
  assert.match(rust, /mod claude_proxy;/);
  assert.match(rust, /fn normalize_claude_api_format/);
  assert.match(rust, /api_format:\s*String,/);
  assert.match(rust, /claude_proxy::ensure_server\(\)\?/);
  assert.match(rust, /apply_auth_to_system_env\(&profile\.api_key, &effective_base_url\)/);
  assert.match(rust, /get_claude_proxy_status,/);
  // 启动恢复：激活的 openai_chat 配置在重启后重建代理
  assert.match(rust, /p\.is_active && p\.api_format == "openai_chat"/);

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
  const css = fs.readFileSync(require.resolve("./style.css"), "utf8");
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
  const rust = fs.readFileSync(require.resolve("../src-tauri/src/lib.rs"), "utf8");

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
    assert.match(rust, new RegExp(`\\n\\s*${command},`));
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
  const css = fs.readFileSync(require.resolve("./style.css"), "utf8");
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
  assert.match(app, /function mountDeveloperToolsPage\(\)/);
  assert.match(app, /openDeveloperTools\("skills"\)/);
});

test("compact navigation accounts for every primary destination", () => {
  const css = fs.readFileSync(require.resolve("./style.css"), "utf8");
  assert.match(
    css,
    /@media\s*\(max-width:\s*760px\)[\s\S]*?\.sidebar-nav\s*\{[^}]*grid-auto-columns:\s*minmax\(76px,\s*1fr\);/s
  );
});

test("console provides keyboard focus and reduced-motion affordances", () => {
  const css = fs.readFileSync(require.resolve("./style.css"), "utf8");
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

  const css = fs.readFileSync(require.resolve("./style.css"), "utf8");
  assert.match(css, /#statusGrid,\s*#codexStatusGrid,\s*#grokStatusGrid,\s*#geminiStatusGrid\s*\{[^}]*grid-template-columns:\s*1fr;/s);
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
  const css = fs.readFileSync(require.resolve("./style.css"), "utf8");
  const app = fs.readFileSync(require.resolve("./app.js"), "utf8");
  const renderStart = app.indexOf("function renderProfiles()");
  const renderEnd = app.indexOf("function updateActiveConfigBar()", renderStart);
  const renderProfiles = app.slice(renderStart, renderEnd);

  assert.match(renderProfiles, /class="btn btn-danger btn-sm"/);
  assert.doesNotMatch(renderProfiles, /profile-delete-btn/);
  assert.match(css, /\.profile-actions\s+\.btn\s*\{[^}]*flex:\s*1;/s);
  assert.doesNotMatch(css, /#pageClaudeCode\s+\.profile-actions/);
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
