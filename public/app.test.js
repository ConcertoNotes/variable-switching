const test = require("node:test");
const assert = require("node:assert/strict");

const {
  shouldAutoOpenUsageGuide,
  getUpdateActionMode,
  formatVersionTag,
  getEditorPathMode,
  validateEditorPathInput,
  normalizeCodexPluginMarketplaceInput,
  getCodexPluginMarketplaceOption,
  getCodexToolboxLayout,
  shouldRenderChannelQr,
  getMobileBindingUiState,
  CODEX_PLUGIN_MARKETPLACE_URL,
  VARSWITCH_GITHUB_PLUGIN_MARKETPLACE_URL,
  AWESOME_CODEX_PLUGIN_MARKETPLACE_URL,
  CODEX_PLUGIN_MARKETPLACES,
} = require("./app-helpers.js");

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
