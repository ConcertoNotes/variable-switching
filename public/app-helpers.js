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
    "https://gitcode.com/2301_79703673/codex-plugins.git";
  const VARSWITCH_GITHUB_PLUGIN_MARKETPLACE_URL =
    "https://github.com/ConcertoNotes/codex-plugins.git";
  const UPSTREAM_GITCODE_PLUGIN_MARKETPLACE_URL =
    "https://gitcode.com/weixin_65003717/codex-plugin.git";
  const AWESOME_CODEX_PLUGIN_MARKETPLACE_URL =
    "https://github.com/hashgraph-online/awesome-codex-plugins.git";
  const CODEX_PLUGIN_MARKETPLACES = [
    {
      id: "varswitch-gitcode",
      name: "VarSwitch 插件合集（GitCode）",
      url: CODEX_PLUGIN_MARKETPLACE_URL,
      count: 316,
      zh: "你的默认插件市场，融合官方常用插件和社区工作流插件，国内访问更稳。",
      en: "Your default marketplace, combining common service plugins and community workflow plugins; better access in China.",
    },
    {
      id: "varswitch-github",
      name: "VarSwitch 插件合集（GitHub）",
      url: VARSWITCH_GITHUB_PLUGIN_MARKETPLACE_URL,
      count: 316,
      zh: "同一套 VarSwitch 插件市场的 GitHub 版本，适合作为海外或备用源。",
      en: "The GitHub mirror of the same VarSwitch marketplace; useful as an overseas or fallback source.",
    },
    {
      id: "upstream-gitcode",
      name: "上游常用插件合集",
      url: UPSTREAM_GITCODE_PLUGIN_MARKETPLACE_URL,
      count: 189,
      zh: "常用服务和桌面能力插件来源，保留为参考/备选源。",
      en: "Source collection for common service and desktop plugins, kept as a reference or fallback.",
    },
    {
      id: "awesome",
      name: "Awesome Codex Plugins",
      url: AWESOME_CODEX_PLUGIN_MARKETPLACE_URL,
      count: 128,
      zh: "偏社区开发工作流和工具扩展，适合补充代码审查、记忆、自动化类插件。",
      en: "Community workflow and tool extensions; useful for review, memory, and automation plugins.",
    },
  ];

  function normalizeCodexPluginMarketplaceInput(value) {
    const normalized = typeof value === "string" ? value.trim() : "";
    if (normalized === "git@github.com:ConcertoNotes/codex-plugins.git") {
      return VARSWITCH_GITHUB_PLUGIN_MARKETPLACE_URL;
    }
    if (normalized === "git@github.com:hashgraph-online/awesome-codex-plugins.git") {
      return AWESOME_CODEX_PLUGIN_MARKETPLACE_URL;
    }
    if (CODEX_PLUGIN_MARKETPLACES.some((item) => item.url === normalized)) {
      return normalized;
    }
    return normalized || CODEX_PLUGIN_MARKETPLACE_URL;
  }

  function getCodexPluginMarketplaceOption(value) {
    const normalized = normalizeCodexPluginMarketplaceInput(value);
    return (
      CODEX_PLUGIN_MARKETPLACES.find((item) => item.url === normalized) ||
      CODEX_PLUGIN_MARKETPLACES[0]
    );
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
    VARSWITCH_GITHUB_PLUGIN_MARKETPLACE_URL,
    UPSTREAM_GITCODE_PLUGIN_MARKETPLACE_URL,
    AWESOME_CODEX_PLUGIN_MARKETPLACE_URL,
    CODEX_PLUGIN_MARKETPLACES,
    shouldAutoOpenUsageGuide,
    getUpdateActionMode,
    formatVersionTag,
    getEditorPathMode,
    validateEditorPathInput,
    normalizeCodexPluginMarketplaceInput,
    getCodexPluginMarketplaceOption,
    getCodexToolboxLayout,
    isFreshMobileQr,
    isQqAuthorizationTarget,
    shouldRenderChannelQr,
  };
});
