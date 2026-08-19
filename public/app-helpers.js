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

  // 统一供应商同一个网关地址在不同应用间的落地形式：
  // Claude / Gemini 使用根地址（客户端自行拼 /v1/messages 等路径），
  // Codex / Grok 遵循 OpenAI 兼容惯例要求以 /v1 结尾，未带版本段时自动补全。
  function resolveUniversalAppBaseUrl(app, baseUrl) {
    const trimmed = String(baseUrl || "").trim().replace(/\/+$/, "");
    if (!trimmed) return "";
    if (app !== "codex" && app !== "grok") return trimmed;
    if (/\/v\d+[a-z]*$/i.test(trimmed)) return trimmed;
    return `${trimmed}/v1`;
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

  function isDeepseekCodexConfig(providerName, baseUrl) {
    if (String(providerName || "").trim().toLowerCase() !== "deepseek") return false;
    try {
      const url = new URL(String(baseUrl || "").trim());
      return (url.protocol === "https:" || url.protocol === "http:")
        && url.hostname.toLowerCase() === "api.deepseek.com";
    } catch (_) {
      return false;
    }
  }

  function getCodexToolboxLayout(tab) {
    if (tab === "session") {
      return {
        showSessionBindings: false,
        showRemoteBindings: false,
      };
    }
    return {
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

  function sanitizeMobileLogValue(value) {
    const secretKeys = new Set([
      "appSecret", "app_secret", "botToken", "bot_token", "apiKey", "api_key",
      "accessKey", "ticket", "authorization",
    ]);
    if (typeof value === "string") {
      return value
        .replace(/([?&](?:access_key|ticket|token|secret|authorization|app_secret|bot_token|api[_-]?key)=)[^&#\s]*/gi, "$1***")
        .replace(/\b(access_key|ticket|token|secret|authorization|app_secret|bot_token|api[_-]?key)\s*[=:]\s*([^\s,&;\]\}"']+)/gi, "$1=***");
    }
    if (Array.isArray(value)) return value.map((item) => sanitizeMobileLogValue(item));
    if (!value || typeof value !== "object") return value;
    return Object.fromEntries(Object.entries(value).map(([key, item]) => [
      key,
      secretKeys.has(key) && typeof item === "string" && item ? "***" : sanitizeMobileLogValue(item),
    ]));
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
      ["Gemini", current.gemini],
    ].filter(([, name]) => Boolean(name));
    const title = entries.length
      ? entries.map(([type, name]) => `${type}: ${name}`).join(" · ")
      : (isZh ? "尚未启用配置" : "No active configuration");

    if (page === "claude") return { label: "Claude", name: current.claude || inactive, title };
    if (["codex", "sessions", "mobile"].includes(page)) {
      return { label: "Codex", name: current.codex || inactive, title };
    }
    if (page === "grok") return { label: "Grok", name: current.grok || inactive, title };
    if (page === "gemini") return { label: "Gemini", name: current.gemini || inactive, title };
    if (page === "add-provider") {
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

  function maskDeepLinkSecret(value) {
    const secret = String(value || "");
    if (!secret) return "--";
    if (secret.length <= 12) return `${secret.slice(0, 2)}****`;
    return `${secret.slice(0, 6)}...${secret.slice(-4)}`;
  }

  function getDeepLinkImportView(payload) {
    const importPayload = payload || {};
    const data = importPayload.data || {};
    const isMcp = importPayload.kind === "mcp";
    const isV1 = importPayload.source === "cc_switch_v1";
    const conflict = importPayload.conflict || null;
    const appLabels = {
      claude: "Claude",
      codex: "Codex",
      gemini: "Gemini",
      grok: "Grok / xAI",
    };
    const mcpApps = data.apps || { claude: true, codex: true, gemini: true };
    const mcpAppLabel = ["claude", "codex", "gemini"]
      .filter((app) => mcpApps[app])
      .map((app) => appLabels[app])
      .join(" / ") || "MCP";

    return {
      isMcp,
      appLabel: isMcp ? `MCP → ${mcpAppLabel}` : appLabels[importPayload.app] || importPayload.app || "--",
      name: data.name || "--",
      baseUrl: data.baseUrl || "--",
      apiKeyMasked: maskDeepLinkSecret(data.apiKey),
      model: data.model || "--",
      haikuModel: data.haikuModel || "--",
      sonnetModel: data.sonnetModel || "--",
      opusModel: data.opusModel || "--",
      homepage: data.homepage || "--",
      configText: isMcp ? JSON.stringify(data.config || {}, null, 2) : "",
      showProviderDetails: !isMcp,
      showClaudeModels: !isMcp && importPayload.app === "claude",
      showHomepage: !isMcp && isV1,
      showConflict: !isMcp && isV1 && Boolean(conflict),
      existingName: conflict?.existingName || "--",
      suggestedName: conflict?.suggestedName || "--",
      defaultConflictAction: "rename",
    };
  }

  function buildDeepLinkApplyRequest(payload, conflictAction) {
    const importPayload = payload || {};
    const source = importPayload.source || "legacy";
    const hasV1Conflict = source === "cc_switch_v1" && Boolean(importPayload.conflict);
    return {
      kind: importPayload.kind || "",
      app: importPayload.app || "",
      data: importPayload.data || {},
      source,
      conflictAction: hasV1Conflict
        ? (conflictAction === "overwrite" ? "overwrite" : "rename")
        : null,
    };
  }

  // 用量监控时间范围（unix 秒，前后端过滤均含端点）。两个易混概念的准确语义：
  // - today：自然日，本地今天 00:00:00 → 23:59:59（凌晨零点到当天晚上零点）
  // - 1d：滚动窗口，当前时刻往前推整 24 小时 → 当前时刻
  // 其余：7d/14d/30d = (N-1) 天前本地零点 → 当前时刻（近 N 天，含今天）；
  // all = 不限；custom = 用户输入（未填开始默认最近 24 小时）。
  function resolveUsageRange(range, opts) {
    const options = opts || {};
    const nowMs = Number.isFinite(options.nowMs) ? options.nowMs : Date.now();
    const nowSec = Math.floor(nowMs / 1000);
    // 先取零点再挪日历日，避免用 86400000 毫秒近似“一天”在夏令时切换日出错
    const localMidnight = (daysAgo) => {
      const d = new Date(nowMs);
      d.setHours(0, 0, 0, 0);
      d.setDate(d.getDate() - daysAgo);
      return Math.floor(d.getTime() / 1000);
    };
    switch (range) {
      case "today":
        // 结束边界是次日零点前一秒，而不是“不设上限”：
        // 保证“今天”严格落在自然日内，与滚动的“24 小时”概念分明
        return { startTs: localMidnight(0), endTs: localMidnight(-1) - 1 };
      case "1d":
        return { startTs: nowSec - 86400, endTs: nowSec };
      case "7d":
        return { startTs: localMidnight(6), endTs: nowSec };
      case "14d":
        return { startTs: localMidnight(13), endTs: nowSec };
      case "30d":
        return { startTs: localMidnight(29), endTs: nowSec };
      case "custom": {
        const startTs = Number.isFinite(options.customStartTs)
          ? options.customStartTs
          : nowSec - 86400;
        const endTs =
          options.customLiveEnd !== false
            ? nowSec
            : Number.isFinite(options.customEndTs)
              ? options.customEndTs
              : null;
        return { startTs, endTs };
      }
      default:
        return { startTs: null, endTs: null };
    }
  }

  return {
    shouldAutoOpenUsageGuide,
    getUpdateActionMode,
    formatVersionTag,
    getEditorPathMode,
    getSwitchOverlayState,
    validateEditorPathInput,
    resolveUniversalAppBaseUrl,
    normalizeFetchedModels,
    isDeepseekCodexConfig,
    getCodexToolboxLayout,
    isFreshMobileQr,
    isQqAuthorizationTarget,
    shouldRenderChannelQr,
    getMobileBindingUiState,
    sanitizeMobileLogValue,
    getCodexSessionMetrics,
    getGlobalConfigDisplay,
    maskDeepLinkSecret,
    getDeepLinkImportView,
    buildDeepLinkApplyRequest,
    resolveUsageRange,
  };
});
