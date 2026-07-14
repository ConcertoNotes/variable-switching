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

  function getSwitchOverlayState(kind) {
    // Claude 切换有多步进度；Codex / Grok 为环境写入型，使用不确定进度条
    const isClaude = kind === "claude";
    return {
      cancellable: isClaude,
      indeterminate: !isClaude,
      showSteps: isClaude,
    };
  }

  function validateEditorPathInput(value) {
    const normalized = typeof value === "string" ? value.trim() : "";
    if (!normalized) {
      return { valid: false, reason: "empty" };
    }
    return { valid: true, value: normalized };
  }

  function normalizeFetchedModels(models) {
    if (!Array.isArray(models)) return [];
    const seen = new Set();
    return models
      .map((item) => {
        const id =
          typeof item === "string"
            ? item
            : item && typeof item.id === "string"
              ? item.id
              : "";
        return id.trim();
      })
      .filter((id) => {
        if (!id || seen.has(id)) return false;
        seen.add(id);
        return true;
      })
      .sort((a, b) => a.localeCompare(b));
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

  function getMobileBindingUiState(binding, now = Date.now()) {
    const channel = binding && binding.channel ? binding.channel : "";
    const hasCredential = Boolean(
      binding &&
        (binding.bound ||
          binding.botToken ||
          binding.appId ||
          binding.appSecret ||
          binding.accountId ||
          binding.userId ||
          binding.botOpenId)
    );
    const statusText = String(
      (binding && (binding.status || binding.qrStatus || binding.credentialStatus)) || ""
    ).trim();
    const hasQr = Boolean(binding && (binding.qrDataUrl || binding.qrUrl));
    const qrFresh = hasQr && isFreshMobileQr(binding, now);
    const looksExpired = /过期|失效|expired|timeout|超时/i.test(statusText);
    const looksBusy = /正在|生成|获取|等待扫码|connecting|loading|pending/i.test(statusText);
    const looksOnline = /在线|已绑定|listening|running|connected|ready|saved/i.test(statusText);

    if (binding && binding.lastError) {
      return { kind: "error", hasCredential, showQr: false, busy: false };
    }
    if (hasCredential && (looksOnline || !hasQr || looksExpired || !qrFresh)) {
      return { kind: "bound", hasCredential, showQr: false, busy: false };
    }
    if (hasQr && qrFresh && shouldRenderChannelQr(binding, now)) {
      return { kind: "qr", hasCredential, showQr: true, busy: false };
    }
    if (looksBusy || (hasQr && !qrFresh)) {
      return { kind: "binding", hasCredential, showQr: false, busy: true };
    }
    return { kind: hasCredential ? "bound" : "unbound", hasCredential, showQr: false, busy: false, channel };
  }

  function matchesPluginFilter(plugin, filter = "all", query = "") {
    const text = String(plugin?.text || "").toLowerCase();
    const normalizedQuery = String(query || "").trim().toLowerCase();
    if (normalizedQuery && !text.includes(normalizedQuery)) return false;
    if (filter === "enabled") return !!plugin?.enabled;
    if (filter === "repair") return !!plugin?.needsRepair;
    if (filter === "installed") return !!plugin?.installed;
    return true;
  }

  function getCodexSessionMetrics(toolbox) {
    const source = toolbox || {};
    const sync = source.sessionSync || {};
    const synced = source.syncedCodexThreads || source.syncedThreads || source.sessions || [];
    const trashed = source.trashedCodexThreads || source.trashedThreads || [];
    const syncedLength = Array.isArray(synced) ? synced.length : 0;
    const trashCount = Array.isArray(trashed) ? trashed.length : 0;
    const explicitCount = Number(sync.total ?? source.syncedCount);
    const count = Number.isFinite(explicitCount) && (explicitCount > 0 || syncedLength === 0)
      ? explicitCount
      : syncedLength;
    return {
      count,
      trashCount,
      lastSyncedAt: sync.lastSyncedAt || source.lastSyncedAt || "",
    };
  }

  function getGlobalConfigDisplay(page, state, language = "zh") {
    const current = state || {};
    const isZh = language === "zh";
    const inactive = isZh ? "未启用" : "Inactive";
    const entries = [
      ["Claude", current.claude],
      ["Codex", current.codex],
      ["Grok", current.grok],
    ].filter(([, name]) => Boolean(name));
    const title = entries.length
      ? entries.map(([type, name]) => `${type}: ${name}`).join(" · ")
      : (isZh ? "尚未启用配置" : "No active configuration");

    if (page === "claude") return { label: "Claude", name: current.claude || inactive, title };
    if (["codex", "plugins", "sessions", "mobile"].includes(page)) {
      return { label: "Codex", name: current.codex || inactive, title };
    }
    if (page === "grok") return { label: "Grok", name: current.grok || inactive, title };
    if (page === "configurations") {
      const total = Number(current.total) || 0;
      return {
        label: isZh ? "配置" : "Configs",
        name: isZh ? `${total} 套配置` : `${total} configs`,
        title,
      };
    }
    return {
      label: isZh ? "环境" : "Environments",
      name: isZh ? `${entries.length} 个已启用` : `${entries.length} active`,
      title,
    };
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
    getSwitchOverlayState,
    validateEditorPathInput,
    normalizeFetchedModels,
    normalizeCodexPluginMarketplaceInput,
    getCodexPluginMarketplaceOption,
    getCodexToolboxLayout,
    isFreshMobileQr,
    isQqAuthorizationTarget,
    shouldRenderChannelQr,
    getMobileBindingUiState,
    matchesPluginFilter,
    getCodexSessionMetrics,
    getGlobalConfigDisplay,
  };
});
