const test = require("node:test");
const assert = require("node:assert/strict");

const {
  shouldAutoOpenUsageGuide,
  getUpdateActionMode,
  formatVersionTag,
  getEditorPathMode,
  validateEditorPathInput,
} = require("./app-helpers.js");

test("shouldAutoOpenUsageGuide defaults to showing the guide", () => {
  assert.equal(shouldAutoOpenUsageGuide(null), true);
  assert.equal(shouldAutoOpenUsageGuide({}), true);
  assert.equal(
    shouldAutoOpenUsageGuide({ neverShowUsageGuide: false }),
    true
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
