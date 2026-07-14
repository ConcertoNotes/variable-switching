const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");

const {
  shouldAutoOpenUsageGuide,
  getUpdateActionMode,
  formatVersionTag,
  getEditorPathMode,
  validateEditorPathInput,
  normalizeFetchedModels,
  normalizeCodexPluginMarketplaceInput,
  getCodexPluginMarketplaceOption,
  getCodexToolboxLayout,
  shouldRenderChannelQr,
  getMobileBindingUiState,
  getSwitchOverlayState,
  matchesPluginFilter,
  getCodexSessionMetrics,
  getGlobalConfigDisplay,
  CODEX_PLUGIN_MARKETPLACE_URL,
  VARSWITCH_GITHUB_PLUGIN_MARKETPLACE_URL,
  AWESOME_CODEX_PLUGIN_MARKETPLACE_URL,
  CODEX_PLUGIN_MARKETPLACES,
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
    "overview",
    "claude",
    "codex",
    "grok",
    "configurations",
    "plugins",
    "sessions",
    "mobile",
    "settings",
  ]);
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
  assert.match(css, /#codexStatusGrid,\s*#grokStatusGrid\s*\{[^}]*grid-template-columns:\s*1fr;/s);
  assert.match(
    css,
    /#codexStatusGrid\s+\.status-card,\s*#grokStatusGrid\s+\.status-card\s*\{[^}]*repeat\(auto-fit,\s*minmax\(/s
  );
  assert.match(
    css,
    /#codexStatusGrid\s+\.status-value,\s*#grokStatusGrid\s+\.status-value\s*\{[^}]*text-overflow:\s*ellipsis;/s
  );
});

test("Grok and Codex runtime status share the same responsive layout", () => {
  const css = fs.readFileSync(require.resolve("./style.css"), "utf8");
  assert.match(css, /#codexStatusGrid,\s*#grokStatusGrid\s*\{[^}]*grid-template-columns:\s*1fr;/s);
  assert.match(
    css,
    /#codexStatusGrid\s+\.status-card,\s*#grokStatusGrid\s+\.status-card\s*\{[^}]*repeat\(auto-fit,\s*minmax\(/s
  );
});

test("configuration rows use container-responsive content and action groups", () => {
  const css = fs.readFileSync(require.resolve("./style.css"), "utf8");
  const app = fs.readFileSync(require.resolve("./app.js"), "utf8");
  assert.match(css, /\.configuration-list\s*\{[^}]*container-type:\s*inline-size;/s);
  assert.match(css, /\.configuration-data\s*\{[^}]*display:\s*grid;/s);
  assert.match(css, /@container\s*\(max-width:\s*720px\)/s);
  assert.match(app, /class="configuration-data"/);
});

test("configuration actions keep stable column slots across active states", () => {
  const css = fs.readFileSync(require.resolve("./style.css"), "utf8");
  const app = fs.readFileSync(require.resolve("./app.js"), "utf8");
  assert.match(css, /\.configuration-row-overview\s*\{[^}]*grid-template-columns:\s*minmax\(0,\s*1fr\)\s+132px;/s);
  assert.match(css, /\.configuration-row-manager\s*\{[^}]*grid-template-columns:\s*minmax\(0,\s*1fr\)\s+252px;/s);
  assert.match(css, /\.configuration-row-manager\s+\.configuration-actions\s*\{[^}]*grid-template-columns:\s*52px\s+44px\s+70px\s+68px;/s);
  assert.ok((app.match(/configuration-action-slot/g) || []).length >= 2);
});

test("plugin marketplace install action stays beside the page title", () => {
  const html = fs.readFileSync(require.resolve("./index.html"), "utf8");
  const pageStart = html.indexOf('id="pagePlugins"');
  const hostStart = html.indexOf('id="pluginPageHost"', pageStart);
  const pageHeader = html.slice(pageStart, hostStart);
  const marketplaceContentStart = html.indexOf('id="toolboxMarketContent"');
  const marketplaceContentEnd = html.indexOf('id="toolboxSessionContent"', marketplaceContentStart);
  const marketplaceContent = html.slice(marketplaceContentStart, marketplaceContentEnd);

  assert.match(pageHeader, /id="toolboxMarketApplyBtn"/);
  assert.doesNotMatch(marketplaceContent, /id="toolboxMarketApplyBtn"/);
});

test("overview status metrics use all five desktop columns", () => {
  const css = fs.readFileSync(require.resolve("./style.css"), "utf8");
  assert.match(
    css,
    /\.overview-metrics\s*\{[^}]*grid-template-columns:\s*repeat\(5,\s*minmax\(0,\s*1fr\)\);/s
  );
});

test("compact navigation accounts for every primary destination", () => {
  const css = fs.readFileSync(require.resolve("./style.css"), "utf8");
  assert.match(
    css,
    /@media\s*\(max-width:\s*760px\)[\s\S]*?\.sidebar-nav\s*\{[^}]*grid-template-columns:\s*repeat\(8,\s*minmax\(76px,\s*1fr\)\);/s
  );
});

test("console provides keyboard focus and reduced-motion affordances", () => {
  const css = fs.readFileSync(require.resolve("./style.css"), "utf8");
  assert.match(css, /:where\([^)]*button[^)]*\):focus-visible\s*\{/s);
  assert.match(css, /@media\s*\(prefers-reduced-motion:\s*reduce\)\s*\{/s);
});

test("global toolbar and page sections do not duplicate primary config actions", () => {
  const html = fs.readFileSync(require.resolve("./index.html"), "utf8");
  assert.doesNotMatch(html, /id="(?:importBtn|addBtn|settingsBtn)"/);
  assert.doesNotMatch(
    html,
    /id="(?:syncNowBtn|codexSyncNowBtn|toolboxSessionSyncBtn)"/
  );
  assert.match(html, /id="(?:claudeSyncBtn|codexCardSyncBtn|sessionPageSyncBtn)"/);
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
  assert.equal(getGlobalConfigDisplay("configurations", state, "zh").name, "17 套配置");
});

test("matchesPluginFilter combines text search with plugin state", () => {
  const plugin = {
    text: "Chrome browser 已启用",
    enabled: true,
    installed: true,
    needsRepair: false,
  };
  assert.equal(matchesPluginFilter(plugin, "enabled", "chrome"), true);
  assert.equal(matchesPluginFilter(plugin, "repair", "chrome"), false);
  assert.equal(matchesPluginFilter(plugin, "installed", "missing"), false);
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

test("normalizeCodexPluginMarketplaceInput falls back to the VarSwitch GitCode marketplace", () => {
  assert.equal(normalizeCodexPluginMarketplaceInput(""), CODEX_PLUGIN_MARKETPLACE_URL);
  assert.equal(
    normalizeCodexPluginMarketplaceInput(" https://gitcode.com/2301_79703673/codex-plugins.git "),
    CODEX_PLUGIN_MARKETPLACE_URL
  );
});

test("normalizeCodexPluginMarketplaceInput supports alternate plugin marketplaces", () => {
  assert.equal(
    normalizeCodexPluginMarketplaceInput("git@github.com:ConcertoNotes/codex-plugins.git"),
    VARSWITCH_GITHUB_PLUGIN_MARKETPLACE_URL
  );
  assert.equal(
    normalizeCodexPluginMarketplaceInput("git@github.com:hashgraph-online/awesome-codex-plugins.git"),
    AWESOME_CODEX_PLUGIN_MARKETPLACE_URL
  );
  assert.equal(CODEX_PLUGIN_MARKETPLACES.length, 4);
  assert.equal(
    getCodexPluginMarketplaceOption(AWESOME_CODEX_PLUGIN_MARKETPLACE_URL).id,
    "awesome"
  );
});

test("getCodexToolboxLayout keeps channel binding out of session sync", () => {
  assert.deepEqual(getCodexToolboxLayout("session"), {
    showMarketplaceList: false,
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
