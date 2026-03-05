(function (root, factory) {
  const api = factory();
  if (typeof module !== "undefined" && module.exports) {
    module.exports = api;
  }
  root.VarSwitchHelpers = api;
})(typeof globalThis !== "undefined" ? globalThis : this, function () {
  function shouldAutoOpenUsageGuide(settings) {
    return !settings || settings.neverShowUsageGuide !== true;
  }

  function getUpdateActionMode(updateInfo, isBusy) {
    if (isBusy) {
      return "busy";
    }
    if (updateInfo && updateInfo.hasUpdate) {
      return "release";
    }
    return "check";
  }

  function formatVersionTag(version) {
    if (!version) {
      return "";
    }
    return version.startsWith("v") ? version : `v${version}`;
  }

  function getEditorPathMode(editorInfo) {
    if (editorInfo && editorInfo.customized) {
      return "custom";
    }
    if (editorInfo && editorInfo.detected) {
      return "detected";
    }
    return "default";
  }

  function validateEditorPathInput(value) {
    const normalized = typeof value === "string" ? value.trim() : "";
    if (!normalized) {
      return { valid: false, reason: "empty" };
    }
    return { valid: true, value: normalized };
  }

  return {
    shouldAutoOpenUsageGuide,
    getUpdateActionMode,
    formatVersionTag,
    getEditorPathMode,
    validateEditorPathInput,
  };
});
