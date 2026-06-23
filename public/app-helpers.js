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

  const CODEX_PLUGIN_MARKETPLACE_URL =
    "https://gitcode.com/weixin_65003717/codex-plugin.git";

  function normalizeCodexPluginMarketplaceInput(value) {
    const normalized = typeof value === "string" ? value.trim() : "";
    return normalized || CODEX_PLUGIN_MARKETPLACE_URL;
  }

  function getCodexToolboxLayout(tab) {
    if (tab === "market") {
      return {
        showMarketplaceList: false,
        showSessionBindings: false,
        showRemoteBindings: false,
      };
    }
    if (tab === "session") {
      return {
        showMarketplaceList: false,
        showSessionBindings: false,
        showRemoteBindings: false,
      };
    }
    return {
      showMarketplaceList: false,
      showSessionBindings: false,
      showRemoteBindings: true,
    };
  }

  const MOBILE_QR_CACHE_TTL_MS = 10 * 60 * 1000;

  function isFreshMobileQr(binding, now = Date.now()) {
    const startedAt = Number(binding && binding.qrStartedAt ? binding.qrStartedAt : 0);
    if (!Number.isFinite(startedAt) || startedAt <= 0) return false;
    return now - startedAt <= MOBILE_QR_CACHE_TTL_MS;
  }

  function isQqAuthorizationTarget(value) {
    return /^(https?:\/\/|mqqapi:\/\/|qqbot:\/\/)/i.test(String(value || "").trim());
  }

  function shouldRenderChannelQr(binding, now = Date.now()) {
    if (!binding || (!binding.qrDataUrl && !binding.qrUrl)) return false;
    if (binding.channel === "lark") return Boolean(binding.qrDataUrl || binding.qrUrl);
    if (binding.channel === "qq") {
      return Boolean(binding.qrDataUrl && isFreshMobileQr(binding, now) && isQqAuthorizationTarget(binding.qrUrl));
    }
    if (binding.channel === "wechat") {
      const dataUrl = String(binding.qrDataUrl || "").trim().toLowerCase();
      return Boolean(
        binding.qrDataUrl &&
        binding.qrDeviceCode &&
        isFreshMobileQr(binding, now) &&
        !dataUrl.startsWith("data:image/svg+xml")
      );
    }
    return Boolean(binding.qrDataUrl);
  }

  return {
    CODEX_PLUGIN_MARKETPLACE_URL,
    shouldAutoOpenUsageGuide,
    getUpdateActionMode,
    formatVersionTag,
    getEditorPathMode,
    validateEditorPathInput,
    normalizeCodexPluginMarketplaceInput,
    getCodexToolboxLayout,
    isFreshMobileQr,
    isQqAuthorizationTarget,
    shouldRenderChannelQr,
  };
});
