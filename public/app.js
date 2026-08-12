const $ = (id) => document.getElementById(id);

// 安全 DOM 写入，避免初始化因缺失节点中断
function setText(id, value) {
  const el = $(id);
  if (el) el.textContent = value;
}
function setHtml(id, value) {
  const el = $(id);
  if (el) el.innerHTML = value;
}
function setPlaceholder(id, value) {
  const el = $(id);
  if (el) el.placeholder = value;
}
function setTitle(id, value) {
  const el = $(id);
  if (el) el.title = value;
}
function on(id, event, handler) {
  const el = $(id);
  if (el) el.addEventListener(event, handler);
  return el;
}

/**
 * 绑定「点击遮罩空白处关闭弹窗」。
 *
 * 只监听 click 会有一个经典缺陷：在输入框里按下鼠标、拖到框外（比如从右往左选中文本）
 * 再松开时，浏览器会把 click 派发到 mousedown / mouseup 两个节点的最近公共祖先，
 * 也就是遮罩本身，于是 event.target === overlay 成立，弹窗被误关闭。
 *
 * 这里要求「按下」和「松开」都发生在遮罩自身，才算一次真正的空白点击。
 */
function bindOverlayDismiss(overlayId, closeFn) {
  const overlay = $(overlayId);
  if (!overlay) return null;
  let pressedOnOverlay = false;

  const markPress = (event) => {
    // 只认鼠标左键 / 触摸 / 笔的主按键
    pressedOnOverlay = event.button === 0 && event.target === overlay;
  };

  overlay.addEventListener("pointerdown", markPress);
  // 老 WebView 没有 pointer 事件时的兜底
  overlay.addEventListener("mousedown", (event) => {
    if (window.PointerEvent) return;
    markPress(event);
  });

  overlay.addEventListener("click", (event) => {
    const shouldClose = pressedOnOverlay && event.target === overlay;
    pressedOnOverlay = false;
    if (shouldClose) closeFn(event);
  });

  return overlay;
}

const tauriApi = window.__TAURI__;
const helpers = window.VarSwitchHelpers || {};

if (!tauriApi?.core?.invoke || !tauriApi?.event?.listen) {
  document.body.innerHTML = `
    <div style="max-width:760px;margin:48px auto;padding:0 20px;font-family:-apple-system,BlinkMacSystemFont,'Segoe UI','PingFang SC','Microsoft YaHei',sans-serif;color:#18181B;line-height:1.6;">
      <h1 style="font-size:22px;margin-bottom:8px;">VarSwitch 启动失败</h1>
      <p style="margin-bottom:6px;">未检测到 Tauri 运行时 API（window.__TAURI__）。</p>
      <p style="opacity:.75;">请使用 <code>dev.bat</code> 或 <code>npm run tauri -- dev</code> 启动，而不是直接在浏览器打开 HTML。</p>
    </div>
  `;
  throw new Error("Tauri API unavailable");
}

const invoke = tauriApi.core.invoke;
const { listen } = tauriApi.event;

// B5: 打印调试日志前过滤凭据字段，避免 secret 出现在 devtools console
const SECRET_KEYS = new Set(["appSecret", "app_secret", "botToken", "bot_token", "apiKey", "api_key", "accessKey", "ticket", "authorization"]);
function sanitizeLogText(value) {
  return String(value || "")
    .replace(/([?&](?:access_key|ticket|token|secret|authorization|app_secret|bot_token|api[_-]?key)=)[^&#\s]*/gi, "$1***")
    .replace(/\b(access_key|ticket|token|secret|authorization|app_secret|bot_token|api[_-]?key)\s*[=:]\s*([^\s,&;\]\}"']+)/gi, "$1=***");
}
function sanitizeForLog(value) {
  if (typeof value === "string") return sanitizeLogText(value);
  if (value instanceof Error) return { name: value.name, message: sanitizeLogText(value.message) };
  if (Array.isArray(value)) return value.map((item) => sanitizeForLog(item));
  if (!value || typeof value !== "object") return value;
  const out = {};
  for (const [k, v] of Object.entries(value)) {
    out[k] = SECRET_KEYS.has(k) && typeof v === "string" && v.length > 0 ? "***" : sanitizeForLog(v);
  }
  return out;
}

function mobileDebug(label, payload = {}) {
  console.log(`[mobile-control] ${label}`, sanitizeForLog({
    selectedMobileChannel,
    selectedCodexThreadId,
    ...payload,
  }));
}

function mobileDebugError(label, error, payload = {}) {
  console.error(`[mobile-control] ${label}`, sanitizeForLog({
    selectedMobileChannel,
    selectedCodexThreadId,
    error,
    ...payload,
  }));
}

const LANG_STORAGE_KEY = "varswitch.lang";
// 记录用户是否在设置里手动选过语言;没有手动选择时,进入软件一律默认中文
const LANG_USER_SET_KEY = "varswitch.lang.userSet";
const THEME_STORAGE_KEY = "varswitch.theme";
const APP_REPOSITORY_URL = "https://github.com/ConcertoNotes/variable-switching";
const APP_DOWNLOAD_PAGE_URL = "https://download.varswitch.strova.top/";
// ── cc-switch 式供应商预设 ──────────────────────────────
// 选择预设即自动填入官方地址与默认模型；Base URL 留空时后端回退到各官方端点。
// Codex 的 wire 表示上游协议：responses = OpenAI Responses（官方），chat = OpenAI Chat Completions（DeepSeek 等第三方）。
const CLAUDE_PRESETS = [
  {
    id: "anthropic",
    name: "Anthropic 官方",
    baseUrl: "https://api.anthropic.com",
    model: "",
  },
  {
    id: "deepseek",
    name: "DeepSeek",
    baseUrl: "https://api.deepseek.com/anthropic",
    model: "deepseek-v4-flash",
  },
  {
    id: "kimi",
    name: "Kimi (Moonshot)",
    baseUrl: "https://api.moonshot.cn/anthropic",
    model: "kimi-k2.6",
  },
  {
    id: "zhipu_glm",
    name: "智谱 GLM",
    baseUrl: "https://open.bigmodel.cn/api/anthropic",
    model: "glm-5.1",
  },
  {
    id: "minimax",
    name: "MiniMax",
    baseUrl: "https://api.minimaxi.com/anthropic",
    model: "MiniMax-M2.7",
  },
];

const CODEX_PRESETS = [
  {
    id: "openai",
    name: "OpenAI 官方",
    baseUrl: "https://api.openai.com/v1",
    model: "gpt-5.5",
    providerName: "custom",
    wire: "responses",
  },
  {
    id: "deepseek",
    name: "DeepSeek",
    baseUrl: "https://api.deepseek.com",
    model: "deepseek-v4-flash",
    providerName: "deepseek",
    wire: "chat",
  },
  {
    id: "kimi",
    name: "Kimi (Moonshot)",
    baseUrl: "https://api.moonshot.cn/v1",
    model: "kimi-k2.6",
    providerName: "kimi",
    wire: "chat",
  },
  {
    id: "zhipu_glm",
    name: "智谱 GLM",
    baseUrl: "https://open.bigmodel.cn/api/coding/paas/v4",
    model: "glm-5.1",
    providerName: "zhipu_glm",
    wire: "chat",
  },
  {
    id: "minimax",
    name: "MiniMax",
    baseUrl: "https://api.minimaxi.com/v1",
    model: "MiniMax-M2.7",
    providerName: "minimax",
    wire: "chat",
  },
  {
    id: "siliconflow",
    name: "SiliconFlow",
    baseUrl: "https://api.siliconflow.cn/v1",
    model: "Pro/MiniMaxAI/MiniMax-M2.7",
    providerName: "siliconflow",
    wire: "chat",
  },
  {
    id: "openrouter",
    name: "OpenRouter",
    baseUrl: "https://openrouter.ai/api/v1",
    model: "openai/gpt-5.3-codex",
    providerName: "openrouter",
    wire: "chat",
  },
];

// Base URL 留空时前端验证 / 后端保存都会回退到官方地址
const OFFICIAL_BASE_URLS = {
  claude: "https://api.anthropic.com",
  codex: "https://api.openai.com/v1",
  grok: "https://api.x.ai/v1",
  gemini: "https://generativelanguage.googleapis.com",
};

const GROK_PRESETS = [
  {
    id: "xai_official",
    name: "xAI Official",
    baseUrl: "https://api.x.ai/v1",
    model: "grok-4",
  },
  {
    id: "xai_fast",
    name: "xAI Grok Fast",
    baseUrl: "https://api.x.ai/v1",
    model: "grok-3-mini",
  },
];

const UNIVERSAL_PROVIDER_PRESETS = [
  {
    id: "newapi",
    name: "NewAPI",
    description: "支持 Anthropic、OpenAI 与多种兼容协议的统一 API 网关",
    models: { claude: "claude-sonnet-5", codex: "gpt-5.5", grok: "grok-4", gemini: "gemini-2.5-pro" },
    providerName: "custom",
  },
  {
    id: "custom_gateway",
    name: "自定义网关",
    description: "使用同一组 API 地址和密钥同步到多个应用",
    models: { claude: "", codex: "", grok: "", gemini: "" },
    providerName: "custom",
  },
];

const I18N = {
  en: {
    appTitle: "VarSwitch",
    appSubtitle: "Environment Sync Manager",
    importBtn: "Import Current",
    addBtn: "+ Add Config",
    statusTitle: "Claude Status",
    statusHint: "Restart the terminal after switching to apply environment variables.",
    profilesTitle: "Config List",
    activeProfileLabel: "Current active",
    addConfig: "Add Config",
    editConfig: "Edit Config",
    nameLabel: "Config Name",
    tokenLabel: "Token",
    urlLabel: "URL",
    cancel: "Cancel",
    save: "Save",
    switchingTo: "Switching to",
    preparing: "Preparing...",
    switchDone: "Switch complete",
    cancelSwitch: "Cancel Switch",
    stepSystem: "System Env",
    stepEditors: "Editors",
    stepClaude: "Claude",
    progressSystem: "Updating system environment variables...",
    progressEditors: "Updating editor settings...",
    progressClaude: "Updating Claude settings...",
    progressFinalize: "Finalizing switch...",
    progressDone: "Done",
    progressCancelling: "Cancelling...",
    statusSystemEnv: "System Environment",
    statusClaude: "Claude Settings",
    codexPageTab: "Codex",
    codexStatusTitle: "Codex Status",
    codexProfilesTitle: "Codex Config List",
    codexAddConfig: "Add Codex Config",
    codexEditConfig: "Edit Codex Config",
    codexNameLabel: "Config Name",
    codexPresetLabel: "Preset",
    codexPresetCustom: "Custom",
    codexPresetHintDefault: "Choose a preset to fill common Codex provider settings.",
    codexApiKeyLabel: "Codex API Key",
    codexBaseUrlLabel: "Codex Base URL",
    codexModelLabel: "Codex Model",
    codexProviderLabel: "Codex Provider",
    codexImageSectionTitle: "Codex Image Skill",
    codexImageSectionHint: "Optional. Enabling this profile installs a local image-generation Skill and configures its API. Restart Codex after switching.",
    codexImageApiKeyLabel: "Image API Key",
    codexImageBaseUrlLabel: "Image Base URL",
    codexImageSkillReady: "Installed · restart Codex to refresh",
    codexImageSkillNeedsSwitch: "Not installed · enable this profile",
    codexAuthModeLabel: "Write Mode",
    codexAuthModeDefaultTitle: "Default write",
    codexAuthModeDefaultHint: "~/.codex/auth.json + ~/.codex/config.toml",
    codexAuthModeOfficialTitle: "Official account login, API quota",
    codexAuthModeOfficialHint: "Only write ~/.codex/config.toml",
    codexOfficialConfigLabel: "Official account API quota config",
    copy: "Copy",
    codexSwitching: "Writing Codex configuration...",
    codexSwitchedTo: "Codex switched to {name}",
    codexImportPrompt: "Name for the imported Codex config:",
    codexImportDefaultName: "Current Codex Config",
    codexToastImported: "Current Codex config imported",
    codexToastAdded: "Codex config added",
    codexToastUpdated: "Codex config updated",
    codexToastDeleted: "Codex config deleted",
    codexNoConfigsTitle: "No Codex configs yet",
    codexNoConfigsDesc: "Create a config to sync Codex settings in one click.",
    codexAddFirstConfig: "Add your first Codex config",
    grokPageTab: "Grok / xAI",
    grokStatusTitle: "Grok Status",
    grokProfilesTitle: "Grok Config List",
    grokAddConfig: "Add Grok Config",
    grokEditConfig: "Edit Grok Config",
    grokNameLabel: "Config Name",
    grokPresetLabel: "Preset",
    grokPresetCustom: "Custom",
    grokPresetHintDefault: "Choose a preset to fill common xAI / Grok settings.",
    grokApiKeyLabel: "XAI API Key",
    grokBaseUrlLabel: "XAI Base URL",
    grokModelLabel: "Model",
    grokModelHint: "Optional. Written to [model.varswitch].model and XAI_MODEL.",
    grokApiBackendLabel: "API Backend",
    grokApiBackendHint: "Maps to api_backend in ~/.grok/config.toml.",
    grokOpenFolder: "Open .grok folder",
    grokBackupRuntime: "Backup config.toml",
    grokImportCurrent: "Import current",
    grokStatusHint: "Switching writes ~/.grok/config.toml (default model + API), and also syncs XAI_API_KEY / XAI_BASE_URL. Restart Grok CLI to apply.",
    grokActiveConfigLabel: "Active Grok Config",
    grokSwitching: "Writing ~/.grok/config.toml...",
    grokSwitchedTo: "Grok switched to {name}",
    grokImportPrompt: "Name for the imported Grok config:",
    grokImportDefaultName: "Current Grok Config",
    grokToastImported: "Current Grok config imported",
    grokToastAdded: "Grok config added",
    grokToastUpdated: "Grok config updated",
    grokToastDeleted: "Grok config deleted",
    grokNoConfigsTitle: "No Grok configs yet",
    grokNoConfigsDesc: "Create a config to switch ~/.grok/config.toml (and env vars) in one click.",
    geminiNoConfigsTitle: "No Gemini configs yet",
    geminiNoConfigsDesc: "Create a config to switch the Gemini CLI gateway and model in one click.",
    grokAddFirstConfig: "Add your first Grok config",
    codexToolbox: "Toolbox",
    codexToolboxTitle: "Codex Toolbox",
    toolboxTabSession: "Session Sync",
    toolboxTabRemote: "Mobile Control",
    toolboxSessionHint: "Click sync to import local Codex history into VarSwitch. Mobile bindings are managed only in Mobile Control.",
    toolboxSessionSummaryEmpty: "Not synced yet",
    toolboxSessionSummary: "Synced {count} conversations · {time}",
    toolboxSyncedThreadsTitle: "Synced Codex Threads",
    toolboxThreadsTitle: "Control Conversation",
    toolboxBindingsTitle: "Mobile Channels",
    toolboxMobileAppLabel: "Bind App",
    toolboxMobileThreadLabel: "Control Conversation",
    toolboxMobileNoThreadOption: "Sync local history first",
    toolboxBindWechat: "Bind WeChat",
    toolboxBindLark: "Bind Feishu/Lark",
    toolboxBindQq: "Bind QQ",
    toolboxSelectThread: "Switch",
    toolboxSelectedThread: "Selected",
    toolboxSyncNow: "Sync Now",
    toolboxUnbind: "Unbind",
    toolboxNoThreads: "No synced Codex threads. Sync local history first.",
    toolboxNoBindings: "No mobile channels are bound yet.",
    toolboxRemoteHint: "Select a synced conversation, bind Feishu/WeChat/QQ, then start platform-bot listening for mobile control outside your LAN.",
    toolboxRemoteStatus: "Status",
    toolboxRemoteAccessUrl: "Active Conversation",
    toolboxRemoteToken: "Token",
    toolboxRemoteDevice: "Device",
    toolboxRemoteRunning: "Platform Listening",
    toolboxRemoteStopped: "Stopped",
    toolboxSmartControlConnected: "Advanced control connected",
    toolboxSmartControlWaiting: "Advanced control waiting",
    toolboxSmartControlCompat: "Compatibility mode",
    toolboxSmartControlCopied: "Protocol event copied",
    toolboxSmartControlApprovalTitle: "Pending approvals",
    toolboxSmartControlApprovalEmpty: "No pending approval",
    toolboxSmartControlApprovalSubmitted: "Approval submitted",
    toolboxRemoteStart: "Start Platform Link",
    toolboxRemoteStop: "Stop",
    toolboxChannelCredentials: "Platform Credentials",
    toolboxCredentialStatus: "Credential Status",
    toolboxAppId: "App ID",
    toolboxAppSecret: "App Secret",
    toolboxBotToken: "Bot Token",
    toolboxAccountId: "Account ID",
    toolboxBaseUrl: "Base URL",
    toolboxUserId: "User ID",
    toolboxBotOpenId: "Bot Open ID",
    toolboxSaveChannel: "Save Channel",
    toolboxCreateLarkBot: "Create Lark Bot",
    toolboxRebindLarkBot: "Bind Existing Bot",
    toolboxUnbindLarkSession: "Unbind Conversation",
    toolboxStopLarkListen: "Stop Lark Listening",
    toolboxClearLarkBinding: "Clear Binding",
    toolboxStartQqQr: "Scan QQ QR",
    toolboxStartWechatQr: "Bind WeChat",
    toolboxPollWechatQr: "Check WeChat Scan",
    toolboxQrOpen: "Open QR Link",
    toolboxQrStatus: "QR Status",
    toolboxLastError: "Last Error",
    toolboxChannelSaved: "Mobile channel saved",
    toolboxDraftSaved: "Toolbox draft saved",
    toolboxBindingSaved: "Session binding saved",
    toolboxBindingRemoved: "Session binding removed",
    toolboxBindingSynced: "Session binding synced",
    toolboxSessionsSynced: "Local Codex history synced",
    toolboxSessionSafeNote: "Session manager actions only hide records inside VarSwitch; Codex App history files stay untouched.",
    toolboxSessionSearchPlaceholder: "Search title, project, message, or session ID",
    toolboxSessionSelectAll: "Select All",
    toolboxSessionClearSelected: "Clear Selection",
    toolboxSessionCopyIds: "Copy IDs",
    toolboxSessionOpenTrash: "Trash",
    toolboxSessionMoveTrash: "Move to Trash",
    toolboxSessionTrashTitle: "Session Trash",
    toolboxSessionTrashHint: "Restore sessions hidden from this toolbox. Original Codex files were not deleted.",
    toolboxSessionTrashClose: "Close",
    toolboxSessionRestoreSelected: "Restore Selected",
    toolboxSessionTrashEmpty: "Trash is empty",
    toolboxSessionNoSearchResults: "No sessions match the search.",
    toolboxSessionSelectedSummary: "{selected} selected · {total} visible · {trash} in trash",
    toolboxSessionCopiedIds: "Copied selected session IDs",
    toolboxSessionMovedTrash: "Moved selected sessions to toolbox trash",
    toolboxSessionRestored: "Sessions restored",
    toolboxSessionConfirmTrash: "Move selected sessions to VarSwitch trash? This will not delete Codex App history files.",
    toolboxSessionProjectUnknown: "Unknown Project",
    toolboxSessionIdLabel: "Session ID",
    toolboxSessionFileLabel: "File",
    toolboxSessionDeletedAt: "Hidden at",
    toolboxSessionCopyId: "Copy session ID",
    toolboxThreadSelected: "Control conversation switched",
    toolboxRemoteStarted: "Mobile platform link is starting",
    toolboxRemoteStopped: "Mobile control stopped",
    readFailed: "Read failed",
    synced: "Synced",
    unsynced: "Not Synced",
    noConfigsTitle: "No configs yet",
    noConfigsDesc: "Create a config to sync System / VSCode / Claude in one click.",
    addFirstConfig: "Add your first config",
    inUse: "In Use",
    switchUse: "Switch",
    edit: "Edit",
    delete: "Delete",
    toastUpdated: "Config updated",
    toastAdded: "Config added",
    toastSaving: "Saving...",
    toastDeleted: "Config deleted",
    toastImported: "Current config imported",
    toastCopied: "Copied to clipboard",
    switchedTo: "Switched to {name}",
    partialSuccess: "Partially succeeded: {ok}\nFailed: {errors}",
    cancelledRestored: "Switch cancelled. Previous config restored",
    cancelRestoreFailed: "Restore after cancellation failed: {error}",
    switchFailed: "Switch failed: {error}",
    snapshotFailed: "Snapshot failed: {error}",
    confirmDelete: "Delete \"{name}\"?",
    importPrompt: "Name for the imported config:",
    importDefaultName: "Current Config",
    loadStatusFailed: "Failed to load status: {error}",
    loadProfilesFailed: "Failed to load profiles: {error}",
    activeConfigLabel: "Active Config",
    syncNow: "Sync Now",
    switchToDark: "Dark",
    switchToLight: "Light",
    placeholderName: "e.g. Production",
    placeholderApiKey: "sk-...",
    placeholderBaseUrl: "Leave empty for official https://api.anthropic.com",
    modelIdLabel: "Model ID",
    placeholderModelId: "e.g. opus, sonnet",
    modelIdHint: "Optional. Sets model in editor and Claude settings.",
    endpointTest: "Verify",
    endpointTesting: "Verifying...",
    endpointUse: "Use",
    modelFetch: "Fetch Models",
    modelFetching: "Fetching...",
    modelFetchMissing: "Enter an API Key first.",
    providerPresetLabel: "Provider Preset",
    providerPresetCustom: "Custom",
    claudePresetHintDefault: "Pick a provider to auto-fill its endpoint and default model. Leave Base URL empty for the official API.",
    claudeApiFormatLabel: "API Format",
    claudeApiFormatHint: "Pick OpenAI format when the upstream only exposes an OpenAI API: requests go through the local proxy at 127.0.0.1:25789, so keep VarSwitch running.",
    claudeApiFormatOptionAnthropic: "Anthropic Messages (direct, default)",
    claudeApiFormatOptionOpenAiChat: "OpenAI Chat Completions (via local proxy)",
    claudeApiFormatNeedsBaseUrl: "OpenAI format requires an upstream Base URL.",
    claudeApiFormatNeedsModel: "OpenAI format needs a Model ID (upstream model name, e.g. deepseek-chat).",
    codexWireApiLabel: "Upstream Protocol",
    codexWireApiHint: "Written to wire_api in ~/.codex/config.toml. Presets set this automatically.",
    codexAdvancedTitle: "Advanced options",
    verifyOkLabel: "Verified",
    verifyModelsSuffix: "models",
    verifyOkToast: "Connection verified: key and endpoint are working.",
    verifyAuthFailed: "Authentication failed (401/403). Check your API Key.",
    verifyNoModelsEndpoint: "This endpoint does not expose /models (404/405), so it cannot be auto-verified.",
    modelFetchNoResults: "No models returned",
    modelSelected: "Model selected",
    skillsManage: "Skills",
    skillsTitle: "Skills Management",
    addSkill: "+ Add Skill",
    skillName: "Command Name",
    skillContent: "Content",
    skillNamePlaceholder: "command-name",
    toastSkillSaved: "Skill saved",
    toastSkillDeleted: "Skill deleted",
    confirmDeleteSkill: "Delete skill \"{name}\"?",
    noSkills: "No skills yet. Create slash commands for Claude Code.",
    promptsManage: "Prompts",
    promptsTitle: "Claude Prompts",
    promptsPathLabel: "~/.claude/CLAUDE.md",
    toastPromptSaved: "Prompt saved",
    mcpManage: "MCP Servers",
    mcpTitle: "MCP Server Management",
    mcpPathLabel: "~/.claude.json",
    addMcp: "+ Add Server",
    mcpName: "Server Name",
    mcpConfig: "Config (JSON)",
    mcpNamePlaceholder: "server-name",
    toastMcpSaved: "MCP server saved",
    toastMcpDeleted: "MCP server deleted",
    confirmDeleteMcp: "Delete MCP server \"{name}\"?",
    invalidJson: "Invalid JSON format",
    noMcpServers: "No MCP servers configured.",
    // Skills Discovery
    skillsTabInstalled: "Installed",
    skillsTabDiscover: "Discover",
    installFromZip: "Install from ZIP",
    discoverSearchPlaceholder: "Search skills...",
    allRepos: "All Repos",
    filterAll: "All",
    filterInstalled: "Installed",
    filterNotInstalled: "Not Installed",
    manageRepos: "Repos",
    discoverLoading: "Loading skills from repositories...",
    discoverEmpty: "No skills found.",
    discoverNoMatch: "No skills match your search.",
    installBtn: "Install",
    installedBadge: "Installed",
    uninstallBtn: "Uninstall",
    repoManagerTitle: "Manage Repositories",
    addRepoLabel: "Add Repository",
    addRepoPlaceholder: "owner/repo or https://github.com/owner/repo",
    toastSkillInstalled: "Skill \"{name}\" installed",
    toastSkillUninstalled: "Skill \"{name}\" uninstalled",
    toastRepoAdded: "Repository added",
    toastRepoRemoved: "Repository removed",
    toastZipInstalled: "{count} skill(s) installed from ZIP",
    toastZipNoSkills: "No skills found in ZIP file",
    commands: "commands",
    localSkill: "Local",
    repoSkill: "Repo",
    sourceCommand: "Command",
    sourceSkill: "Skill",
    // Prompt Templates
    promptTabEditor: "Editor",
    promptTabTemplates: "Templates",
    insertSnippet: "-- Insert snippet --",
    appendToPrompt: "Append",
    replacePrompt: "Replace",
    snippetLanguagePref: "Language: Chinese",
    snippetCodeQuality: "Code Quality Rules",
    snippetSecurity: "Security Guidelines",
    snippetConcise: "Concise Mode",
    snippetArchitect: "Architecture Guidelines",
    toastSnippetInserted: "Snippet inserted",
    toastTemplateApplied: "Template applied",
    mcpTabInstalled: "Installed",
    mcpTabPresets: "Presets",
    mcpSearchPlaceholder: "Search MCP servers on GitHub...",
    mcpSearchLoading: "Searching GitHub...",
    mcpRequiresApiKey: "Requires API key configuration",
    mcpInstalled: "Installed",
    mcpInstallBtn: "Install",
    mcpGithubBtn: "GitHub",
    mcpNoPresets: "No presets available.",
    // Skills Discovery GitHub search
    skillsSearchGithub: "Search GitHub",
    skillsSearchGithubPlaceholder: "Search skills on GitHub...",
    skillsGithubLoading: "Searching GitHub...",
    skillsGithubResults: "Showing GitHub search results",
    skillsBackToCatalog: "Back to catalog",
    // Settings
    settingsTitle: "Settings",
    settingsGroupGeneral: "General",
    settingsGroupPaths: "Paths",
    settingsGroupBackup: "Backup",
    settingsAutoStart: "Launch at startup",
    settingsAutoStartDesc: "Automatically start VarSwitch when you log in",
    settingsMinTray: "Minimize to tray",
    settingsMinTrayDesc: "Hide to system tray when closing the window",
    settingsConfigDir: "Config directory",
    settingsClaudePath: "Claude settings",
    settingsCodexPath: "Codex settings",
    settingsLogs: "Runtime logs",
    settingsLogsDesc: "Open the logs folder to view or report issues",
    settingsOpen: "Open",
    settingsCopyPath: "Copy path",
    settingsBrowse: "Browse",
    settingsSavePath: "Save Path",
    settingsReset: "Reset",
    settingsPathPlaceholder: "Paste a settings directory or settings.json path",
    settingsPathEmpty: "Enter an editor settings path first",
    settingsCurrentPath: "Current path",
    settingsDefaultPathLabel: "Built-in default",
    settingsEditorStatusCustom: "Manual path",
    settingsEditorStatusDetected: "Auto-detected",
    settingsEditorStatusDefault: "Default path",
    settingsEditorHintCustom: "Uses your saved path for detection and environment sync.",
    settingsEditorHintDetected: "Using the built-in path detected on this device.",
    settingsEditorHintDefault: "Not detected yet. Save a path here if this editor is installed elsewhere.",
    settingsDefaultPath: "Default: {path}",
    settingsExport: "Export Profiles",
    settingsImport: "Import Profiles",
    settingsManualBackup: "Manual config backup",
    settingsManualBackupDesc: "Import or export all current profiles",
    settingsAutoBackup: "Auto backup",
    settingsAutoBackupDesc: "Snapshots are taken before every switch; you can roll back mistakes",
    settingsViewBackups: "Roll back",
    toastSettingsSaved: "Settings saved",
    toastEditorPathSaved: "{name} path saved",
    toastEditorPathReset: "{name} path reset",
    toastEditorPathBrowseCancelled: "Path selection cancelled",
    toastExported: "Profiles exported",
    toastImported2: "{count} profile(s) imported",
    toastImportNone: "No new profiles to import",
    settingsSilentStart: "Silent startup",
    settingsSilentStartDesc: "Start minimized to system tray",
    settingsLang: "Language",
    settingsTheme: "Theme",
    supportSectionTitle: "Quick Actions",
    supportHintDefault: "Open the usage guide, check for updates, or jump to the repository.",
    supportHintUpdateAvailable: "New version available: current {currentVersion}, latest {latestVersion}. Click to download and install automatically.",
    supportHintUpToDate: "You're already on the latest version: {version}.",
    usageGuideBtn: "Usage Guide",
    updateCheckBtn: "Check for Updates",
    updateReleaseBtn: "Install Update {version}",
    downloadSiteBtn: "Download Site",
    githubRepoBtn: "GitHub Repo",
    usageGuideKicker: "Usage Guide",
    usageGuideTitle: "How to use VarSwitch",
    usageGuideIntro: "VarSwitch centralizes Claude Code, Codex, prompts, MCP servers, mobile control, settings, and backups.",
    usageGuideStep1Title: "Claude Code configs",
    usageGuideStep1Desc: "Add or import Token/Base URL configs and verify connectivity with your key, then switch to sync system env, supported editors, and ~/.claude/settings.json.",
    usageGuideStep2Title: "Codex configs",
    usageGuideStep2Desc: "Manage Codex API Key, Base URL, model, provider, and auth mode, then sync ~/.codex/config.toml and auth.json.",
    usageGuideStep3Title: "Codex Toolbox",
    usageGuideStep3Desc: "Sync Codex sessions and bind mobile control.",
    usageGuideStep4Title: "Skills, prompts, and MCP",
    usageGuideStep4Desc: "Use toolbar entries to edit Claude Skills/Commands, manage CLAUDE.md templates, and edit MCP server JSON in ~/.claude.json.",
    usageGuideStep5Title: "Settings and backups",
    usageGuideStep5Desc: "Set language, theme, tray behavior, startup options, editor paths, profile export/import, automatic backups, and rollback.",
    usageGuideStep6Title: "Updates and restart tips",
    usageGuideStep6Desc: "Use the home shortcuts to check updates, open the download site or repository. After switching configs, restart terminals and editors.",
    usageGuideClose: "Close",
    usageGuideNever: "Never remind again",
    toastGuideDisabled: "Usage guide disabled",
    checkingUpdates: "Checking for Updates...",
    downloadingUpdate: "Downloading update {percent}%",
    installingUpdate: "Installing update...",
    openingReleasePage: "Opening Download Page...",
    toastUpdateAvailable: "Update available: current {currentVersion}, latest {latestVersion}",
    updatePillText: "New version available: current {currentVersion}, latest {latestVersion}",
    updatePillAction: "Download",
    toastUpdateDownloaded: "Update installer started. VarSwitch will restart automatically if needed.",
    toastAlreadyLatest: "You're already on the latest version",
    toastReleaseOpened: "Download page opened",
    toastRepoOpened: "Repository opened"
  },
  zh: {
    appTitle: "VarSwitch",
    appSubtitle: "环境变量同步工具",
    importBtn: "导入当前配置",
    addBtn: "+ 添加配置",
    statusTitle: "Claude 状态",
    statusHint: "切换后请重启终端，使环境变量生效。",
    profilesTitle: "配置列表",
    activeProfileLabel: "当前生效",
    addConfig: "添加配置",
    editConfig: "编辑配置",
    nameLabel: "配置名称",
    tokenLabel: "令牌",
    urlLabel: "地址",
    cancel: "取消",
    save: "保存",
    switchingTo: "正在切换到",
    preparing: "正在准备...",
    switchDone: "切换完成",
    cancelSwitch: "取消切换",
    stepSystem: "系统环境变量",
    stepEditors: "编辑器",
    stepClaude: "Claude",
    progressSystem: "正在更新系统环境变量...",
    progressEditors: "正在更新编辑器设置...",
    progressClaude: "正在更新 Claude 设置...",
    progressFinalize: "正在收尾...",
    progressDone: "完成",
    progressCancelling: "正在取消...",
    statusSystemEnv: "系统环境变量",
    statusClaude: "Claude 设置",
    codexPageTab: "Codex",
    codexStatusTitle: "Codex 状态",
    codexProfilesTitle: "Codex 配置列表",
    codexAddConfig: "添加 Codex 配置",
    codexEditConfig: "编辑 Codex 配置",
    codexNameLabel: "配置名称",
    codexPresetLabel: "预设",
    codexPresetCustom: "自定义",
    codexPresetHintDefault: "选择预设可自动填充常用 Codex 供应商配置。",
    codexApiKeyLabel: "Codex API Key",
    codexBaseUrlLabel: "Codex Base URL",
    codexModelLabel: "Codex 模型",
    codexProviderLabel: "Codex 供应商",
    codexImageSectionTitle: "Codex 图片生成 Skill",
    codexImageSectionHint: "可选。启用此配置时会自动安装本地图片生成 Skill 并同步 API；切换后请重启 Codex。",
    codexImageApiKeyLabel: "图片 API Key",
    codexImageBaseUrlLabel: "图片 Base URL",
    codexImageSkillReady: "已安装 · 重启 Codex 后刷新",
    codexImageSkillNeedsSwitch: "尚未安装 · 请启用此配置",
    codexAuthModeLabel: "写入方式",
    codexAuthModeDefaultTitle: "默认写入",
    codexAuthModeDefaultHint: "~/.codex/auth.json + ~/.codex/config.toml",
    codexAuthModeOfficialTitle: "官方账号登录，api额度消耗",
    codexAuthModeOfficialHint: "只写 ~/.codex/config.toml",
    codexOfficialConfigLabel: "官方账号登录，api额度消耗配置",
    copy: "复制",
    codexSwitching: "正在写入 Codex 配置...",
    codexSwitchedTo: "Codex 已切换到 {name}",
    codexImportPrompt: "请输入导入的 Codex 配置名称：",
    codexImportDefaultName: "当前 Codex 配置",
    codexToastImported: "当前 Codex 配置已导入",
    codexToastAdded: "Codex 配置已添加",
    codexToastUpdated: "Codex 配置已更新",
    codexToastDeleted: "Codex 配置已删除",
    codexNoConfigsTitle: "暂无 Codex 配置",
    codexNoConfigsDesc: "创建一个配置，一键同步 Codex 设置。",
    codexAddFirstConfig: "添加第一个 Codex 配置",
    grokPageTab: "Grok / xAI",
    grokStatusTitle: "Grok 状态",
    grokProfilesTitle: "Grok 配置列表",
    grokAddConfig: "添加 Grok 配置",
    grokEditConfig: "编辑 Grok 配置",
    grokNameLabel: "配置名称",
    grokPresetLabel: "预设",
    grokPresetCustom: "自定义",
    grokPresetHintDefault: "选择预设可自动填充常用 xAI / Grok 配置。",
    grokApiKeyLabel: "XAI API Key",
    grokBaseUrlLabel: "XAI Base URL",
    grokModelLabel: "模型",
    grokModelHint: "可选。写入 [model.varswitch].model，并同步 XAI_MODEL。",
    grokApiBackendLabel: "API Backend",
    grokApiBackendHint: "对应 ~/.grok/config.toml 的 api_backend 字段。",
    grokOpenFolder: "打开 .grok 目录",
    grokBackupRuntime: "备份 config.toml",
    grokImportCurrent: "导入当前配置",
    grokStatusHint: "切换后会写入 ~/.grok/config.toml（默认模型与 API），并同步 XAI_API_KEY / XAI_BASE_URL。请重启 Grok CLI 使配置生效。",
    grokActiveConfigLabel: "当前 Grok 配置",
    grokSwitching: "正在写入 ~/.grok/config.toml...",
    grokSwitchedTo: "Grok 已切换到 {name}",
    grokImportPrompt: "请输入导入的 Grok 配置名称：",
    grokImportDefaultName: "当前 Grok 配置",
    grokToastImported: "当前 Grok 配置已导入",
    grokToastAdded: "Grok 配置已添加",
    grokToastUpdated: "Grok 配置已更新",
    grokToastDeleted: "Grok 配置已删除",
    grokNoConfigsTitle: "暂无 Grok 配置",
    grokNoConfigsDesc: "创建一个配置，一键切换 ~/.grok/config.toml（并同步环境变量）。",
    geminiNoConfigsTitle: "暂无 Gemini 配置",
    geminiNoConfigsDesc: "创建一个配置，一键切换 Gemini CLI 的网关与模型。",
    grokAddFirstConfig: "添加第一个 Grok 配置",
    codexToolbox: "工具箱",
    codexToolboxTitle: "Codex 工具箱",
    toolboxTabSession: "会话同步",
    toolboxTabRemote: "手机控制",
    toolboxSessionHint: "点击同步即可把本地 Codex 历史记录同步到这个软件里；飞书/微信/QQ 绑定只在手机控制里管理。",
    toolboxSessionSummaryEmpty: "尚未同步",
    toolboxSessionSummary: "已同步 {count} 个对话 · {time}",
    toolboxSyncedThreadsTitle: "已同步 Codex 对话",
    toolboxThreadsTitle: "控制对话",
    toolboxBindingsTitle: "手机通道",
    toolboxMobileAppLabel: "绑定应用",
    toolboxMobileThreadLabel: "控制对话",
    toolboxMobileNoThreadOption: "请先同步本地历史",
    toolboxBindWechat: "绑定微信",
    toolboxBindLark: "绑定飞书",
    toolboxBindQq: "绑定 QQ",
    toolboxSelectThread: "切换",
    toolboxSelectedThread: "已选",
    toolboxSyncNow: "立即同步",
    toolboxUnbind: "解绑",
    toolboxNoThreads: "还没有同步 Codex 对话，请先同步本地历史。",
    toolboxNoBindings: "还没有绑定任何通道。",
    toolboxRemoteHint: "先选择已同步的对话，再绑定飞书/微信/QQ，最后开启平台机器人监听；手机不需要和电脑在同一局域网。",
    toolboxRemoteStatus: "状态",
    toolboxRemoteAccessUrl: "当前控制对话",
    toolboxRemoteToken: "令牌",
    toolboxRemoteDevice: "设备",
    toolboxRemoteRunning: "平台监听中",
    toolboxRemoteStopped: "未启动",
    toolboxSmartControlConnected: "高级控制通道已连接",
    toolboxSmartControlWaiting: "高级控制通道等待 Codex 连接",
    toolboxSmartControlCompat: "兼容模式",
    toolboxSmartControlCopied: "协议事件已复制",
    toolboxSmartControlApprovalTitle: "待处理审批",
    toolboxSmartControlApprovalEmpty: "暂无待处理审批",
    toolboxSmartControlApprovalSubmitted: "审批已提交",
    toolboxRemoteStart: "开启平台连接",
    toolboxRemoteStop: "停止",
    toolboxChannelCredentials: "平台凭据",
    toolboxCredentialStatus: "凭据状态",
    toolboxAppId: "App ID",
    toolboxAppSecret: "App Secret",
    toolboxBotToken: "Bot Token",
    toolboxAccountId: "Account ID",
    toolboxBaseUrl: "Base URL",
    toolboxUserId: "User ID",
    toolboxBotOpenId: "Bot Open ID",
    toolboxSaveChannel: "保存通道",
    toolboxCreateLarkBot: "创建新飞书机器人",
    toolboxRebindLarkBot: "换绑已有机器人",
    toolboxUnbindLarkSession: "解除会话绑定",
    toolboxStopLarkListen: "停止飞书监听",
    toolboxClearLarkBinding: "解除绑定",
    toolboxStartQqQr: "QQ 扫码绑定",
    toolboxStartWechatQr: "绑定微信",
    toolboxPollWechatQr: "检查微信扫码",
    toolboxQrOpen: "打开二维码链接",
    toolboxQrStatus: "扫码状态",
    toolboxLastError: "最近错误",
    toolboxChannelSaved: "手机通道已保存",
    toolboxDraftSaved: "工具箱草稿已保存",
    toolboxBindingSaved: "会话绑定已保存",
    toolboxBindingRemoved: "会话绑定已解除",
    toolboxBindingSynced: "会话绑定已同步",
    toolboxSessionsSynced: "本地 Codex 历史已同步",
    toolboxSessionSafeNote: "会话管理只在 VarSwitch 内隐藏记录，不会删除 Codex App 的历史会话文件。",
    toolboxSessionSearchPlaceholder: "搜索标题、项目、消息或会话 ID",
    toolboxSessionSelectAll: "全选",
    toolboxSessionClearSelected: "取消选择",
    toolboxSessionCopyIds: "复制 ID",
    toolboxSessionOpenTrash: "回收站",
    toolboxSessionMoveTrash: "移到回收站",
    toolboxSessionTrashTitle: "会话回收站",
    toolboxSessionTrashHint: "恢复从本工具列表中隐藏的会话；Codex 原始文件从未删除。",
    toolboxSessionTrashClose: "关闭",
    toolboxSessionRestoreSelected: "恢复所选",
    toolboxSessionTrashEmpty: "回收站为空",
    toolboxSessionNoSearchResults: "没有匹配的会话。",
    toolboxSessionSelectedSummary: "已选 {selected} 个 · 当前显示 {total} 个 · 回收站 {trash} 个",
    toolboxSessionCopiedIds: "已复制所选会话 ID",
    toolboxSessionMovedTrash: "已移入工具内回收站",
    toolboxSessionRestored: "会话已恢复",
    toolboxSessionConfirmTrash: "把所选会话移入 VarSwitch 回收站？这不会删除 Codex App 的历史会话文件。",
    toolboxSessionProjectUnknown: "未知项目",
    toolboxSessionIdLabel: "会话 ID",
    toolboxSessionFileLabel: "文件",
    toolboxSessionDeletedAt: "隐藏时间",
    toolboxSessionCopyId: "复制会话 ID",
    toolboxThreadSelected: "手机控制对话已切换",
    toolboxRemoteStarted: "手机平台连接正在启动",
    toolboxRemoteStopped: "手机控制已停止",
    readFailed: "读取失败",
    synced: "已同步",
    unsynced: "未同步",
    noConfigsTitle: "暂无配置",
    noConfigsDesc: "创建一个配置，一键同步系统环境变量 / VSCode / Claude。",
    addFirstConfig: "添加第一个配置",
    inUse: "使用中",
    switchUse: "切换使用",
    edit: "编辑",
    delete: "删除",
    toastUpdated: "配置已更新",
    toastAdded: "配置已添加",
    toastSaving: "保存中...",
    toastDeleted: "配置已删除",
    toastImported: "当前配置已导入",
    toastCopied: "已复制到剪贴板",
    switchedTo: "已切换到 {name}",
    partialSuccess: "部分成功: {ok}\n失败: {errors}",
    cancelledRestored: "已取消切换，已恢复之前配置",
    cancelRestoreFailed: "取消后恢复失败: {error}",
    switchFailed: "切换失败: {error}",
    snapshotFailed: "快照失败: {error}",
    confirmDelete: "确认删除 \"{name}\"？",
    importPrompt: "请输入导入配置名称：",
    importDefaultName: "当前配置",
    loadStatusFailed: "读取状态失败: {error}",
    loadProfilesFailed: "读取配置失败: {error}",
    activeConfigLabel: "当前配置",
    syncNow: "立即同步",
    switchToDark: "夜间",
    switchToLight: "白天",
    placeholderName: "例如：生产环境",
    placeholderApiKey: "sk-...",
    placeholderBaseUrl: "留空使用官方地址 https://api.anthropic.com",
    modelIdLabel: "模型 ID",
    placeholderModelId: "如 opus, sonnet",
    modelIdHint: "可选。设置编辑器和 Claude 系统设置中的模型。",
    endpointTest: "验证连接",
    endpointTesting: "验证中...",
    endpointUse: "使用",
    modelFetch: "获取模型",
    modelFetching: "获取中...",
    modelFetchMissing: "请先填写 API Key。",
    providerPresetLabel: "供应商预设",
    providerPresetCustom: "自定义",
    claudePresetHintDefault: "选择供应商自动填充官方地址与默认模型；Base URL 留空即使用官方 API。",
    claudeApiFormatLabel: "API 格式",
    claudeApiFormatHint: "上游仅提供 OpenAI 接口时选第二项：请求经 127.0.0.1:25789 本地代理转换协议，需保持 VarSwitch 运行。",
    claudeApiFormatOptionAnthropic: "Anthropic Messages（默认直连）",
    claudeApiFormatOptionOpenAiChat: "OpenAI Chat Completions（经本地代理转换）",
    claudeApiFormatNeedsBaseUrl: "OpenAI 格式必须填写上游 Base URL。",
    claudeApiFormatNeedsModel: "OpenAI 格式需要填写 Model ID（上游真实模型名，例如 deepseek-chat）。",
    codexWireApiLabel: "上游协议",
    codexWireApiHint: "写入 ~/.codex/config.toml 的 wire_api 字段；选择预设时自动匹配。",
    codexAdvancedTitle: "高级选项",
    verifyOkLabel: "验证通过",
    verifyModelsSuffix: "个模型",
    verifyOkToast: "连接验证通过：API Key 与端点均可用。",
    verifyAuthFailed: "鉴权失败（401/403），请检查 API Key 是否有效。",
    verifyNoModelsEndpoint: "该端点不支持 /models 接口（404/405），无法自动验证。",
    modelFetchNoResults: "没有返回模型",
    modelSelected: "已选择模型",
    skillsManage: "技能",
    skillsTitle: "技能管理",
    addSkill: "+ 添加技能",
    skillName: "命令名称",
    skillContent: "内容",
    skillNamePlaceholder: "command-name",
    toastSkillSaved: "技能已保存",
    toastSkillDeleted: "技能已删除",
    confirmDeleteSkill: "确认删除技能 \"{name}\"？",
    noSkills: "暂无技能。为 Claude Code 创建斜杠命令。",
    promptsManage: "提示词",
    promptsTitle: "Claude 提示词",
    promptsPathLabel: "~/.claude/CLAUDE.md",
    toastPromptSaved: "提示词已保存",
    mcpManage: "MCP 服务器",
    mcpTitle: "MCP 服务器管理",
    mcpPathLabel: "~/.claude.json",
    addMcp: "+ 添加服务器",
    mcpName: "服务器名称",
    mcpConfig: "配置 (JSON)",
    mcpNamePlaceholder: "server-name",
    toastMcpSaved: "MCP 服务器已保存",
    toastMcpDeleted: "MCP 服务器已删除",
    confirmDeleteMcp: "确认删除 MCP 服务器 \"{name}\"？",
    invalidJson: "JSON 格式无效",
    noMcpServers: "暂无 MCP 服务器配置。",
    // Skills Discovery
    skillsTabInstalled: "已安装",
    skillsTabDiscover: "发现",
    installFromZip: "从 ZIP 安装",
    discoverSearchPlaceholder: "搜索技能...",
    allRepos: "全部仓库",
    filterAll: "全部",
    filterInstalled: "已安装",
    filterNotInstalled: "未安装",
    manageRepos: "仓库",
    discoverLoading: "正在从仓库加载技能...",
    discoverEmpty: "未发现技能。",
    discoverNoMatch: "没有匹配的技能。",
    installBtn: "安装",
    installedBadge: "已安装",
    uninstallBtn: "卸载",
    repoManagerTitle: "管理仓库",
    addRepoLabel: "添加仓库",
    addRepoPlaceholder: "owner/repo 或 https://github.com/owner/repo",
    toastSkillInstalled: "技能 \"{name}\" 已安装",
    toastSkillUninstalled: "技能 \"{name}\" 已卸载",
    toastRepoAdded: "仓库已添加",
    toastRepoRemoved: "仓库已删除",
    toastZipInstalled: "已从 ZIP 安装 {count} 个技能",
    toastZipNoSkills: "ZIP 文件中未找到技能",
    commands: "个命令",
    localSkill: "本地",
    repoSkill: "仓库",
    sourceCommand: "命令",
    sourceSkill: "技能",
    // Prompt Templates
    promptTabEditor: "编辑器",
    promptTabTemplates: "模板库",
    insertSnippet: "-- 插入片段 --",
    appendToPrompt: "追加",
    replacePrompt: "替换",
    snippetLanguagePref: "语言：中文",
    snippetCodeQuality: "代码质量规则",
    snippetSecurity: "安全指南",
    snippetConcise: "简洁模式",
    snippetArchitect: "架构指南",
    toastSnippetInserted: "片段已插入",
    toastTemplateApplied: "模板已应用",
    mcpTabInstalled: "已安装",
    mcpTabPresets: "预设",
    mcpSearchPlaceholder: "在 GitHub 搜索 MCP 服务器...",
    mcpSearchLoading: "正在搜索 GitHub...",
    mcpRequiresApiKey: "需要配置 API 密钥",
    mcpInstalled: "已安装",
    mcpInstallBtn: "安装",
    mcpGithubBtn: "GitHub",
    mcpNoPresets: "暂无预设。",
    // Skills Discovery GitHub search
    skillsSearchGithub: "搜索 GitHub",
    skillsSearchGithubPlaceholder: "在 GitHub 搜索技能...",
    skillsGithubLoading: "正在搜索 GitHub...",
    // Settings
    settingsTitle: "设置",
    settingsGroupGeneral: "通用",
    settingsGroupPaths: "目录",
    settingsGroupBackup: "备份",
    settingsAutoStart: "开机自启",
    settingsAutoStartDesc: "登录系统时自动启动 VarSwitch",
    settingsMinTray: "最小化到托盘",
    settingsMinTrayDesc: "关闭窗口时隐藏到系统托盘",
    settingsConfigDir: "配置目录",
    settingsClaudePath: "Claude 设置",
    settingsCodexPath: "Codex 设置",
    settingsLogs: "运行日志",
    settingsLogsDesc: "出现问题时可打开日志文件夹查看或反馈",
    settingsOpen: "打开",
    settingsCopyPath: "复制路径",
    settingsBrowse: "浏览",
    settingsSavePath: "保存路径",
    settingsReset: "重置",
    settingsPathPlaceholder: "粘贴 settings 目录或 settings.json 路径",
    settingsPathEmpty: "请先输入编辑器设置路径",
    settingsCurrentPath: "当前使用路径",
    settingsDefaultPathLabel: "内置默认路径",
    settingsEditorStatusCustom: "手动路径",
    settingsEditorStatusDetected: "已自动识别",
    settingsEditorStatusDefault: "默认路径",
    settingsEditorHintCustom: "当前使用你保存的路径进行环境识别和同步。",
    settingsEditorHintDetected: "当前使用此设备上自动识别到的内置路径。",
    settingsEditorHintDefault: "暂未识别到该编辑器；如果装在其他位置，可以在这里手动指定。",
    settingsDefaultPath: "默认路径：{path}",
    settingsExport: "导出配置",
    settingsImport: "导入配置",
    settingsManualBackup: "手动配置备份",
    settingsManualBackupDesc: "导入与导出当前的所有配置",
    settingsAutoBackup: "自动备份",
    settingsAutoBackupDesc: "每次切换配置前自动快照，误操作可回滚",
    settingsViewBackups: "回滚",
    toastSettingsSaved: "设置已保存",
    toastEditorPathSaved: "已保存 {name} 路径",
    toastEditorPathReset: "已重置 {name} 路径",
    toastEditorPathBrowseCancelled: "已取消选择路径",
    toastExported: "配置已导出",
    toastImported2: "已导入 {count} 个配置",
    toastImportNone: "没有新配置可导入",
    settingsSilentStart: "静默启动",
    settingsSilentStartDesc: "启动时最小化到系统托盘",
    settingsLang: "语言",
    settingsTheme: "主题",
    supportSectionTitle: "快捷操作",
    supportHintDefault: "打开使用说明、检查更新，或直接访问仓库。",
    supportHintUpdateAvailable: "发现新版本：当前版本 {currentVersion}，新版本 {latestVersion}，点击即可后台下载并自动安装。",
    supportHintUpToDate: "当前已经是最新版本：{version}。",
    usageGuideBtn: "使用说明",
    updateCheckBtn: "检查更新",
    updateReleaseBtn: "安装更新 {version}",
    downloadSiteBtn: "下载网站",
    githubRepoBtn: "GitHub 仓库",
    usageGuideKicker: "使用说明",
    usageGuideTitle: "VarSwitch 使用说明",
    usageGuideIntro: "VarSwitch 可集中管理 Claude Code、Codex、提示词、MCP Server、移动端控制、设置与备份。",
    usageGuideStep1Title: "Claude Code 配置",
    usageGuideStep1Desc: "添加或导入 Token/Base URL 配置，可带 Key 验证接口连通性；切换后会同步系统环境变量、已支持编辑器和 ~/.claude/settings.json。",
    usageGuideStep2Title: "Codex 配置",
    usageGuideStep2Desc: "管理 Codex API Key、Base URL、模型、Provider 和写入方式，并同步 ~/.codex/config.toml 与 auth.json。",
    usageGuideStep3Title: "Codex Toolbox",
    usageGuideStep3Desc: "同步 Codex 会话，并绑定飞书/Lark、QQ、微信移动端控制。",
    usageGuideStep4Title: "Skills、Prompts 与 MCP",
    usageGuideStep4Desc: "顶部工具栏可编辑 Claude Skills/Commands、维护 CLAUDE.md 模板，并管理 ~/.claude.json 中的 MCP Server。",
    usageGuideStep5Title: "设置与备份",
    usageGuideStep5Desc: "设置语言、主题、托盘行为、开机启动、编辑器路径，支持配置导入导出、自动备份和回滚。",
    usageGuideStep6Title: "更新与生效提示",
    usageGuideStep6Desc: "首页可检查更新、打开下载站或仓库。切换配置后，请重启终端和编辑器让环境变量生效。",
    usageGuideClose: "关闭",
    usageGuideNever: "永不提醒",
    toastGuideDisabled: "已关闭使用说明提醒",
    checkingUpdates: "正在检查更新...",
    downloadingUpdate: "正在下载更新 {percent}%",
    installingUpdate: "正在安装更新...",
    openingReleasePage: "正在打开下载页...",
    toastUpdateAvailable: "发现新版本：当前版本 {currentVersion}，新版本 {latestVersion}",
    updatePillText: "发现新版本：当前版本 {currentVersion}，新版本 {latestVersion}",
    updatePillAction: "下载更新",
    toastUpdateDownloaded: "更新安装程序已启动，必要时 VarSwitch 会自动重启。",
    toastAlreadyLatest: "当前已经是最新版本",
    toastReleaseOpened: "已打开下载页",
    toastRepoOpened: "已打开仓库地址"
  }
};

let currentLang = localStorage.getItem(LANG_STORAGE_KEY) || "zh";
if (!I18N[currentLang]) {
  currentLang = "zh";
}
// 用户没有手动切换过语言时,忽略历史遗留的存储值,进入软件默认中文
if (!localStorage.getItem(LANG_USER_SET_KEY)) {
  currentLang = "zh";
  localStorage.setItem(LANG_STORAGE_KEY, "zh");
}

let currentTheme = localStorage.getItem(THEME_STORAGE_KEY) || "light";
if (currentTheme !== "light" && currentTheme !== "dark") {
  currentTheme = "light";
}

let profiles = [];
let codexProfiles = [];
let grokProfiles = [];
let geminiProfiles = [];
let grokDiagnostics = null;
let codexToolbox = null;
let codexDiagnostics = null;
let currentPage = "add-provider";
let activeConsolePage = "add-provider";
let selectedUniversalProviderPreset = "newapi";
let universalProviderSaving = false;
let codexEnableAfterSave = false;
let configurationFilter = "all";
let configurationSearch = "";
let lastClaudeStatus = null;
let lastCodexStatus = null;
let lastGrokStatus = null;
let lastGeminiStatus = null;
let editingGrokId = null;
let editingGeminiId = null;
let detectedEditors = {}; // { id: displayName }
let editingId = null;
let profileSaving = false;
let grokProfileSaving = false;
let geminiProfileSaving = false;
let switchingSnapshot = null;
let progressUnlisten = null;
let mobileChannelStatusUnlisten = null;
let isSwitchingProfile = false;
let skillsData = [];
let editingSkillName = null;
let skillSaving = false;
let mcpServers = {};
let editingMcpName = null;
let mcpSaving = false;
let discoverSkills = [];
let skillRepos = [];
let repoAdding = false;
let activeSkillsTab = "installed";
let discoverSearchQuery = "";
let discoverRepoFilter = "all";
let discoverStatusFilter = "all";
let isDiscovering = false;
let promptTemplates = [];
let activePromptTab = "editor";
let isShowingGithubSkills = false;
let updateInfo = null;
let updateBusy = false;
let updateBusyAction = null;
let updatePillHideTimer = null;
let updateDownloadPercent = 0;
let updateDownloadUnlisten = null;
let appSettings = null;
let appPaths = null;
let usageGuideAutoHandled = false;
let activeToolboxTab = "session";
let activeDeveloperTool = "skills";
let selectedCodexThreadId = "";
let selectedMobileChannel = "wechat";
let toolboxRefreshTimer = null;
let toolboxRefreshBusy = false;
let pendingApprovalIds = new Set(); // 正在提交的审批 requestId，防止同一卡片双重提交
let larkCredentialSaveTimer = null;
let toolboxRemoteBusy = false;
let toolboxSessionSyncBusy = false;
let toolboxSessionProgressTimer = null;
let toolboxSessionProgressValue = 0;
let toolboxSessionSearchQuery = "";
let toolboxSelectedSessionIds = new Set();
let toolboxSelectedTrashSessionIds = new Set();
let toolboxSessionTrashOpen = false;
let toolboxCopiedSessionId = "";
let mobileBindBusyAction = "";

/**
 * A10: 容器级事件委托，替代「innerHTML 全量重建后逐个卡片 addEventListener」。
 *
 * 为什么要这么做：
 * 1. 逐卡片绑定要求每次重渲染都重新绑一遍，一旦将来做局部更新（只替换某张卡片），
 *    极易出现同一个按钮被绑定两次、点一次触发两次的 bug。
 * 2. 委托只在容器上绑一次，重建 innerHTML 不影响监听器，也不会随卡片数量线性增长。
 *
 * 用 dataset 标记保证同一个容器只绑定一次（容器本身是 index.html 里的静态节点，
 * 不会被 innerHTML 替换掉，所以标记不会丢）。
 *
 * @param {HTMLElement|null} container 静态容器节点
 * @param {string} flag 该容器的绑定标记名，同一容器上多种用途请用不同 flag
 * @param {(action: string, target: HTMLElement, event: MouseEvent) => void} dispatch 分派函数
 */
function bindDelegatedActions(container, flag, dispatch) {
  if (!container) return;
  const key = `bound${flag}`;
  if (container.dataset[key] === "1") return;
  container.dataset[key] = "1";
  container.addEventListener("click", (event) => {
    const target = event.target.closest("[data-action]");
    // 只处理落在本容器内的按钮，避免嵌套容器互相截胡
    if (!target || !container.contains(target)) return;
    const action = target.getAttribute("data-action");
    if (!action) return;
    dispatch(action, target, event);
  });
}

/**
 * A10: 状态卡片里「复制」按钮的容器级委托。
 * 这些按钮用 data-copy 携带待复制文本，语义单一，单独给一个绑定入口。
 */
function bindDelegatedCopyButtons(container, flag) {
  if (!container) return;
  const key = `bound${flag}`;
  if (container.dataset[key] === "1") return;
  container.dataset[key] = "1";
  container.addEventListener("click", (event) => {
    const btn = event.target.closest(".copy-btn");
    if (!btn || !container.contains(btn)) return;
    event.stopPropagation();
    const text = btn.getAttribute("data-copy");
    if (!text) return;
    navigator.clipboard.writeText(text).then(() => showToast(t("toastCopied"), "success"));
  });
}

/**
 * A10: profile 卡片网格的公共委托绑定。
 * 四个 provider 的卡片按钮语义完全一致（switch / edit / delete + data-id），
 * 差异只有 provider 类型，所以共用一份分派逻辑。
 */
function bindProfileGridActions(grid, flag, type) {
  bindDelegatedActions(grid, flag, (action, target) => {
    const id = target.getAttribute("data-id");
    if (!id) return;
    // data-action 允许带 provider 前缀（如 codex-switch），统一去前缀后分派
    const kind = action.replace(/^(?:claude|codex|grok|gemini)-/, "");
    if (kind === "switch") switchAnyProviderProfile(type, id);
    else if (kind === "edit") editProviderProfile(type, id);
    else if (kind === "delete") deleteProviderProfile(type, id);
  });
}

function t(key, params) {
  const dict = I18N[currentLang] || I18N.en;
  const raw = dict[key] || I18N.en[key] || key;
  if (!params) {
    return raw;
  }
  return raw.replace(/\{(\w+)\}/g, (_, token) => {
    if (Object.prototype.hasOwnProperty.call(params, token)) {
      return String(params[token]);
    }
    return `{${token}}`;
  });
}

function esc(s) {
  const div = document.createElement("div");
  div.textContent = s;
  return div.innerHTML;
}

function setButtonBusy(button, busy, label) {
  if (!button) return;
  if (busy) {
    if (!button.dataset.originalText) {
      button.dataset.originalText = button.textContent;
    }
    button.disabled = true;
    button.classList.add("is-busy");
    button.innerHTML = `<span class="inline-spinner" aria-hidden="true"></span><span>${esc(label || button.dataset.originalText || "")}</span>`;
  } else {
    button.disabled = false;
    button.classList.remove("is-busy");
    if (button.dataset.originalText) {
      button.textContent = button.dataset.originalText;
      delete button.dataset.originalText;
    }
  }
}

function productIcon(kind) {
  if (kind === "anthropic") {
    return `<span class="product-icon product-icon-anthropic" aria-hidden="true">
      <img src="anthropic-color.svg" alt="">
    </span>`;
  }
  if (kind === "codex") {
    return `<span class="product-icon product-icon-codex" aria-hidden="true">
      <img src="OpenAI-black-monoblossom.svg" alt="">
    </span>`;
  }
  if (kind === "grok") {
    return `<span class="product-icon product-icon-grok" aria-hidden="true">
      <img src="grok-color.svg" width="16" height="16" alt="">
    </span>`;
  }
  if (kind === "gemini") {
    return `<span class="product-icon product-icon-gemini" aria-hidden="true">
      <img src="gemini-color.svg" width="16" height="16" alt="">
    </span>`;
  }
  return "";
}

function maskKey(key) {
  if (!key || key.length < 12) return key || "--";
  return `${key.slice(0, 6)}****${key.slice(-4)}`;
}

function truncUrl(url, max = 40) {
  if (!url) return "--";
  return url.length > max ? `${url.slice(0, max)}...` : url;
}

function shouldAutoOpenUsageGuide(settings) {
  if (typeof helpers.shouldAutoOpenUsageGuide === "function") {
    return helpers.shouldAutoOpenUsageGuide(settings);
  }
  return !settings || settings.neverShowUsageGuide !== true;
}

function getUpdateActionMode() {
  if (typeof helpers.getUpdateActionMode === "function") {
    return helpers.getUpdateActionMode(updateInfo, updateBusy);
  }
  if (updateBusy) return "busy";
  if (updateInfo?.hasUpdate) return "release";
  return "check";
}

function formatVersionTag(version) {
  if (typeof helpers.formatVersionTag === "function") {
    return helpers.formatVersionTag(version);
  }
  if (!version) return "";
  return version.startsWith("v") ? version : `v${version}`;
}

function updateVersionParams(info = updateInfo) {
  return {
    currentVersion: formatVersionTag(info?.currentVersion),
    latestVersion: formatVersionTag(info?.latestVersion),
    version: formatVersionTag(info?.latestVersion)
  };
}

function getEditorPathMode(editorInfo) {
  if (typeof helpers.getEditorPathMode === "function") {
    return helpers.getEditorPathMode(editorInfo);
  }
  if (editorInfo?.customized) return "custom";
  if (editorInfo?.detected) return "detected";
  return "default";
}

function validateEditorPathInput(value) {
  if (typeof helpers.validateEditorPathInput === "function") {
    return helpers.validateEditorPathInput(value);
  }
  const normalized = typeof value === "string" ? value.trim() : "";
  if (!normalized) {
    return { valid: false, reason: "empty" };
  }
  return { valid: true, value: normalized };
}

function syncAppSettingsAppearance() {
  if (!appSettings) return;
  appSettings.language = currentLang;
  appSettings.theme = currentTheme;
}

function currentSupportHint() {
  if (updateBusy) {
    if (updateBusyAction === "download") {
      return t("downloadingUpdate", { percent: updateDownloadPercent });
    }
    if (updateBusyAction === "install") {
      return t("installingUpdate");
    }
    return t("checkingUpdates");
  }
  if (updateInfo?.hasUpdate) {
    return t("supportHintUpdateAvailable", updateVersionParams());
  }
  return t("supportHintDefault");
}

function renderUpdateButton() {
  const btn = $("updateBtn");
  const textEl = $("updateBtnText");
  const hintEl = $("supportHint");
  if (!btn || !textEl || !hintEl) return;

  const mode = getUpdateActionMode();
  if (mode === "busy") {
    btn.disabled = true;
    if (updateBusyAction === "download") {
      textEl.textContent = t("downloadingUpdate", { percent: updateDownloadPercent });
    } else if (updateBusyAction === "install") {
      textEl.textContent = t("installingUpdate");
    } else {
      textEl.textContent = t("checkingUpdates");
    }
  } else if (mode === "release" && updateInfo?.latestVersion) {
    btn.disabled = false;
    textEl.textContent = t("updateReleaseBtn", updateVersionParams());
  } else {
    btn.disabled = false;
    textEl.textContent = t("updateCheckBtn");
  }

  hintEl.textContent = currentSupportHint();
  renderUpdatePill();
  renderUpdateDownloadProgress();
}

function renderUpdatePill() {
  const banner = $("updatePillBanner");
  const textEl = $("updatePillText");
  const actionEl = $("updatePillAction");
  if (!banner || !textEl || !actionEl) return;

  if (!updateInfo?.hasUpdate || !updateInfo.latestVersion) {
    banner.hidden = true;
    return;
  }

  textEl.textContent = t("updatePillText", updateVersionParams());
  actionEl.textContent = updateBusyAction === "download"
    ? `${updateDownloadPercent}%`
    : t("updatePillAction");
  banner.hidden = false;
}

function renderUpdateDownloadProgress() {
  const progress = $("updateDownloadProgress");
  const bar = $("updateDownloadProgressBar");
  if (!progress || !bar) return;
  const show = updateBusyAction === "download" || updateBusyAction === "install";
  progress.hidden = !show;
  bar.style.width = `${Math.max(0, Math.min(100, updateDownloadPercent))}%`;
}

function updateDownloadProgress(payload = {}) {
  const total = Number(payload.total) || 100;
  const step = Number(payload.step) || 0;
  const label = payload.label || "download";
  updateDownloadPercent = Math.max(0, Math.min(100, Math.round((step / total) * 100)));
  if (label === "install" || label === "done" || label === "open") {
    updateBusyAction = "install";
  } else {
    updateBusyAction = "download";
  }
  renderUpdateButton();
}

function hideUpdatePillSoon() {
  if (updatePillHideTimer) {
    clearTimeout(updatePillHideTimer);
  }
  updatePillHideTimer = setTimeout(() => {
    const banner = $("updatePillBanner");
    if (banner) {
      banner.hidden = true;
    }
    updatePillHideTimer = null;
  }, 3000);
}

function openUsageGuide() {
  $("usageGuideOverlay").classList.add("open");
}

function closeUsageGuide() {
  $("usageGuideOverlay").classList.remove("open");
}

async function loadAppSettings() {
  try {
    appSettings = await invoke("get_app_settings");
  } catch (error) {
    console.error("Failed to load app settings:", error);
    appSettings = appSettings || { neverShowUsageGuide: false };
  }
  appSettings = appSettings || {};
  appSettings.editorPaths = appSettings.editorPaths || {};
  syncAppSettingsAppearance();
  return appSettings;
}

async function persistAppSettings() {
  if (!appSettings) {
    await loadAppSettings();
  }
  appSettings.editorPaths = appSettings.editorPaths || {};
  syncAppSettingsAppearance();
  await invoke("save_app_settings", { settings: appSettings });
}

async function handleNeverShowUsageGuide() {
  try {
    if (!appSettings) {
      await loadAppSettings();
    }
    appSettings.neverShowUsageGuide = true;
    await persistAppSettings();
    closeUsageGuide();
    showToast(t("toastGuideDisabled"), "success");
  } catch (error) {
    showToast(String(error), "error");
  }
}

async function maybeOpenUsageGuide() {
  if (usageGuideAutoHandled) return;
  usageGuideAutoHandled = true;
  if (shouldAutoOpenUsageGuide(appSettings)) {
    openUsageGuide();
  }
}

async function checkForUpdates() {
  updateBusy = true;
  updateBusyAction = "check";
  renderUpdateButton();

  try {
    updateInfo = await invoke("check_app_update");
    if (updateInfo?.hasUpdate) {
      showToast(
        t("toastUpdateAvailable", updateVersionParams()),
        "success"
      );
      hideUpdatePillSoon();
    }
  } catch (error) {
    showToast(String(error), "error");
  } finally {
    updateBusy = false;
    updateBusyAction = null;
    renderUpdateButton();
  }
  return updateInfo;
}

async function checkForUpdatesOnStartup() {
  if (updateBusy) return;
  updateBusy = true;
  updateBusyAction = "startup";
  renderUpdateButton();

  try {
    updateInfo = await invoke("check_app_update");
  } catch (error) {
    console.warn("Startup update check failed:", error);
  } finally {
    updateBusy = false;
    updateBusyAction = null;
    renderUpdateButton();
    // 启动时只在版本不一致时显示更新横幅；相同版本保持安静。
  }
}

async function openUpdateReleasePage() {
  try {
    await invoke("open_external_target", { target: APP_DOWNLOAD_PAGE_URL });
    showToast(t("toastReleaseOpened"), "success");
  } catch (error) {
    showToast(String(error), "error");
  }
}

async function installAppUpdate() {
  updateBusy = true;
  updateBusyAction = "download";
  updateDownloadPercent = 0;
  renderUpdateButton();

  try {
    if (updateDownloadUnlisten) {
      updateDownloadUnlisten();
      updateDownloadUnlisten = null;
    }
    updateDownloadUnlisten = await listen("update-download-progress", (event) => {
      updateDownloadProgress(event.payload || {});
    });
    const result = await invoke("install_app_update");
    updateDownloadPercent = 100;
    updateBusyAction = "install";
    renderUpdateButton();
    showToast(
      t("toastUpdateDownloaded", { fileName: result?.fileName || "" }),
      "success"
    );
    hideUpdatePillSoon();
  } catch (error) {
    showToast(String(error), "error");
  } finally {
    if (updateDownloadUnlisten) {
      updateDownloadUnlisten();
      updateDownloadUnlisten = null;
    }
    setTimeout(() => {
      updateBusy = false;
      updateBusyAction = null;
      updateDownloadPercent = 0;
      renderUpdateButton();
    }, 900);
  }
}

async function handleUpdateButton() {
  const mode = getUpdateActionMode();
  if (mode === "busy") return;
  if (mode === "release") {
    await installAppUpdate();
    return;
  }
  const info = await checkForUpdates();
  if (info?.hasUpdate) {
    await installAppUpdate();
  }
}

async function openGitHubRepo() {
  try {
    await invoke("open_external_target", { target: APP_REPOSITORY_URL });
    showToast(t("toastRepoOpened"), "success");
  } catch (error) {
    showToast(String(error), "error");
  }
}

function updateSettingsSegControls() {
  const states = [
    ["settingsLangZh", currentLang === "zh"],
    ["settingsLangEn", currentLang === "en"],
    ["settingsThemeLight", currentTheme === "light"],
    ["settingsThemeDark", currentTheme === "dark"],
  ];
  states.forEach(([id, active]) => {
    const button = $(id);
    if (!button) return;
    button.classList.toggle("active", active);
    button.setAttribute("aria-pressed", String(active));
  });
}

function updateThemeSegControl() {
  updateSettingsSegControls();
  const lightBtn = $("themeLightBtn");
  const darkBtn = $("themeDarkBtn");
  if (!lightBtn || !darkBtn) return;
  if (currentTheme === "light") {
    lightBtn.classList.add("active");
    darkBtn.classList.remove("active");
  } else {
    lightBtn.classList.remove("active");
    darkBtn.classList.add("active");
  }
  lightBtn.textContent = t("switchToLight");
  darkBtn.textContent = t("switchToDark");
}

function updateLangSegControl() {
  updateSettingsSegControls();
  const zhBtn = $("langZhBtn");
  const enBtn = $("langEnBtn");
  if (!zhBtn || !enBtn) return;
  if (currentLang === "zh") {
    zhBtn.classList.add("active");
    enBtn.classList.remove("active");
  } else {
    zhBtn.classList.remove("active");
    enBtn.classList.add("active");
  }
}

function applyTheme() {
  document.documentElement.setAttribute("data-theme", currentTheme);
  updateThemeSegControl();
}

function applyLanguage() {
  try {
  document.documentElement.lang = currentLang === "zh" ? "zh-CN" : "en";
  document.title = t("appTitle");

  setText("appTitle", t("appTitle"));
  setText("appSubtitle", t("appSubtitle"));
  const supportTitle = $("supportSectionTitle");
  if (supportTitle) supportTitle.textContent = t("supportSectionTitle");
  setText("usageGuideBtnText", t("usageGuideBtn"));
  const downloadSiteBtnText = $("downloadSiteBtnText");
  if (downloadSiteBtnText) downloadSiteBtnText.textContent = t("downloadSiteBtn");
  setText("githubRepoBtnText", t("githubRepoBtn"));
  updateClaudeStatusTitle();
  setText("statusHint", t("statusHint"));
  setText("profilesSectionTitle", currentLang === "zh" ? "Claude 配置列表" : "Claude Config List");
  setText("profileNameLabel", t("nameLabel"));
  setText("profileApiKeyLabel", t("tokenLabel"));
  setText("profileBaseUrlLabel", t("urlLabel"));
  setText("profileEndpointTestBtn", t("endpointTest"));
  setText("profileModelFetchBtn", t("modelFetch"));
  setText("cancelBtn", t("cancel"));
  setText("submitBtn", t("save"));
  setText("switchPanelTitle", t("switchingTo"));
  setText("switchStep1Text", t("stepSystem"));
  setText("switchStep2Text", t("stepEditors"));
  setText("switchStep3Text", t("stepClaude"));
  setText("switchCancelBtn", t("cancelSwitch"));
  setText("switchStepLabel", t("preparing"));
  setText("activeConfigLabel", t("activeConfigLabel"));
  setText("claudeSyncBtn", t("syncNow"));
  setText("claudePageImportBtn", t("importBtn"));
  setText("usageGuideKicker", t("usageGuideKicker"));
  setText("usageGuideTitle", t("usageGuideTitle"));
  setText("usageGuideIntro", t("usageGuideIntro"));
  setText("usageGuideStep1Title", t("usageGuideStep1Title"));
  setText("usageGuideStep1Desc", t("usageGuideStep1Desc"));
  setText("usageGuideStep2Title", t("usageGuideStep2Title"));
  setText("usageGuideStep2Desc", t("usageGuideStep2Desc"));
  setText("usageGuideStep3Title", t("usageGuideStep3Title"));
  setText("usageGuideStep3Desc", t("usageGuideStep3Desc"));
  setText("usageGuideStep4Title", t("usageGuideStep4Title"));
  setText("usageGuideStep4Desc", t("usageGuideStep4Desc"));
  setText("usageGuideStep5Title", t("usageGuideStep5Title"));
  setText("usageGuideStep5Desc", t("usageGuideStep5Desc"));
  setText("usageGuideStep6Title", t("usageGuideStep6Title"));
  setText("usageGuideStep6Desc", t("usageGuideStep6Desc"));
  setText("usageGuideCloseBtn", t("usageGuideClose"));
  setText("usageGuideNeverBtn", t("usageGuideNever"));

  applyUsagePanelLanguage();

  setPlaceholder("profileName", t("placeholderName"));
  setPlaceholder("profileApiKey", t("placeholderApiKey"));
  setPlaceholder("profileBaseUrl", t("placeholderBaseUrl"));
  setText("profileModelIdLabel", t("modelIdLabel"));
  setPlaceholder("profileModelId", t("placeholderModelId"));
  setText("profileModelIdHint", t("modelIdHint"));
  setText("profilePresetLabel", t("providerPresetLabel"));
  setText("profilePresetHint", t("claudePresetHintDefault"));
  setText("profileApiFormatLabel", t("claudeApiFormatLabel"));
  setText("profileApiFormatHint", t("claudeApiFormatHint"));
  const claudeApiFormatSelect = $("profileApiFormat");
  if (claudeApiFormatSelect && claudeApiFormatSelect.options.length >= 2) {
    claudeApiFormatSelect.options[0].textContent = t("claudeApiFormatOptionAnthropic");
    claudeApiFormatSelect.options[1].textContent = t("claudeApiFormatOptionOpenAiChat");
  }
  renderClaudePresetOptions();

  // Codex page labels
  updateCodexStatusTitle();
  setText("codexProfilesSectionTitle", t("codexProfilesTitle"));
  setText("codexSyncBtn", t("syncNow"));
  setText("codexPageImportBtn", t("importBtn"));
  setText("codexPresetLabel", t("providerPresetLabel"));
  setText("codexWireApiLabel", t("codexWireApiLabel"));
  setText("codexWireApiHint", t("codexWireApiHint"));
  const codexWireSelect = $("codexWireApi");
  if (codexWireSelect && codexWireSelect.options.length >= 2) {
    codexWireSelect.options[0].textContent = currentLang === "zh"
      ? "OpenAI Responses（官方 / 兼容网关）"
      : "OpenAI Responses (official / compatible gateways)";
    codexWireSelect.options[1].textContent = currentLang === "zh"
      ? "OpenAI Chat Completions（DeepSeek、Kimi 等）"
      : "OpenAI Chat Completions (DeepSeek, Kimi, etc.)";
  }
  setText("codexAdvancedTitle", t("codexAdvancedTitle"));
  setPlaceholder("codexBaseUrl", currentLang === "zh" ? "留空使用官方地址 https://api.openai.com/v1" : "Leave empty for official https://api.openai.com/v1");
  setText("codexNameLabel", t("codexNameLabel"));
  setText("codexApiKeyLabel", t("codexApiKeyLabel"));
  setText("codexBaseUrlLabel", t("codexBaseUrlLabel"));
  setText("codexEndpointTestBtn", t("endpointTest"));
  setText("codexModelFetchBtn", t("modelFetch"));
  setText("codexModelLabel", t("codexModelLabel"));
  setText("codexProviderLabel", t("codexProviderLabel"));
  setText("codexImageSectionTitle", t("codexImageSectionTitle"));
  setText("codexImageSectionHint", t("codexImageSectionHint"));
  setText("codexImageApiKeyLabel", t("codexImageApiKeyLabel"));
  setText("codexImageBaseUrlLabel", t("codexImageBaseUrlLabel"));
  setText("codexAuthModeLabel", t("codexAuthModeLabel"));
  setText("codexAuthModeDefaultTitle", t("codexAuthModeDefaultTitle"));
  setText("codexAuthModeDefaultHint", t("codexAuthModeDefaultHint"));
  setText("codexAuthModeOfficialTitle", t("codexAuthModeOfficialTitle"));
  setText("codexAuthModeOfficialHint", t("codexAuthModeOfficialHint"));
  setText("codexOfficialConfigLabel", t("codexOfficialConfigLabel"));
  setText("codexCopyOfficialConfigBtn", t("copy"));
  setText("codexCancelBtn", t("cancel"));
  setText("codexSubmitBtn", t("save"));
  // Grok page labels
  updateGrokActiveConfigBar();
  if ($("grokProfilesSectionTitle")) setText("grokProfilesSectionTitle", t("grokProfilesTitle"));
  if ($("grokPresetLabel")) setText("grokPresetLabel", t("grokPresetLabel"));
  if ($("grokNameLabel")) setText("grokNameLabel", t("grokNameLabel"));
  if ($("grokApiKeyLabel")) setText("grokApiKeyLabel", t("grokApiKeyLabel"));
  if ($("grokBaseUrlLabel")) setText("grokBaseUrlLabel", t("grokBaseUrlLabel"));
  if ($("grokEndpointTestBtn")) setText("grokEndpointTestBtn", t("endpointTest"));
  if ($("grokModelFetchBtn")) setText("grokModelFetchBtn", t("modelFetch"));
  if ($("grokModelLabel")) setText("grokModelLabel", t("grokModelLabel"));
  if ($("grokModelHint")) setText("grokModelHint", t("grokModelHint"));
  if ($("grokApiBackendLabel")) setText("grokApiBackendLabel", t("grokApiBackendLabel"));
  if ($("grokApiBackendHint")) setText("grokApiBackendHint", t("grokApiBackendHint"));
  if ($("grokCancelBtn")) setText("grokCancelBtn", t("cancel"));
  if ($("grokSubmitBtn")) setText("grokSubmitBtn", t("save"));
  if ($("grokPageAddBtn")) setText("grokPageAddBtn", t("grokAddConfig"));
  if ($("grokRefreshBtn")) setText("grokRefreshBtn", currentLang === "zh" ? "立即同步" : "Sync now");
  if ($("grokPageImportBtn")) setText("grokPageImportBtn", t("grokImportCurrent"));
  if ($("grokProfileName")) setPlaceholder("grokProfileName", t("placeholderName"));
  if ($("grokApiKey")) setPlaceholder("grokApiKey", "xai-...");
  if ($("grokBaseUrl")) setPlaceholder("grokBaseUrl", currentLang === "zh" ? "留空使用官方地址 https://api.x.ai/v1" : "Leave empty for official https://api.x.ai/v1");
  if ($("grokModel")) setPlaceholder("grokModel", "e.g. grok-4");
  renderGrokPresetOptions();
  const codexToolboxBtn = $("codexToolboxBtn");
  if (codexToolboxBtn) codexToolboxBtn.textContent = t("codexToolbox");
  setText("codexToolboxTitle", t("codexToolboxTitle"));
  setText("toolboxTabSession", t("toolboxTabSession"));
  setText("toolboxTabRemote", t("toolboxTabRemote"));
  setText("toolboxSessionHint", t("toolboxSessionHint"));
  setText("sessionPageSyncBtn", t("toolboxSyncNow"));
  setText("toolboxSyncedThreadsTitle", t("toolboxSyncedThreadsTitle"));
  setText("toolboxSessionSafeNote", t("toolboxSessionSafeNote"));
  setPlaceholder("toolboxSessionSearchInput", t("toolboxSessionSearchPlaceholder"));
  setText("toolboxSessionTrashTitle", t("toolboxSessionTrashTitle"));
  setText("toolboxSessionTrashHint", t("toolboxSessionTrashHint"));
  setText("toolboxSessionTrashCloseBtn", t("toolboxSessionTrashClose"));
  setText("toolboxSessionRestoreBtn", t("toolboxSessionRestoreSelected"));
  setText("toolboxRemoteHint", t("toolboxRemoteHint"));
  setText("toolboxMobileAppLabel", t("toolboxMobileAppLabel"));
  setText("toolboxMobileThreadLabel", t("toolboxMobileThreadLabel"));
  setText("toolboxRemoteStartBtn", t("toolboxRemoteStart"));
  setText("toolboxRemoteStopBtn", t("toolboxRemoteStop"));
  renderCodexPresetOptions();
  updateCodexPresetHint();

  // Management panel labels
  setTitle("skillsBtn", t("skillsManage"));
  setTitle("promptsBtn", t("promptsManage"));
  setTitle("mcpBtn", t("mcpManage"));
  setText("skillsTitle", t("skillsTitle"));
  setText("addSkillBtn", t("addSkill"));
  setText("skillNameLabel2", t("skillName"));
  setText("skillContentLabel", t("skillContent"));
  setPlaceholder("skillNameInput", t("skillNamePlaceholder"));
  setText("skillCancelBtn", t("cancel"));
  setText("skillSaveBtn", t("save"));
  setText("promptsTitle2", t("promptsTitle"));
  setText("promptsPath", t("promptsPathLabel"));
  setText("promptSaveBtn", t("save"));
  setText("mcpTitle2", t("mcpTitle"));
  setText("mcpTabInstalled", t("mcpTabInstalled"));
  setText("mcpTabPresets", t("mcpTabPresets"));
  setPlaceholder("mcpPresetSearch", t("mcpSearchPlaceholder"));
  setText("mcpPresetLoadingText", t("mcpSearchLoading"));
  setText("mcpPath", t("mcpPathLabel"));
  setText("addMcpBtn", t("addMcp"));
  setText("mcpNameLabel2", t("mcpName"));
  setText("mcpConfigLabel", t("mcpConfig"));
  setPlaceholder("mcpNameInput", t("mcpNamePlaceholder"));
  setText("mcpCancelBtn", t("cancel"));
  setText("mcpSaveBtn", t("save"));

  // Skills Discovery labels
  setText("skillsTabInstalled", t("skillsTabInstalled"));
  setText("skillsTabDiscover", t("skillsTabDiscover"));
  setPlaceholder("discoverSearch", t("discoverSearchPlaceholder"));
  setText("discoverLoadingText", t("discoverLoading"));
  setText("manageReposBtn", t("manageRepos"));
  setText("repoManagerTitle", t("repoManagerTitle"));
  setText("addRepoLabel", t("addRepoLabel"));
  setPlaceholder("repoUrlInput", t("addRepoPlaceholder"));
  setText("searchGithubSkillsBtnText", t("skillsSearchGithub"));
  setText("discoverGithubBannerText", t("skillsGithubResults"));
  setText("backToCatalogBtnText", t("skillsBackToCatalog"));

  // Prompt tabs
  setText("promptTabEditor", t("promptTabEditor"));
  setText("promptTabTemplates", t("promptTabTemplates"));
  const insertSelect = $("promptInsertSelect");
  if (insertSelect.options.length > 0) {
    insertSelect.options[0].textContent = t("insertSnippet");
  }

  // Update discover filter labels
  const repoFilter = $("discoverRepoFilter");
  if (repoFilter.options.length > 0) {
    repoFilter.options[0].textContent = t("allRepos");
  }
  const statusFilter = $("discoverStatusFilter");
  if (statusFilter.options.length >= 3) {
    statusFilter.options[0].textContent = t("filterAll");
    statusFilter.options[1].textContent = t("filterInstalled");
    statusFilter.options[2].textContent = t("filterNotInstalled");
  }

  // Settings panel labels
  setText("settingsTitle2", t("settingsTitle"));
  setText("settingsGroupGeneral", t("settingsGroupGeneral"));
  setText("settingsGroupPaths", t("settingsGroupPaths"));
  setText("settingsGroupBackup", t("settingsGroupBackup"));
  setText("settingsAutoStartLabel", t("settingsAutoStart"));
  setText("settingsAutoStartDesc", t("settingsAutoStartDesc"));
  setText("settingsMinTrayLabel", t("settingsMinTray"));
  setText("settingsMinTrayDesc", t("settingsMinTrayDesc"));
  setText("settingsConfigDirLabel", t("settingsConfigDir"));
  setText("settingsClaudePathLabel", t("settingsClaudePath"));
  setText("settingsCodexPathLabel", t("settingsCodexPath"));
  setText("settingsLogsLabel", t("settingsLogs"));
  setText("settingsLogsValue", t("settingsLogsDesc"));
  setText("settingsOpenConfigDir", t("settingsOpen"));
  setText("settingsOpenClaudeDir", t("settingsOpen"));
  setText("settingsOpenCodexDir", t("settingsOpen"));
  setText("settingsOpenLogsDir", t("settingsOpen"));
  setText("settingsOpenBackupsBtn", t("settingsOpen"));
  document.querySelectorAll("[data-settings-copy-path]").forEach((button) => {
    button.title = t("settingsCopyPath");
    button.setAttribute("aria-label", t("settingsCopyPath"));
  });
  setText("settingsManualBackupLabel", t("settingsManualBackup"));
  setText("settingsManualBackupDesc", t("settingsManualBackupDesc"));
  setText("settingsExportBtn", t("settingsExport"));
  setText("settingsImportBtn", t("settingsImport"));
  setText("settingsAutoBackupLabel", t("settingsAutoBackup"));
  setText("settingsAutoBackupDesc", t("settingsAutoBackupDesc"));
  setText("settingsViewBackupsBtn", t("settingsViewBackups"));
  setText("settingsSilentStartLabel", t("settingsSilentStart"));
  setText("settingsSilentStartDesc", t("settingsSilentStartDesc"));

  updateLangSegControl();
  updateThemeSegControl();
  renderUpdateButton();

  if ($("modalOverlay")?.classList.contains("open")) {
    setText("modalTitle", editingId ? t("editConfig") : t("addConfig"));
  }
  if ($("settingsOverlay")?.classList.contains("open")) {
    try { renderSettingsEditorPaths(getSettingsEditorPathInfos()); } catch (_) {}
  }
  } catch (error) {
    console.error("applyLanguage failed:", error);
  }
}

function setLanguage(lang) {
  currentLang = lang;
  localStorage.setItem(LANG_STORAGE_KEY, currentLang);
  localStorage.setItem(LANG_USER_SET_KEY, "1");
  syncAppSettingsAppearance();
  applyLanguage();
  renderProfiles();
  loadStatus();
  if (currentPage === "codex") {
    renderCodexProfiles();
    loadCodexStatus();
  }
  if (currentPage === "grok") {
    renderGrokProfiles();
    loadGrokStatus();
    loadGrokDiagnostics();
  }
  if (currentPage === "gemini") {
    renderGeminiProfiles();
    loadGeminiStatus();
  }
  if (codexToolbox) {
    renderCodexToolbox();
  }
  if (activeConsolePage === "usage") {
    loadUsageDashboard();
  }
}

function setTheme(theme) {
  currentTheme = theme;
  localStorage.setItem(THEME_STORAGE_KEY, currentTheme);
  syncAppSettingsAppearance();
  applyTheme();
}

async function loadCodexToolbox() {
  try {
    codexToolbox = await invoke("get_codex_toolbox");
    await loadSmartControlDebug(false);
    renderCodexToolbox();
    renderNavigationStatus();
  } catch (error) {
    console.error("loadCodexToolbox failed:", error);
    if (typeof showToast === "function") showToast(String(error), "error");
  }
}

/**
 * B15: 订阅后端推送的手机通道状态。
 *
 * 后端在通道状态发生变化时 emit("mobile-channel-status", <ToolboxSnapshot>)，
 * payload 与 get_codex_toolbox 返回值同结构，这里直接整体替换本地缓存。
 * 只在工具箱页可见时重渲染，其他页面留给下次进入时统一渲染，避免无谓开销。
 */
async function bindMobileChannelStatusListener() {
  if (mobileChannelStatusUnlisten) return;
  try {
    mobileChannelStatusUnlisten = await listen("mobile-channel-status", (event) => {
      if (!event?.payload) return;
      codexToolbox = event.payload;
      if (activeConsolePage === "toolbox") renderCodexToolbox();
      // 侧边栏绑定状态指示灯不依赖当前页面，始终同步
      renderNavigationStatus();
    });
  } catch (error) {
    console.error("bindMobileChannelStatusListener failed:", error);
  }
}

async function loadSmartControlDebug(showError = false) {
  try {
    codexDiagnostics = await invoke("get_smart_control_debug");
    return codexDiagnostics;
  } catch (error) {
    if (showError) showToast(String(error), "error");
    return null;
  }
}

function switchToolboxTab(tab) {
  activeToolboxTab = tab;
  const sessionTab = $("toolboxTabSession");
  const remoteTab = $("toolboxTabRemote");
  sessionTab?.classList.toggle("active", tab === "session");
  remoteTab?.classList.toggle("active", tab === "remote");
  sessionTab?.setAttribute("aria-selected", String(tab === "session"));
  remoteTab?.setAttribute("aria-selected", String(tab === "remote"));
  const session = $("toolboxSessionContent");
  const remote = $("toolboxRemoteContent");
  if (session) session.style.display = tab === "session" ? "" : "none";
  if (remote) remote.style.display = tab === "remote" ? "" : "none";
}

function openCodexToolbox() {
  switchConsolePage("toolbox");
  switchToolboxTab("session");
  loadCodexToolbox();
}

function closeCodexToolbox() {
  $("codexToolboxOverlay")?.classList.remove("open");
  stopToolboxRefresh();
}

function startToolboxRefresh(ticks = 8) {
  stopToolboxRefresh();
  let remaining = ticks;
  toolboxRefreshTimer = setInterval(async () => {
    if (toolboxRefreshBusy) return;
    // Toolbox 已改为内联页面，存活条件按当前页面判定（旧 overlay 不再加 open class）
    if (activeConsolePage !== "toolbox" || remaining <= 0) {
      stopToolboxRefresh();
      return;
    }
    remaining -= 1;
    toolboxRefreshBusy = true;
    try {
      // B13: 飞书注册轮询已由后端 worker 按服务端下发的 interval 单点执行，
      // 前端这里不再并行调 poll_lark_bot_registration，避免双重轮询打爆接口频率限制。
      // 飞书状态只读后端缓存（get_codex_toolbox / mobile-channel-status 事件推送）。
      // 微信暂时仍只有前端在轮询，删掉会导致扫码绑定卡住，因此保留。
      const wechat = codexToolbox?.mobileChannels?.find((binding) => binding.channel === "wechat");
      if (wechat?.qrDeviceCode && !wechat?.botToken) {
        codexToolbox = await invoke("poll_wechat_qr_binding", { verifyCode: "" });
      } else {
        codexToolbox = await invoke("get_codex_toolbox");
      }
      await loadSmartControlDebug(false);
      renderCodexToolbox();
    } catch (_) {
      codexToolbox = await invoke("get_codex_toolbox").catch(() => codexToolbox);
      await loadSmartControlDebug(false);
      renderCodexToolbox();
    } finally {
      toolboxRefreshBusy = false;
    }
  }, 1000);
}

function stopToolboxRefresh() {
  if (toolboxRefreshTimer) {
    clearInterval(toolboxRefreshTimer);
    toolboxRefreshTimer = null;
  }
  toolboxRefreshBusy = false;
}

function renderCodexToolbox() {
  if (!codexToolbox) return;
  renderToolboxSessionSync();
  renderToolboxSyncedThreads();
  renderToolboxMobileControl();
  renderToolboxRemote();
}

function renderToolboxSessionSync() {
  const summary = $("toolboxSessionSummary");
  const state = codexToolbox?.sessionSync || {};
  const count = state.total || (codexToolbox?.syncedCodexThreads || []).length || 0;
  renderToolboxSessionProgress();
  if (!state.lastSyncedAt && !count) {
    summary.textContent = t("toolboxSessionSummaryEmpty");
    return;
  }
  summary.textContent = t("toolboxSessionSummary", {
    count,
    time: state.lastSyncedAt || "--",
  });
}

function renderToolboxSessionProgress() {
  const progress = $("toolboxSessionProgress");
  if (!progress) return;
  progress.hidden = !toolboxSessionSyncBusy;
  [$("sessionPageSyncBtn")].filter(Boolean).forEach((button) => {
    button.disabled = toolboxSessionSyncBusy;
    button.classList.toggle("is-busy", toolboxSessionSyncBusy);
  });
}

function updateToolboxSessionProgress(percent, label) {
  toolboxSessionProgressValue = Math.max(0, Math.min(100, Number(percent) || 0));
  const progress = $("toolboxSessionProgress");
  const bar = $("toolboxSessionProgressBar");
  const labelEl = $("toolboxSessionProgressLabel");
  const percentEl = $("toolboxSessionProgressPercent");
  if (progress) progress.hidden = false;
  if (bar) bar.style.width = `${toolboxSessionProgressValue}%`;
  if (labelEl) labelEl.textContent = label || "正在同步 Codex 会话...";
  if (percentEl) percentEl.textContent = `${Math.round(toolboxSessionProgressValue)}%`;
}

function startToolboxSessionProgress() {
  toolboxSessionSyncBusy = true;
  updateToolboxSessionProgress(8, "正在扫描本地 Codex 会话...");
  if (toolboxSessionProgressTimer) {
    clearInterval(toolboxSessionProgressTimer);
  }
  toolboxSessionProgressTimer = setInterval(() => {
    const next = toolboxSessionProgressValue < 72
      ? toolboxSessionProgressValue + 9
      : toolboxSessionProgressValue < 92
        ? toolboxSessionProgressValue + 2
        : toolboxSessionProgressValue;
    updateToolboxSessionProgress(next, next < 72 ? "正在导入会话记录..." : "正在整理会话索引...");
  }, 360);
  renderToolboxSessionProgress();
}

function finishToolboxSessionProgress(success) {
  if (toolboxSessionProgressTimer) {
    clearInterval(toolboxSessionProgressTimer);
    toolboxSessionProgressTimer = null;
  }
  updateToolboxSessionProgress(success ? 100 : Math.max(35, toolboxSessionProgressValue), success ? "同步完成" : "同步失败");
  window.setTimeout(() => {
    toolboxSessionSyncBusy = false;
    const progress = $("toolboxSessionProgress");
    if (progress) progress.hidden = true;
    renderToolboxSessionProgress();
  }, success ? 520 : 900);
}

function parseToolboxSessionTime(value) {
  if (typeof value === "number" && Number.isFinite(value)) {
    return value > 1_000_000_000_000 ? value : value * 1000;
  }
  const text = String(value || "").trim();
  if (!text) return 0;
  if (/^\d+$/.test(text)) {
    const numeric = Number(text);
    return numeric > 1_000_000_000_000 ? numeric : numeric * 1000;
  }
  const parsed = Date.parse(text);
  return Number.isFinite(parsed) ? parsed : 0;
}

function formatToolboxSessionTime(value) {
  const time = parseToolboxSessionTime(value);
  if (!time) return "--";
  const diffSeconds = Math.max(0, Math.floor((Date.now() - time) / 1000));
  if (diffSeconds < 3600) {
    const minutes = Math.max(1, Math.floor(diffSeconds / 60));
    return currentLang === "zh" ? `${minutes} 分钟前` : `${minutes}m ago`;
  }
  if (diffSeconds < 86400) {
    const hours = Math.floor(diffSeconds / 3600);
    return currentLang === "zh" ? `${hours} 小时前` : `${hours}h ago`;
  }
  if (diffSeconds < 604800) {
    const days = Math.floor(diffSeconds / 86400);
    return currentLang === "zh" ? `${days} 天前` : `${days}d ago`;
  }
  return new Date(time).toLocaleString();
}

function shortToolboxSessionId(sessionId) {
  const text = String(sessionId || "");
  return text.length <= 18 ? text : `${text.slice(0, 8)}...${text.slice(-6)}`;
}

function toolboxSessionTitle(thread) {
  return thread.threadName || thread.lastUserMessage || thread.id || t("toolboxSessionProjectUnknown");
}

function toolboxSessionProjectLabel(cwd) {
  const normalized = String(cwd || "").replace(/\\/g, "/").replace(/\/+$/, "");
  const parts = normalized.split("/").filter(Boolean);
  return parts[parts.length - 1] || t("toolboxSessionProjectUnknown");
}

function filterToolboxSessions(threads) {
  const query = toolboxSessionSearchQuery.trim().toLowerCase();
  if (!query) return threads;
  return threads.filter((thread) => {
    const haystack = [
      thread.id,
      thread.threadName,
      thread.cwd,
      thread.sessionFile,
      thread.lastUserMessage,
      thread.lastAssistantMessage,
    ].join("\n").toLowerCase();
    return haystack.includes(query);
  });
}

function groupToolboxSessions(threads) {
  const groups = new Map();
  threads.forEach((thread) => {
    const key = thread.cwd || t("toolboxSessionProjectUnknown");
    if (!groups.has(key)) groups.set(key, []);
    groups.get(key).push(thread);
  });
  return Array.from(groups.entries())
    .map(([cwd, sessions]) => ({
      cwd,
      sessions: sessions
        .slice()
        .sort((a, b) => parseToolboxSessionTime(b.updatedAt) - parseToolboxSessionTime(a.updatedAt)),
      latest: Math.max(...sessions.map((item) => parseToolboxSessionTime(item.updatedAt)), 0),
    }))
    .sort((a, b) => b.latest - a.latest || toolboxSessionProjectLabel(a.cwd).localeCompare(toolboxSessionProjectLabel(b.cwd)));
}

function syncToolboxSessionSelection(threads) {
  const valid = new Set(threads.map((thread) => thread.id));
  toolboxSelectedSessionIds = new Set([...toolboxSelectedSessionIds].filter((id) => valid.has(id)));
  const trashed = new Set((codexToolbox?.trashedCodexThreads || []).map((thread) => thread.id));
  toolboxSelectedTrashSessionIds = new Set([...toolboxSelectedTrashSessionIds].filter((id) => trashed.has(id)));
}

function renderToolboxSessionRow(thread) {
  const selected = toolboxSelectedSessionIds.has(thread.id);
  const copied = toolboxCopiedSessionId === thread.id;
  const preview = thread.lastUserMessage || thread.lastAssistantMessage || "";
  return `
    <div class="toolbox-session-row ${selected ? "selected" : ""}">
      <label class="toolbox-session-row-main">
        <input type="checkbox" data-toolbox-session-check="${esc(thread.id)}" ${selected ? "checked" : ""}>
        <span class="toolbox-session-row-text">
          <strong title="${esc(toolboxSessionTitle(thread))}">${esc(toolboxSessionTitle(thread))}</strong>
          <span class="toolbox-session-row-meta">${esc(t("toolboxSessionIdLabel"))}: ${esc(shortToolboxSessionId(thread.id))}</span>
          ${thread.sessionFile ? `<span class="toolbox-session-row-meta">${esc(t("toolboxSessionFileLabel"))}: ${esc(thread.sessionFile)}</span>` : ""}
          ${preview ? `<span class="toolbox-session-row-preview">${esc(preview)}</span>` : ""}
        </span>
      </label>
      <div class="toolbox-session-row-actions">
        <button class="toolbox-session-copy-btn ${copied ? "is-copied" : ""}" data-toolbox-session-copy="${esc(thread.id)}" type="button" title="${esc(t("toolboxSessionCopyId"))}">
          ${copied ? "✓" : "⧉"}
        </button>
        <span class="toolbox-session-row-time">${esc(formatToolboxSessionTime(thread.updatedAt))}</span>
      </div>
    </div>
  `;
}

function renderToolboxSessionGroup(group) {
  const ids = group.sessions.map((thread) => thread.id);
  const allSelected = ids.length > 0 && ids.every((id) => toolboxSelectedSessionIds.has(id));
  return `
    <section class="toolbox-session-folder">
      <div class="toolbox-session-folder-row">
        <label class="toolbox-session-folder-main">
          <input type="checkbox" data-toolbox-session-group="${esc(ids.join(","))}" ${allSelected ? "checked" : ""}>
          <span class="toolbox-session-folder-icon" aria-hidden="true">▣</span>
          <span class="toolbox-session-folder-text">
            <strong title="${esc(group.cwd)}">${esc(toolboxSessionProjectLabel(group.cwd))}</strong>
            <span>${esc(group.cwd || t("toolboxSessionProjectUnknown"))}</span>
          </span>
        </label>
        <span class="toolbox-session-folder-meta">${group.sessions.length} · ${esc(formatToolboxSessionTime(group.latest))}</span>
      </div>
      <div class="toolbox-session-folder-children">
        ${group.sessions.map(renderToolboxSessionRow).join("")}
      </div>
    </section>
  `;
}

function renderToolboxSessionTrash() {
  const drawer = $("toolboxSessionTrashDrawer");
  const list = $("toolboxSessionTrashList");
  const trash = codexToolbox?.trashedCodexThreads || [];
  drawer.hidden = !toolboxSessionTrashOpen;
  if (!toolboxSessionTrashOpen) return;
  $("toolboxSessionRestoreBtn").disabled = toolboxSelectedTrashSessionIds.size === 0;
  if (trash.length === 0) {
    list.innerHTML = `<div class="mgmt-empty">${t("toolboxSessionTrashEmpty")}</div>`;
    return;
  }
  list.innerHTML = trash.map((thread) => {
    const selected = toolboxSelectedTrashSessionIds.has(thread.id);
    return `
      <label class="toolbox-session-trash-row">
        <input type="checkbox" data-toolbox-trash-check="${esc(thread.id)}" ${selected ? "checked" : ""}>
        <span class="toolbox-session-trash-row-text">
          <strong>${esc(toolboxSessionTitle(thread))}</strong>
          <span>${esc(toolboxSessionProjectLabel(thread.cwd))} · ${esc(shortToolboxSessionId(thread.id))}</span>
          <span>${esc(t("toolboxSessionDeletedAt"))}: ${esc(thread.deletedAt || "--")}</span>
        </span>
      </label>
    `;
  }).join("");
  list.querySelectorAll("[data-toolbox-trash-check]").forEach((input) => {
    input.addEventListener("change", () => {
      const id = input.getAttribute("data-toolbox-trash-check");
      if (!id) return;
      if (input.checked) toolboxSelectedTrashSessionIds.add(id);
      else toolboxSelectedTrashSessionIds.delete(id);
      renderToolboxSyncedThreads();
    });
  });
}

function renderToolboxSessionManagerControls(filteredThreads, allThreads) {
  const filteredIds = filteredThreads.map((thread) => thread.id);
  const allFilteredSelected = filteredIds.length > 0 && filteredIds.every((id) => toolboxSelectedSessionIds.has(id));
  $("toolboxSessionSelectAllBtn").textContent = allFilteredSelected ? t("toolboxSessionClearSelected") : t("toolboxSessionSelectAll");
  $("toolboxSessionCopyIdsBtn").textContent = `${t("toolboxSessionCopyIds")} (${toolboxSelectedSessionIds.size})`;
  $("toolboxSessionTrashBtn").textContent = `${t("toolboxSessionMoveTrash")} (${toolboxSelectedSessionIds.size})`;
  $("toolboxSessionRestoreOpenBtn").textContent = `${t("toolboxSessionOpenTrash")} (${(codexToolbox?.trashedCodexThreads || []).length})`;
  $("toolboxSessionCopyIdsBtn").disabled = toolboxSelectedSessionIds.size === 0;
  $("toolboxSessionTrashBtn").disabled = toolboxSelectedSessionIds.size === 0;
  $("toolboxSessionSelectAllBtn").disabled = filteredIds.length === 0;
  $("toolboxSessionManagerStatus").textContent = t("toolboxSessionSelectedSummary", {
    selected: toolboxSelectedSessionIds.size,
    total: filteredThreads.length,
    trash: (codexToolbox?.trashedCodexThreads || []).length,
  });
}

function renderToolboxSyncedThreads() {
  const list = $("toolboxSyncedThreadsList");
  const allThreads = codexToolbox?.syncedCodexThreads || [];
  syncToolboxSessionSelection(allThreads);
  const filteredThreads = filterToolboxSessions(allThreads);
  const groups = groupToolboxSessions(filteredThreads);
  renderToolboxSessionManagerControls(filteredThreads, allThreads);
  renderToolboxSessionTrash();
  if (allThreads.length === 0) {
    list.innerHTML = `<div class="mgmt-empty">${t("toolboxNoThreads")}</div>`;
    bindToolboxSessionManagerEvents(filteredThreads);
    return;
  }
  if (filteredThreads.length === 0) {
    list.innerHTML = `<div class="mgmt-empty">${t("toolboxSessionNoSearchResults")}</div>`;
    bindToolboxSessionManagerEvents(filteredThreads);
    return;
  }
  list.innerHTML = groups.map(renderToolboxSessionGroup).join("");
  bindToolboxSessionManagerEvents(filteredThreads);
}

async function copyToolboxText(text) {
  if (navigator.clipboard?.writeText) {
    await navigator.clipboard.writeText(text);
    return;
  }
  const input = document.createElement("textarea");
  input.value = text;
  input.style.position = "fixed";
  input.style.opacity = "0";
  document.body.appendChild(input);
  input.select();
  document.execCommand("copy");
  input.remove();
}

function bindToolboxSessionManagerEvents(filteredThreads) {
  const list = $("toolboxSyncedThreadsList");
  list.querySelectorAll("[data-toolbox-session-check]").forEach((input) => {
    input.addEventListener("change", () => {
      const id = input.getAttribute("data-toolbox-session-check");
      if (!id) return;
      if (input.checked) toolboxSelectedSessionIds.add(id);
      else toolboxSelectedSessionIds.delete(id);
      renderToolboxSyncedThreads();
    });
  });
  list.querySelectorAll("[data-toolbox-session-group]").forEach((input) => {
    input.addEventListener("change", () => {
      const ids = (input.getAttribute("data-toolbox-session-group") || "").split(",").filter(Boolean);
      const allSelected = ids.every((id) => toolboxSelectedSessionIds.has(id));
      ids.forEach((id) => {
        if (allSelected) toolboxSelectedSessionIds.delete(id);
        else toolboxSelectedSessionIds.add(id);
      });
      renderToolboxSyncedThreads();
    });
  });
  list.querySelectorAll("[data-toolbox-session-copy]").forEach((btn) => {
    btn.addEventListener("click", async () => {
      const id = btn.getAttribute("data-toolbox-session-copy");
      if (!id) return;
      try {
        await copyToolboxText(id);
        toolboxCopiedSessionId = id;
        renderToolboxSyncedThreads();
        window.setTimeout(() => {
          if (toolboxCopiedSessionId === id) {
            toolboxCopiedSessionId = "";
            renderToolboxSyncedThreads();
          }
        }, 1000);
      } catch (error) {
        showToast(String(error), "error");
      }
    });
  });
  $("toolboxSessionSelectAllBtn").onclick = () => {
    const ids = filteredThreads.map((thread) => thread.id);
    const allSelected = ids.length > 0 && ids.every((id) => toolboxSelectedSessionIds.has(id));
    ids.forEach((id) => {
      if (allSelected) toolboxSelectedSessionIds.delete(id);
      else toolboxSelectedSessionIds.add(id);
    });
    renderToolboxSyncedThreads();
  };
  $("toolboxSessionCopyIdsBtn").onclick = async () => {
    const ids = [...toolboxSelectedSessionIds];
    if (ids.length === 0) return;
    try {
      await copyToolboxText(ids.join("\n"));
      showToast(t("toolboxSessionCopiedIds"), "success");
    } catch (error) {
      showToast(String(error), "error");
    }
  };
  $("toolboxSessionTrashBtn").onclick = handleToolboxMoveSelectedToTrash;
  $("toolboxSessionRestoreOpenBtn").onclick = () => {
    toolboxSessionTrashOpen = !toolboxSessionTrashOpen;
    renderToolboxSyncedThreads();
  };
  $("toolboxSessionTrashCloseBtn").onclick = () => {
    toolboxSessionTrashOpen = false;
    renderToolboxSyncedThreads();
  };
  $("toolboxSessionRestoreBtn").onclick = handleToolboxRestoreSelectedSessions;
}

async function handleToolboxMoveSelectedToTrash() {
  const threadIds = [...toolboxSelectedSessionIds];
  if (threadIds.length === 0) return;
  if (!(await appConfirm(t("toolboxSessionConfirmTrash"), { title: currentLang === "zh" ? "移入回收站" : "Move to trash", danger: true, confirmText: currentLang === "zh" ? "移入回收站" : "Move" }))) return;
  try {
    codexToolbox = await invoke("trash_codex_sessions", { threadIds });
    toolboxSelectedSessionIds.clear();
    showToast(t("toolboxSessionMovedTrash"), "success");
    renderCodexToolbox();
  } catch (error) {
    showToast(String(error), "error");
  }
}

async function handleToolboxRestoreSelectedSessions() {
  const threadIds = [...toolboxSelectedTrashSessionIds];
  if (threadIds.length === 0) return;
  try {
    codexToolbox = await invoke("restore_codex_sessions", { threadIds });
    toolboxSelectedTrashSessionIds.clear();
    showToast(t("toolboxSessionRestored"), "success");
    renderCodexToolbox();
  } catch (error) {
    showToast(String(error), "error");
  }
}

function getSelectedMobileBinding() {
  const bindings = codexToolbox?.mobileChannels || [];
  return bindings.find((binding) => binding.channel === selectedMobileChannel) || { channel: selectedMobileChannel };
}

function renderToolboxMobileControl() {
  const appSelect = $("toolboxMobileAppSelect");
  const threadSelect = $("toolboxMobileThreadSelect");
  const threads = codexToolbox?.syncedCodexThreads || [];

  const appOptionsHtml = [
    `<option value="wechat">${channelLabel("wechat")}</option>`,
    `<option value="qq">${channelLabel("qq")}</option>`,
    `<option value="lark">${channelLabel("lark")}</option>`,
  ].join("");
  // 仅在内容变化时重建，避免每秒轮询时下拉被强行关闭
  if (appSelect.innerHTML !== appOptionsHtml) appSelect.innerHTML = appOptionsHtml;
  if (appSelect.value !== selectedMobileChannel) appSelect.value = selectedMobileChannel;

  if (codexToolbox?.selectedMobileThreadId) {
    selectedCodexThreadId = codexToolbox.selectedMobileThreadId;
  } else if (!selectedCodexThreadId && threads[0]) {
    selectedCodexThreadId = threads[0].id;
  }
  const threadOptionsHtml = threads.length
    ? threads.map((thread) => `<option value="${esc(thread.id)}">${esc(thread.threadName || thread.lastUserMessage || thread.id)}</option>`).join("")
    : `<option value="">${t("toolboxMobileNoThreadOption")}</option>`;
  if (threadSelect.innerHTML !== threadOptionsHtml) threadSelect.innerHTML = threadOptionsHtml;
  const nextThreadValue = threads.some((thread) => thread.id === selectedCodexThreadId) ? selectedCodexThreadId : "";
  if (threadSelect.value !== nextThreadValue) threadSelect.value = nextThreadValue;

  bindMobileControlSelectEvents();

  const panel = $("toolboxMobileBindPanel");
  if (!panel) return;
  const nextPanelHtml = renderSelectedMobileBinding(getSelectedMobileBinding());
  // 用户正在填写凭据输入框时跳过重建，否则每秒轮询会销毁输入框、丢焦点丢内容
  const active = document.activeElement;
  const typingInPanel =
    active && active !== document.body && panel.contains(active) && active.hasAttribute?.("data-channel-field");
  if (typingInPanel) return;
  if (panel.innerHTML === nextPanelHtml) return;
  panel.innerHTML = nextPanelHtml;
  bindMobileControlPanelEvents();
}

function bindMobileControlSelectEvents() {
  const appSelect = $("toolboxMobileAppSelect");
  if (appSelect) {
    appSelect.onchange = () => {
      selectedMobileChannel = appSelect.value || "wechat";
      renderCodexToolbox();
    };
  }

  const threadSelect = $("toolboxMobileThreadSelect");
  if (threadSelect) {
    threadSelect.onchange = async () => {
      selectedCodexThreadId = threadSelect.value || "";
      if (!selectedCodexThreadId) return;
      try {
        await bindCurrentMobileSelection();
        showToast(t("toolboxThreadSelected"), "success");
        renderCodexToolbox();
      } catch (error) {
        showToast(String(error), "error");
      }
    };
  }
}

function bindMobileControlPanelEvents() {
  $("toolboxMobileBindPanel").querySelectorAll("button[data-action]").forEach((btn) => {
    btn.addEventListener("click", handleMobileBindAction);
  });
  $("toolboxMobileBindPanel").querySelectorAll("[data-channel-field]").forEach((input) => {
    input.addEventListener("input", scheduleMobileChannelAutoSave);
    input.addEventListener("change", () => saveSelectedMobileChannelDraft({ render: true }));
  });
}

async function bindCurrentMobileSelection() {
  mobileDebug("bind selection requested");
  if (!selectedCodexThreadId) {
    const first = codexToolbox?.syncedCodexThreads?.[0];
    selectedCodexThreadId = first?.id || "";
    mobileDebug("bind selection fallback thread", { fallbackThreadId: selectedCodexThreadId });
  }
  if (!selectedCodexThreadId) {
    mobileDebugError("bind selection failed: no thread", t("toolboxNoThreads"));
    throw new Error(t("toolboxNoThreads"));
  }
  mobileDebug("select_mobile_thread invoke:start");
  codexToolbox = await invoke("select_mobile_thread", { threadId: selectedCodexThreadId });
  mobileDebug("select_mobile_thread invoke:success", {
    activeThreadId: codexToolbox?.mobileRemote?.activeThreadId,
    activeThreadName: codexToolbox?.mobileRemote?.activeThreadName,
  });
  mobileDebug("bind_codex_thread invoke:start");
  codexToolbox = await invoke("bind_codex_thread", {
    channel: selectedMobileChannel,
    threadId: selectedCodexThreadId,
    syncEnabled: true,
    note: "mobile-control",
  });
  mobileDebug("bind_codex_thread invoke:success", {
    activeThreadId: codexToolbox?.mobileRemote?.activeThreadId,
    activeThreadName: codexToolbox?.mobileRemote?.activeThreadName,
    binding: getSelectedMobileBinding(),
  });
}

function scheduleMobileChannelAutoSave() {
  window.clearTimeout(larkCredentialSaveTimer);
  larkCredentialSaveTimer = window.setTimeout(() => {
    saveSelectedMobileChannelDraft({ render: false });
  }, 700);
}

async function saveSelectedMobileChannelDraft({ render = false } = {}) {
  if (selectedMobileChannel !== "lark") return;
  const panel = $("toolboxMobileBindPanel");
  const appId = panel.querySelector("[data-channel-field='appId']")?.value || "";
  const appSecret = panel.querySelector("[data-channel-field='appSecret']")?.value || "";
  if (!appId.trim() && !appSecret.trim()) return;
  try {
    mobileDebug("configure_mobile_channel invoke:start", {
      channel: "lark",
      hasAppId: Boolean(appId.trim()),
      hasAppSecret: Boolean(appSecret.trim()),
      render,
    });
    codexToolbox = await invoke("configure_mobile_channel", {
      channel: "lark",
      appId,
      appSecret,
      botToken: "",
      accountId: "",
      baseUrl: "https://open.feishu.cn",
      userId: "",
      botOpenId: "",
    });
    mobileDebug("configure_mobile_channel invoke:success", {
      binding: getSelectedMobileBinding(),
    });
    if (render) renderCodexToolbox();
  } catch (error) {
    mobileDebugError("configure_mobile_channel invoke:failed", error);
    showToast(String(error), "error");
  }
}

async function handleMobileBindAction(event) {
  const btn = event.currentTarget;
  const action = btn.getAttribute("data-action");
  // 已有绑定操作在执行时直接忽略，避免重复调用后端命令
  if (mobileBindBusyAction && action !== "toolbox-cancel-qq-qr") return;
  mobileBindBusyAction = action || "";
  // 注意：renderCodexToolbox() 会重建整个面板，btn 随即变成游离节点，
  // 因此这里不再对 btn 设 busy，改由模板按 mobileBindBusyAction 输出 disabled。
  renderCodexToolbox();
  try {
    if (action === "toolbox-open-lark-create") {
      selectedMobileChannel = "lark";
      mobileDebug("start_lark_bot_registration invoke:start", { createOnly: true });
      codexToolbox = await invoke("start_lark_bot_registration", { createOnly: true });
      mobileDebug("start_lark_bot_registration invoke:success", { createOnly: true });
      startToolboxRefresh(120);
      showToast(t("toolboxCreateLarkBot"), "success");
    } else if (action === "toolbox-open-lark-existing") {
      selectedMobileChannel = "lark";
      mobileDebug("start_lark_bot_registration invoke:start", { createOnly: false });
      codexToolbox = await invoke("start_lark_bot_registration", { createOnly: false });
      mobileDebug("start_lark_bot_registration invoke:success", { createOnly: false });
      startToolboxRefresh(120);
      showToast(t("toolboxRebindLarkBot"), "success");
    } else if (action === "toolbox-unbind-lark-session") {
      mobileDebug("unbind_codex_thread invoke:start", { channel: "lark" });
      codexToolbox = await invoke("unbind_codex_thread", { channel: "lark" });
      mobileDebug("unbind_codex_thread invoke:success", { channel: "lark" });
      showToast(t("toolboxUnbindLarkSession"), "success");
    } else if (action === "toolbox-stop-lark-listen") {
      mobileDebug("stop_mobile_remote invoke:start", { source: "lark-panel" });
      codexToolbox = await invoke("stop_mobile_remote", {});
      mobileDebug("stop_mobile_remote invoke:success", { source: "lark-panel" });
      showToast(t("toolboxStopLarkListen"), "success");
    } else if (action === "toolbox-clear-lark-binding") {
      mobileDebug("clear_mobile_channel_binding invoke:start", { channel: "lark" });
      codexToolbox = await invoke("clear_mobile_channel_binding", { channel: "lark" });
      mobileDebug("clear_mobile_channel_binding invoke:success", { channel: "lark" });
      showToast(t("toolboxClearLarkBinding"), "success");
    } else if (action === "toolbox-clear-qq-binding") {
      mobileDebug("clear_mobile_channel_binding invoke:start", { channel: "qq" });
      codexToolbox = await invoke("clear_mobile_channel_binding", { channel: "qq" });
      mobileDebug("clear_mobile_channel_binding invoke:success", { channel: "qq" });
      showToast("QQ 绑定已清除", "success");
    } else if (action === "toolbox-clear-wechat-binding") {
      mobileDebug("clear_mobile_channel_binding invoke:start", { channel: "wechat" });
      codexToolbox = await invoke("clear_mobile_channel_binding", { channel: "wechat" });
      mobileDebug("clear_mobile_channel_binding invoke:success", { channel: "wechat" });
      showToast("微信绑定已清除", "success");
    } else if (action === "toolbox-start-qq-qr") {
      selectedMobileChannel = "qq";
      startToolboxRefresh(120);
      showToast(t("toolboxStartQqQr"), "success");
      const qqBinding = getSelectedMobileBinding();
      qqBinding.qrStatus = "正在生成 QQ 扫码二维码，请稍等...";
      qqBinding.status = "正在生成 QQ 扫码二维码，请稍等...";
      renderCodexToolbox();
      // 必须 await：否则 finally 会在二维码生成前清空 busy，导致可以连点重复绑定
      codexToolbox = await invoke("start_qq_qr_binding", {});
      startToolboxRefresh(120);
    } else if (action === "toolbox-cancel-qq-qr") {
      mobileDebug("cancel_qq_qr_binding invoke:start");
      codexToolbox = await invoke("cancel_qq_qr_binding", {});
      mobileDebug("cancel_qq_qr_binding invoke:success");
      showToast(currentLang === "zh" ? "已取消 QQ 扫码绑定" : "QQ QR binding cancelled", "success");
    } else if (action === "toolbox-start-wechat-qr") {
      selectedMobileChannel = "wechat";
      startToolboxRefresh(120);
      showToast(t("toolboxStartWechatQr"), "success");
      codexToolbox = await invoke("start_wechat_qr_binding", {});
    } else if (action === "toolbox-poll-wechat-qr") {
      codexToolbox = await invoke("poll_wechat_qr_binding", { verifyCode: "" });
      showToast(t("toolboxPollWechatQr"), "success");
    } else if (action === "toolbox-open-qr") {
      await invoke("open_external_target", { target: btn.getAttribute("data-url") || "" });
    }
    renderCodexToolbox();
  } catch (error) {
    mobileDebugError("mobile bind action failed", error, { action });
    showToast(String(error), "error");
    if (action === "toolbox-start-qq-qr") loadCodexToolbox();
  } finally {
    mobileBindBusyAction = "";
    renderCodexToolbox();
  }
}

function renderSelectedMobileBinding(binding) {
  const channel = binding.channel || selectedMobileChannel;
  const title = channelLabel(channel);
  const uiState = getMobileBindingUiState(binding);
  const status = getMobileBindingStatusText(binding, uiState);
  const error = binding.lastError ? `<div class="mgmt-item-desc danger">${t("toolboxLastError")}: ${esc(binding.lastError)}</div>` : "";
  const qr = uiState.showQr ? renderChannelQr(binding) : "";
  const busy = uiState.busy ||
    mobileBindBusyAction.startsWith(`toolbox-${channel}`) ||
    mobileBindBusyAction.includes(channel) ||
    mobileBindBusyAction.includes("remote");
  const loading = busy
    ? `<div class="toolbox-loading-line"><span class="inline-spinner" aria-hidden="true"></span><span>${esc(getMobileBindingLoadingText(channel, mobileBindBusyAction))}</span></div>`
    : "";
  const stateBadge = `<span class="toolbox-binding-state toolbox-binding-state-${esc(uiState.kind)}">${esc(getMobileBindingStateLabel(uiState.kind))}</span>`;
  // 有绑定操作在进行中就禁用全部动作按钮，避免重复调用后端命令
  const dis = mobileBindBusyAction ? " disabled" : "";
  let body = "";
  let actions = "";

  if (channel === "wechat") {
    body = `${qr}${loading}<div class="mgmt-item-desc">${esc(status.detail || "点击绑定微信后，用手机微信扫码并确认绑定。")}</div>`;
    actions = `
      <button class="btn btn-primary btn-sm" data-action="toolbox-start-wechat-qr"${dis}>${t("toolboxStartWechatQr")}</button>
      <button class="btn btn-secondary btn-sm danger-text" data-action="toolbox-clear-wechat-binding"${dis}>清除绑定</button>
    `;
  } else if (channel === "qq") {
    const qqQrPending = Boolean(
      binding.qrStartedAt &&
      !binding.appId &&
      !binding.appSecret &&
      /扫码|二维码|正在|等待/i.test(`${binding.status || ""} ${binding.qrStatus || ""}`)
    );
    body = `${qr}${loading}<div class="mgmt-item-desc">${esc(status.detail || "点击 QQ 扫码绑定后，用 QQ 扫描二维码即可保存机器人凭据。")}</div>`;
    actions = `
      <button class="btn btn-primary btn-sm" data-action="toolbox-start-qq-qr"${dis}>${t("toolboxStartQqQr")}</button>
      ${qqQrPending ? `<button class="btn btn-secondary btn-sm" data-action="toolbox-cancel-qq-qr"${mobileBindBusyAction && mobileBindBusyAction !== "toolbox-start-qq-qr" ? " disabled" : ""}>取消扫码</button>` : ""}
      <button class="btn btn-secondary btn-sm danger-text" data-action="toolbox-clear-qq-binding"${dis}>清除绑定</button>
    `;
  } else {
    body = `
      ${qr}
      ${loading}
      <div class="mgmt-item-desc">创建新机器人会打开飞书开放平台；创建完成后 App ID 和 App Secret 会自动填充并绑定当前对话。</div>
      <div class="toolbox-channel-form toolbox-channel-form-compact">
        <label class="toolbox-channel-field">
          <span>${t("toolboxAppId")}</span>
          <input type="text" data-channel-field="appId" autocomplete="off" value="${esc(binding.appId || "")}">
        </label>
        <label class="toolbox-channel-field">
          <span>${t("toolboxAppSecret")}</span>
          <input type="password" data-channel-field="appSecret" autocomplete="off" placeholder="${binding.appSecret === '*' ? '(已保存)' : ''}" value="">
        </label>
      </div>
    `;
    actions = `
      <button class="btn btn-success btn-sm" data-action="toolbox-open-lark-existing"${dis}>${t("toolboxRebindLarkBot")}</button>
      <button class="btn btn-secondary btn-sm" data-action="toolbox-open-lark-create"${dis}>${t("toolboxCreateLarkBot")}</button>
      <button class="btn btn-secondary btn-sm danger-text" data-action="toolbox-unbind-lark-session"${dis}>${t("toolboxUnbindLarkSession")}</button>
      <button class="btn btn-secondary btn-sm danger-text" data-action="toolbox-stop-lark-listen"${dis}>${t("toolboxStopLarkListen")}</button>
      <button class="btn btn-secondary btn-sm danger-text" data-action="toolbox-clear-lark-binding"${dis}>${t("toolboxClearLarkBinding")}</button>
    `;
  }

  return `
    <div class="toolbox-mobile-card">
      <div class="toolbox-mobile-card-title"><span>${esc(title)}</span>${stateBadge}</div>
      <div class="mgmt-item-desc">${esc(status.main)}</div>
      ${error}
      ${body}
      <div class="toolbox-thread-actions">${actions}</div>
    </div>
  `;
}

function getMobileBindingUiState(binding) {
  if (typeof helpers.getMobileBindingUiState === "function") {
    return helpers.getMobileBindingUiState(binding);
  }
  const hasCredential = Boolean(binding?.bound || binding?.botToken || binding?.appId || binding?.appSecret || binding?.accountId || binding?.userId || binding?.botOpenId);
  if (binding?.lastError) return { kind: "error", hasCredential, showQr: false, busy: false };
  if (hasCredential) return { kind: "bound", hasCredential, showQr: false, busy: false };
  if (shouldRenderChannelQr(binding)) return { kind: "qr", hasCredential, showQr: true, busy: false };
  return { kind: "unbound", hasCredential, showQr: false, busy: false };
}

function getMobileBindingStateLabel(kind) {
  const zh = {
    bound: "已绑定",
    qr: "等待扫码",
    binding: "连接中",
    error: "异常",
    unbound: "未绑定",
  };
  const en = {
    bound: "Bound",
    qr: "Scan QR",
    binding: "Connecting",
    error: "Error",
    unbound: "Unbound",
  };
  return (currentLang === "zh" ? zh : en)[kind] || kind;
}

function getMobileBindingStatusText(binding, uiState) {
  const channel = channelLabel(binding?.channel || selectedMobileChannel);
  if (uiState.kind === "bound") {
    return {
      main: `${channel} 已绑定，可直接开启平台连接`,
      detail: `如果手机端没有收到消息，请点击「开启平台连接」重新监听；不需要重新扫码。`,
    };
  }
  if (uiState.kind === "qr") {
    return {
      main: `${channel} 二维码已生成`,
      detail: `请使用 ${channel} 扫码并在手机端确认。`,
    };
  }
  if (uiState.kind === "binding") {
    return {
      main: `${channel} 正在连接`,
      detail: binding?.status || binding?.qrStatus || `正在准备 ${channel} 绑定，请稍等。`,
    };
  }
  if (uiState.kind === "error") {
    return {
      main: `${channel} 状态异常`,
      detail: `请重新绑定或清除后再试。`,
    };
  }
  return {
    main: `${channel} 尚未绑定`,
    detail: binding?.status || binding?.qrStatus || `请选择对话后绑定 ${channel}。`,
  };
}

function getMobileBindingLoadingText(channel, action) {
  const label = channelLabel(channel);
  if (action.includes("remote")) return `正在开启 ${label} 平台连接...`;
  if (action.includes("clear")) return `正在清除 ${label} 绑定...`;
  if (action.includes("lark")) return `正在准备飞书绑定流程...`;
  if (action.includes("qq")) return `正在生成 QQ 扫码二维码...`;
  if (action.includes("wechat")) return `正在生成微信扫码二维码...`;
  return `正在处理 ${label} 状态...`;
}

function renderChannelQr(binding) {
  if (!shouldRenderChannelQr(binding)) return "";
  const canOpenUrl = binding.channel === "lark";
  const image = binding.qrDataUrl
    ? `<img class="toolbox-qr-image" src="${esc(binding.qrDataUrl)}" alt="${esc(channelLabel(binding.channel))} QR">`
    : "";
  const open = canOpenUrl && binding.qrUrl
    ? `<button class="btn btn-link btn-sm toolbox-qr-link" type="button" data-action="toolbox-open-qr" data-url="${esc(binding.qrUrl)}">${t("toolboxQrOpen")}</button>`
    : "";
  if (!image && !open) return "";
  return `<div class="toolbox-qr-box">${image}${open}</div>`;
}

function shouldRenderChannelQr(binding) {
  if (typeof helpers.shouldRenderChannelQr === "function") {
    return helpers.shouldRenderChannelQr(binding);
  }
  if (!binding || (!binding.qrDataUrl && !binding.qrUrl)) return false;
  if (binding.channel === "lark") return Boolean(binding.qrDataUrl || binding.qrUrl);
  if (binding.channel === "qq") {
    return Boolean(binding.qrDataUrl && isFreshMobileQr(binding));
  }
  if (binding.channel === "wechat") {
    const dataUrl = String(binding.qrDataUrl || "").trim().toLowerCase();
    return Boolean(
      binding.qrDataUrl &&
      binding.qrDeviceCode &&
      isFreshMobileQr(binding) &&
      !dataUrl.startsWith("data:image/svg+xml")
    );
  }
  return Boolean(binding.qrDataUrl);
}

function isFreshMobileQr(binding) {
  if (typeof helpers.isFreshMobileQr === "function") {
    return helpers.isFreshMobileQr(binding);
  }
  const startedAt = Number(binding?.qrStartedAt || 0);
  if (!Number.isFinite(startedAt) || startedAt <= 0) return false;
  return Date.now() - startedAt <= 10 * 60 * 1000;
}

function isQqAuthorizationTarget(value) {
  if (typeof helpers.isQqAuthorizationTarget === "function") {
    return helpers.isQqAuthorizationTarget(value);
  }
  return /^(https?:\/\/|mqqapi:\/\/|qqbot:\/\/)/i.test(String(value || "").trim());
}

function renderToolboxRemote() {
  const remote = codexToolbox?.mobileRemote;
  if (!remote) return;
  const binding = getSelectedMobileBinding();
  const activeThread = remote.activeThreadName || remote.activeThreadId || "";
  const smartControlText = remote.remoteControlConnected
    ? t("toolboxSmartControlConnected")
    : (remote.remoteControlStatus || remote.remoteControlDetail
      ? `${t("toolboxSmartControlWaiting")}: ${remote.remoteControlStatus || remote.remoteControlDetail}`
      : t("toolboxSmartControlCompat"));
  const stateText = binding?.lastError || (remote.enabled ? t("toolboxRemoteRunning") : (remote.lastError || t("toolboxRemoteStopped")));
  $("toolboxRemoteStatusValue").textContent = [
    smartControlText,
    stateText,
    channelLabel(selectedMobileChannel),
    activeThread,
    binding.status || binding.qrStatus || "",
  ].filter(Boolean).join(" · ");
  $("toolboxRemoteStatusValue").title = currentLang === "zh"
    ? "点击复制最近高级控制协议事件"
    : "Click to copy the latest advanced-control protocol event";
  $("toolboxRemoteStatusValue").style.cursor = "copy";
  $("toolboxRemoteStatusValue").onclick = copySmartControlDebugEvent;
  renderSmartControlApprovals();
}

function ensureSmartControlApprovalPanel() {
  let panel = $("smartControlApprovalPanel");
  if (panel) return panel;
  const status = $("toolboxRemoteStatusValue");
  if (!status || !status.parentElement) return null;
  panel = document.createElement("div");
  panel.id = "smartControlApprovalPanel";
  panel.className = "smart-control-approval-panel";
  status.insertAdjacentElement("afterend", panel);
  return panel;
}

function renderSmartControlApprovals() {
  const panel = ensureSmartControlApprovalPanel();
  if (!panel) return;
  const approvals = codexDiagnostics?.approvals || [];
  if (!approvals.length) {
    panel.innerHTML = `<div class="smart-control-approval-empty">${esc(t("toolboxSmartControlApprovalEmpty"))}</div>`;
    return;
  }
  panel.innerHTML = `<div class="smart-control-approval-title">${esc(t("toolboxSmartControlApprovalTitle"))}</div>
    ${approvals.slice(-3).reverse().map((approval) => {
      const options = approval.options?.length ? approval.options : ["approve", "deny"];
      // 整卡片 disabled — 只要这张卡正在提交，所有按钮都禁用
      const pending = pendingApprovalIds.has(approval.requestId || "");
      const dis = pending ? " disabled" : "";
      return `<div class="smart-control-approval-card">
        <div class="smart-control-approval-card-title">${esc(approval.title || approval.method || "Approval")}</div>
        <div class="smart-control-approval-card-body">${esc(approval.body || approval.rawPreview || "")}</div>
        <div class="smart-control-approval-actions">
          ${options.map((option) => `<button class="btn btn-secondary btn-sm" type="button" data-approval-id="${esc(approval.requestId || "")}" data-approval-decision="${esc(option)}"${dis}>${esc(option)}</button>`).join("")}
        </div>
      </div>`;
    }).join("")}`;
  panel.querySelectorAll("[data-approval-id]").forEach((btn) => {
    btn.addEventListener("click", async () => {
      const requestId = btn.getAttribute("data-approval-id");
      const decision = btn.getAttribute("data-approval-decision");
      if (!requestId || !decision) return;
      // 整卡片加入 pending 集合，下次渲染时所有按钮都 disabled
      if (pendingApprovalIds.has(requestId)) return;
      pendingApprovalIds.add(requestId);
      renderSmartControlApprovals();
      try {
        await invoke("submit_smart_control_approval", { requestId, decision });
        showToast(t("toolboxSmartControlApprovalSubmitted"), "success");
        await loadSmartControlDebug(true);
        renderSmartControlApprovals();
      } catch (error) {
        showToast(String(error), "error");
        pendingApprovalIds.delete(requestId);
        renderSmartControlApprovals();
      }
    });
  });
}

function channelLabel(channel) {
  if (channel === "lark") return currentLang === "zh" ? "飞书" : "Feishu/Lark";
  if (channel === "wechat") return currentLang === "zh" ? "微信" : "WeChat";
  if (channel === "qq") return "QQ";
  return String(channel || "").toUpperCase();
}

async function copySmartControlDebugEvent() {
  try {
    const debug = await invoke("get_smart_control_debug");
    const latest = debug?.lastEvent || debug?.events?.[debug.events.length - 1] || null;
    const payload = latest || debug || {};
    await navigator.clipboard.writeText(JSON.stringify(payload, null, 2));
    showToast(t("toolboxSmartControlCopied"), "success");
  } catch (error) {
    showToast(String(error), "error");
  }
}

function waitForNextPaint() {
  return new Promise((resolve) => {
    requestAnimationFrame(() => requestAnimationFrame(resolve));
  });
}

function showSwitchOverlay(profileName, kind = "claude") {
  const state = helpers.getSwitchOverlayState?.(kind) || {
    cancellable: kind === "claude",
    indeterminate: kind !== "claude",
    showSteps: kind === "claude",
  };
  const progressBar = $("switchProgressBar");
  $("switchProfileName").textContent = profileName;
  progressBar.classList.toggle("indeterminate", state.indeterminate);
  progressBar.style.width = state.indeterminate ? "38%" : "0%";
  const switchingLabel =
    kind === "codex" ? t("codexSwitching") : kind === "grok" ? t("grokSwitching") : t("preparing");
  $("switchStepLabel").textContent = switchingLabel;
  $("switchProgressPercent").textContent = "0%";
  $("switchProgressPercent").hidden = state.indeterminate;
  $("switchStep1").className = "switch-step";
  $("switchStep2").className = "switch-step";
  $("switchStep3").className = "switch-step";
  $("switchSteps").hidden = !state.showSteps;
  $("switchCancelBtn").hidden = !state.cancellable;
  $("switchCancelBtn").disabled = false;
  $("switchOverlay").classList.add("open");
}

function hideSwitchOverlay() {
  $("switchOverlay").classList.remove("open");
}

function completeSwitchOverlay() {
  $("switchProgressBar").classList.remove("indeterminate");
  $("switchProgressBar").style.width = "100%";
  $("switchProgressPercent").hidden = false;
  $("switchProgressPercent").textContent = "100%";
  $("switchStepLabel").textContent = t("switchDone");
}

function switchProgressLabel(step) {
  if (step <= 1) return t("preparing");
  if (step === 2) return t("progressSystem");
  if (step === 3) return t("progressEditors");
  if (step === 4) return t("progressClaude");
  if (step === 5) return t("progressFinalize");
  if (step >= 6) return t("progressDone");
  return t("preparing");
}

function updateSwitchProgress(payload) {
  const step = Math.max(1, Number(payload?.step || 1));
  const total = Math.max(1, Number(payload?.total || 6));
  const pct = Math.round((step / total) * 100);

  $("switchProgressBar").style.width = `${pct}%`;
  $("switchProgressPercent").textContent = `${pct}%`;

  const labelMap = {
    prepare: t("preparing"),
    system: t("progressSystem"),
    vscode: t("progressEditors"),
    editors: t("progressEditors"),
    claude: t("progressClaude"),
    finalize: t("progressFinalize"),
    done: t("progressDone")
  };
  const payloadLabel = payload?.label ? labelMap[payload.label] : null;
  $("switchStepLabel").textContent = payloadLabel || switchProgressLabel(step);

  for (let i = 1; i <= total; i += 1) {
    const el = $(`switchStep${i}`);
    if (!el) continue;
    if (i < step) {
      el.className = "switch-step done";
    } else if (i === step) {
      el.className = "switch-step active";
    } else {
      el.className = "switch-step";
    }
  }
}

function updateClaudeStatusTitle() {
  const active = profiles.find((profile) => profile.isActive);
  const activeContext = active
    ? (currentLang === "zh" ? ` (当前: ${active.name})` : ` (Current: ${active.name})`)
    : "";
  setText("statusSectionTitle", `${t("statusTitle")}${activeContext}`);
}

function renderClaudeStatus(status) {
  const grid = $("statusGrid");
  if (!grid) return;

  const item = status?.claude || {};
  const model = status?.claudeModel || "--";
  const apiKeyLabel = "Claude API Key";
  const baseUrlLabel = "Claude Base URL";
  const modelLabel = currentLang === "zh" ? "Claude 模型" : "Claude Model";
  updateClaudeStatusTitle();
  const COPY_ICON = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>`;

  grid.innerHTML = `
    <div class="status-card">
      <div class="status-card-title">
        <span class="status-card-title-text">${productIcon("anthropic")}Claude</span>
      </div>
      <div class="status-item">
        <span class="status-label">${apiKeyLabel}</span>
        <div class="status-value-wrapper">
          <span class="status-value" title="${esc(item.apiKey || "--")}">${esc(maskKey(item.apiKey))}</span>
          ${item.apiKey ? `<button class="copy-btn" type="button" data-copy="${esc(item.apiKey)}" title="${t("copy")}" aria-label="${t("copy")}">${COPY_ICON}</button>` : ""}
        </div>
      </div>
      <div class="status-item">
        <span class="status-label">${baseUrlLabel}</span>
        <div class="status-value-wrapper">
          <span class="status-value" title="${esc(item.baseUrl || "--")}">${esc(item.baseUrl || "--")}</span>
          ${item.baseUrl ? `<button class="copy-btn" type="button" data-copy="${esc(item.baseUrl)}" title="${t("copy")}" aria-label="${t("copy")}">${COPY_ICON}</button>` : ""}
        </div>
      </div>
      <div class="status-item">
        <span class="status-label">${modelLabel}</span>
        <div class="status-value-wrapper">
          <span class="status-value" title="${esc(model)}">${esc(model)}</span>
          ${model !== "--" ? `<button class="copy-btn" type="button" data-copy="${esc(model)}" title="${t("copy")}" aria-label="${t("copy")}">${COPY_ICON}</button>` : ""}
        </div>
      </div>
    </div>`;

  bindDelegatedCopyButtons(grid, "ClaudeStatusCopy");
}

async function loadStatus() {
  try {
    const [status, editors] = await Promise.all([
      invoke("get_status"),
      invoke("get_detected_editors"),
    ]);
    detectedEditors = editors || {};
    lastClaudeStatus = status;
    renderClaudeStatus(status);
    renderNavigationStatus();
  } catch (error) {
    console.error("loadStatus failed:", error);
    showToast(t("loadStatusFailed", { error: String(error) }), "error");
  }
}

async function loadProfiles() {
  try {
    const data = await invoke("get_profiles");
    profiles = data.profiles || [];
    renderProfiles();
    if (lastClaudeStatus) renderClaudeStatus(lastClaudeStatus);
    renderNavigationStatus();
  } catch (error) {
    console.error(error);
    showToast(t("loadProfilesFailed", { error: String(error) }), "error");
  }
}

function renderProfiles() {
  const grid = $("profilesGrid");
  if (!grid) return;

  if (profiles.length === 0) {
    grid.innerHTML = `
      <div class="empty-state">
        <svg class="empty-state-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
          <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
          <polyline points="14 2 14 8 20 8"/>
          <line x1="12" y1="18" x2="12" y2="12"/>
          <line x1="9" y1="15" x2="15" y2="15"/>
        </svg>
        <div class="empty-state-title">${t("noConfigsTitle")}</div>
        <p>${t("noConfigsDesc")}</p>
      </div>`;
    updateActiveConfigBar();
    return;
  }

  grid.innerHTML = profiles.map((profile) => `
    <div class="profile-card ${profile.isActive ? "active" : ""}">
      <div class="profile-header">
        <span class="profile-name">${esc(profile.name)}</span>
        ${profile.isActive ? `<span class="active-badge">${t("inUse")}</span>` : ""}
      </div>
      <div class="profile-body">
        <div class="profile-field">
          <span class="field-label">Claude API Key</span>
          <span class="field-value">${esc(maskKey(profile.apiKey))}</span>
        </div>
        <div class="profile-field">
          <span class="field-label">Claude Base URL</span>
          <span class="field-value">${esc(truncUrl(profile.baseUrl, 50))}</span>
        </div>
        ${profile.apiFormat === "openai_chat" ? `<div class="profile-field">
          <span class="field-label">${currentLang === "zh" ? "API 格式" : "API Format"}</span>
          <span class="field-value">${currentLang === "zh" ? "OpenAI Chat（经本地代理）" : "OpenAI Chat (via local proxy)"}</span>
        </div>` : ""}
        ${profile.modelId ? `<div class="profile-field">
          <span class="field-label">${currentLang === "zh" ? "Claude 模型" : "Claude Model"}</span>
          <span class="field-value">${esc(profile.modelId)}</span>
        </div>` : ""}
      </div>
      <div class="profile-actions">
        ${profile.isActive ? "" : `<button class="btn btn-switch btn-sm" data-action="switch" data-id="${profile.id}" type="button">${t("switchUse")}</button>`}
        <button class="btn btn-secondary btn-sm" data-action="edit" data-id="${profile.id}" type="button">${t("edit")}</button>
        <button class="btn btn-danger btn-sm" data-action="delete" data-id="${profile.id}" type="button">${t("delete")}</button>
      </div>
    </div>
  `).join("");

  bindProfileGridActions(grid, "ClaudeProfiles", "claude");
  updateActiveConfigBar();
}

function updateActiveConfigBar() {
  updateClaudeStatusTitle();
}

function handleSyncNow() {
  const activeProfile = profiles.find((p) => p.isActive);
  if (activeProfile) {
    handleSwitch(activeProfile.id);
  } else {
    // A9: 无活动配置时给用户明确提示，而非静默无反应
    showToast(currentLang === "zh" ? "没有活动配置，请先切换到一个配置" : "No active config. Please switch to one first.", "warning");
  }
}

// 剪贴板自动检测填写 URL / Key
async function tryClipboardAutoFill(field, targetId) {
  try {
    const target = $(targetId);
    if (!target || target.value.trim()) return;

    const text = (await navigator.clipboard.readText() || "").trim();
    if (!text) return;
    if (field === "url") {
      if (/^https?:\/\//i.test(text)) target.value = text;
    } else if (field === "key") {
      if (!/^https?:\/\//i.test(text) && text.length >= 8) target.value = text;
    }
  } catch (_) { /* 剪贴板不可用时静默忽略 */ }
}

function productFieldMap(kind) {
  if (kind === "codex") {
    return {
      baseUrl: "codexBaseUrl",
      apiKey: "codexApiKey",
      model: "codexModel",
      endpointResults: "codexEndpointResults",
      modelResults: "codexModelResults",
      modelFetchBtn: "codexModelFetchBtn",
      endpointTestBtn: "codexEndpointTestBtn",
    };
  }
  if (kind === "grok") {
    return {
      baseUrl: "grokBaseUrl",
      apiKey: "grokApiKey",
      model: "grokModel",
      endpointResults: "grokEndpointResults",
      modelResults: "grokModelResults",
      modelFetchBtn: "grokModelFetchBtn",
      endpointTestBtn: "grokEndpointTestBtn",
    };
  }
  if (kind === "gemini") {
    return {
      baseUrl: "geminiBaseUrl",
      apiKey: "geminiApiKey",
      model: "geminiModel",
      endpointResults: "geminiEndpointResults",
      modelResults: "geminiModelResults",
      modelFetchBtn: "geminiModelFetchBtn",
      endpointTestBtn: "geminiEndpointTestBtn",
    };
  }
  return {
    baseUrl: "profileBaseUrl",
    apiKey: "profileApiKey",
    model: "profileModelId",
    endpointResults: "profileEndpointResults",
    modelResults: "profileModelResults",
    modelFetchBtn: "profileModelFetchBtn",
    endpointTestBtn: "profileEndpointTestBtn",
  };
}

function clearEndpointResults(kind) {
  const resultsId = productFieldMap(kind).endpointResults;
  const container = $(resultsId);
  if (!container) return;
  container.innerHTML = "";
  container.classList.remove("open");
}

// Claude 配置选了 OpenAI 格式时，验证/取模型也要按 OpenAI 方式带 Bearer 头
function productProtocol(kind) {
  if (kind === "claude" && $("profileApiFormat")?.value === "openai_chat") return "codex";
  return kind;
}

function modelInputId(kind) {
  return productFieldMap(kind).model;
}

function clearModelResults(kind) {
  const resultsId = productFieldMap(kind).modelResults;
  const container = $(resultsId);
  if (!container) return;
  container.innerHTML = "";
  container.classList.remove("open");
}

function renderModelResults(kind, models) {
  const resultsId = productFieldMap(kind).modelResults;
  const container = $(resultsId);
  if (!container) return;
  const normalizedModels = helpers.normalizeFetchedModels
    ? helpers.normalizeFetchedModels(models)
    : (models || []).map((item) => item.id || item).filter(Boolean).sort();

  if (normalizedModels.length === 0) {
    container.innerHTML = `<div class="model-row"><span class="model-id">${t("modelFetchNoResults")}</span></div>`;
    container.classList.add("open");
    return;
  }

  container.innerHTML = normalizedModels.map((model) => `
    <button class="model-row model-use-btn" data-action="use-model" data-model="${esc(model)}" type="button" title="${esc(model)}">
      <span class="model-id">${esc(model)}</span>
      <span class="model-use">${t("endpointUse")}</span>
    </button>
  `).join("");
  container.classList.add("open");
  // A10: 容器级委托，kind 存在容器 dataset 上按需回读
  container.dataset.modelKind = kind;
  bindDelegatedActions(container, "ModelResults", (action, target) => {
    if (action !== "use-model") return;
    const model = target.getAttribute("data-model");
    if (!model) return;
    const input = $(modelInputId(container.dataset.modelKind));
    if (!input) return;
    input.value = model;
    updateCodexOfficialConfigPreview();
    showToast(t("modelSelected"), "success");
  });
}

async function handleModelFetch(kind) {
  const fields = productFieldMap(kind);
  const buttonId = fields.modelFetchBtn;
  const baseUrlId = fields.baseUrl;
  const apiKeyId = fields.apiKey;
  const button = $(buttonId);
  const baseUrl = $(baseUrlId).value.trim() || OFFICIAL_BASE_URLS[kind] || "";
  const apiKey = $(apiKeyId).value.trim();
  if (!apiKey) {
    showToast(t("modelFetchMissing"), "warning");
    return;
  }

  const previousText = button.textContent;
  button.disabled = true;
  button.textContent = t("modelFetching");
  try {
    const models = await invoke("fetch_available_models", { baseUrl, apiKey, timeoutSecs: 12, protocol: productProtocol(kind) });
    renderModelResults(kind, models || []);
  } catch (error) {
    showToast(describeVerifyError(String(error)), "error");
  } finally {
    button.disabled = false;
    button.textContent = previousText || t("modelFetch");
  }
}

// 把后端返回的原始错误翻译成用户可理解的验证结论
function describeVerifyError(raw) {
  if (/HTTP 40[13]\b/.test(raw)) return t("verifyAuthFailed");
  if (/HTTP 40[45]\b/.test(raw)) return t("verifyNoModelsEndpoint");
  return raw;
}

// 真实连通性验证：带 API Key 请求 /models 端点（后端会按协议自动补 /v1 等路径），
// 能同时验证端点可达性与 Key 有效性，并顺带展示可用模型列表。
async function handleEndpointTest(kind) {
  const fields = productFieldMap(kind);
  const button = $(fields.endpointTestBtn);
  const baseUrl = $(fields.baseUrl).value.trim() || OFFICIAL_BASE_URLS[kind] || "";
  const apiKey = $(fields.apiKey).value.trim();
  const container = $(fields.endpointResults);
  if (!apiKey) {
    showToast(t("modelFetchMissing"), "warning");
    return;
  }

  const previousText = button.textContent;
  button.disabled = true;
  button.textContent = t("endpointTesting");
  const startedAt = performance.now();
  try {
    const models = await invoke("fetch_available_models", { baseUrl, apiKey, timeoutSecs: 12, protocol: productProtocol(kind) });
    const elapsed = Math.round(performance.now() - startedAt);
    const count = (models || []).length;
    if (container) {
      container.innerHTML = `<div class="endpoint-row"><span class="endpoint-url" title="${esc(baseUrl)}">${esc(baseUrl)}</span><span class="endpoint-meta fast">${t("verifyOkLabel")} · ${count} ${t("verifyModelsSuffix")} · ${elapsed}ms</span></div>`;
      container.classList.add("open");
    }
    renderModelResults(kind, models || []);
    showToast(t("verifyOkToast"), "success");
  } catch (error) {
    const message = describeVerifyError(String(error));
    if (container) {
      container.innerHTML = `<div class="endpoint-row"><span class="endpoint-url" title="${esc(baseUrl)}">${esc(baseUrl)}</span><span class="endpoint-meta failed">${esc(message)}</span></div>`;
      container.classList.add("open");
    }
    showToast(message, "error");
  } finally {
    button.disabled = false;
    button.textContent = previousText || t("endpointTest");
  }
}

function getSelectedClaudePreset() {
  const presetId = $("profilePresetSelect")?.value || "";
  return CLAUDE_PRESETS.find((preset) => preset.id === presetId) || null;
}

function renderClaudePresetOptions() {
  const select = $("profilePresetSelect");
  if (!select) return;
  const currentValue = select.value;
  select.innerHTML = [
    `<option value="">${t("providerPresetCustom")}</option>`,
    ...CLAUDE_PRESETS.map((preset) => `<option value="${esc(preset.id)}">${esc(preset.name)}</option>`),
  ].join("");
  select.value = CLAUDE_PRESETS.some((preset) => preset.id === currentValue) ? currentValue : "";
}

function applyClaudePreset(preset) {
  if (!preset) return;
  if (!$("profileName").value.trim()) {
    $("profileName").value = preset.name;
  }
  $("profileBaseUrl").value = preset.baseUrl;
  $("profileModelId").value = preset.model || "";
  // 内置预设均为 Anthropic 兼容端点，重置为直连
  if ($("profileApiFormat")) $("profileApiFormat").value = "anthropic";
  clearEndpointResults("claude");
  clearModelResults("claude");
}

function openModal(profile) {
  editingId = profile ? profile.id : null;
  $("modalTitle").textContent = profile ? t("editConfig") : t("addConfig");
  $("profileId").value = editingId || "";
  if ($("profilePresetSelect")) $("profilePresetSelect").value = "";
  $("profileName").value = profile ? profile.name : "";
  $("profileApiKey").value = profile ? profile.apiKey : "";
  $("profileBaseUrl").value = profile ? profile.baseUrl : "";
  $("profileModelId").value = profile ? (profile.modelId || "") : "";
  if ($("profileApiFormat")) $("profileApiFormat").value = profile ? (profile.apiFormat || "anthropic") : "anthropic";
  clearEndpointResults("claude");
  clearModelResults("claude");
  $("modalOverlay").classList.add("open");
  $("profileName").focus();
}

function closeModal() {
  $("modalOverlay").classList.remove("open");
  editingId = null;
}

async function handleSubmit(event) {
  event.preventDefault();
  if (profileSaving) return;
  const isNewProfile = !editingId;

  const name = $("profileName").value.trim();
  const apiKey = $("profileApiKey").value.trim();
  const baseUrl = $("profileBaseUrl").value.trim();
  const modelId = $("profileModelId").value.trim();
  const apiFormat = $("profileApiFormat")?.value || "anthropic";
  if (apiFormat === "openai_chat") {
    // OpenAI 格式没有官方地址可回退；模型名必填（Claude Code 发来的 claude-* 上游不认识）
    if (!baseUrl) {
      showToast(t("claudeApiFormatNeedsBaseUrl"), "warning");
      return;
    }
    if (!modelId) {
      showToast(t("claudeApiFormatNeedsModel"), "warning");
      return;
    }
  }

  profileSaving = true;
  const submitButton = $("submitBtn");
  setButtonBusy(submitButton, true, t("toastSaving") || "保存中...");
  try {
    if (editingId) {
      await invoke("update_profile", { id: editingId, name, apiKey, baseUrl, modelId, apiFormat });
      showToast(t("toastUpdated"), "success");
    } else {
      await invoke("add_profile", { name, apiKey, baseUrl, modelId: modelId || null, apiFormat });
      showToast(t("toastAdded"), "success");
    }

    closeModal();
    await loadProfiles();
    await loadStatus();
    if (isNewProfile) switchConsolePage("claude");
  } catch (error) {
    showToast(String(error), "error");
  } finally {
    profileSaving = false;
    setButtonBusy(submitButton, false);
  }
}

async function handleSwitch(id) {
  if (isSwitchingProfile) return;
  const profile = profiles.find((item) => item.id === id);
  if (!profile) return;

  isSwitchingProfile = true;
  showSwitchOverlay(profile.name, "claude");

  try {
    await waitForNextPaint();
    progressUnlisten = await listen("switch-progress", (event) => {
      updateSwitchProgress(event.payload);
    });
    switchingSnapshot = await invoke("snapshot_config");
    const result = await invoke("switch_profile", { id });

    if (result.success) {
      completeSwitchOverlay();
      $("switchStep1").className = "switch-step done";
      $("switchStep2").className = "switch-step done";
      $("switchStep3").className = "switch-step done";
      await new Promise((resolve) => setTimeout(resolve, 350));
    }

    if (result.cancelled) {
      try {
        await invoke("restore_config", { snapshot: switchingSnapshot });
        showToast(t("cancelledRestored"), "warning");
      } catch (restoreError) {
        showToast(t("cancelRestoreFailed", { error: String(restoreError) }), "error");
      }
    } else if (result.success) {
      showToast(t("switchedTo", { name: result.profileName }), "success");
    } else {
      // 构建成功项列表
      const okItems = [];
      if (result.results?.envVars) okItems.push(t("statusSystemEnv"));
      // 动态编辑器结果
      if (result.results?.editors) {
        for (const [editorId, success] of Object.entries(result.results.editors)) {
          if (success) okItems.push(detectedEditors[editorId] || editorId);
        }
      }
      if (result.results?.claude) okItems.push(t("statusClaude"));
      showToast(
        t("partialSuccess", {
          ok: okItems.join(", ") || "--",
          errors: (result.errors || []).join("; ") || "--"
        }),
        "warning"
      );
    }
  } catch (error) {
    const message = switchingSnapshot
      ? t("switchFailed", { error: String(error) })
      : t("snapshotFailed", { error: String(error) });
    showToast(message, "error");
  } finally {
    if (progressUnlisten) {
      progressUnlisten();
      progressUnlisten = null;
    }
    switchingSnapshot = null;
    hideSwitchOverlay();
    isSwitchingProfile = false;
    await Promise.all([loadProfiles(), loadStatus()]);
  }
}

async function handleCancelSwitch() {
  $("switchStepLabel").textContent = t("progressCancelling");
  $("switchCancelBtn").disabled = true;
  try {
    await invoke("cancel_switch");
  } catch (error) {
    showToast(String(error), "error");
  }
}

// A10: 编辑/删除逻辑统一走 PROVIDER_CONFIG 驱动的公共实现
function handleEdit(id) {
  editProviderProfile("claude", id);
}

async function handleDelete(id) {
  return deleteProviderProfile("claude", id);
}

async function handleImport() {
  const input = await appPrompt(
    currentLang === "zh"
      ? "系统将从当前 Claude 环境读取 Token / Base URL，并保存为新配置。"
      : "Import current Claude environment as a new profile.",
    t("importDefaultName"),
    {
      title: currentLang === "zh" ? "导入当前 Claude 配置" : "Import Claude Config",
      inputLabel: currentLang === "zh" ? "配置名称" : "Config name",
      confirmText: currentLang === "zh" ? "导入配置" : "Import",
    }
  );
  if (input === null) return;

  const name = String(input).trim() || t("importDefaultName");
  try {
    await invoke("import_current", { name });
    showToast(t("toastImported"), "success");
    await loadProfiles();
    await loadStatus();
  } catch (error) {
    showToast(String(error), "error");
  }
}

// ── Page Switching ──────────────────────────────────

function switchPage(page) {
  // 兼容旧横向 Tab：统一走控制台导航
  const map = { claude: "claude", codex: "codex", grok: "grok" };
  switchConsolePage(map[page] || page || "claude");
}

// ── Grok Profile Management ─────────────────────────

function getSelectedGrokPreset() {
  const presetId = $("grokPresetSelect")?.value || "";
  return GROK_PRESETS.find((preset) => preset.id === presetId) || null;
}

function renderGrokPresetOptions() {
  const select = $("grokPresetSelect");
  if (!select) return;
  const currentValue = select.value;
  select.innerHTML = [
    `<option value="">${t("grokPresetCustom")}</option>`,
    ...GROK_PRESETS.map((preset) => `<option value="${esc(preset.id)}">${esc(preset.name)}</option>`),
  ].join("");
  select.value = GROK_PRESETS.some((preset) => preset.id === currentValue) ? currentValue : "";
}

function updateGrokPresetHint() {
  const hint = $("grokPresetHint");
  if (!hint) return;
  hint.textContent = t("grokPresetHintDefault");
}

function applyGrokPreset(preset) {
  if (!preset) {
    updateGrokPresetHint();
    return;
  }
  if (!$("grokProfileName").value.trim()) {
    $("grokProfileName").value = preset.name;
  }
  $("grokBaseUrl").value = preset.baseUrl;
  $("grokModel").value = preset.model || "";
  clearEndpointResults("grok");
  clearModelResults("grok");
  updateGrokPresetHint();
}

async function loadGrokProfiles({ rethrow = false } = {}) {
  try {
    const data = await invoke("get_grok_profiles");
    grokProfiles = data.profiles || [];
    renderGrokProfiles();
    renderNavigationStatus();
  } catch (error) {
    showToast(String(error), "error");
    if (rethrow) throw error;
  }
}

function renderGrokProfiles() {
  const grid = $("grokProfilesGrid");
  if (!grid) return;

  if (grokProfiles.length === 0) {
    grid.innerHTML = `
      <div class="empty-state">
        <svg class="empty-state-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
          <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
          <polyline points="14 2 14 8 20 8"/>
          <line x1="12" y1="18" x2="12" y2="12"/>
          <line x1="9" y1="15" x2="15" y2="15"/>
        </svg>
        <div class="empty-state-title">${t("grokNoConfigsTitle")}</div>
        <p>${t("grokNoConfigsDesc")}</p>
      </div>`;
    updateGrokActiveConfigBar();
    return;
  }

  grid.innerHTML = grokProfiles.map((profile) => `
    <div class="profile-card ${profile.isActive ? "active" : ""}">
      <div class="profile-header">
        <span class="profile-name">${esc(profile.name)}</span>
        ${profile.isActive ? `<span class="active-badge">${t("inUse")}</span>` : ""}
      </div>
      <div class="profile-body">
        <div class="profile-field">
          <span class="field-label">${t("grokApiKeyLabel")}</span>
          <span class="field-value">${esc(maskKey(profile.apiKey))}</span>
        </div>
        <div class="profile-field">
          <span class="field-label">${t("grokBaseUrlLabel")}</span>
          <span class="field-value">${esc(truncUrl(profile.baseUrl, 50))}</span>
        </div>
        ${profile.model ? `<div class="profile-field">
          <span class="field-label">${t("grokModelLabel")}</span>
          <span class="field-value">${esc(profile.model)}</span>
        </div>` : ""}
        ${profile.apiBackend ? `<div class="profile-field">
          <span class="field-label">${t("grokApiBackendLabel")}</span>
          <span class="field-value">${esc(profile.apiBackend)}</span>
        </div>` : ""}
      </div>
      <div class="profile-actions">
        ${profile.isActive ? "" : `<button class="btn btn-switch btn-sm" data-action="grok-switch" data-id="${profile.id}" type="button">${t("switchUse")}</button>`}
        <button class="btn btn-secondary btn-sm" data-action="grok-edit" data-id="${profile.id}" type="button">${t("edit")}</button>
        <button class="btn btn-danger btn-sm" data-action="grok-delete" data-id="${profile.id}" type="button">${t("delete")}</button>
      </div>
    </div>
  `).join("");

  bindProfileGridActions(grid, "GrokProfiles", "grok");
  updateGrokActiveConfigBar();
}

function updateGrokActiveConfigBar() {
  // 与 Claude 保持一致:当前配置名直接显示在状态区标题中
  if (!$("grokStatusSectionTitle")) return;
  const active = grokProfiles.find((p) => p.isActive);
  const activeContext = active
    ? (currentLang === "zh" ? ` (当前: ${active.name})` : ` (Current: ${active.name})`)
    : "";
  setText("grokStatusSectionTitle", `${t("grokStatusTitle")}${activeContext}`);
}

async function loadGrokStatus({ rethrow = false } = {}) {
  try {
    const status = await invoke("get_grok_status");
    lastGrokStatus = status;
    const grid = $("grokStatusGrid");
    if (!grid) {
      renderNavigationStatus();
      return;
    }
    if (!status || (!status.apiKey && !status.configExists)) {
      grid.innerHTML = `<div class="status-card" style="display:flex;align-items:center;justify-content:center;color:var(--text-muted);font-size:13px;">Grok: --</div>`;
      renderNavigationStatus();
      return;
    }
    const COPY_ICON = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>`;
    grid.innerHTML = `
      <div class="status-card">
        <div class="status-card-title">
          <span class="status-card-title-text">${productIcon("grok")}Grok</span>
        </div>
        <div class="status-item">
          <span class="status-label">${t("grokApiKeyLabel")}</span>
          <div class="status-value-wrapper">
            <span class="status-value" title="${esc(status.apiKey || "--")}">${esc(maskKey(status.apiKey))}</span>
            ${status.apiKey ? `<button class="copy-btn" type="button" data-copy="${esc(status.apiKey)}" title="${t("copy")}" aria-label="${t("copy")}">${COPY_ICON}</button>` : ""}
          </div>
        </div>
        <div class="status-item">
          <span class="status-label">${t("grokBaseUrlLabel")}</span>
          <div class="status-value-wrapper">
            <span class="status-value" title="${esc(status.baseUrl || "--")}">${esc(status.baseUrl || "--")}</span>
            ${status.baseUrl ? `<button class="copy-btn" type="button" data-copy="${esc(status.baseUrl)}" title="${t("copy")}" aria-label="${t("copy")}">${COPY_ICON}</button>` : ""}
          </div>
        </div>
        <div class="status-item">
          <span class="status-label">${t("grokModelLabel")}</span>
          <div class="status-value-wrapper">
            <span class="status-value" title="${esc(status.model || "--")}">${esc(status.model || "--")}</span>
            ${status.model ? `<button class="copy-btn" type="button" data-copy="${esc(status.model)}" title="${t("copy")}" aria-label="${t("copy")}">${COPY_ICON}</button>` : ""}
          </div>
        </div>
        ${status.defaultModelId ? `<div class="status-item">
          <span class="status-label">${currentLang === "zh" ? "默认模型" : "Default Model"}</span>
          <div class="status-value-wrapper">
            <span class="status-value" title="${esc(status.defaultModelId)}">${esc(status.defaultModelId)}</span>
            <button class="copy-btn" type="button" data-copy="${esc(status.defaultModelId)}" title="${t("copy")}" aria-label="${t("copy")}">${COPY_ICON}</button>
          </div>
        </div>` : ""}
      </div>`;
    bindDelegatedCopyButtons(grid, "GrokStatusCopy");
    renderNavigationStatus();
  } catch (error) {
    // 与 loadStatus 保持一致，失败时弹 toast 让用户感知
    showToast(`Grok 状态加载失败: ${error}`, "error");
    console.error("Failed to load grok status:", error);
    if (rethrow) throw error;
  }
}

function renderGrokDiagnostics() {
  const panel = $("grokDiagnosticsPanel");
  if (!panel) return;
  const d = grokDiagnostics;
  if (!d) {
    panel.innerHTML = `<div class="diagnostics-card muted">Grok diagnostics: --</div>`;
    return;
  }
  const healthy = Array.isArray(d.issues) && d.issues.length === 0;
  const issueItems = (d.issues || []).map((item) => `<li>${esc(item)}</li>`).join("");
  const suggestionItems = (d.suggestions || []).map((item) => `<li>${esc(item)}</li>`).join("");
  panel.innerHTML = `
    <div class="diagnostics-card ${healthy ? "healthy" : "warning"}">
      <div class="diagnostics-head">
        <div>
          <div class="diagnostics-kicker">Grok Health</div>
          <div class="diagnostics-title">${healthy ? (currentLang === "zh" ? "配置健康" : "Healthy") : (currentLang === "zh" ? "需要处理" : "Needs attention")}</div>
        </div>
        <span class="diagnostics-state ${healthy ? "ok" : "warn"}">${healthy ? "OK" : `${d.issues.length} issues`}</span>
      </div>
      <div class="diagnostics-grid">
        <div><span>Active</span><strong>${esc(d.activeProfileName || "--")}</strong></div>
        <div><span>default</span><strong>${esc(d.defaultModelId || "--")}</strong></div>
        <div><span>Model</span><strong>${esc(d.model || "--")}</strong></div>
        <div><span>Backend</span><strong>${esc(d.apiBackend || "--")}</strong></div>
      </div>
      <div class="diagnostics-paths">
        <div title="${esc(d.configPath || "")}">config: ${esc(d.configExists ? d.configPath : (currentLang === "zh" ? "未找到" : "missing"))}</div>
        <div>source: ${esc(d.source || "--")}</div>
      </div>
      ${issueItems ? `<div class="diagnostics-list"><strong>${currentLang === "zh" ? "问题" : "Issues"}</strong><ul>${issueItems}</ul></div>` : ""}
      ${suggestionItems ? `<div class="diagnostics-list"><strong>${currentLang === "zh" ? "建议" : "Suggestions"}</strong><ul>${suggestionItems}</ul></div>` : ""}
      <div class="diagnostics-footer">Last checked: ${esc(d.lastCheckedAt || "--")}</div>
    </div>`;
}

async function loadGrokDiagnostics({ rethrow = false } = {}) {
  try {
    grokDiagnostics = await invoke("get_grok_diagnostics");
    renderGrokDiagnostics();
  } catch (error) {
    const panel = $("grokDiagnosticsPanel");
    if (panel) {
      panel.innerHTML = `<div class="diagnostics-card warning">${currentLang === "zh" ? "诊断加载失败" : "Diagnostics failed"}：${esc(String(error))}</div>`;
    }
    if (rethrow) throw error;
  }
}

async function handleGrokRuntimeBackup() {
  try {
    const path = await invoke("backup_grok_runtime");
    showToast(currentLang === "zh" ? `已备份: ${path}` : `Backed up: ${path}`, "success");
    await loadGrokDiagnostics();
  } catch (error) {
    showToast(String(error), "error");
  }
}

async function handleOpenGrokFolder() {
  try {
    await invoke("open_grok_config_folder");
  } catch (error) {
    showToast(String(error), "error");
  }
}

function openGrokModal(profile) {
  editingGrokId = profile ? profile.id : null;
  $("grokModalTitle").textContent = profile ? t("grokEditConfig") : t("grokAddConfig");
  $("grokProfileId").value = editingGrokId || "";
  $("grokPresetSelect").value = "";
  $("grokProfileName").value = profile ? profile.name : "";
  $("grokApiKey").value = profile ? profile.apiKey : "";
  $("grokBaseUrl").value = profile ? profile.baseUrl : "";
  $("grokModel").value = profile ? (profile.model || "") : "";
  if ($("grokApiBackend")) {
    $("grokApiBackend").value = profile?.apiBackend || "chat_completions";
  }
  updateGrokPresetHint();
  clearEndpointResults("grok");
  clearModelResults("grok");
  $("grokModalOverlay").classList.add("open");
  document.body.classList.add("modal-open");
  $("grokProfileName").focus();
}

function closeGrokModal() {
  $("grokModalOverlay").classList.remove("open");
  document.body.classList.remove("modal-open");
  editingGrokId = null;
}

async function handleGrokSubmit(event) {
  event.preventDefault();
  if (grokProfileSaving) return;
  const isNewProfile = !editingGrokId;
  const name = $("grokProfileName").value.trim();
  const apiKey = $("grokApiKey").value.trim();
  const baseUrl = $("grokBaseUrl").value.trim() || "https://api.x.ai/v1";
  const model = $("grokModel").value.trim();
  const apiBackend = $("grokApiBackend")?.value || "chat_completions";

  grokProfileSaving = true;
  const submitButton = $("grokSubmitBtn");
  setButtonBusy(submitButton, true, t("toastSaving"));
  try {
    if (editingGrokId) {
      await invoke("update_grok_profile", {
        id: editingGrokId,
        name,
        apiKey,
        baseUrl,
        model: model || null,
        apiBackend,
      });
      showToast(t("grokToastUpdated"), "success");
    } else {
      await invoke("add_grok_profile", {
        name,
        apiKey,
        baseUrl,
        model: model || null,
        apiBackend,
      });
      showToast(t("grokToastAdded"), "success");
    }
    closeGrokModal();
    await Promise.all([loadGrokProfiles(), loadGrokStatus(), loadGrokDiagnostics()]);
    if (isNewProfile) switchConsolePage("grok");
  } catch (error) {
    showToast(String(error), "error");
  } finally {
    grokProfileSaving = false;
    setButtonBusy(submitButton, false);
  }
}

// A10: 切换/删除逻辑统一走 PROVIDER_CONFIG 驱动的公共实现
async function handleGrokSwitch(id) {
  return switchProviderProfile("grok", id);
}

async function handleGrokDelete(id) {
  return deleteProviderProfile("grok", id);
}

async function handleGrokImport() {
  const input = await appPrompt(
    currentLang === "zh"
      ? "系统将从 ~/.grok/config.toml 读取当前 Grok 配置并保存。"
      : "Import current Grok config.toml as a new profile.",
    t("grokImportDefaultName"),
    {
      title: currentLang === "zh" ? "导入当前 Grok 配置" : "Import Grok Config",
      inputLabel: currentLang === "zh" ? "配置名称" : "Config name",
      confirmText: currentLang === "zh" ? "导入配置" : "Import",
    }
  );
  if (input === null) return;
  const name = String(input).trim() || t("grokImportDefaultName");
  try {
    await invoke("import_grok_current", { name });
    showToast(t("grokToastImported"), "success");
    await Promise.all([loadGrokProfiles(), loadGrokStatus(), loadGrokDiagnostics()]);
  } catch (error) {
    showToast(String(error), "error");
  }
}

// ── Gemini Profile Management ──────────────────────

async function loadGeminiProfiles({ rethrow = false } = {}) {
  try {
    const data = await invoke("get_gemini_profiles");
    geminiProfiles = data.profiles || [];
    renderGeminiProfiles();
    renderNavigationStatus();
  } catch (error) {
    showToast(String(error), "error");
    if (rethrow) throw error;
  }
}

function updateGeminiStatusTitle() {
  // 与 Claude 保持一致:当前配置名直接显示在状态区标题中
  if (!$("geminiStatusSectionTitle")) return;
  const base = currentLang === "zh" ? "Gemini 状态" : "Gemini Status";
  const active = geminiProfiles.find((p) => p.isActive);
  const activeContext = active
    ? (currentLang === "zh" ? ` (当前: ${active.name})` : ` (Current: ${active.name})`)
    : "";
  setText("geminiStatusSectionTitle", `${base}${activeContext}`);
}

function renderGeminiProfiles() {
  const grid = $("geminiProfilesGrid");
  if (!grid) return;
  updateGeminiStatusTitle();
  if (geminiProfiles.length === 0) {
    grid.innerHTML = `
      <div class="empty-state">
        <svg class="empty-state-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
          <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
          <polyline points="14 2 14 8 20 8"/>
          <line x1="12" y1="18" x2="12" y2="12"/>
          <line x1="9" y1="15" x2="15" y2="15"/>
        </svg>
        <div class="empty-state-title">${t("geminiNoConfigsTitle")}</div>
        <p>${t("geminiNoConfigsDesc")}</p>
      </div>`;
    return;
  }
  grid.innerHTML = geminiProfiles.map((profile) => `
    <div class="profile-card ${profile.isActive ? "active" : ""}">
      <div class="profile-header">
        <span class="profile-name">${esc(profile.name)}</span>
        ${profile.isActive ? `<span class="active-badge">${t("inUse")}</span>` : ""}
      </div>
      <div class="profile-body">
        <div class="profile-field">
          <span class="field-label">Gemini API Key</span>
          <span class="field-value">${esc(maskKey(profile.apiKey))}</span>
        </div>
        <div class="profile-field">
          <span class="field-label">Gemini Base URL</span>
          <span class="field-value">${esc(truncUrl(profile.baseUrl, 50))}</span>
        </div>
        ${profile.model ? `<div class="profile-field">
          <span class="field-label">${currentLang === "zh" ? "Gemini 模型" : "Gemini Model"}</span>
          <span class="field-value">${esc(profile.model)}</span>
        </div>` : ""}
      </div>
      <div class="profile-actions">
        ${profile.isActive ? "" : `<button class="btn btn-switch btn-sm" data-action="gemini-switch" data-id="${profile.id}" type="button">${t("switchUse")}</button>`}
        <button class="btn btn-secondary btn-sm" data-action="gemini-edit" data-id="${profile.id}" type="button">${t("edit")}</button>
        <button class="btn btn-danger btn-sm" data-action="gemini-delete" data-id="${profile.id}" type="button">${t("delete")}</button>
      </div>
    </div>
  `).join("");
  bindProfileGridActions(grid, "GeminiProfiles", "gemini");
}

async function loadGeminiStatus({ rethrow = false } = {}) {
  try {
    const status = await invoke("get_gemini_status");
    lastGeminiStatus = status;
    const grid = $("geminiStatusGrid");
    if (!grid) return;
    if (!status) {
      grid.innerHTML = `<div class="status-card" style="display:flex;align-items:center;justify-content:center;color:var(--text-muted);font-size:13px;">Gemini: --</div>`;
      renderNavigationStatus();
      return;
    }
    const COPY_ICON = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>`;
    grid.innerHTML = `
      <div class="status-card">
        <div class="status-card-title">
          <span class="status-card-title-text">${productIcon("gemini")}Gemini</span>
        </div>
        <div class="status-item">
          <span class="status-label">Gemini API Key</span>
          <div class="status-value-wrapper">
            <span class="status-value" title="${esc(status.apiKey || "--")}">${esc(maskKey(status.apiKey))}</span>
            ${status.apiKey ? `<button class="copy-btn" type="button" data-copy="${esc(status.apiKey)}" title="${t("copy")}" aria-label="${t("copy")}">${COPY_ICON}</button>` : ""}
          </div>
        </div>
        <div class="status-item">
          <span class="status-label">Gemini Base URL</span>
          <div class="status-value-wrapper">
            <span class="status-value" title="${esc(status.baseUrl || "--")}">${esc(status.baseUrl || "--")}</span>
            ${status.baseUrl ? `<button class="copy-btn" type="button" data-copy="${esc(status.baseUrl)}" title="${t("copy")}" aria-label="${t("copy")}">${COPY_ICON}</button>` : ""}
          </div>
        </div>
        <div class="status-item">
          <span class="status-label">${currentLang === "zh" ? "Gemini 模型" : "Gemini Model"}</span>
          <div class="status-value-wrapper">
            <span class="status-value" title="${esc(status.model || "--")}">${esc(status.model || "--")}</span>
            ${status.model ? `<button class="copy-btn" type="button" data-copy="${esc(status.model)}" title="${t("copy")}" aria-label="${t("copy")}">${COPY_ICON}</button>` : ""}
          </div>
        </div>
        <div class="status-item">
          <span class="status-label">Auth</span>
          <div class="status-value-wrapper">
            <span class="status-value" title="${esc(status.authType || "--")}">${esc(status.authType || "--")}</span>
          </div>
        </div>
      </div>`;
    bindDelegatedCopyButtons(grid, "GeminiStatusCopy");
    renderNavigationStatus();
  } catch (error) {
    showToast(String(error), "error");
    if (rethrow) throw error;
  }
}

function openGeminiModal(profile) {
  editingGeminiId = profile?.id || null;
  $("geminiModalTitle").textContent = profile ? "编辑 Gemini 配置" : "添加 Gemini 配置";
  $("geminiProfileId").value = editingGeminiId || "";
  $("geminiProfileName").value = profile?.name || "";
  $("geminiApiKey").value = profile?.apiKey || "";
  $("geminiBaseUrl").value = profile?.baseUrl || "";
  $("geminiModel").value = profile?.model || "gemini-2.5-pro";
  clearEndpointResults("gemini");
  clearModelResults("gemini");
  $("geminiModalOverlay").classList.add("open");
  document.body.classList.add("modal-open");
  $("geminiProfileName").focus();
}

function closeGeminiModal() {
  $("geminiModalOverlay").classList.remove("open");
  document.body.classList.remove("modal-open");
  editingGeminiId = null;
}

async function handleGeminiSubmit(event) {
  event.preventDefault();
  if (geminiProfileSaving) return;
  const payload = {
    name: $("geminiProfileName").value.trim(),
    apiKey: $("geminiApiKey").value.trim(),
    baseUrl: $("geminiBaseUrl").value.trim(),
    model: $("geminiModel").value.trim() || null,
  };
  geminiProfileSaving = true;
  const submitButton = $("geminiSubmitBtn");
  setButtonBusy(submitButton, true, t("toastSaving"));
  try {
    if (editingGeminiId) await invoke("update_gemini_profile", { id: editingGeminiId, ...payload });
    else await invoke("add_gemini_profile", payload);
    showToast(editingGeminiId ? "Gemini 配置已更新" : "Gemini 配置已添加", "success");
    closeGeminiModal();
    await Promise.all([loadGeminiProfiles(), loadGeminiStatus()]);
    switchConsolePage("gemini");
  } catch (error) {
    showToast(String(error), "error");
  } finally {
    geminiProfileSaving = false;
    setButtonBusy(submitButton, false);
  }
}

// A10: 切换/删除逻辑统一走 PROVIDER_CONFIG 驱动的公共实现
async function handleGeminiSwitch(id) {
  return switchProviderProfile("gemini", id);
}

async function handleGeminiDelete(id) {
  return deleteProviderProfile("gemini", id);
}

async function handleGeminiImport() {
  const input = await appPrompt("从 Gemini CLI 当前环境变量与 settings.json 导入配置。", "当前 Gemini 配置", { title: "导入 Gemini 配置", inputLabel: "配置名称", confirmText: "导入配置" });
  if (input === null) return;
  try {
    await invoke("import_gemini_current", { name: String(input).trim() || "当前 Gemini 配置" });
    showToast("当前 Gemini 配置已导入", "success");
    await Promise.all([loadGeminiProfiles(), loadGeminiStatus()]);
  } catch (error) {
    showToast(String(error), "error");
  }
}

// ── Codex Profile Management ────────────────────────

let editingCodexId = null;
let codexProfileSaving = false;

function getSelectedCodexPreset() {
  const presetId = $("codexPresetSelect")?.value || "";
  return CODEX_PRESETS.find((preset) => preset.id === presetId) || null;
}

function renderCodexPresetOptions() {
  const select = $("codexPresetSelect");
  if (!select) return;
  const currentValue = select.value;
  select.innerHTML = [
    `<option value="">${t("codexPresetCustom")}</option>`,
    ...CODEX_PRESETS.map((preset) => `<option value="${esc(preset.id)}">${esc(preset.name)}</option>`),
  ].join("");
  select.value = CODEX_PRESETS.some((preset) => preset.id === currentValue) ? currentValue : "";
}

function updateCodexPresetHint() {
  const hint = $("codexPresetHint");
  if (!hint) return;
  hint.classList.remove("warning");
  hint.textContent = t("codexPresetHintDefault");
}

function applyCodexPreset(preset) {
  if (!preset) {
    updateCodexPresetHint();
    return;
  }
  if (!$("codexProfileName").value.trim()) {
    $("codexProfileName").value = preset.name;
  }
  $("codexBaseUrl").value = preset.baseUrl;
  $("codexModel").value = preset.model;
  $("codexProvider").value = preset.providerName;
  if ($("codexWireApi")) $("codexWireApi").value = preset.wire || "responses";
  clearEndpointResults("codex");
  clearModelResults("codex");
  updateCodexPresetHint();
  updateCodexOfficialConfigPreview();
}

function getCodexAuthMode() {
  if ($("codexAuthModeSaveOnly")?.checked) return "save_only";
  if ($("codexAuthModeOfficial")?.checked) return "official_account_api_quota";
  return "auth_json";
}

function setCodexAuthMode(mode) {
  const official = mode === "official_account_api_quota";
  const saveOnly = mode === "save_only";
  if ($("codexAuthModeOfficial")) $("codexAuthModeOfficial").checked = official;
  if ($("codexAuthModeSaveOnly")) $("codexAuthModeSaveOnly").checked = saveOnly;
  if ($("codexAuthModeDefault")) $("codexAuthModeDefault").checked = !official && !saveOnly;
  updateCodexAuthModeUi();
}

function buildCodexOfficialConfig() {
  const baseUrl = $("codexBaseUrl").value.trim().replace(/\\/g, "\\\\").replace(/"/g, '\\"');
  const apiKey = $("codexApiKey").value.trim().replace(/\\/g, "\\\\").replace(/"/g, '\\"');
  return `model_provider = "customer"
model = "gpt-5.5"
review_model = "gpt-5.5"
model_reasoning_effort = "xhigh"
disable_response_storage = true
preferred_auth_method = "apikey"

[model_providers.customer]
name = "customer"
wire_api = "responses"
requires_openai_auth = true
base_url = "${baseUrl}"
experimental_bearer_token = "${apiKey}"`;
}

function updateCodexOfficialConfigPreview() {
  const preview = $("codexOfficialConfig");
  if (preview) preview.value = buildCodexOfficialConfig();
}

function updateCodexAuthModeUi() {
  const isOfficialMode = getCodexAuthMode() === "official_account_api_quota";
  if ($("codexOfficialConfigGroup")) {
    $("codexOfficialConfigGroup").style.display = isOfficialMode ? "block" : "none";
  }
  updateCodexOfficialConfigPreview();
}

async function copyCodexOfficialConfig() {
  updateCodexOfficialConfigPreview();
  try {
    await navigator.clipboard.writeText($("codexOfficialConfig").value);
    showToast(t("toastCopied"), "success");
  } catch (error) {
    showToast(String(error), "error");
  }
}

async function loadCodexProfiles() {
  try {
    const data = await invoke("get_codex_profiles");
    codexProfiles = data.profiles || [];
    renderCodexProfiles();
    renderNavigationStatus();
  } catch (error) {
    showToast(String(error), "error");
  }
}

function renderCodexProfiles() {
  const grid = $("codexProfilesGrid");
  if (!grid) return;

  if (codexProfiles.length === 0) {
    grid.innerHTML = `
      <div class="empty-state">
        <svg class="empty-state-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
          <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
          <polyline points="14 2 14 8 20 8"/>
          <line x1="12" y1="18" x2="12" y2="12"/>
          <line x1="9" y1="15" x2="15" y2="15"/>
        </svg>
        <div class="empty-state-title">${t("codexNoConfigsTitle")}</div>
        <p>${t("codexNoConfigsDesc")}</p>
      </div>`;
    updateCodexStatusTitle();
    return;
  }

  grid.innerHTML = codexProfiles.map((profile) => `
    <div class="profile-card ${profile.isActive ? "active" : ""}">
      <div class="profile-header">
        <span class="profile-name">${esc(profile.name)}</span>
        ${profile.isActive ? `<span class="active-badge">${t("inUse")}</span>` : ""}
      </div>
      <div class="profile-body">
        <div class="profile-field">
          <span class="field-label">${t("codexApiKeyLabel")}</span>
          <span class="field-value">${esc(maskKey(profile.apiKey))}</span>
        </div>
        <div class="profile-field">
          <span class="field-label">${t("codexBaseUrlLabel")}</span>
          <span class="field-value">${esc(truncUrl(profile.baseUrl, 50))}</span>
        </div>
        ${profile.model ? `<div class="profile-field">
          <span class="field-label">${t("codexModelLabel")}</span>
          <span class="field-value">${esc(profile.model)}</span>
        </div>` : ""}
        ${profile.providerName ? `<div class="profile-field">
          <span class="field-label">${t("codexProviderLabel")}</span>
          <span class="field-value">${esc(profile.providerName)}</span>
        </div>` : ""}
        ${profile.wireApi === "chat" ? `<div class="profile-field">
          <span class="field-label">${t("codexWireApiLabel")}</span>
          <span class="field-value">Chat Completions</span>
        </div>` : ""}
        ${profile.imageApiKey ? `<div class="profile-field">
          <span class="field-label">${t("codexImageApiKeyLabel")}</span>
          <span class="field-value">${esc(maskKey(profile.imageApiKey))}</span>
        </div>` : ""}
        ${profile.imageApiKey && profile.imageBaseUrl ? `<div class="profile-field">
          <span class="field-label">${t("codexImageBaseUrlLabel")}</span>
          <span class="field-value">${esc(truncUrl(profile.imageBaseUrl, 50))}</span>
        </div>` : ""}
      </div>
      <div class="profile-actions">
        ${profile.isActive ? "" : `<button class="btn btn-switch btn-sm" data-action="codex-switch" data-id="${profile.id}" type="button">${t("switchUse")}</button>`}
        <button class="btn btn-secondary btn-sm" data-action="codex-edit" data-id="${profile.id}" type="button">${t("edit")}</button>
        <button class="btn btn-danger btn-sm" data-action="codex-delete" data-id="${profile.id}" type="button">${t("delete")}</button>
      </div>
    </div>
  `).join("");

  bindProfileGridActions(grid, "CodexProfiles", "codex");
  updateCodexStatusTitle();
}

function updateCodexStatusTitle() {
  const active = codexProfiles.find((p) => p.isActive);
  const activeContext = active
    ? (currentLang === "zh" ? ` (当前: ${active.name})` : ` (Current: ${active.name})`)
    : "";
  setText("codexStatusSectionTitle", `${t("codexStatusTitle")}${activeContext}`);
}

async function loadCodexStatus() {
  try {
    const status = await invoke("get_codex_status");
    lastCodexStatus = status;
    const grid = $("codexStatusGrid");
    if (!status || !status.apiKey) {
      if (grid) {
        grid.innerHTML = `<div class="status-card" style="display:flex;align-items:center;justify-content:center;color:var(--text-muted);font-size:13px;">Codex: --</div>`;
      }
      renderNavigationStatus();
      return;
    }
    const COPY_ICON = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>`;
    grid.innerHTML = `
      <div class="status-card">
        <div class="status-card-title">
          <span class="status-card-title-text">${productIcon("codex")}Codex</span>
        </div>
        <div class="status-item">
          <span class="status-label">${t("codexApiKeyLabel")}</span>
          <div class="status-value-wrapper">
            <span class="status-value">${esc(maskKey(status.apiKey))}</span>
            <button class="copy-btn" type="button" data-copy="${esc(status.apiKey || "")}" title="Copy">${COPY_ICON}</button>
          </div>
        </div>
        <div class="status-item">
          <span class="status-label">${t("codexBaseUrlLabel")}</span>
          <div class="status-value-wrapper">
            <span class="status-value has-tooltip" data-tooltip="${esc(status.baseUrl || "")}" title="${esc(status.baseUrl || "")}">${esc(status.baseUrl || "--")}</span>
            <button class="copy-btn" type="button" data-copy="${esc(status.baseUrl || "")}" title="Copy">${COPY_ICON}</button>
          </div>
        </div>
        <div class="status-item">
          <span class="status-label">${currentLang === "zh" ? "Codex 模型" : "Codex Model"}</span>
          <div class="status-value-wrapper">
            <span class="status-value" title="${esc(status.model || "--")}">${esc(status.model || "--")}</span>
            ${status.model ? `<button class="copy-btn" type="button" data-copy="${esc(status.model)}" title="${t("copy")}" aria-label="${t("copy")}">${COPY_ICON}</button>` : ""}
          </div>
        </div>
        ${status.imageApiKey ? `<div class="status-item">
          <span class="status-label">${t("codexImageApiKeyLabel")}</span>
          <div class="status-value-wrapper">
            <span class="status-value">${esc(maskKey(status.imageApiKey))}</span>
            <button class="copy-btn" type="button" data-copy="${esc(status.imageApiKey || "")}" title="Copy">${COPY_ICON}</button>
          </div>
        </div>
        <div class="status-item">
          <span class="status-label">${t("codexImageBaseUrlLabel")}</span>
          <div class="status-value-wrapper">
            <span class="status-value has-tooltip" data-tooltip="${esc(status.imageBaseUrl || "")}" title="${esc(status.imageBaseUrl || "")}">${esc(status.imageBaseUrl || "--")}</span>
            <button class="copy-btn" type="button" data-copy="${esc(status.imageBaseUrl || "")}" title="Copy">${COPY_ICON}</button>
          </div>
        </div>
        <div class="status-item">
          <span class="status-label">${t("codexImageSectionTitle")}</span>
          <div class="status-value-wrapper">
            <span class="status-value">${t(status.imageSkillInstalled ? "codexImageSkillReady" : "codexImageSkillNeedsSwitch")}</span>
          </div>
        </div>` : ""}
      </div>`;
    if (grid) {
      bindDelegatedCopyButtons(grid, "CodexStatusCopy");
    }
    renderNavigationStatus();
  } catch (error) {
    console.error("Failed to load codex status:", error);
  }
}

function openCodexModal(profile) {
  editingCodexId = profile ? profile.id : null;
  codexEnableAfterSave = false;
  $("codexModalTitle").textContent = profile ? t("codexEditConfig") : t("codexAddConfig");
  $("codexProfileId").value = editingCodexId || "";
  if ($("codexPresetSelect")) $("codexPresetSelect").value = "";
  if ($("codexProfileName")) $("codexProfileName").value = profile ? profile.name : "";
  if ($("codexApiKey")) {
    $("codexApiKey").value = profile ? profile.apiKey : "";
    $("codexApiKey").type = "password";
  }
  if ($("codexApiKeyToggle")) $("codexApiKeyToggle").textContent = currentLang === "zh" ? "显示" : "Show";
  if ($("codexBaseUrl")) $("codexBaseUrl").value = profile ? profile.baseUrl : "";
  if ($("codexModel")) $("codexModel").value = profile ? (profile.model || "") : "";
  if ($("codexWireApi")) $("codexWireApi").value = profile ? (profile.wireApi || "responses") : "responses";
  if ($("codexProvider")) $("codexProvider").value = profile ? (profile.providerName || "") : "";
  if ($("codexImageApiKey")) $("codexImageApiKey").value = profile ? (profile.imageApiKey || "") : "";
  if ($("codexImageBaseUrl")) $("codexImageBaseUrl").value = profile ? (profile.imageBaseUrl || "") : "";
  setCodexAuthMode(profile ? (profile.authMode || "auth_json") : "auth_json");
  // 有高级字段时自动展开高级选项，避免用户找不到已保存的内容
  const advanced = $("codexAdvancedSection");
  if (advanced) {
    advanced.open = !!(profile && (
      (profile.authMode && profile.authMode !== "auth_json")
      || profile.providerName
      || profile.imageApiKey
      || profile.imageBaseUrl
    ));
  }
  updateCodexPresetHint();
  clearEndpointResults("codex");
  clearModelResults("codex");
  $("codexModalOverlay").classList.add("open");
  document.body.classList.add("modal-open");
  setTimeout(() => {
    if (profile) $("codexProfileName")?.focus();
    else $("codexPresetSelect")?.focus();
  }, 20);
}

function closeCodexModal() {
  $("codexModalOverlay").classList.remove("open");
  document.body.classList.remove("modal-open");
  editingCodexId = null;
}

async function handleCodexSubmit(event) {
  event.preventDefault();
  if (codexProfileSaving) return;
  const isNewProfile = !editingCodexId;
  const editingActiveProfile = codexProfiles.some((profile) => profile.id === editingCodexId && profile.isActive);
  const name = $("codexProfileName").value.trim();
  const apiKey = $("codexApiKey").value.trim();
  const baseUrl = $("codexBaseUrl").value.trim();
  const model = $("codexModel").value.trim();
  const wireApi = $("codexWireApi")?.value === "chat" ? "chat" : "responses";
  const providerName = $("codexProvider").value.trim();
  const imageApiKey = $("codexImageApiKey").value.trim();
  const imageBaseUrl = $("codexImageBaseUrl").value.trim();
  if (!name) {
    showToast(currentLang === "zh" ? "请填写配置名称" : "Config name is required", "warning");
    return;
  }
  if (!apiKey) {
    showToast(currentLang === "zh" ? "请填写 API Key" : "API Key is required", "warning");
    return;
  }
  let authMode = getCodexAuthMode();
  // save_only：前端仅保存配置，后端仍用 auth_json 字段存储，不自动切换
  const saveOnly = authMode === "save_only";
  if (saveOnly) authMode = "auth_json";
  const enableAfter = (codexEnableAfterSave || editingActiveProfile) && !saveOnly;
  codexEnableAfterSave = false;

  codexProfileSaving = true;
  const submitButton = $("codexSubmitBtn");
  setButtonBusy(submitButton, true, t("toastSaving"));
  try {
    let savedId = editingCodexId;
    if (editingCodexId) {
      await invoke("update_codex_profile", { id: editingCodexId, name, apiKey, baseUrl, model: model || null, providerName: providerName || null, authMode, wireApi, imageApiKey: imageApiKey || null, imageBaseUrl: imageBaseUrl || null });
      showToast(t("codexToastUpdated"), "success");
    } else {
      const created = await invoke("add_codex_profile", { name, apiKey, baseUrl, model: model || null, providerName: providerName || null, authMode, wireApi, imageApiKey: imageApiKey || null, imageBaseUrl: imageBaseUrl || null });
      savedId = created?.id || created?.profile?.id || null;
      showToast(t("codexToastAdded"), "success");
    }
    closeCodexModal();
    await loadCodexProfiles();
    if (!savedId) {
      const found = codexProfiles.find((p) => p.name === name);
      savedId = found?.id || null;
    }
    if (enableAfter && savedId) {
      await handleCodexSwitch(savedId);
    } else {
      await loadCodexStatus();
    }
    if (isNewProfile) switchConsolePage("codex");
  } catch (error) {
    showToast(String(error), "error");
  } finally {
    codexProfileSaving = false;
    setButtonBusy(submitButton, false);
  }
}

// A10: 切换/删除逻辑统一走 PROVIDER_CONFIG 驱动的公共实现
async function handleCodexSwitch(id) {
  return switchProviderProfile("codex", id);
}

async function handleCodexDelete(id) {
  return deleteProviderProfile("codex", id);
}

async function handleCodexImport() {
  const input = await appPrompt(
    currentLang === "zh"
      ? "系统将从本地 Codex 配置文件读取 API Key、Base URL、模型等信息，并保存为一个新的 VarSwitch 配置项。"
      : "Import current Codex runtime files as a new profile.",
    t("codexImportDefaultName"),
    {
      title: currentLang === "zh" ? "导入当前 Codex 配置" : "Import Codex Config",
      inputLabel: currentLang === "zh" ? "配置名称" : "Config name",
      confirmText: currentLang === "zh" ? "导入配置" : "Import",
    }
  );
  if (input === null) return;
  const name = String(input).trim() || t("codexImportDefaultName");
  try {
    await invoke("import_codex_current", { name });
    showToast(t("codexToastImported"), "success");
    await loadCodexProfiles();
    await loadCodexStatus();
  } catch (error) {
    showToast(String(error), "error");
  }
}

// ── Skills Management ───────────────────────────────

function openSkillsPanel() {
  openDeveloperTools("skills");
}

function closeSkillsPanel() {
  $("skillsOverlay").classList.remove("open");
  hideSkillsEdit();
}

function switchSkillsTab(tab) {
  activeSkillsTab = tab;
  $("skillsTabInstalled").classList.toggle("active", tab === "installed");
  $("skillsTabDiscover").classList.toggle("active", tab === "discover");
  $("skillsInstalledContent").style.display = tab === "installed" ? "" : "none";
  $("skillsDiscoverContent").style.display = tab === "discover" ? "" : "none";

  if (tab === "discover" && discoverSkills.length === 0 && !isDiscovering) {
    discoverSkillsFromRepos();
  }
}

async function loadSkills() {
  try {
    skillsData = await invoke("get_skills");
    renderSkills();
  } catch (error) {
    showToast(String(error), "error");
  }
}

function renderSkills() {
  const list = $("skillsList");
  if (skillsData.length === 0) {
    list.innerHTML = `<div class="mgmt-empty">${t("noSkills")}</div>`;
    return;
  }

  list.innerHTML = skillsData.map((skill) => {
    const isSkillType = skill.sourceType === "skill";
    const typeLabel = isSkillType ? t("sourceSkill") : t("sourceCommand");
    const typeBadge = `<span class="skill-card-badge ${isSkillType ? "installed" : "repo"}">${typeLabel}</span>`;
    const prefix = isSkillType ? "" : "/";
    // 显示描述：优先用 frontmatter 中的 description，否则取内容第一行
    const desc = skill.description || (skill.content || "").split("\n").find((l) => l.trim() && !l.startsWith("---")) || "";

    return `
    <div class="mgmt-item">
      <div class="mgmt-item-info">
        <div class="mgmt-item-name">${prefix}${esc(skill.name)} ${typeBadge}</div>
        <div class="mgmt-item-desc">${esc(desc.substring(0, 100))}</div>
      </div>
      <div class="mgmt-item-actions">
        <button class="btn btn-secondary btn-sm" data-action="edit-skill" data-name="${esc(skill.name)}" data-source-type="${esc(skill.sourceType || "command")}">${t("edit")}</button>
        <button class="btn btn-danger btn-sm" data-action="delete-skill" data-name="${esc(skill.name)}" data-source-type="${esc(skill.sourceType || "command")}">${t("delete")}</button>
      </div>
    </div>
    `;
  }).join("");

  // A10: 容器级委托，避免每次重建列表都逐个按钮绑定
  bindDelegatedActions(list, "SkillsList", (action, target) => {
    const name = target.getAttribute("data-name");
    const sourceType = target.getAttribute("data-source-type") || "command";
    if (action === "edit-skill") showSkillsEdit(name, sourceType);
    else if (action === "delete-skill") handleDeleteSkill(name, sourceType);
  });
}

function showSkillsEdit(name, sourceType) {
  editingSkillName = name || null;
  const skill = name ? skillsData.find((s) => s.name === name) : null;
  $("skillNameInput").value = skill ? skill.name : "";
  $("skillContentInput").value = skill ? skill.content : "";
  $("skillNameInput").disabled = !!name;
  // 记录当前编辑的 sourceType
  $("skillsEdit").dataset.sourceType = sourceType || (skill ? skill.sourceType : "command") || "command";
  $("skillsList").style.display = "none";
  $("skillsToolbar").style.display = "none";
  $("skillsEdit").style.display = "";
}

function hideSkillsEdit() {
  $("skillsList").style.display = "";
  $("skillsToolbar").style.display = "";
  $("skillsEdit").style.display = "none";
  editingSkillName = null;
}

async function handleSaveSkill() {
  if (skillSaving) return;
  const name = $("skillNameInput").value.trim();
  const content = $("skillContentInput").value;
  const sourceType = $("skillsEdit").dataset.sourceType || "command";
  if (!name) return;

  skillSaving = true;
  const saveBtn = $("skillSaveBtn");
  setButtonBusy(saveBtn, true, t("toastSaving"));
  try {
    await invoke("save_skill", { name, content, sourceType });
    showToast(t("toastSkillSaved"), "success");
    hideSkillsEdit();
    await loadSkills();
  } catch (error) {
    showToast(String(error), "error");
  } finally {
    skillSaving = false;
    setButtonBusy(saveBtn, false);
  }
}

async function handleDeleteSkill(name, sourceType) {
  const confirmed = await appConfirm(t("confirmDeleteSkill", { name }), {
    title: t("delete"),
    danger: true,
    confirmText: currentLang === "zh" ? "删除" : "Delete",
  });
  if (!confirmed) return;
  try {
    await invoke("delete_skill", { name, sourceType: sourceType || "command" });
    showToast(t("toastSkillDeleted"), "success");
    await loadSkills();
  } catch (error) {
    showToast(String(error), "error");
  }
}

// ── Skills Discovery Functions ──────────────────────

async function loadSkillRepos() {
  try {
    skillRepos = await invoke("get_skill_repos");
  } catch (error) {
    showToast(String(error), "error");
  }
}

function renderRepoFilter() {
  const select = $("discoverRepoFilter");
  const current = select.value;
  select.innerHTML = `<option value="all">${t("allRepos")}</option>`;
  const sources = new Set();
  discoverSkills.forEach((s) => {
    if (s.source && !sources.has(s.source)) {
      sources.add(s.source);
      const opt = document.createElement("option");
      opt.value = s.source;
      opt.textContent = s.source;
      select.appendChild(opt);
    }
  });
  select.value = current || "all";
}

async function discoverSkillsFromRepos() {
  if (isDiscovering) return;
  isDiscovering = true;
  isShowingGithubSkills = false;
  $("discoverLoading").style.display = "";
  $("discoverGrid").innerHTML = "";
  $("discoverGithubBanner").style.display = "none";
  $("discoverRepoFilter").style.display = "";
  $("discoverStatusFilter").style.display = "";

  try {
    // Load curated catalog (instant, no network)
    discoverSkills = await invoke("get_catalog_skills");
    renderRepoFilter();
    renderDiscoverGrid();
  } catch (error) {
    const errMsg = String(error);
    showToast(errMsg, "error");
    $("discoverGrid").innerHTML = `<div class="discover-empty">${esc(errMsg)}</div>`;
  } finally {
    isDiscovering = false;
    $("discoverLoading").style.display = "none";
  }
}

async function searchGitHubSkills() {
  // 防并发：与 discoverSkillsFromRepos 共用同一守卫
  if (isDiscovering) return;
  const query = $("discoverSearch").value.trim();
  if (!query) {
    // 没有搜索词时回到目录
    backToCatalog();
    return;
  }

  isDiscovering = true;
  isShowingGithubSkills = true;
  $("discoverLoading").style.display = "";
  $("discoverGrid").innerHTML = "";
  $("discoverGithubBanner").style.display = "";
  // GitHub 搜索时隐藏目录筛选器
  $("discoverRepoFilter").style.display = "none";
  $("discoverStatusFilter").style.display = "none";

  try {
    const results = await invoke("search_github_skills", { query });
    discoverSkills = results || [];
    renderDiscoverGrid();
    if (discoverSkills.length === 0) {
      $("discoverGrid").innerHTML = `<div class="discover-empty">${t("discoverNoMatch")}</div>`;
    }
  } catch (error) {
    showToast(String(error), "error");
    $("discoverGrid").innerHTML = `<div class="discover-empty">${esc(String(error))}</div>`;
  } finally {
    isDiscovering = false;
    $("discoverLoading").style.display = "none";
  }
}

function backToCatalog() {
  isShowingGithubSkills = false;
  $("discoverSearch").value = "";
  discoverSearchQuery = "";
  $("discoverGithubBanner").style.display = "none";
  $("discoverRepoFilter").style.display = "";
  $("discoverStatusFilter").style.display = "";
  discoverSkills = [];
  discoverSkillsFromRepos();
}

function renderDiscoverGrid() {
  const grid = $("discoverGrid");
  let filtered = [...discoverSkills];

  if (!isShowingGithubSkills) {
    // 仅在目录模式下应用筛选器
    if (discoverRepoFilter !== "all") {
      filtered = filtered.filter((s) => s.source === discoverRepoFilter);
    }

    if (discoverStatusFilter === "installed") {
      filtered = filtered.filter((s) => s.installed);
    } else if (discoverStatusFilter === "not-installed") {
      filtered = filtered.filter((s) => !s.installed);
    }

    // 本地搜索过滤（目录模式）
    if (discoverSearchQuery.trim()) {
      const q = discoverSearchQuery.toLowerCase();
      filtered = filtered.filter((s) =>
        s.name.toLowerCase().includes(q) ||
        s.description.toLowerCase().includes(q) ||
        (s.descriptionZh || "").toLowerCase().includes(q) ||
        s.category.toLowerCase().includes(q)
      );
    }
  }

  if (filtered.length === 0) {
    grid.innerHTML = `<div class="discover-empty">${discoverSkills.length === 0 ? t("discoverEmpty") : t("discoverNoMatch")}</div>`;
    return;
  }

  grid.innerHTML = filtered.map((skill) => {
    const desc = currentLang === "zh" ? (skill.descriptionZh || skill.description) : skill.description;
    const starsHtml = skill.stars ? `<span class="skill-card-badge">\u2605 ${esc(String(skill.stars))}</span>` : "";
    const repoLink = skill.repoUrl ? `<button class="btn btn-secondary btn-sm" data-action="open-skill-url" data-url="${esc(skill.repoUrl)}">${t("mcpGithubBtn")}</button>` : "";
    return `
    <div class="skill-card">
      <div class="skill-card-header">
        <div class="skill-card-name">${esc(skill.name)}</div>
        ${skill.installed ? `<span class="skill-card-badge installed">${t("installedBadge")}</span>` : ""}
      </div>
      ${desc ? `<div class="skill-card-desc">${esc(desc)}</div>` : ""}
      <div class="skill-card-meta">
        <span class="skill-card-badge repo">${esc(skill.source)}</span>
        ${skill.category ? `<span class="skill-card-badge">${esc(skill.category)}</span>` : ""}
        ${starsHtml}
      </div>
      <div class="skill-card-actions">
        ${skill.installed
          ? `<button class="btn btn-secondary btn-sm" disabled>${t("installedBadge")}</button>`
          : `<button class="btn btn-primary btn-sm" data-action="install-catalog" data-name="${esc(skill.name)}" data-url="${esc(skill.downloadUrl || "")}">${t("installBtn")}</button>`
        }
        ${repoLink}
      </div>
    </div>
    `;
  }).join("");

  // A10: 容器级委托。btn 就是被点中的那个按钮，禁用/改文案的行为与原来一致
  bindDelegatedActions(grid, "DiscoverGrid", async (action, btn) => {
    if (action === "open-skill-url") {
      const url = btn.getAttribute("data-url");
      if (url) await invoke("open_external_target", { target: url });
      return;
    }
    if (action !== "install-catalog") return;
    const name = btn.getAttribute("data-name");
    const url = btn.getAttribute("data-url");
    if (btn.disabled) return; // 防重复提交
    btn.disabled = true;
    btn.textContent = "...";
    try {
      await invoke("install_skill_from_url", { name, url });
      showToast(t("toastSkillInstalled", { name }), "success");
      // Update local state
      const skill = discoverSkills.find((s) => s.name === name);
      if (skill) skill.installed = true;
      renderDiscoverGrid();
      await loadSkills();
    } catch (error) {
      showToast(String(error), "error");
      btn.disabled = false;
      btn.textContent = t("installBtn");
    }
  });
}

// ── Repo Manager ─────────────────────────────────────

function openRepoManager() {
  $("repoManagerOverlay").classList.add("open");
  renderRepoList();
}

function closeRepoManager() {
  $("repoManagerOverlay").classList.remove("open");
}

function renderRepoList() {
  const list = $("repoList");
  if (skillRepos.length === 0) {
    list.innerHTML = `<div class="mgmt-empty">${t("discoverEmpty")}</div>`;
    return;
  }
  list.innerHTML = skillRepos.map((repo) => {
    const match = repo.url.match(/github\.com\/([^/]+\/[^/]+)/);
    const label = match ? match[1] : repo.url;
    return `
    <div class="mgmt-item">
      <div class="mgmt-item-info">
        <div class="mgmt-item-name">${esc(label)}</div>
        <div class="mgmt-item-desc">${esc(repo.branch)} branch</div>
      </div>
      <div class="mgmt-item-actions">
        <button class="btn btn-danger btn-sm" data-action="remove-repo" data-url="${esc(repo.url)}">${t("delete")}</button>
      </div>
    </div>
    `;
  }).join("");

  // A10: 容器级委托
  bindDelegatedActions(list, "RepoList", (action, target) => {
    if (action === "remove-repo") handleRemoveRepo(target.getAttribute("data-url"));
  });
}

async function handleAddRepo() {
  if (repoAdding) return;
  const url = $("repoUrlInput").value.trim();
  if (!url) return;

  repoAdding = true;
  const addBtn = $("addRepoBtn");
  setButtonBusy(addBtn, true, t("toastSaving"));
  try {
    await invoke("add_skill_repo", { url, branch: "main" });
    $("repoUrlInput").value = "";
    showToast(t("toastRepoAdded"), "success");
    await loadSkillRepos();
    renderRepoList();
  } catch (error) {
    showToast(String(error), "error");
  } finally {
    repoAdding = false;
    setButtonBusy(addBtn, false);
  }
}

async function handleRemoveRepo(url) {
  try {
    await invoke("remove_skill_repo", { url });
    showToast(t("toastRepoRemoved"), "success");
    await loadSkillRepos();
    renderRepoList();
  } catch (error) {
    showToast(String(error), "error");
  }
}

// ── Prompts Management ──────────────────────────────

function openPromptsPanel() {
  openDeveloperTools("prompts");
}

function closePromptsPanel() {
  $("promptsOverlay").classList.remove("open");
}

function switchPromptTab(tab) {
  activePromptTab = tab;
  $("promptTabEditor").classList.toggle("active", tab === "editor");
  $("promptTabTemplates").classList.toggle("active", tab === "templates");
  $("promptEditorContent").style.display = tab === "editor" ? "" : "none";
  $("promptTemplatesContent").style.display = tab === "templates" ? "" : "none";
}

async function loadClaudeMd() {
  try {
    const content = await invoke("get_claude_md");
    $("promptContentInput").value = content;
  } catch (error) {
    showToast(String(error), "error");
  }
}

async function loadPromptTemplates() {
  try {
    promptTemplates = await invoke("get_prompt_templates");
    renderPromptTemplates();
    renderSnippetDropdown();
  } catch (error) {
    showToast(String(error), "error");
  }
}

function renderPromptTemplates() {
  const grid = $("promptTemplatesGrid");
  if (!promptTemplates || promptTemplates.length === 0) {
    grid.innerHTML = `<div class="discover-empty">No templates available.</div>`;
    return;
  }

  grid.innerHTML = promptTemplates.map((tpl) => {
    const name = currentLang === "zh" ? (tpl.nameZh || tpl.name) : tpl.name;
    const desc = currentLang === "zh" ? (tpl.descZh || tpl.desc) : tpl.desc;
    const category = tpl.category || "";
    return `
    <div class="prompt-template-card" data-id="${esc(tpl.id)}">
      <div class="prompt-template-name">${esc(name)}</div>
      ${category ? `<span class="skill-card-badge">${esc(category)}</span>` : ""}
      <div class="prompt-template-desc">${esc(desc)}</div>
      <div class="prompt-template-actions">
        <button class="btn btn-secondary btn-sm" data-action="append-template" data-id="${esc(tpl.id)}">${t("appendToPrompt")}</button>
        <button class="btn btn-primary btn-sm" data-action="replace-template" data-id="${esc(tpl.id)}">${t("replacePrompt")}</button>
      </div>
    </div>
    `;
  }).join("");

  // A10: 容器级委托
  bindDelegatedActions(grid, "PromptTemplates", (action, target, e) => {
    e.stopPropagation();
    const id = target.getAttribute("data-id");
    const tpl = promptTemplates.find((t) => t.id === id);
    if (!tpl) return;
    if (action === "append-template") {
      const current = $("promptContentInput").value;
      $("promptContentInput").value = current ? current + "\n\n" + tpl.content : tpl.content;
      switchPromptTab("editor");
      showToast(t("toastSnippetInserted"), "success");
    } else if (action === "replace-template") {
      $("promptContentInput").value = tpl.content;
      switchPromptTab("editor");
      showToast(t("toastTemplateApplied"), "success");
    }
  });
}

function renderSnippetDropdown() {
  const select = $("promptInsertSelect");
  select.innerHTML = `<option value="">${t("insertSnippet")}</option>`;
  if (promptTemplates) {
    promptTemplates.forEach((tpl) => {
      const name = currentLang === "zh" ? (tpl.nameZh || tpl.name) : tpl.name;
      const opt = document.createElement("option");
      opt.value = tpl.id;
      opt.textContent = name;
      select.appendChild(opt);
    });
  }
}

async function handleSavePrompt() {
  const content = $("promptContentInput").value;
  try {
    await invoke("save_claude_md", { content });
    showToast(t("toastPromptSaved"), "success");
  } catch (error) {
    showToast(String(error), "error");
  }
}

// ── MCP Server Management ───────────────────────────

function openMcpPanel() {
  openDeveloperTools("mcp");
}

function closeMcpPanel() {
  $("mcpOverlay").classList.remove("open");
  hideMcpEdit();
}

async function loadMcpServers() {
  try {
    mcpServers = await invoke("get_mcp_servers_list");
    renderMcpServers();
  } catch (error) {
    showToast(String(error), "error");
  }
}

function renderMcpServers() {
  const list = $("mcpList");
  const entries = Object.entries(mcpServers || {});
  if (entries.length === 0) {
    list.innerHTML = `<div class="mgmt-empty">${t("noMcpServers")}</div>`;
    return;
  }
  list.innerHTML = entries.map(([name, config]) => {
    const serverType = config.url ? "SSE" : "stdio";
    const desc = config.command
      ? `${config.command} ${(config.args || []).join(" ")}`
      : config.url || "";
    return `
      <div class="mgmt-item">
        <div class="mgmt-item-info">
          <div class="mgmt-item-name">${esc(name)}</div>
          <div class="mgmt-item-desc">${esc(serverType)}: ${esc(desc.substring(0, 80))}</div>
        </div>
        <div class="mgmt-item-actions">
          <button class="btn btn-secondary btn-sm" data-action="edit-mcp" data-name="${esc(name)}">${t("edit")}</button>
          <button class="btn btn-danger btn-sm" data-action="delete-mcp" data-name="${esc(name)}">${t("delete")}</button>
        </div>
      </div>
    `;
  }).join("");

  // A10: 容器级委托
  bindDelegatedActions(list, "McpList", (action, target) => {
    const name = target.getAttribute("data-name");
    if (action === "edit-mcp") showMcpEdit(name);
    else if (action === "delete-mcp") handleDeleteMcp(name);
  });
}

function showMcpEdit(name) {
  editingMcpName = name || null;
  const config = name ? mcpServers[name] : null;
  $("mcpNameInput").value = name || "";
  $("mcpConfigInput").value = config
    ? JSON.stringify(config, null, 2)
    : '{\n  "command": "",\n  "args": []\n}';
  $("mcpNameInput").disabled = !!name;
  $("mcpList").style.display = "none";
  $("mcpToolbar").style.display = "none";
  $("mcpEdit").style.display = "";
}

function hideMcpEdit() {
  $("mcpList").style.display = "";
  $("mcpToolbar").style.display = "";
  $("mcpEdit").style.display = "none";
  editingMcpName = null;
}

async function handleSaveMcp() {
  if (mcpSaving) return;
  const name = $("mcpNameInput").value.trim();
  const configStr = $("mcpConfigInput").value;
  if (!name) return;

  let config;
  try {
    config = JSON.parse(configStr);
  } catch {
    showToast(t("invalidJson"), "error");
    return;
  }

  mcpSaving = true;
  const saveBtn = $("mcpSaveBtn");
  setButtonBusy(saveBtn, true, t("toastSaving"));
  try {
    await invoke("save_mcp_server", { name, config });
    showToast(t("toastMcpSaved"), "success");
    hideMcpEdit();
    await loadMcpServers();
  } catch (error) {
    showToast(String(error), "error");
  } finally {
    mcpSaving = false;
    setButtonBusy(saveBtn, false);
  }
}

async function handleDeleteMcp(name) {
  const confirmed = await appConfirm(t("confirmDeleteMcp", { name }), {
    title: t("delete"),
    danger: true,
    confirmText: currentLang === "zh" ? "删除" : "Delete",
  });
  if (!confirmed) return;
  try {
    await invoke("delete_mcp_server_entry", { name });
    showToast(t("toastMcpDeleted"), "success");
    await loadMcpServers();
  } catch (error) {
    showToast(String(error), "error");
  }
}

// ── MCP Presets ──────────────────────────────────────

let mcpPresets = [];
let mcpGitHubResults = [];
let mcpSearching = false;
let activeMcpTab = "installed";

function switchMcpTab(tab) {
  activeMcpTab = tab;
  $("mcpTabInstalled").classList.toggle("active", tab === "installed");
  $("mcpTabPresets").classList.toggle("active", tab === "presets");
  $("mcpInstalledContent").style.display = tab === "installed" ? "" : "none";
  $("mcpPresetsContent").style.display = tab === "presets" ? "" : "none";
  if (tab === "presets" && mcpPresets.length === 0) {
    loadMcpPresets();
  }
}

async function loadMcpPresets() {
  try {
    mcpPresets = await invoke("get_mcp_presets");
    mcpGitHubResults = [];
    renderMcpPresets();
  } catch (error) {
    showToast(String(error), "error");
  }
}

async function searchGitHubMcp() {
  // 防并发：防止连点搜索使后完成的旧请求覆盖结果
  if (mcpSearching) return;
  const query = $("mcpPresetSearch").value.trim();
  if (!query) {
    mcpGitHubResults = [];
    renderMcpPresets();
    return;
  }

  mcpSearching = true;
  $("mcpPresetLoading").style.display = "";
  $("mcpPresetsGrid").innerHTML = "";

  try {
    const results = await invoke("search_github_mcp", { query });
    mcpGitHubResults = results || [];
    renderMcpPresets();
  } catch (error) {
    showToast(String(error), "error");
    $("mcpPresetsGrid").innerHTML = `<div class="discover-empty">${esc(String(error))}</div>`;
  } finally {
    mcpSearching = false;
    $("mcpPresetLoading").style.display = "none";
  }
}

function renderMcpPresets() {
  const grid = $("mcpPresetsGrid");
  const installedNames = Object.keys(mcpServers || {});
  const showGitHub = mcpGitHubResults.length > 0;
  const items = showGitHub ? mcpGitHubResults : mcpPresets;

  if (items.length === 0) {
    grid.innerHTML = `<div class="discover-empty">${showGitHub ? t("discoverNoMatch") : t("mcpNoPresets")}</div>`;
    return;
  }

  grid.innerHTML = items.map((preset) => {
    const name = currentLang === "zh" ? (preset.nameZh || preset.name) : preset.name;
    const desc = currentLang === "zh" ? (preset.descZh || preset.desc) : preset.desc;
    const isInstalled = installedNames.includes(preset.id);
    const needsEnv = preset.config && preset.config.env && Object.values(preset.config.env).some((v) => typeof v === "string" && v.startsWith("<"));
    const stars = preset.stars ? `<span class="skill-card-badge">\u2605 ${esc(String(preset.stars))}</span>` : "";
    const source = preset.source ? `<span class="skill-card-badge repo">${esc(preset.source)}</span>` : "";

    return `
    <div class="prompt-template-card">
      <div class="prompt-template-name">${esc(name)}</div>
      <div class="prompt-template-desc">${esc(desc)}</div>
      <div class="skill-card-meta">
        ${stars}${source}
      </div>
      ${needsEnv ? `<div class="prompt-template-desc" style="color:var(--warning-text);">${t("mcpRequiresApiKey")}</div>` : ""}
      <div class="prompt-template-actions">
        ${isInstalled
          ? `<button class="btn btn-secondary btn-sm" disabled>${t("mcpInstalled")}</button>`
          : `<button class="btn btn-primary btn-sm" data-action="install-mcp-preset" data-id="${esc(preset.id)}">${t("mcpInstallBtn")}</button>`
        }
        ${preset.url ? `<button class="btn btn-secondary btn-sm" data-action="open-mcp-url" data-url="${esc(preset.url)}">${t("mcpGithubBtn")}</button>` : ""}
      </div>
    </div>
    `;
  }).join("");

  // A10: 容器级委托
  bindDelegatedActions(grid, "McpPresets", async (action, btn) => {
    if (action === "open-mcp-url") {
      const url = btn.getAttribute("data-url");
      if (url) await invoke("open_external_target", { target: url });
      return;
    }
    if (action !== "install-mcp-preset") return;
    const id = btn.getAttribute("data-id");
    const allItems = [...mcpPresets, ...mcpGitHubResults];
    const preset = allItems.find((p) => p.id === id);
    if (!preset) return;
    if (btn.disabled) return; // 防重复提交

    btn.disabled = true;
    btn.textContent = "...";
    try {
      await invoke("save_mcp_server", { name: preset.id, config: preset.config });
      showToast(`${preset.name} ${t("mcpInstalled").toLowerCase()}`, "success");
      await loadMcpServers();
      renderMcpPresets();
    } catch (error) {
      showToast(String(error), "error");
      btn.disabled = false;
      btn.textContent = t("mcpInstallBtn");
    }
  });
}

function showToast(message, type = "success") {
  try {
    const container = $("toastContainer");
    if (container) {
      const toast = document.createElement("div");
      toast.className = `toast ${type}`;
      toast.textContent = String(message);
      container.appendChild(toast);
      setTimeout(() => {
        toast.style.opacity = "0";
        setTimeout(() => toast.remove(), 300);
      }, 3200);
    }
  } catch (e) {
    console.error("showToast failed", e, message);
  }
}

// ── 统一 Dialog（替代 alert/confirm/prompt）────────────────
let appDialogResolver = null;

function closeAppDialog(result) {
  const overlay = $("appDialogOverlay");
  if (overlay) overlay.classList.remove("open");
  document.body.classList.remove("modal-open");
  const resolver = appDialogResolver;
  appDialogResolver = null;
  if (resolver) resolver(result);
}

function openAppDialog(options = {}) {
  return new Promise((resolve) => {
    // 若已有对话框在等待，先 resolve null 再接管，避免之前的 await 永久挂起
    if (appDialogResolver) {
      appDialogResolver(options.mode === "prompt" ? null : false);
      appDialogResolver = null;
    }
    appDialogResolver = resolve;
    const overlay = $("appDialogOverlay");
    const dialog = overlay?.querySelector(".app-dialog");
    if (!overlay || !dialog) {
      appDialogResolver = null;
      resolve(options.mode === "prompt" ? null : false);
      return;
    }

    dialog.classList.toggle("danger", !!options.danger);
    $("appDialogKicker").textContent = options.kicker || "VarSwitch";
    $("appDialogTitle").textContent = options.title || (currentLang === "zh" ? "确认操作" : "Confirm");
    $("appDialogMessage").textContent = options.message || "";
    $("appDialogCancel").textContent = options.cancelText || t("cancel");
    $("appDialogConfirm").textContent = options.confirmText || (currentLang === "zh" ? "确认" : "Confirm");

    const inputGroup = $("appDialogInputGroup");
    const input = $("appDialogInput");
    if (options.mode === "prompt") {
      inputGroup.hidden = false;
      $("appDialogInputLabel").textContent = options.inputLabel || (currentLang === "zh" ? "配置名称" : "Name");
      $("appDialogInputHint").textContent = options.inputHint || "";
      input.value = options.defaultValue || "";
      input.placeholder = options.placeholder || "";
    } else {
      inputGroup.hidden = true;
      input.value = "";
    }

    overlay.classList.add("open");
    document.body.classList.add("modal-open");
    setTimeout(() => {
      if (options.mode === "prompt") input.focus();
      else $("appDialogConfirm").focus();
    }, 30);
  });
}

async function appConfirm(message, options = {}) {
  return openAppDialog({
    mode: "confirm",
    title: options.title || (currentLang === "zh" ? "确认操作" : "Confirm"),
    message,
    confirmText: options.confirmText || (currentLang === "zh" ? "确认" : "Confirm"),
    cancelText: options.cancelText || t("cancel"),
    danger: !!options.danger,
    kicker: options.kicker || "VarSwitch",
  });
}

async function appPrompt(message, defaultValue = "", options = {}) {
  const result = await openAppDialog({
    mode: "prompt",
    title: options.title || (currentLang === "zh" ? "请输入" : "Input"),
    message,
    defaultValue,
    inputLabel: options.inputLabel,
    inputHint: options.inputHint,
    confirmText: options.confirmText || (currentLang === "zh" ? "确定" : "OK"),
    cancelText: options.cancelText || t("cancel"),
    kicker: options.kicker || "VarSwitch",
  });
  return result;
}

function bindAppDialogOnce() {
  if (window.__varswitchDialogBound) return;
  window.__varswitchDialogBound = true;
  $("appDialogClose")?.addEventListener("click", () => closeAppDialog(null));
  $("appDialogCancel")?.addEventListener("click", () => closeAppDialog(null));
  $("appDialogConfirm")?.addEventListener("click", () => {
    const inputGroup = $("appDialogInputGroup");
    if (inputGroup && !inputGroup.hidden) {
      closeAppDialog($("appDialogInput").value);
    } else {
      closeAppDialog(true);
    }
  });
  bindOverlayDismiss("appDialogOverlay", () => closeAppDialog(null));
  $("appDialogInput")?.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      event.preventDefault();
      $("appDialogConfirm")?.click();
    }
  });
}

function setBadge(el, text, tone = "") {
  if (!el) return;
  el.textContent = text;
  el.className = `status-badge-console${tone ? ` ${tone}` : ""}`;
}

function setNavDot(id, tone = "") {
  const el = $(id);
  if (!el) return;
  el.className = `nav-state-dot${tone ? ` ${tone}` : ""}`;
}

function mountToolboxPages() {
  const tabs = $("toolboxPageTabs");
  const session = $("toolboxSessionContent");
  const remote = $("toolboxRemoteContent");
  const toolboxHost = $("toolboxPageHost");
  if (tabs && toolboxHost && tabs.parentElement !== toolboxHost) toolboxHost.appendChild(tabs);
  if (session && toolboxHost && session.parentElement !== toolboxHost) toolboxHost.appendChild(session);
  if (remote && toolboxHost && remote.parentElement !== toolboxHost) toolboxHost.appendChild(remote);

  // 设置页：把设置面板 body 挂入页面
  const settingsBody = document.querySelector("#settingsOverlay .settings-body");
  const settingsHost = $("settingsPageHost");
  if (settingsBody && settingsHost && settingsBody.parentElement !== settingsHost) {
    settingsHost.appendChild(settingsBody);
  }
}

function getSelectedUniversalProviderPreset() {
  return UNIVERSAL_PROVIDER_PRESETS.find((preset) => preset.id === selectedUniversalProviderPreset)
    || UNIVERSAL_PROVIDER_PRESETS[0];
}

function getUniversalProviderApps() {
  return Object.fromEntries(
    [...document.querySelectorAll("[data-universal-app]")].map((input) => [
      input.getAttribute("data-universal-app"),
      input.checked,
    ])
  );
}

function renderUniversalProviderForm() {
  const preset = getSelectedUniversalProviderPreset();
  document.querySelectorAll("[data-universal-preset]").forEach((button) => {
    const active = button.getAttribute("data-universal-preset") === preset.id;
    button.classList.toggle("active", active);
    button.setAttribute("aria-checked", String(active));
  });
  setText("universalProviderPresetHint", preset.description);

  const apps = getUniversalProviderApps();
  document.querySelectorAll("[data-universal-app-card]").forEach((card) => {
    card.hidden = !apps[card.getAttribute("data-universal-app-card")];
  });
  updateUniversalUrlPreviews();
  const labels = [apps.claude && "Claude", apps.codex && "Codex", apps.grok && "Grok", apps.gemini && "Gemini"].filter(Boolean);
  setText("universalProviderFooterHint", labels.length
    ? `将同步到 ${labels.join("、")}`
    : "请至少启用一个应用");
}

// 同一个网关地址在各应用的实际写入形式不同（OpenAI 兼容协议自动补 /v1），实时展示避免歧义
function updateUniversalUrlPreviews() {
  const raw = $("universalProviderBaseUrl")?.value.trim() || "";
  const claudeOpenAi = $("universalClaudeApiFormat")?.value === "openai_chat";
  document.querySelectorAll("[data-universal-url-preview]").forEach((hint) => {
    const app = hint.getAttribute("data-universal-url-preview");
    // Claude 选 OpenAI 格式时按 OpenAI 惯例处理地址（补 /v1），并提示走本地代理
    const usesOpenAiRules = app === "codex" || app === "grok" || (app === "claude" && claudeOpenAi);
    if (!raw) {
      hint.textContent = usesOpenAiRules ? "保存时地址自动补全 /v1 后缀" : "";
      hint.hidden = !hint.textContent;
      return;
    }
    const resolveApp = usesOpenAiRules ? (app === "grok" ? "grok" : "codex") : app;
    const resolved = helpers.resolveUniversalAppBaseUrl
      ? helpers.resolveUniversalAppBaseUrl(resolveApp, raw)
      : raw;
    hint.textContent = app === "claude" && claudeOpenAi
      ? `上游地址：${resolved}（经 127.0.0.1:25789 本地代理转换）`
      : `写入地址：${resolved}`;
    hint.hidden = false;
  });
}

function fillUniversalModelOptions(models) {
  const datalist = $("universalModelOptions");
  if (!datalist) return;
  const normalized = helpers.normalizeFetchedModels
    ? helpers.normalizeFetchedModels(models)
    : [];
  datalist.innerHTML = normalized.map((model) => `<option value="${esc(model)}"></option>`).join("");
}

// 统一供应商的连通性验证：走 OpenAI 兼容 /models 端点（NewAPI 等网关通用），
// 成功后把模型列表填入 datalist，四个应用的模型输入框都能自动补全。
async function handleUniversalEndpointTest() {
  const button = $("universalEndpointTestBtn");
  const container = $("universalEndpointResults");
  const baseUrl = $("universalProviderBaseUrl").value.trim();
  const apiKey = $("universalProviderApiKey").value.trim();
  if (!baseUrl || !apiKey) {
    showToast("请先填写 API 地址和 API Key", "warning");
    return;
  }

  const previousText = button.textContent;
  button.disabled = true;
  button.textContent = t("endpointTesting");
  const startedAt = performance.now();
  try {
    const models = await invoke("fetch_available_models", { baseUrl, apiKey, timeoutSecs: 12, protocol: "codex" });
    const elapsed = Math.round(performance.now() - startedAt);
    const count = (models || []).length;
    fillUniversalModelOptions(models || []);
    if (container) {
      container.innerHTML = `<div class="endpoint-row"><span class="endpoint-url" title="${esc(baseUrl)}">${esc(baseUrl)}</span><span class="endpoint-meta fast">${t("verifyOkLabel")} · ${count} ${t("verifyModelsSuffix")} · ${elapsed}ms</span></div>`;
      container.classList.add("open");
    }
    showToast(t("verifyOkToast"), "success");
  } catch (error) {
    const message = describeVerifyError(String(error));
    if (container) {
      container.innerHTML = `<div class="endpoint-row"><span class="endpoint-url" title="${esc(baseUrl)}">${esc(baseUrl)}</span><span class="endpoint-meta failed">${esc(message)}</span></div>`;
      container.classList.add("open");
    }
    showToast(message, "error");
  } finally {
    button.disabled = false;
    button.textContent = previousText || t("endpointTest");
  }
}

function applyUniversalProviderPreset(presetId) {
  const preset = UNIVERSAL_PROVIDER_PRESETS.find((item) => item.id === presetId);
  if (!preset) return;
  selectedUniversalProviderPreset = preset.id;
  $("universalProviderName").value = preset.name;
  $("universalClaudeModel").value = preset.models.claude;
  $("universalCodexModel").value = preset.models.codex;
  $("universalGrokModel").value = preset.models.grok;
  $("universalGeminiModel").value = preset.models.gemini;
  renderUniversalProviderForm();
}

function resetUniversalProviderForm(protocol = null) {
  selectedUniversalProviderPreset = "newapi";
  $("universalProviderForm")?.reset();
  document.querySelectorAll("[data-universal-app]").forEach((input) => {
    input.checked = protocol ? input.getAttribute("data-universal-app") === protocol : true;
  });
  $("universalProviderBaseUrl").value = "";
  $("universalProviderApiKey").value = "";
  $("universalProviderApiKey").type = "password";
  setText("universalProviderApiKeyToggle", "显示");
  const claudeApiFormat = $("universalClaudeApiFormat");
  if (claudeApiFormat) claudeApiFormat.value = "anthropic";
  const codexWireApi = $("universalCodexWireApi");
  if (codexWireApi) codexWireApi.value = "responses";
  const grokApiBackend = $("universalGrokApiBackend");
  if (grokApiBackend) grokApiBackend.value = "chat_completions";
  const endpointResults = $("universalEndpointResults");
  if (endpointResults) {
    endpointResults.innerHTML = "";
    endpointResults.classList.remove("open");
  }
  const modelOptions = $("universalModelOptions");
  if (modelOptions) modelOptions.innerHTML = "";
  applyUniversalProviderPreset("newapi");
}

function openProviderOnboarding(protocol = null) {
  switchConsolePage("add-provider");
  resetUniversalProviderForm(["claude", "codex", "grok", "gemini"].includes(protocol) ? protocol : null);
}

async function rollbackUniversalProviderProfiles(createdProfiles) {
  const deleteCommands = {
    claude: "delete_profile",
    codex: "delete_codex_profile",
    grok: "delete_grok_profile",
    gemini: "delete_gemini_profile",
  };
  const failures = [];
  for (const profile of [...createdProfiles].reverse()) {
    try {
      await invoke(deleteCommands[profile.app], { id: profile.id });
    } catch (error) {
      failures.push(`${profile.app}: ${String(error)}`);
    }
  }
  return failures;
}

async function handleUniversalProviderSubmit(event) {
  event.preventDefault();
  if (universalProviderSaving) return;

  const apps = getUniversalProviderApps();
  const enabledApps = ["claude", "codex", "grok", "gemini"].filter((app) => apps[app]);
  if (enabledApps.length === 0) {
    showToast("请至少启用一个应用", "warning");
    return;
  }

  const name = $("universalProviderName").value.trim();
  const baseUrl = $("universalProviderBaseUrl").value.trim();
  const apiKey = $("universalProviderApiKey").value.trim();
  if (!name || !baseUrl || !apiKey) {
    showToast("请填写供应商名称、API 地址和 API Key", "warning");
    return;
  }
  const claudeApiFormat = $("universalClaudeApiFormat")?.value || "anthropic";
  if (apps.claude && claudeApiFormat === "openai_chat" && !$("universalClaudeModel").value.trim()) {
    showToast(t("claudeApiFormatNeedsModel"), "warning");
    return;
  }

  const createdProfiles = [];
  const submitButton = $("universalProviderSubmitBtn");
  universalProviderSaving = true;
  setButtonBusy(submitButton, true, "正在同步...");

  const appBaseUrl = (app) => (helpers.resolveUniversalAppBaseUrl
    ? helpers.resolveUniversalAppBaseUrl(app, baseUrl)
    : baseUrl);

  try {
    if (apps.claude) {
      const created = await invoke("add_profile", {
        name,
        apiKey,
        // OpenAI 格式的上游地址遵循 OpenAI 惯例（复用 codex 的 /v1 规则）
        baseUrl: claudeApiFormat === "openai_chat" ? appBaseUrl("codex") : appBaseUrl("claude"),
        modelId: $("universalClaudeModel").value.trim() || null,
        apiFormat: claudeApiFormat,
      });
      createdProfiles.push({ app: "claude", id: created.id });
    }
    if (apps.codex) {
      const created = await invoke("add_codex_profile", {
        name,
        apiKey,
        baseUrl: appBaseUrl("codex"),
        authMode: "auth_json",
        wireApi: $("universalCodexWireApi")?.value || "responses",
        model: $("universalCodexModel").value.trim() || null,
        providerName: getSelectedUniversalProviderPreset().providerName,
        imageApiKey: null,
        imageBaseUrl: null,
      });
      createdProfiles.push({ app: "codex", id: created.id });
    }
    if (apps.grok) {
      const created = await invoke("add_grok_profile", {
        name,
        apiKey,
        baseUrl: appBaseUrl("grok"),
        model: $("universalGrokModel").value.trim() || null,
        apiBackend: $("universalGrokApiBackend")?.value || "chat_completions",
      });
      createdProfiles.push({ app: "grok", id: created.id });
    }
    if (apps.gemini) {
      const created = await invoke("add_gemini_profile", {
        name,
        apiKey,
        baseUrl: appBaseUrl("gemini"),
        model: $("universalGeminiModel").value.trim() || null,
      });
      createdProfiles.push({ app: "gemini", id: created.id });
    }

    await Promise.allSettled([loadProfiles(), loadCodexProfiles(), loadGrokProfiles(), loadGeminiProfiles()]);
    renderProviderNavigation();
    showToast(`已将 ${name} 同步到 ${enabledApps.length} 个应用`, "success");
    switchConsolePage(enabledApps[0]);
  } catch (error) {
    const rollbackFailures = await rollbackUniversalProviderProfiles(createdProfiles);
    await Promise.allSettled([loadProfiles(), loadCodexProfiles(), loadGrokProfiles(), loadGeminiProfiles()]);
    const suffix = rollbackFailures.length ? `；回滚失败：${rollbackFailures.join("；")}` : "，已回滚本次写入";
    showToast(`同步失败：${String(error)}${suffix}`, "error");
  } finally {
    universalProviderSaving = false;
    setButtonBusy(submitButton, false);
  }
}

function mountDeveloperToolsPage() {
  const panels = [
    ["skillsOverlay", "developerToolSkillsContent"],
    ["promptsOverlay", "developerToolPromptsContent"],
    ["mcpOverlay", "developerToolMcpContent"],
  ];
  panels.forEach(([overlayId, hostId]) => {
    const overlay = $(overlayId);
    const host = $(hostId);
    const panel = overlay?.querySelector(".mgmt-panel");
    if (!panel || !host || panel.parentElement === host) return;
    overlay.classList.remove("open");
    panel.classList.add("developer-tool-panel");
    host.appendChild(panel);
  });
}

function switchDeveloperTool(tool) {
  const nextTool = ["skills", "prompts", "mcp"].includes(tool) ? tool : "skills";
  activeDeveloperTool = nextTool;
  document.querySelectorAll("[data-developer-tool]").forEach((button) => {
    const active = button.getAttribute("data-developer-tool") === nextTool;
    button.classList.toggle("active", active);
    button.setAttribute("aria-selected", String(active));
  });
  const surfaces = {
    skills: $("developerToolSkillsContent"),
    prompts: $("developerToolPromptsContent"),
    mcp: $("developerToolMcpContent"),
  };
  Object.entries(surfaces).forEach(([name, surface]) => {
    if (surface) surface.hidden = name !== nextTool;
  });

  if (nextTool === "skills") {
    hideSkillsEdit();
    switchSkillsTab("installed");
    loadSkills();
  } else if (nextTool === "prompts") {
    switchPromptTab("editor");
    loadClaudeMd();
    loadPromptTemplates();
  } else {
    hideMcpEdit();
    switchMcpTab("installed");
    loadMcpServers();
  }
}

function openDeveloperTools(tool = "skills") {
  mountDeveloperToolsPage();
  switchConsolePage("developer-tools");
  switchDeveloperTool(tool);
}

function renderProviderNavigation() {
  const availability = {
    claude: profiles.length > 0,
    codex: codexProfiles.length > 0,
    grok: grokProfiles.length > 0,
    gemini: true,
  };
  document.querySelectorAll("[data-provider-nav]").forEach((item) => {
    const available = availability[item.getAttribute("data-provider-nav")] === true;
    item.hidden = !available;
  });
  if (availability[activeConsolePage] === false) {
    switchConsolePage("add-provider");
  }
}

// ===========================================================================
// 用量监控面板（数据口径迁移自 cc-switch）
// ===========================================================================
let usageRange = "7d";
let usageAppFilter = "all";
let usageModelFilter = "all";
let usageLoadInFlight = false;
let usageLoadedOnce = false;
let usageReloadQueued = false;
// 最近一次请求实际使用的起止时间（unix 秒），用于在面板上标注统计区间
let usageResolvedRange = null;
// 请求序号：只渲染最新一次请求的结果，防止慢响应覆盖新筛选
let usageRequestSeq = 0;

const USAGE_APP_LABELS = {
  claude: "Claude Code",
  codex: "Codex",
  gemini: "Gemini CLI",
  grok: "Grok CLI",
};

const USAGE_APP_ICONS = {
  claude: "anthropic-color.svg",
  codex: "OpenAI-black-monoblossom.svg",
  gemini: "gemini-color.svg",
  grok: "grok-color.svg",
};

// Codex / Gemini / Grok 由服务端自动管理缓存，没有显式缓存写入
const USAGE_AUTO_CACHE_APPS = new Set(["codex", "gemini", "grok"]);

function escapeUsageHtml(value) {
  return String(value ?? "")
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

function usageAppLabel(app) {
  return USAGE_APP_LABELS[app] || app;
}

function formatUsageCost(cost) {
  const value = Number(cost) || 0;
  if (value === 0) return "$0.00";
  if (value < 0.01) return `$${value.toFixed(4)}`;
  if (value < 1) return `$${value.toFixed(3)}`;
  return `$${value.toFixed(2)}`;
}

function formatUsageTokens(tokens) {
  const value = Number(tokens) || 0;
  if (value >= 1_000_000_000) return `${(value / 1_000_000_000).toFixed(2)}B`;
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(2)}M`;
  if (value >= 1_000) return `${(value / 1_000).toFixed(1)}K`;
  return String(value);
}

// 完整千分位数字（应用大卡的主数字）
function formatUsageTokensExact(tokens) {
  return (Number(tokens) || 0).toLocaleString("en-US");
}

// 人性化换算：中文用 亿/万，英文用 B/M/K
function formatUsageTokensHuman(tokens) {
  const value = Number(tokens) || 0;
  if (currentLang !== "zh") return formatUsageTokens(value);
  if (value >= 100_000_000) return `${(value / 100_000_000).toFixed(2)} 亿`;
  if (value >= 10_000) return `${(value / 10_000).toFixed(1)} 万`;
  return String(value);
}

function formatUsageTime(unixSeconds) {
  if (!unixSeconds) return "--";
  const date = new Date(unixSeconds * 1000);
  const pad = (n) => String(n).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

// 相对时间（最近记录表用），精确时间放 title
function formatUsageRelTime(unixSeconds) {
  if (!unixSeconds) return "--";
  const zh = currentLang === "zh";
  const diff = Math.floor(Date.now() / 1000) - unixSeconds;
  if (diff < 60) return zh ? "刚刚" : "just now";
  if (diff < 3600) {
    const m = Math.floor(diff / 60);
    return zh ? `${m} 分钟前` : `${m}m ago`;
  }
  if (diff < 86400) {
    const h = Math.floor(diff / 3600);
    return zh ? `${h} 小时前` : `${h}h ago`;
  }
  if (diff < 86400 * 7) {
    const d = Math.floor(diff / 86400);
    return zh ? `${d} 天前` : `${d}d ago`;
  }
  return formatUsageTime(unixSeconds);
}

// 当前范围按钮的显示名
function usageRangeLabelText() {
  const zh = currentLang === "zh";
  const labels = {
    today: zh ? "今天" : "Today",
    "1d": zh ? "24 小时" : "24 hours",
    "7d": zh ? "近 7 天" : "Last 7 days",
    "14d": zh ? "近 14 天" : "Last 14 days",
    "30d": zh ? "近 30 天" : "Last 30 days",
    all: zh ? "全部" : "All time",
    custom: zh ? "自定义" : "Custom",
  };
  return labels[usageRange] || usageRange;
}

// 把当前统计区间写到过滤器下方的标注条（请求发起时立即调用，
// 避免加载期间用户把旧数据误读为新范围的结果）
function renderUsageRangeNote() {
  const note = $("usageRangeNote");
  if (!note) return;
  const zh = currentLang === "zh";
  const label = usageRangeLabelText();
  if (!usageResolvedRange) {
    note.innerHTML = "";
    return;
  }
  if (!usageResolvedRange.startTs && !usageResolvedRange.endTs) {
    note.innerHTML = `<strong>${escapeUsageHtml(label)}</strong><span>${zh ? "全部历史记录" : "All history"}</span>`;
    return;
  }
  const startText = usageResolvedRange.startTs
    ? formatUsageTime(usageResolvedRange.startTs)
    : zh ? "最早" : "earliest";
  const endText = usageResolvedRange.endTs
    ? formatUsageTime(usageResolvedRange.endTs)
    : zh ? "现在" : "now";
  note.innerHTML = `<strong>${escapeUsageHtml(label)}</strong><span>${escapeUsageHtml(startText)} → ${escapeUsageHtml(endText)}</span>`;
}

// 加载状态：首次加载显示骨架屏；刷新时旧内容半透明置灰 + 顶部状态芯片，
// 明确告知“正在统计，当前显示的是旧结果”
function setUsageLoadingState(loading) {
  const skeleton = $("usageSkeleton");
  const chip = $("usageLoadingChip");
  const body = $("usageBody");
  const empty = $("usageEmpty");
  if (loading) {
    if (!usageLoadedOnce) {
      if (skeleton) skeleton.hidden = false;
      if (body) body.hidden = true;
      if (empty) empty.hidden = true;
    } else {
      if (chip) chip.hidden = false;
      if (body) body.classList.add("usage-stale");
      if (empty) empty.classList.add("usage-stale");
    }
  } else {
    if (skeleton) skeleton.hidden = true;
    if (chip) chip.hidden = true;
    if (body) body.classList.remove("usage-stale");
    if (empty) empty.classList.remove("usage-stale");
  }
}

// 时间范围语义见 app-helpers.js resolveUsageRange：
// today = 自然日（本地 00:00 → 24:00）；1d = 滚动 24 小时（now-24h → now）；
// 7d/14d/30d = (N-1) 天前本地零点 → 现在；custom = 用户选定起止时间
function resolveUsageRange(range) {
  if (typeof helpers.resolveUsageRange !== "function") {
    return { startTs: null, endTs: null };
  }
  const parseInput = (id) => {
    const el = $(id);
    if (!el || !el.value) return null;
    const ts = new Date(el.value).getTime();
    return Number.isFinite(ts) ? Math.floor(ts / 1000) : null;
  };
  return helpers.resolveUsageRange(range, {
    nowMs: Date.now(),
    customStartTs: parseInput("usageCustomStart"),
    customEndTs: parseInput("usageCustomEnd"),
    customLiveEnd: $("usageCustomLiveEnd")?.checked !== false,
  });
}

async function loadUsageDashboard() {
  const seq = ++usageRequestSeq;
  // 发起请求前先按最新筛选更新区间标注与加载态，
  // 用户点击范围按钮后立即获得视觉反馈，不会把旧数据误读为新结果
  const range = resolveUsageRange(usageRange);
  usageResolvedRange = range;
  renderUsageRangeNote();
  setUsageLoadingState(true);
  const errorBox = $("usageError");
  if (errorBox) errorBox.hidden = true;

  // 加载中再次触发（切换筛选）时排队，结束后按最新筛选重载一次
  if (usageLoadInFlight) {
    usageReloadQueued = true;
    return;
  }
  usageLoadInFlight = true;
  try {
    const dashboard = await invoke("get_usage_dashboard", {
      startTs: range.startTs,
      endTs: range.endTs,
      app: usageAppFilter === "all" ? null : usageAppFilter,
      model: usageModelFilter === "all" ? null : usageModelFilter,
      // 后端期望 JS getTimezoneOffset() 原值（UTC−本地，UTC+8 为 -480）
      tzOffsetMinutes: new Date().getTimezoneOffset(),
    });
    // 慢响应回来时若用户已切到别的筛选，丢弃本次结果，等排队的重载
    if (seq === usageRequestSeq) {
      usageLoadedOnce = true;
      renderUsageDashboard(dashboard);
    }
  } catch (error) {
    if (seq === usageRequestSeq) {
      const body = $("usageBody");
      const empty = $("usageEmpty");
      if (body) body.hidden = true;
      if (empty) empty.hidden = true;
      if (errorBox) {
        errorBox.hidden = false;
        errorBox.textContent = (currentLang === "zh" ? "用量统计加载失败：" : "Failed to load usage stats: ") + String(error);
      }
    }
  } finally {
    usageLoadInFlight = false;
    if (usageReloadQueued) {
      usageReloadQueued = false;
      loadUsageDashboard();
    } else {
      setUsageLoadingState(false);
    }
  }
}

// 模型下拉与应用/时间筛选级联：选项来自当前应用+时间范围下实际出现过的模型
function updateUsageModelOptions(models) {
  const select = $("usageModelFilter");
  if (!select) return false;
  const zh = currentLang === "zh";
  const list = Array.isArray(models) ? models : [];
  const selectionInvalid = usageModelFilter !== "all" && !list.includes(usageModelFilter);
  if (selectionInvalid) usageModelFilter = "all";
  select.innerHTML = [`<option value="all">${zh ? "全部模型" : "All models"}</option>`]
    .concat(list.map((m) => `<option value="${escapeUsageHtml(m)}">${escapeUsageHtml(m)}</option>`))
    .join("");
  select.value = usageModelFilter;
  return selectionInvalid;
}

function renderUsageDashboard(dashboard) {
  const body = $("usageBody");
  const empty = $("usageEmpty");
  const zh = currentLang === "zh";
  // 选中的模型在新筛选下已不存在时重置为“全部模型”并重载
  if (updateUsageModelOptions(dashboard?.availableModels)) {
    loadUsageDashboard();
    return;
  }
  if (!dashboard || !dashboard.summary || Number(dashboard.summary.totalRequests) === 0) {
    if (body) body.hidden = true;
    if (empty) empty.hidden = false;
    // 空态区分“该范围没有数据”与“从未有过数据”
    const scanned = Number(dashboard?.filesScanned) || 0;
    const rangeScoped = usageRange !== "all" && scanned > 0;
    setText(
      "usageEmptyTitle",
      rangeScoped
        ? (zh ? `「${usageRangeLabelText()}」内没有用量` : `No usage in "${usageRangeLabelText()}"`)
        : (zh ? "暂无用量数据" : "No usage data yet")
    );
    setText(
      "usageEmptyText",
      rangeScoped
        ? (zh
            ? "这个时间范围内没有找到任何请求记录，试试切换到更长的时间范围。"
            : "No requests found in this time range. Try a longer range.")
        : (zh
            ? "使用 Claude Code / Codex / Gemini CLI / Grok CLI 后，这里会自动统计本地会话日志。"
            : "Once you use Claude Code / Codex / Gemini CLI / Grok CLI, local session logs will be aggregated here automatically.")
    );
    return;
  }
  if (empty) empty.hidden = true;
  if (body) body.hidden = false;

  renderUsageHero(dashboard.summary, dashboard.prevSummary || null, dashboard.byApp || []);
  renderUsageTrend(dashboard);
  renderUsageApps(dashboard.byApp || [], dashboard.summary);
  renderUsageModels(dashboard.models || []);
  renderUsageRecent(dashboard.recent || []);
  renderUsageMeta(dashboard);
}

function renderUsageMeta(dashboard) {
  const meta = $("usageMetaLine");
  if (!meta) return;
  const zh = currentLang === "zh";
  const parts = [];
  parts.push(
    zh
      ? `已扫描 ${dashboard.filesScanned} 个会话日志文件`
      : `Scanned ${dashboard.filesScanned} session log files`
  );
  if (dashboard.deferredFiles > 0) {
    parts.push(
      zh
        ? `${dashboard.deferredFiles} 个 Codex fork 文件待父线程就绪`
        : `${dashboard.deferredFiles} Codex fork files awaiting parent thread`
    );
  }
  const errors = dashboard.parseErrors || [];
  if (errors.length > 0) {
    parts.push(zh ? `${errors.length} 个文件解析失败` : `${errors.length} files failed to parse`);
  }
  if (dashboard.generatedAt) {
    parts.push((zh ? "更新于 " : "Updated at ") + formatUsageTime(dashboard.generatedAt));
  }
  meta.textContent = parts.join(" · ");
  meta.title = errors.length > 0 ? errors.join("\n") : "";
}

const USAGE_HERO_ICONS = {
  cost: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"><circle cx="8" cy="8" r="6.4"/><path d="M8 4.6v6.8M10 5.9c-.5-.6-1.2-.9-2-.9-1.2 0-2.1.7-2.1 1.6 0 2.2 4.2.9 4.2 3 0 .9-.9 1.6-2.1 1.6-.8 0-1.5-.3-2-.9"/></svg>',
  tokens: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"><path d="M8 1.8 14 5v6L8 14.2 2 11V5z"/><path d="M2 5l6 3.2L14 5M8 8.2v6"/></svg>',
  requests: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"><path d="M2.5 5.5h9M9 2.5l3 3-3 3M13.5 10.5h-9M7 13.5l-3-3 3-3"/></svg>',
  cache: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"><path d="M8.8 1.8 3.2 9h3.6l-.9 5.2L11.5 7H7.9z"/></svg>',
};

// 环比 badge：direction = up / down / flat；tone = good / bad / neutral
function usageHeroDelta(current, previous, options) {
  const { lowerIsBetter = false, formatter = (v) => String(v) } = options || {};
  if (previous == null) return "";
  const zh = currentLang === "zh";
  const cur = Number(current) || 0;
  const prev = Number(previous) || 0;
  if (prev === 0 && cur === 0) return "";
  let text;
  let direction;
  if (prev === 0) {
    text = zh ? "新增" : "new";
    direction = "up";
  } else {
    const pct = ((cur - prev) / prev) * 100;
    if (Math.abs(pct) < 0.05) {
      text = zh ? "持平" : "flat";
      direction = "flat";
    } else {
      text = `${pct > 0 ? "+" : ""}${Math.abs(pct) >= 100 ? pct.toFixed(0) : pct.toFixed(1)}%`;
      direction = pct > 0 ? "up" : "down";
    }
  }
  const tone =
    direction === "flat" ? "neutral" : (direction === "up") === lowerIsBetter ? "bad" : "good";
  const title = zh
    ? `上一时段：${formatter(prev)}`
    : `Previous period: ${formatter(prev)}`;
  const arrow = direction === "up" ? "↑" : direction === "down" ? "↓" : "→";
  return `<span class="usage-hero-delta usage-delta-${tone}" title="${escapeUsageHtml(title)}">${arrow} ${escapeUsageHtml(text)}</span>`;
}

function renderUsageHero(summary, prevSummary, byApp) {
  const grid = $("usageHeroGrid");
  if (!grid) return;
  const zh = currentLang === "zh";
  const cacheRate = `${((Number(summary.cacheHitRate) || 0) * 100).toFixed(1)}%`;
  const prev = prevSummary || null;
  // 缓存命中率是当前筛选内全部应用/模型的加权聚合；
  // 跨多个应用时在卡片上注明口径，避免被当成某单一工具的命中率
  const apps = Array.isArray(byApp) ? byApp : [];
  const cacheSub =
    apps.length > 1
      ? (zh
          ? `${apps.length} 个应用加权 · 各应用见下方卡片`
          : `Weighted across ${apps.length} apps · see per-app cards`)
      : `${zh ? "缓存读取" : "Cache read"} ${formatUsageTokens(summary.cacheReadTokens)} · ${zh ? "写入" : "Write"} ${formatUsageTokens(summary.cacheCreationTokens)}`;
  const cacheTitle = zh
    ? "命中率 = 缓存读取 ÷（新输入 + 缓存写入 + 缓存读取）\n为当前筛选范围内所有应用与模型的加权聚合。\n各应用的命中率见「按应用」卡片，各模型的命中率见「模型统计」表。"
    : "Hit rate = cache read ÷ (fresh input + cache write + cache read)\nWeighted aggregate across all apps and models in the current filter.\nSee per-app cards and the model table for individual rates.";
  const cards = [
    {
      label: zh ? "总成本" : "Total Cost",
      value: formatUsageCost(summary.totalCost),
      sub: zh ? "按各模型官方定价估算" : "Estimated with official pricing",
      accent: "cost",
      delta: usageHeroDelta(summary.totalCost, prev?.totalCost, {
        lowerIsBetter: true,
        formatter: formatUsageCost,
      }),
    },
    {
      label: zh ? "总 Tokens" : "Total Tokens",
      value: formatUsageTokens(summary.realTotalTokens),
      sub: `${zh ? "输入" : "In"} ${formatUsageTokens(summary.inputTokens)} · ${zh ? "输出" : "Out"} ${formatUsageTokens(summary.outputTokens)}`,
      accent: "tokens",
      delta: usageHeroDelta(summary.realTotalTokens, prev?.realTotalTokens, {
        formatter: formatUsageTokens,
      }),
    },
    {
      label: zh ? "请求数" : "Requests",
      value: formatUsageTokens(summary.totalRequests),
      sub: zh ? "去重后的 API 消息数" : "Deduplicated API messages",
      accent: "requests",
      delta: usageHeroDelta(summary.totalRequests, prev?.totalRequests, {
        formatter: formatUsageTokens,
      }),
    },
    {
      label: zh ? "缓存命中率" : "Cache Hit Rate",
      value: cacheRate,
      sub: cacheSub,
      accent: "cache",
      delta: "",
      title: cacheTitle,
    },
  ];
  grid.innerHTML = cards
    .map(
      (card) => `
      <div class="usage-hero-card console-card usage-accent-${card.accent}"${card.title ? ` title="${escapeUsageHtml(card.title)}"` : ""}>
        <div class="usage-hero-head">
          <span class="usage-hero-icon">${USAGE_HERO_ICONS[card.accent] || ""}</span>
          <span class="usage-hero-label">${escapeUsageHtml(card.label)}</span>
        </div>
        <div class="usage-hero-value-row">
          <span class="usage-hero-value">${escapeUsageHtml(card.value)}</span>
          ${card.delta}
        </div>
        <div class="usage-hero-sub">${escapeUsageHtml(card.sub)}</div>
      </div>`
    )
    .join("");
}

const USAGE_QUAD_ICONS = {
  input: '<svg viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round"><path d="M7 2.2v7.2M3.8 6.4 7 9.6l3.2-3.2M2.6 11.8h8.8"/></svg>',
  output: '<svg viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round"><path d="M7 11.8V4.6M3.8 7.8 7 4.6l3.2 3.2M2.6 2.2h8.8"/></svg>',
  creation: '<svg viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round"><rect x="2.2" y="2.2" width="9.6" height="9.6" rx="2"/><path d="M7 4.8v4.4M4.8 7h4.4"/></svg>',
  hit: '<svg viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round"><path d="M7.6 1.6 2.8 8h3.1l-.8 4.4L9.9 6H6.8z"/></svg>',
};

// cc-switch 风格的应用大卡：完整千分位主数字 + 人性化换算 +
// 输入/输出/创建/命中四宫格 + 缓存命中率进度条
function renderUsageApps(byApp, totalSummary) {
  const grid = $("usageAppGrid");
  if (!grid) return;
  const zh = currentLang === "zh";
  setText(
    "usageAppsHint",
    zh ? "真实消耗 = 新增输入 + 输出 + 缓存写入 + 缓存读取" : "Real usage = fresh input + output + cache write + cache read"
  );
  if (!byApp.length) {
    grid.innerHTML = `<p class="usage-muted">${zh ? "当前筛选下没有数据" : "No data for current filter"}</p>`;
    return;
  }
  const totalCost = Number(totalSummary?.totalCost) || 0;
  const sorted = [...byApp].sort(
    (a, b) => (Number(b.summary?.totalCost) || 0) - (Number(a.summary?.totalCost) || 0)
  );
  grid.innerHTML = sorted
    .map((entry) => {
      const s = entry.summary || {};
      const app = entry.app;
      const cost = Number(s.totalCost) || 0;
      const sharePct = totalCost > 0 ? (cost / totalCost) * 100 : 0;
      const hitPct = (Number(s.cacheHitRate) || 0) * 100;
      const autoCache = USAGE_AUTO_CACHE_APPS.has(app);
      const creationValue = autoCache ? "N/A" : formatUsageTokensHuman(s.cacheCreationTokens);
      const creationTitle = autoCache
        ? (zh
            ? "该工具由服务端自动管理缓存，不产生显式缓存写入费用"
            : "Cache is managed automatically by the provider; no explicit cache writes")
        : `${formatUsageTokensExact(s.cacheCreationTokens)} tokens`;
      const icon = USAGE_APP_ICONS[app];
      const iconHtml = icon
        ? `<img src="${icon}" alt="">`
        : `<span class="usage-app-badge usage-app-${escapeUsageHtml(app)}">${escapeUsageHtml(usageAppLabel(app).slice(0, 2))}</span>`;
      const quads = [
        {
          key: "input",
          label: zh ? "新增输入" : "Fresh Input",
          value: formatUsageTokensHuman(s.inputTokens),
          title: `${formatUsageTokensExact(s.inputTokens)} tokens`,
        },
        {
          key: "output",
          label: zh ? "输出" : "Output",
          value: formatUsageTokensHuman(s.outputTokens),
          title: `${formatUsageTokensExact(s.outputTokens)} tokens`,
        },
        {
          key: "creation",
          label: zh ? "缓存创建" : "Cache Write",
          value: creationValue,
          title: creationTitle,
          muted: autoCache,
        },
        {
          key: "hit",
          label: zh ? "缓存命中" : "Cache Read",
          value: formatUsageTokensHuman(s.cacheReadTokens),
          title: `${formatUsageTokensExact(s.cacheReadTokens)} tokens`,
        },
      ];
      const quadsHtml = quads
        .map(
          (q) => `
          <div class="usage-app-quad${q.muted ? " usage-quad-muted" : ""}" title="${escapeUsageHtml(q.title)}">
            <span class="usage-quad-icon usage-quad-${q.key}">${USAGE_QUAD_ICONS[q.key]}</span>
            <div class="usage-quad-body">
              <small>${escapeUsageHtml(q.label)}</small>
              <strong>${escapeUsageHtml(q.value)}</strong>
            </div>
          </div>`
        )
        .join("");
      return `
      <div class="usage-app-hero console-card">
        <div class="usage-app-hero-top">
          <div class="usage-app-hero-id">
            <span class="usage-app-hero-avatar">${iconHtml}</span>
            <div class="usage-app-hero-name">
              <strong>${escapeUsageHtml(usageAppLabel(app))}</strong>
              <small>${zh ? "真实消耗 Tokens" : "Real Tokens Used"}${totalCost > 0 && sorted.length > 1 ? ` · ${zh ? "占总成本" : "cost share"} ${sharePct.toFixed(1)}%` : ""}</small>
            </div>
          </div>
          <div class="usage-app-hero-kpis">
            <div class="usage-app-hero-kpi">
              <small>${zh ? "总请求数" : "Requests"}</small>
              <strong>${escapeUsageHtml(formatUsageTokensExact(s.totalRequests))}</strong>
            </div>
            <div class="usage-app-hero-kpi usage-kpi-cost">
              <small>${zh ? "总成本" : "Total Cost"}</small>
              <strong>${escapeUsageHtml(formatUsageCost(cost))}</strong>
            </div>
          </div>
        </div>
        <div class="usage-app-hero-big">
          <span class="usage-app-hero-number">${escapeUsageHtml(formatUsageTokensExact(s.realTotalTokens))}</span>
          <span class="usage-app-hero-approx">≈ ${escapeUsageHtml(formatUsageTokensHuman(s.realTotalTokens))}</span>
        </div>
        <div class="usage-app-hero-quads">${quadsHtml}</div>
        <div class="usage-app-hero-cache">
          <span class="usage-cache-label">${zh ? "缓存命中率" : "Cache hit rate"}</span>
          <div class="usage-cache-track"><div class="usage-cache-fill" style="width:${Math.min(hitPct, 100).toFixed(1)}%"></div></div>
          <strong class="usage-cache-pct">${hitPct.toFixed(1)}%</strong>
        </div>
      </div>`;
    })
    .join("");
}

// 组装趋势槽位：短窗口（≤48h）按小时补全，长窗口按天补全，
// 没有数据的时段补 0 值格子，保证时间轴连续、图形不空洞
function buildUsageTrendSlots(dashboard) {
  const range = usageResolvedRange || {};
  const nowSec = Math.floor(Date.now() / 1000);
  const spanKnown = Number.isFinite(range.startTs) && Number.isFinite(range.endTs);
  const hourlyMode = spanKnown && range.endTs - range.startTs <= 48 * 3600;
  const pad = (n) => String(n).padStart(2, "0");
  const slots = [];

  if (hourlyMode) {
    const hourlyData = new Map();
    (dashboard.hourly || []).forEach((h) => hourlyData.set(Number(h.hourStartTs), h));
    const startHour = new Date(range.startTs * 1000);
    startHour.setMinutes(0, 0, 0);
    for (let ms = startHour.getTime(); ms / 1000 <= range.endTs; ms += 3600000) {
      const ts = Math.floor(ms / 1000);
      const d = new Date(ms);
      const entry = hourlyData.get(ts);
      slots.push({
        label: pad(d.getHours()),
        fullLabel: `${d.getMonth() + 1}-${d.getDate()} ${pad(d.getHours())}:00`,
        cost: Number(entry?.cost) || 0,
        tokens: Number(entry?.totalTokens) || 0,
        requests: Number(entry?.requests) || 0,
        future: ts > nowSec,
      });
      if (slots.length >= 49) break;
    }
    return { mode: "hourly", slots };
  }

  const byDate = new Map();
  (dashboard.daily || []).forEach((d) => byDate.set(String(d.date), d));
  if (Number.isFinite(range.startTs)) {
    const cursor = new Date(range.startTs * 1000);
    cursor.setHours(0, 0, 0, 0);
    const endMs = Math.min((range.endTs ?? nowSec) * 1000, Date.now());
    while (cursor.getTime() <= endMs && slots.length < 60) {
      const dateKey = `${cursor.getFullYear()}-${pad(cursor.getMonth() + 1)}-${pad(cursor.getDate())}`;
      const entry = byDate.get(dateKey);
      slots.push({
        label: `${cursor.getMonth() + 1}-${cursor.getDate()}`,
        fullLabel: dateKey,
        cost: Number(entry?.cost) || 0,
        tokens: Number(entry?.totalTokens) || 0,
        requests: Number(entry?.requests) || 0,
        future: false,
      });
      cursor.setDate(cursor.getDate() + 1);
    }
  } else {
    // “全部”范围：直接用实际有数据的日期（最多最近 30 天）
    (dashboard.daily || []).slice(-30).forEach((d) => {
      slots.push({
        label: String(d.date).slice(5),
        fullLabel: String(d.date),
        cost: Number(d.cost) || 0,
        tokens: Number(d.totalTokens) || 0,
        requests: Number(d.requests) || 0,
        future: false,
      });
    });
  }
  return { mode: "daily", slots };
}

function renderUsageTrend(dashboard) {
  const chart = $("usageTrendChart");
  if (!chart) return;
  const zh = currentLang === "zh";
  const { mode, slots } = buildUsageTrendSlots(dashboard);

  setText(
    "usageTrendHint",
    mode === "hourly" ? (zh ? "按小时" : "Hourly") : (zh ? "按天" : "Daily")
  );

  const footer = $("usageTrendFooter");
  if (!slots.length) {
    chart.innerHTML = `<p class="usage-muted">${zh ? "该范围内暂无数据" : "No data in this range"}</p>`;
    if (footer) footer.textContent = "";
    return;
  }

  const maxCost = Math.max(...slots.map((s) => s.cost), 0);
  const scale = maxCost > 0 ? maxCost : 1;
  // 标签密度：最多约 12 个可见标签
  const labelStep = Math.max(1, Math.ceil(slots.length / 12));
  const peakIndex = maxCost > 0 ? slots.findIndex((s) => s.cost === maxCost) : -1;

  chart.innerHTML = slots
    .map((slot, index) => {
      const heightPct = Math.max((slot.cost / scale) * 100, slot.cost > 0 ? 2.5 : 0);
      const title = `${slot.fullLabel} · ${formatUsageCost(slot.cost)} · ${formatUsageTokens(slot.tokens)} tokens · ${slot.requests} ${zh ? "次请求" : "requests"}`;
      const colClasses = ["usage-trend-col"];
      if (slot.future) colClasses.push("usage-trend-future");
      if (index === peakIndex) colClasses.push("usage-trend-peak");
      const labelHidden = index % labelStep !== 0 && index !== slots.length - 1;
      const peakTag =
        index === peakIndex
          ? `<span class="usage-trend-peak-tag">${escapeUsageHtml(formatUsageCost(slot.cost))}</span>`
          : "";
      return `
      <div class="${colClasses.join(" ")}" title="${escapeUsageHtml(title)}">
        <div class="usage-trend-bar-wrap">${peakTag}<div class="usage-trend-bar" style="height:${heightPct}%"></div></div>
        <span class="usage-trend-date${labelHidden ? " usage-trend-label-hide" : ""}">${escapeUsageHtml(slot.label)}</span>
      </div>`;
    })
    .join("");

  if (footer) {
    const totalCost = slots.reduce((sum, s) => sum + s.cost, 0);
    const totalRequests = slots.reduce((sum, s) => sum + s.requests, 0);
    const activeSlots = slots.filter((s) => s.cost > 0 || s.requests > 0).length;
    const unit = mode === "hourly" ? (zh ? "小时" : "hours") : (zh ? "天" : "days");
    const parts = [
      `${zh ? "合计" : "Total"} ${formatUsageCost(totalCost)}`,
      `${formatUsageTokens(totalRequests)} ${zh ? "次请求" : "requests"}`,
      `${activeSlots}/${slots.length} ${unit}${zh ? "有用量" : " active"}`,
    ];
    if (peakIndex >= 0) {
      parts.push(`${zh ? "峰值" : "Peak"} ${slots[peakIndex].fullLabel}`);
    }
    footer.textContent = parts.join(" · ");
  }
}

function renderUsageModels(models) {
  const tbody = $("usageModelTableBody");
  if (!tbody) return;
  const zh = currentLang === "zh";
  setText("usageModelsHint", zh ? `${models.length} 个模型` : `${models.length} models`);
  if (!models.length) {
    tbody.innerHTML = `<tr><td colspan="7" class="usage-muted">${zh ? "暂无模型数据" : "No model data"}</td></tr>`;
    return;
  }
  const totalCost = models.reduce((sum, m) => sum + (Number(m.cost) || 0), 0);
  tbody.innerHTML = models
    .map((m) => {
      const cost = Number(m.cost) || 0;
      const sharePct = totalCost > 0 ? (cost / totalCost) * 100 : 0;
      // cacheHitRate 为 null 表示该模型没有可缓存输入，显示 “--” 与 0% 区分
      const hitRate =
        m.cacheHitRate == null ? "--" : `${(Number(m.cacheHitRate) * 100).toFixed(1)}%`;
      return `
      <tr>
        <td class="usage-td-model" title="${escapeUsageHtml(m.model)}">${escapeUsageHtml(m.model)}</td>
        <td><span class="usage-app-badge usage-app-${escapeUsageHtml(m.app)}">${escapeUsageHtml(usageAppLabel(m.app))}</span></td>
        <td>${escapeUsageHtml(formatUsageTokens(m.requests))}</td>
        <td>${escapeUsageHtml(formatUsageTokens(m.totalTokens))}</td>
        <td>${escapeUsageHtml(hitRate)}</td>
        <td class="usage-td-cost">${escapeUsageHtml(formatUsageCost(cost))}</td>
        <td class="usage-td-share">
          <div class="usage-share-track"><div class="usage-share-fill" style="width:${Math.min(sharePct, 100).toFixed(1)}%"></div></div>
          <span class="usage-share-text">${sharePct.toFixed(1)}%</span>
        </td>
      </tr>`;
    })
    .join("");
}

function renderUsageRecent(recent) {
  const tbody = $("usageRecentTableBody");
  if (!tbody) return;
  const zh = currentLang === "zh";
  setText(
    "usageRecentHint",
    zh ? `最近 ${recent.length} 条` : `Latest ${recent.length}`
  );
  if (!recent.length) {
    tbody.innerHTML = `<tr><td colspan="5" class="usage-muted">${zh ? "暂无记录" : "No records"}</td></tr>`;
    return;
  }
  tbody.innerHTML = recent
    .map(
      (log) => `
      <tr>
        <td class="usage-td-time" title="${escapeUsageHtml(formatUsageTime(log.time))}">${escapeUsageHtml(formatUsageRelTime(log.time))}</td>
        <td><span class="usage-app-badge usage-app-${escapeUsageHtml(log.app)}">${escapeUsageHtml(usageAppLabel(log.app))}</span></td>
        <td class="usage-td-model" title="${escapeUsageHtml(log.model)}">${escapeUsageHtml(log.model)}</td>
        <td title="${zh ? "输入" : "In"} ${escapeUsageHtml(formatUsageTokens(log.inputTokens))} / ${zh ? "输出" : "Out"} ${escapeUsageHtml(formatUsageTokens(log.outputTokens))} / ${zh ? "缓存读" : "Cache read"} ${escapeUsageHtml(formatUsageTokens(log.cacheReadTokens))}">${escapeUsageHtml(formatUsageTokens(log.totalTokens))}</td>
        <td class="usage-td-cost">${escapeUsageHtml(formatUsageCost(log.cost))}</td>
      </tr>`
    )
    .join("");
}

function applyUsagePanelLanguage() {
  const zh = currentLang === "zh";
  setText("usageNavLabel", zh ? "用量监控" : "Usage Monitor");
  setText("usagePageTitle", zh ? "用量监控" : "Usage Monitor");
  setText(
    "usagePageSubtitle",
    zh
      ? "统计 Claude Code、Codex、Gemini CLI、Grok CLI 的本地会话用量与成本。"
      : "Track local session usage and cost across Claude Code, Codex, Gemini CLI and Grok CLI."
  );
  setText("usageRefreshBtn", zh ? "刷新" : "Refresh");
  setText("usageLoadingChipText", zh ? "正在扫描会话日志…" : "Scanning session logs…");
  setText("usageAppsTitle", zh ? "按应用" : "By App");
  setText("usageTrendTitle", zh ? "成本趋势" : "Cost Trend");
  setText("usageModelsTitle", zh ? "模型统计" : "Model Stats");
  setText("usageRecentTitle", zh ? "最近记录" : "Recent Activity");
  setText("usageThModel", zh ? "模型" : "Model");
  setText("usageThApp2", zh ? "应用" : "App");
  setText("usageThRequests", zh ? "请求数" : "Requests");
  setText("usageThTotalTokens", "Tokens");
  setText("usageThCacheHit", zh ? "缓存命中" : "Cache Hit");
  setText("usageThCost", zh ? "成本 (USD)" : "Cost (USD)");
  setText("usageThShare", zh ? "成本占比" : "Cost Share");
  setText("usageThTime", zh ? "时间" : "Time");
  setText("usageThApp", zh ? "应用" : "App");
  setText("usageThRecentModel", zh ? "模型" : "Model");
  setText("usageThTokens", "Tokens");
  setText("usageThRecentCost", zh ? "成本 (USD)" : "Cost (USD)");
  renderUsageRangeNote();
  const rangeLabels = {
    today: zh ? "今天" : "Today",
    "1d": zh ? "24 小时" : "24 hours",
    "7d": zh ? "近 7 天" : "Last 7 days",
    "14d": zh ? "近 14 天" : "Last 14 days",
    "30d": zh ? "近 30 天" : "Last 30 days",
    all: zh ? "全部" : "All time",
    custom: zh ? "自定义" : "Custom",
  };
  document.querySelectorAll("#usageRangeSeg [data-usage-range]").forEach((btn) => {
    const key = btn.getAttribute("data-usage-range");
    if (rangeLabels[key]) btn.textContent = rangeLabels[key];
  });
  setText("usageCustomStartLabel", zh ? "开始时间" : "Start");
  setText("usageCustomEndLabel", zh ? "结束时间" : "End");
  setText("usageCustomLiveEndLabel", zh ? "结束取当前时间" : "End at now");
  setText("usageCustomApplyBtn", zh ? "应用" : "Apply");
  const allAppsBtn = document.querySelector('#usageAppSeg [data-usage-app="all"]');
  if (allAppsBtn) allAppsBtn.textContent = zh ? "全部应用" : "All apps";
  const modelAllOption = document.querySelector('#usageModelFilter option[value="all"]');
  if (modelAllOption) modelAllOption.textContent = zh ? "全部模型" : "All models";
}

function bindUsageUiOnce() {
  if (window.__varswitchUsageBound) return;
  window.__varswitchUsageBound = true;
  $("usageRefreshBtn")?.addEventListener("click", () => loadUsageDashboard());
  const toDatetimeLocalValue = (date) => {
    const pad = (n) => String(n).padStart(2, "0");
    return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}`;
  };
  $("usageRangeSeg")?.addEventListener("click", (event) => {
    const btn = event.target.closest("[data-usage-range]");
    if (!btn) return;
    usageRange = btn.getAttribute("data-usage-range");
    document.querySelectorAll("#usageRangeSeg [data-usage-range]").forEach((b) => {
      b.classList.toggle("active", b === btn);
    });
    const customPanel = $("usageCustomRange");
    if (customPanel) customPanel.hidden = usageRange !== "custom";
    if (usageRange === "custom") {
      const startInput = $("usageCustomStart");
      const endInput = $("usageCustomEnd");
      if (startInput && !startInput.value) {
        startInput.value = toDatetimeLocalValue(new Date(Date.now() - 86400000));
      }
      if (endInput && !endInput.value) {
        endInput.value = toDatetimeLocalValue(new Date());
      }
      syncUsageCustomLiveEnd();
    }
    loadUsageDashboard();
  });
  const syncUsageCustomLiveEnd = () => {
    const endInput = $("usageCustomEnd");
    if (endInput) endInput.disabled = $("usageCustomLiveEnd")?.checked !== false;
  };
  $("usageCustomLiveEnd")?.addEventListener("change", syncUsageCustomLiveEnd);
  $("usageCustomApplyBtn")?.addEventListener("click", () => {
    if (usageRange === "custom") loadUsageDashboard();
  });
  $("usageAppSeg")?.addEventListener("click", (event) => {
    const btn = event.target.closest("[data-usage-app]");
    if (!btn) return;
    usageAppFilter = btn.getAttribute("data-usage-app");
    // 切换应用时重置模型筛选，避免残留上一个应用的模型
    usageModelFilter = "all";
    const modelSelect = $("usageModelFilter");
    if (modelSelect) modelSelect.value = "all";
    document.querySelectorAll("#usageAppSeg [data-usage-app]").forEach((b) => {
      b.classList.toggle("active", b === btn);
    });
    loadUsageDashboard();
  });
  $("usageModelFilter")?.addEventListener("change", (event) => {
    usageModelFilter = event.target.value || "all";
    loadUsageDashboard();
  });
}

function switchConsolePage(page) {
  const next = page || "add-provider";
  // 离开 toolbox 页时停掉轮询，避免后台空转
  if (activeConsolePage === "toolbox" && next !== "toolbox") {
    stopToolboxRefresh();
  }
  activeConsolePage = next;
  // 与旧 page 切换兼容
  if (next === "claude" || next === "codex" || next === "grok" || next === "gemini") {
    currentPage = next;
  }

  document.querySelectorAll(".sidebar-item[data-console-page]").forEach((item) => {
    const isActive = item.getAttribute("data-console-page") === next;
    item.classList.toggle("active", isActive);
    if (isActive) {
      item.setAttribute("aria-current", "page");
    } else {
      item.removeAttribute("aria-current");
    }
  });
  document.querySelectorAll("[data-console-page-panel]").forEach((panel) => {
    panel.classList.toggle("active", panel.getAttribute("data-console-page-panel") === next);
  });

  // 旧横向 tab 同步
  document.querySelectorAll(".page-tab").forEach((tab) => {
    tab.classList.toggle("active", tab.getAttribute("data-page") === next);
  });

  if (next === "claude") {
    loadStatus();
    loadProfiles();
  } else if (next === "codex") {
    loadCodexProfiles();
    loadCodexStatus();
  } else if (next === "grok") {
    loadGrokProfiles();
    loadGrokStatus();
  } else if (next === "gemini") {
    loadGeminiProfiles();
    loadGeminiStatus();
  } else if (next === "add-provider") {
    renderUniversalProviderForm();
  } else if (next === "toolbox") {
    mountToolboxPages();
    loadCodexToolbox();
    switchToolboxTab("session");
  } else if (next === "developer-tools") {
    mountDeveloperToolsPage();
  } else if (next === "usage") {
    bindUsageUiOnce();
    loadUsageDashboard();
  } else if (next === "settings") {
    mountToolboxPages();
    openSettingsInline();
  }

  renderNavigationStatus();

  // 滚动到主内容顶部
  $("workspaceMain")?.scrollTo?.({ top: 0, behavior: "smooth" });
  window.scrollTo({ top: 0, behavior: "smooth" });
}

async function openSettingsInline() {
  try {
    // 复用 openSettingsPanel 的数据加载，但不打开遮罩
    if (typeof loadAppSettings === "function") await loadAppSettings();
    const paths = await invoke("get_app_paths").catch(() => null);
    appPaths = paths || appPaths;
    if (paths) {
      if ($("settingsConfigDirValue")) $("settingsConfigDirValue").textContent = paths.configDir || "--";
      if ($("settingsClaudePathValue")) $("settingsClaudePathValue").textContent = paths.claudeSettings || "--";
      if ($("settingsCodexPathValue")) $("settingsCodexPathValue").textContent = paths.codexDir || paths.codexSettings || "--";
    }
    updateSettingsSegControls();
  } catch (error) {
    showToast(String(error), "error");
  }
}

/**
 * A10: 四套 provider（claude / codex / grok / gemini）的差异集中描述表。
 *
 * 只抽取「行为层」共性：切换 / 删除 / 编辑的后端命令名、toast 文案、刷新函数。
 * 卡片 HTML 不参数化：四种 provider 展示的字段本就不同（grok 有 apiBackend，
 * codex 有 providerName / imageApiKey / imageBaseUrl），硬套模板只会让可读性变差。
 *
 * 注意：claude 的切换流程与其余三家差异很大（要监听 switch-progress、
 * 先 snapshot_config 再切、失败要 restore_config、支持取消与部分成功），
 * 因此 claude 的 switch 保留独立实现，这张表里只复用它的 delete / edit。
 */
const PROVIDER_CONFIG = {
  claude: {
    label: "Claude",
    getProfiles: () => profiles,
    deleteCommand: "delete_profile",
    deletedToast: () => t("toastDeleted"),
    openEditModal: (profile) => openModal(profile),
    reload: () => Promise.all([loadProfiles(), loadStatus()]),
  },
  codex: {
    label: "Codex",
    getProfiles: () => codexProfiles,
    switchCommand: "switch_codex_profile",
    deleteCommand: "delete_codex_profile",
    switchedToast: (name) => t("codexSwitchedTo", { name }),
    deletedToast: () => t("codexToastDeleted"),
    // codex 删除时额外提示「不会自动删除远程 API 服务」
    confirmDeleteMessage: (profile) =>
      currentLang === "zh"
        ? `删除配置 “${profile.name}”？\n\n删除后将无法从 VarSwitch 中恢复，但不会自动删除远程 API 服务。`
        : t("confirmDelete", { name: profile.name }),
    openEditModal: (profile) => openCodexModal(profile),
    reload: () => Promise.all([loadCodexProfiles(), loadCodexStatus()]),
    switchSettleMs: 250,
  },
  grok: {
    label: "Grok",
    getProfiles: () => grokProfiles,
    switchCommand: "switch_grok_profile",
    deleteCommand: "delete_grok_profile",
    switchedToast: (name) => t("grokSwitchedTo", { name }),
    deletedToast: () => t("grokToastDeleted"),
    openEditModal: (profile) => openGrokModal(profile),
    reload: () => Promise.all([loadGrokProfiles(), loadGrokStatus(), loadGrokDiagnostics()]),
    switchSettleMs: 250,
  },
  gemini: {
    label: "Gemini",
    getProfiles: () => geminiProfiles,
    switchCommand: "switch_gemini_profile",
    deleteCommand: "delete_gemini_profile",
    switchedToast: (name) => `Gemini 已切换到 ${name}`,
    deletedToast: () => "Gemini 配置已删除",
    openEditModal: (profile) => openGeminiModal(profile),
    reload: () => Promise.all([loadGeminiProfiles(), loadGeminiStatus()]),
    // gemini 原实现切换后没有额外停留，保持一致
    switchSettleMs: 0,
  },
};

function findProviderProfile(type, id) {
  return PROVIDER_CONFIG[type]?.getProfiles()?.find((item) => item.id === id) || null;
}

/**
 * codex / grok / gemini 的通用切换流程（claude 不走这里，见 handleSwitch）。
 */
async function switchProviderProfile(type, id) {
  const config = PROVIDER_CONFIG[type];
  if (!config?.switchCommand) return;
  if (isSwitchingProfile) return;
  const profile = findProviderProfile(type, id);
  if (!profile) return;

  isSwitchingProfile = true;
  showSwitchOverlay(profile.name, type);
  try {
    await waitForNextPaint();
    await invoke(config.switchCommand, { id });
    completeSwitchOverlay();
    if (config.switchSettleMs) {
      await new Promise((resolve) => setTimeout(resolve, config.switchSettleMs));
    }
    showToast(config.switchedToast(profile.name || ""), "success");
  } catch (error) {
    showToast(String(error), "error");
  } finally {
    hideSwitchOverlay();
    isSwitchingProfile = false;
    await config.reload();
  }
}

/**
 * 四套 provider 通用的删除流程（确认弹窗 → 调后端 → 刷新）。
 */
async function deleteProviderProfile(type, id) {
  const config = PROVIDER_CONFIG[type];
  if (!config?.deleteCommand) return;
  const profile = findProviderProfile(type, id);
  if (!profile) return;

  const message = config.confirmDeleteMessage
    ? config.confirmDeleteMessage(profile)
    : t("confirmDelete", { name: profile.name });
  const confirmed = await appConfirm(message, {
    title: t("delete"),
    confirmText: currentLang === "zh" ? "删除配置" : "Delete",
    danger: true,
  });
  if (!confirmed) return;

  try {
    await invoke(config.deleteCommand, { id });
    showToast(config.deletedToast(), "success");
    await config.reload();
  } catch (error) {
    showToast(String(error), "error");
  }
}

function editProviderProfile(type, id) {
  const config = PROVIDER_CONFIG[type];
  const profile = findProviderProfile(type, id);
  if (config && profile) config.openEditModal(profile);
}

async function switchAnyProviderProfile(type, id) {
  // claude 的切换流程特殊，单独走 handleSwitch
  if (type === "claude") return handleSwitch(id);
  return switchProviderProfile(type, id);
}

function renderConfigurationList() {
  const host = $("configurationList");
  if (!host) return;
  const q = (configurationSearch || "").trim().toLowerCase();
  const rows = [];
  profiles.forEach((p) => rows.push({ type: "claude", profile: p }));
  codexProfiles.forEach((p) => rows.push({ type: "codex", profile: p }));
  grokProfiles.forEach((p) => rows.push({ type: "grok", profile: p }));
  geminiProfiles.forEach((p) => rows.push({ type: "gemini", profile: p }));

  const filtered = rows.filter(({ type, profile }) => {
    if (configurationFilter === "claude" && type !== "claude") return false;
    if (configurationFilter === "codex" && type !== "codex") return false;
    if (configurationFilter === "grok" && type !== "grok") return false;
    if (configurationFilter === "gemini" && type !== "gemini") return false;
    if (configurationFilter === "active" && !profile.isActive) return false;
    if (!q) return true;
    const hay = `${profile.name || ""} ${profile.baseUrl || ""} ${profile.providerName || ""} ${profile.model || ""}`.toLowerCase();
    return hay.includes(q);
  });

  if ($("configNavCount")) $("configNavCount").textContent = String(rows.length);

  if (!filtered.length) {
    host.innerHTML = `<div class="empty-inline">${currentLang === "zh" ? "没有匹配的配置" : "No matching configs"}</div>`;
    return;
  }

  host.innerHTML = filtered
    .map(({ type, profile }) => {
      const icon = type === "claude" ? "anthropic-color.svg"
        : type === "codex" ? "OpenAI-black-monoblossom.svg"
        : type === "gemini" ? "gemini-color.svg"
        : "grok-color.svg";
      const typeLabel = type === "claude" ? "Claude"
        : type === "codex" ? "Codex"
        : type === "gemini" ? "Gemini"
        : "Grok";
      const badge = profile.isActive
        ? `<span class="status-badge-console healthy">${currentLang === "zh" ? "使用中" : "Active"}</span>`
        : `<span class="status-badge-console">${currentLang === "zh" ? "未启用" : "Idle"}</span>`;
      return `<div class="configuration-row configuration-row-manager" data-config-type="${type}" data-config-id="${esc(profile.id)}">
        <div class="configuration-data">
          <div class="configuration-main"><img src="${icon}" alt=""><div><strong>${esc(profile.name)}</strong><small>${typeLabel}</small></div></div>
          <div class="configuration-meta">${esc(maskKey(profile.apiKey || ""))}<br>${esc(truncUrl(profile.baseUrl || "", 42))}</div>
          <div class="configuration-state">${badge}<div class="configuration-meta configuration-model">${esc(profile.model || profile.providerName || "--")}</div></div>
        </div>
        <div class="configuration-actions">
          ${profile.isActive
            ? `<span class="configuration-action-slot" aria-hidden="true"></span>`
            : `<button class="btn btn-secondary btn-sm" data-action="switch" type="button">${currentLang === "zh" ? "切换" : "Use"}</button>`}
          <button class="btn btn-ghost btn-sm" data-action="edit" type="button">${currentLang === "zh" ? "编辑" : "Edit"}</button>
          <button class="btn btn-ghost btn-sm" data-action="copy" type="button">${currentLang === "zh" ? "复制" : "Duplicate"}</button>
          <button class="btn btn-danger btn-sm" data-action="delete" type="button">${currentLang === "zh" ? "删除" : "Delete"}</button>
        </div>
      </div>`;
    })
    .join("");

  // A10: 容器级委托，provider 类型与 id 从卡片行的 data-* 上就近读取
  bindDelegatedActions(host, "ConfigurationList", async (action, target) => {
    const row = target.closest(".configuration-row");
    if (!row) return;
    const type = row.getAttribute("data-config-type");
    const id = row.getAttribute("data-config-id");
    if (!PROVIDER_CONFIG[type] || !id) return; // 未知类型不执行
    if (action === "switch") await switchAnyProviderProfile(type, id);
    else if (action === "edit") editProviderProfile(type, id);
    else if (action === "copy") duplicateConfiguration(type, id);
    else if (action === "delete") await deleteProviderProfile(type, id);
    else return;
    renderConfigurationList();
  });
}

function renderSessionStatusCard() {
  const summary = $("sessionStatusSummary");
  const metrics = $("sessionStatusMetrics");
  if (!summary || !metrics) return;
  const sessionMetrics = helpers.getCodexSessionMetrics?.(codexToolbox) || { count: 0, trashCount: 0, lastSyncedAt: "" };
  const { count, trashCount, lastSyncedAt } = sessionMetrics;
  const lastSyncLabel = lastSyncedAt ? formatToolboxSessionTime(lastSyncedAt) : "--";
  summary.textContent = currentLang === "zh" ? `已同步 ${count} 个会话` : `${count} sessions synced`;
  metrics.innerHTML = `
    <div class="wizard-review-item"><small>${currentLang === "zh" ? "已同步" : "Synced"}</small><strong>${count}</strong></div>
    <div class="wizard-review-item"><small>${currentLang === "zh" ? "回收站" : "Trash"}</small><strong>${trashCount}</strong></div>
    <div class="wizard-review-item"><small>${currentLang === "zh" ? "最近同步" : "Last sync"}</small><strong>${esc(lastSyncLabel)}</strong></div>
  `;
  metrics.className = "wizard-review";
  if ($("sessionNavCount")) $("sessionNavCount").textContent = String(count);
}

function renderNavigationStatus() {
  renderProviderNavigation();
  const activeClaude = profiles.find((profile) => profile.isActive);
  const activeCodex = codexProfiles.find((profile) => profile.isActive);
  const activeGrok = grokProfiles.find((profile) => profile.isActive);
  const activeGemini = geminiProfiles.find((profile) => profile.isActive);
  const claudeLoaded = lastClaudeStatus != null || profiles.length > 0;
  const codexLoaded = lastCodexStatus != null || codexProfiles.length > 0;
  const grokLoaded = lastGrokStatus != null || grokProfiles.length > 0;
  const geminiLoaded = lastGeminiStatus != null || geminiProfiles.length > 0;
  const claudeOk = !!(lastClaudeStatus?.envVars || lastClaudeStatus?.claude || activeClaude);
  const codexOk = !!(lastCodexStatus?.apiKey || activeCodex);
  const grokOk = !!(lastGrokStatus?.apiKey || activeGrok);
  const geminiOk = !!(lastGeminiStatus?.apiKey || activeGemini);

  setNavDot("claudeNavState", !claudeLoaded ? "" : claudeOk ? "healthy" : "warning");
  setNavDot("codexNavState", !codexLoaded ? "" : codexOk ? "healthy" : "warning");
  setNavDot("grokNavState", !grokLoaded ? "" : grokOk ? "healthy" : "warning");
  setNavDot("geminiNavState", !geminiLoaded ? "" : geminiOk ? "healthy" : "warning");

  const channels = codexToolbox?.mobileChannels || [];
  const mobileBound = Array.isArray(channels) && channels.some((channel) => channel.bound || channel.appId || channel.botToken);
  setNavDot("mobileNavState", mobileBound ? "healthy" : "");

}

function renderMobileTimeline() {
  const list = $("mobileTimeline");
  if (!list || !codexToolbox) return;
  const channels = codexToolbox.mobileChannels || [];
  const bound = channels.some((c) => c.bound || c.appId || c.botToken || c.qrDeviceCode);
  const hasSession = !!(selectedCodexThreadId || codexToolbox.selectedThreadId);
  const listening = !!(codexToolbox.remoteRunning || codexToolbox.isListening);
  list.querySelectorAll("li").forEach((li) => {
    const step = li.getAttribute("data-step");
    let done = false;
    if (step === "bind") done = bound;
    if (step === "session") done = hasSession;
    if (step === "listen") done = listening;
    if (step === "wait") done = listening;
    li.classList.toggle("done", done);
    li.classList.toggle("active", !done && ((step === "bind" && !bound) || (step === "session" && bound && !hasSession) || (step === "listen" && bound && hasSession && !listening) || (step === "wait" && listening)));
  });
}

function duplicateConfiguration(type, id) {
  const copySuffix = currentLang === "zh" ? "副本" : "Copy";
  if (type === "claude") {
    const profile = profiles.find((item) => item.id === id);
    if (!profile) return;
    openModal(null);
    $("profileName").value = `${profile.name} ${copySuffix}`;
    $("profileApiKey").value = profile.apiKey || "";
    $("profileBaseUrl").value = profile.baseUrl || "";
    $("profileModelId").value = profile.modelId || "";
  } else if (type === "codex") {
    const profile = codexProfiles.find((item) => item.id === id);
    if (!profile) return;
    openCodexModal(null);
    $("codexProfileName").value = `${profile.name} ${copySuffix}`;
    $("codexApiKey").value = profile.apiKey || "";
    $("codexBaseUrl").value = profile.baseUrl || "";
    $("codexModel").value = profile.model || "";
    if ($("codexWireApi")) $("codexWireApi").value = profile.wireApi || "responses";
    $("codexProvider").value = profile.providerName || "";
    $("codexImageApiKey").value = profile.imageApiKey || "";
    $("codexImageBaseUrl").value = profile.imageBaseUrl || "";
    setCodexAuthMode(profile.authMode || "auth_json");
  } else {
    const profile = grokProfiles.find((item) => item.id === id);
    if (!profile) return;
    openGrokModal(null);
    $("grokProfileName").value = `${profile.name} ${copySuffix}`;
    $("grokApiKey").value = profile.apiKey || "";
    $("grokBaseUrl").value = profile.baseUrl || "https://api.x.ai/v1";
    $("grokModel").value = profile.model || "";
    if ($("grokApiBackend")) $("grokApiBackend").value = profile.apiBackend || "chat_completions";
  }
  showToast(currentLang === "zh" ? "已创建配置副本，请确认后保存" : "Config duplicated. Review and save it.", "success");
}

function bindConsoleUiOnce() {
  if (window.__varswitchConsoleBound) return;
  window.__varswitchConsoleBound = true;

  // 事件委托：侧边栏 / 指标卡 / 快捷入口一定可点
  document.addEventListener("click", (event) => {
    const nav = event.target.closest("[data-console-page]");
    if (!nav) return;
    // 忽略弹窗表单内的无关节点
    if (nav.closest(".modal-overlay") || nav.closest(".mgmt-overlay")) return;
    const page = nav.getAttribute("data-console-page");
    if (!page) return;
    if (
      nav.tagName === "BUTTON" ||
      nav.classList.contains("sidebar-item") ||
      nav.classList.contains("quick-action") ||
      nav.classList.contains("sidebar-settings")
    ) {
      event.preventDefault();
      if (page === "developer-tools") openDeveloperTools("skills");
      else switchConsolePage(page);
    }
  });

  document.querySelectorAll("[data-universal-preset]").forEach((button) => {
    button.addEventListener("click", () => applyUniversalProviderPreset(button.getAttribute("data-universal-preset")));
  });
  document.querySelectorAll("[data-universal-app]").forEach((input) => {
    input.addEventListener("change", renderUniversalProviderForm);
  });
  $("universalProviderForm")?.addEventListener("submit", handleUniversalProviderSubmit);
  $("universalProviderResetBtn")?.addEventListener("click", () => resetUniversalProviderForm());
  $("universalEndpointTestBtn")?.addEventListener("click", handleUniversalEndpointTest);
  $("universalProviderBaseUrl")?.addEventListener("input", updateUniversalUrlPreviews);
  $("universalClaudeApiFormat")?.addEventListener("change", updateUniversalUrlPreviews);
  $("universalProviderApiKeyToggle")?.addEventListener("click", () => {
    const input = $("universalProviderApiKey");
    const showing = input.type === "password";
    input.type = showing ? "text" : "password";
    setText("universalProviderApiKeyToggle", showing ? "隐藏" : "显示");
  });
  $("claudePageAddBtn")?.addEventListener("click", () => openModal(null));
  $("claudePageImportBtn")?.addEventListener("click", handleImport);
  $("claudeSyncBtn")?.addEventListener("click", handleSyncNow);
  $("codexPageAddBtn")?.addEventListener("click", () => openCodexModal(null));
  $("codexPageImportBtn")?.addEventListener("click", handleCodexImport);
  $("codexSyncBtn")?.addEventListener("click", () => {
    const active = codexProfiles.find((profile) => profile.isActive);
    if (active) handleCodexSwitch(active.id);
    else showToast(currentLang === "zh" ? "暂无启用的 Codex 配置" : "No active Codex profile", "warning");
  });
  $("geminiPageAddBtn")?.addEventListener("click", () => openGeminiModal(null));
  $("geminiPageImportBtn")?.addEventListener("click", handleGeminiImport);
  // 与 Claude 的“立即同步”语义一致:重新应用当前启用的配置
  $("geminiRefreshBtn")?.addEventListener("click", () => {
    const active = geminiProfiles.find((p) => p.isActive);
    if (active) handleGeminiSwitch(active.id);
    else showToast(currentLang === "zh" ? "暂无启用的 Gemini 配置" : "No active Gemini profile", "warning");
  });
  $("sessionPageSyncBtn")?.addEventListener("click", handleToolboxSessionSync);
  $("settingsLangZh")?.addEventListener("click", () => setLanguage("zh"));
  $("settingsLangEn")?.addEventListener("click", () => setLanguage("en"));
  $("settingsThemeLight")?.addEventListener("click", () => setTheme("light"));
  $("settingsThemeDark")?.addEventListener("click", () => setTheme("dark"));
  $("settingsGuideBtn")?.addEventListener("click", () => openUsageGuide());
  $("settingsUpdateBtn")?.addEventListener("click", () => handleUpdateButton());
  $("settingsDownloadBtn")?.addEventListener("click", () => openUpdateReleasePage());
  $("settingsGithubBtn")?.addEventListener("click", () => openGitHubRepo());
  document.querySelectorAll("[data-developer-tool]").forEach((button) => {
    button.addEventListener("click", () => switchDeveloperTool(button.getAttribute("data-developer-tool")));
  });

  // Codex 表单附加控件
  $("codexSaveEnableBtn")?.addEventListener("click", async () => {
    codexEnableAfterSave = true;
    $("codexSubmitBtn")?.click();
  });
  $("codexApiKeyToggle")?.addEventListener("click", () => {
    const input = $("codexApiKey");
    if (!input) return;
    const show = input.type === "password";
    input.type = show ? "text" : "password";
    $("codexApiKeyToggle").textContent = show ? (currentLang === "zh" ? "隐藏" : "Hide") : (currentLang === "zh" ? "显示" : "Show");
  });
}

function enhanceAfterDataLoad() {
  mountToolboxPages();
  renderNavigationStatus();
  renderUniversalProviderForm();
  renderSessionStatusCard();
  renderMobileTimeline();
}

let configTypeAction = "add";

function closeConfigTypeDialog() {
  $("configTypeOverlay")?.classList.remove("open");
  document.body.classList.remove("modal-open");
}

function openConfigTypeDialog(action = "add") {
  configTypeAction = action === "import" ? "import" : "add";
  if ($("configTypeTitle")) {
    $("configTypeTitle").textContent = configTypeAction === "import"
      ? (currentLang === "zh" ? "选择导入类型" : "Choose import type")
      : (currentLang === "zh" ? "选择配置类型" : "Choose config type");
  }
  if ($("configTypeMessage")) {
    $("configTypeMessage").textContent = configTypeAction === "import"
      ? (currentLang === "zh" ? "选择要从当前本机环境导入的配置类型。" : "Choose which current local configuration to import.")
      : (currentLang === "zh" ? "选择要添加的配置类型。" : "Choose the configuration type to add.");
  }
  $("configTypeOverlay")?.classList.add("open");
  document.body.classList.add("modal-open");
  setTimeout(() => $("configTypeOverlay")?.querySelector("[data-config-kind]")?.focus(), 30);
}

function runConfigTypeAction(kind) {
  closeConfigTypeDialog();
  switchConsolePage(kind);
  if (configTypeAction === "import") {
    if (kind === "claude") handleImport();
    else if (kind === "codex") handleCodexImport();
    else handleGrokImport();
    return;
  }
  if (kind === "claude") openModal(null);
  else if (kind === "codex") openCodexModal(null);
  else openGrokModal(null);
}

function bindConfigTypeDialogOnce() {
  if (window.__varswitchConfigTypeBound) return;
  window.__varswitchConfigTypeBound = true;
  $("configTypeClose")?.addEventListener("click", closeConfigTypeDialog);
  $("configTypeCancel")?.addEventListener("click", closeConfigTypeDialog);
  bindOverlayDismiss("configTypeOverlay", closeConfigTypeDialog);
  $("configTypeOverlay")?.querySelectorAll("[data-config-kind]").forEach((button) => {
    button.addEventListener("click", () => runConfigTypeAction(button.getAttribute("data-config-kind")));
  });
}


// 在应用页上触发添加时直接打开该应用的表单；其他页面进入统一供应商页
function triggerCurrentAdd() {
  if (activeConsolePage === "claude") openModal(null);
  else if (activeConsolePage === "codex") openCodexModal(null);
  else if (activeConsolePage === "grok") openGrokModal(null);
  else if (activeConsolePage === "gemini") openGeminiModal(null);
  else openProviderOnboarding(null);
}

function triggerCurrentImport() {
  if (activeConsolePage === "codex") handleCodexImport();
  else if (activeConsolePage === "grok") handleGrokImport();
  else if (activeConsolePage === "gemini") handleGeminiImport();
  else if (activeConsolePage === "claude") handleImport();
  else handleImport();
}

$("heroToolboxBtn")?.addEventListener("click", openCodexToolbox);
on("langZhBtn", "click", () => setLanguage("zh"));
on("langEnBtn", "click", () => setLanguage("en"));
on("themeLightBtn", "click", () => setTheme("light"));
on("themeDarkBtn", "click", () => setTheme("dark"));
on("cancelBtn", "click", closeModal);
on("modalClose", "click", closeModal);
on("switchCancelBtn", "click", handleCancelSwitch);
on("profileForm", "submit", handleSubmit);
on("profilePresetSelect", "change", () => applyClaudePreset(getSelectedClaudePreset()));
on("profileBaseUrl", "focus", () => tryClipboardAutoFill("url", "profileBaseUrl"));
on("profileApiKey", "focus", () => tryClipboardAutoFill("key", "profileApiKey"));
on("profileModelFetchBtn", "click", () => handleModelFetch("claude"));
on("profileEndpointTestBtn", "click", () => handleEndpointTest("claude"));
bindOverlayDismiss("modalOverlay", closeModal);

// ── Page Tabs Event Listeners ───────────────────────

document.querySelectorAll(".page-tab[data-page]").forEach((tab) => {
  tab.addEventListener("click", () => {
    switchPage(tab.getAttribute("data-page"));
  });
});

// ── Codex Modal Event Listeners ─────────────────────

on("codexCancelBtn", "click", closeCodexModal);
on("codexModalClose", "click", closeCodexModal);
on("codexProfileForm", "submit", handleCodexSubmit);
on("codexPresetSelect", "change", () => applyCodexPreset(getSelectedCodexPreset()));
on("codexBaseUrl", "focus", () => tryClipboardAutoFill("url", "codexBaseUrl"));
on("codexApiKey", "focus", () => tryClipboardAutoFill("key", "codexApiKey"));
on("codexImageBaseUrl", "focus", () => tryClipboardAutoFill("url", "codexImageBaseUrl"));
on("codexImageApiKey", "focus", () => tryClipboardAutoFill("key", "codexImageApiKey"));
on("codexBaseUrl", "input", updateCodexOfficialConfigPreview);
on("codexApiKey", "input", updateCodexOfficialConfigPreview);
on("codexImageBaseUrl", "input", updateCodexOfficialConfigPreview);
on("codexImageApiKey", "input", updateCodexOfficialConfigPreview);
$("codexAuthModeDefault")?.addEventListener("change", updateCodexAuthModeUi);
$("codexAuthModeOfficial")?.addEventListener("change", updateCodexAuthModeUi);
$("codexAuthModeSaveOnly")?.addEventListener("change", updateCodexAuthModeUi);
on("codexCopyOfficialConfigBtn", "click", copyCodexOfficialConfig);
on("codexModelFetchBtn", "click", () => handleModelFetch("codex"));
on("codexEndpointTestBtn", "click", () => handleEndpointTest("codex"));
bindOverlayDismiss("codexModalOverlay", closeCodexModal);

// ── Grok Modal Event Listeners ──────────────────────

$("grokCancelBtn")?.addEventListener("click", closeGrokModal);
$("grokModalClose")?.addEventListener("click", closeGrokModal);
$("grokProfileForm")?.addEventListener("submit", handleGrokSubmit);
$("grokPresetSelect")?.addEventListener("change", () => applyGrokPreset(getSelectedGrokPreset()));
$("grokBaseUrl")?.addEventListener("focus", () => tryClipboardAutoFill("url", "grokBaseUrl"));
$("grokApiKey")?.addEventListener("focus", () => tryClipboardAutoFill("key", "grokApiKey"));
$("grokModelFetchBtn")?.addEventListener("click", () => handleModelFetch("grok"));
$("grokEndpointTestBtn")?.addEventListener("click", () => handleEndpointTest("grok"));
bindOverlayDismiss("grokModalOverlay", closeGrokModal);
$("grokPageAddBtn")?.addEventListener("click", () => openGrokModal(null));
// 与 Claude 的“立即同步”语义一致:重新应用当前启用的配置
$("grokRefreshBtn")?.addEventListener("click", () => {
  const active = grokProfiles.find((p) => p.isActive);
  if (active) handleGrokSwitch(active.id);
  else showToast(currentLang === "zh" ? "暂无启用的 Grok 配置" : "No active Grok profile", "warning");
});
$("grokPageImportBtn")?.addEventListener("click", handleGrokImport);

// ── Gemini Modal Event Listeners ───────────────────

$("geminiCancelBtn")?.addEventListener("click", closeGeminiModal);
$("geminiModalClose")?.addEventListener("click", closeGeminiModal);
$("geminiProfileForm")?.addEventListener("submit", handleGeminiSubmit);
$("geminiBaseUrl")?.addEventListener("focus", () => tryClipboardAutoFill("url", "geminiBaseUrl"));
$("geminiApiKey")?.addEventListener("focus", () => tryClipboardAutoFill("key", "geminiApiKey"));
$("geminiModelFetchBtn")?.addEventListener("click", () => handleModelFetch("gemini"));
$("geminiEndpointTestBtn")?.addEventListener("click", () => handleEndpointTest("gemini"));
bindOverlayDismiss("geminiModalOverlay", closeGeminiModal);
$("codexToolboxBtn")?.addEventListener("click", openCodexToolbox);
on("codexToolboxClose", "click", closeCodexToolbox);
bindOverlayDismiss("codexToolboxOverlay", closeCodexToolbox);
on("toolboxTabSession", "click", () => switchToolboxTab("session"));
on("toolboxTabRemote", "click", () => switchToolboxTab("remote"));
on("toolboxSessionSearchInput", "input", (event) => {
  toolboxSessionSearchQuery = event.target.value || "";
  renderToolboxSyncedThreads();
});
async function handleToolboxSessionSync() {
  if (toolboxSessionSyncBusy) return;
  startToolboxSessionProgress();
  try {
    codexToolbox = await invoke("sync_codex_sessions", {});
    finishToolboxSessionProgress(true);
    showToast(t("toolboxSessionsSynced"), "success");
    renderCodexToolbox();
    renderSessionStatusCard();
  } catch (error) {
    finishToolboxSessionProgress(false);
    showToast(String(error), "error");
  }
}
on("toolboxRemoteStartBtn", "click", async () => {
  if (toolboxRemoteBusy) return;
  mobileDebug("remote start button clicked");
  toolboxRemoteBusy = true;
  mobileBindBusyAction = "toolbox-remote-start";
  $("toolboxRemoteStartBtn").disabled = true;
  $("toolboxRemoteStopBtn").disabled = true;
  $("toolboxRemoteStartBtn").classList.add("is-busy");
  renderCodexToolbox();
  try {
    await bindCurrentMobileSelection();
    mobileDebug("start_mobile_remote invoke:start", {
      binding: getSelectedMobileBinding(),
    });
    codexToolbox = await invoke("start_mobile_remote", {});
    mobileDebug("start_mobile_remote invoke:success", {
      remote: codexToolbox?.mobileRemote,
      binding: getSelectedMobileBinding(),
    });
    startToolboxRefresh(80);
    showToast(t("toolboxRemoteStarted"), "success");
    renderCodexToolbox();
  } catch (error) {
    mobileDebugError("start_mobile_remote flow failed", error);
    showToast(String(error), "error");
  } finally {
    toolboxRemoteBusy = false;
    mobileBindBusyAction = "";
    $("toolboxRemoteStartBtn").disabled = false;
    $("toolboxRemoteStopBtn").disabled = false;
    $("toolboxRemoteStartBtn").classList.remove("is-busy");
    renderCodexToolbox();
  }
});
on("toolboxRemoteStopBtn", "click", async () => {
  if (toolboxRemoteBusy) return;
  mobileDebug("remote stop button clicked");
  toolboxRemoteBusy = true;
  mobileBindBusyAction = "toolbox-remote-stop";
  $("toolboxRemoteStartBtn").disabled = true;
  $("toolboxRemoteStopBtn").disabled = true;
  $("toolboxRemoteStopBtn").classList.add("is-busy");
  renderCodexToolbox();
  try {
    mobileDebug("stop_mobile_remote invoke:start", { source: "remote-main-button" });
    codexToolbox = await invoke("stop_mobile_remote", {});
    mobileDebug("stop_mobile_remote invoke:success", {
      source: "remote-main-button",
      remote: codexToolbox?.mobileRemote,
      binding: getSelectedMobileBinding(),
    });
    showToast(t("toolboxRemoteStopped"), "success");
    startToolboxRefresh(2);
    renderCodexToolbox();
  } catch (error) {
    mobileDebugError("stop_mobile_remote flow failed", error);
    showToast(String(error), "error");
  } finally {
    toolboxRemoteBusy = false;
    mobileBindBusyAction = "";
    $("toolboxRemoteStartBtn").disabled = false;
    $("toolboxRemoteStopBtn").disabled = false;
    $("toolboxRemoteStopBtn").classList.remove("is-busy");
    renderCodexToolbox();
  }
});

// ── Management Panel Event Listeners ────────────────

on("skillsBtn", "click", openSkillsPanel);
on("skillsClose", "click", closeSkillsPanel);
on("addSkillBtn", "click", () => showSkillsEdit(null, "command"));
on("skillCancelBtn", "click", hideSkillsEdit);
on("skillSaveBtn", "click", handleSaveSkill);
bindOverlayDismiss("skillsOverlay", closeSkillsPanel);

// Skills tabs
on("skillsTabInstalled", "click", () => switchSkillsTab("installed"));
on("skillsTabDiscover", "click", () => switchSkillsTab("discover"));

// Skills discover search

// Discover search and filters
on("discoverSearch", "input", (e) => {
  discoverSearchQuery = e.target.value;
  // Local filter for catalog mode
  renderDiscoverGrid();
});
on("discoverSearch", "keydown", (e) => {
  if (e.key === "Enter") {
    e.preventDefault();
    searchGitHubSkills();
  }
});
on("searchGithubSkillsBtn", "click", searchGitHubSkills);
on("backToCatalogBtn", "click", backToCatalog);
on("discoverRepoFilter", "change", (e) => {
  discoverRepoFilter = e.target.value;
  renderDiscoverGrid();
});
on("discoverStatusFilter", "change", (e) => {
  discoverStatusFilter = e.target.value;
  renderDiscoverGrid();
});

// Repo manager
on("manageReposBtn", "click", openRepoManager);
on("refreshDiscoverBtn", "click", () => {
  backToCatalog();
});
on("repoManagerClose", "click", closeRepoManager);
bindOverlayDismiss("repoManagerOverlay", closeRepoManager);
on("addRepoBtn", "click", handleAddRepo);
on("repoUrlInput", "keydown", (e) => {
  if (e.key === "Enter") handleAddRepo();
});

on("promptsBtn", "click", openPromptsPanel);
on("promptsClose", "click", closePromptsPanel);
on("promptSaveBtn", "click", handleSavePrompt);
bindOverlayDismiss("promptsOverlay", closePromptsPanel);
on("promptTabEditor", "click", () => switchPromptTab("editor"));
on("promptTabTemplates", "click", () => switchPromptTab("templates"));
on("promptInsertSelect", "change", (e) => {
  const id = e.target.value;
  if (!id) return;
  const tpl = promptTemplates.find((t) => t.id === id);
  if (tpl) {
    const current = $("promptContentInput").value;
    $("promptContentInput").value = current
      ? current + "\n\n" + tpl.content
      : tpl.content;
    showToast(t("toastSnippetInserted"), "success");
  }
  e.target.value = "";
});

on("mcpBtn", "click", openMcpPanel);
on("mcpClose", "click", closeMcpPanel);
on("addMcpBtn", "click", () => showMcpEdit(null));
on("mcpCancelBtn", "click", hideMcpEdit);
on("mcpSaveBtn", "click", handleSaveMcp);
bindOverlayDismiss("mcpOverlay", closeMcpPanel);
on("mcpTabInstalled", "click", () => switchMcpTab("installed"));
on("mcpTabPresets", "click", () => switchMcpTab("presets"));
on("mcpPresetSearch", "keydown", (e) => {
  if (e.key === "Enter") {
    e.preventDefault();
    searchGitHubMcp();
  }
});

on("usageGuideBtn", "click", openUsageGuide);
on("updateBtn", "click", handleUpdateButton);
const downloadSiteBtn = $("downloadSiteBtn");
if (downloadSiteBtn) downloadSiteBtn.addEventListener("click", openUpdateReleasePage);
on("updatePillBtn", "click", async () => {
  if (updateBusy) return;
  if (updateInfo?.hasUpdate) {
    await installAppUpdate();
  } else {
    await openUpdateReleasePage();
  }
});
on("githubRepoBtn", "click", openGitHubRepo);
on("usageGuideCloseBtn", "click", closeUsageGuide);
on("usageGuideCloseIcon", "click", closeUsageGuide);
on("usageGuideNeverBtn", "click", handleNeverShowUsageGuide);
bindOverlayDismiss("usageGuideOverlay", closeUsageGuide);

// ── Settings Panel ──────────────────────────────────

function getSettingsEditorPathInfos() {
  return Array.isArray(appPaths?.editorSettings) ? appPaths.editorSettings : [];
}

function editorPathInputId(editorId) {
  return `settingsEditorPathInput-${editorId}`;
}

function getEditorPathInput(editorId) {
  return $(editorPathInputId(editorId));
}

function getEditorPathInfo(editorId) {
  return getSettingsEditorPathInfos().find((editorInfo) => editorInfo.id === editorId) || null;
}

function getEditorPathStatusLabel(mode) {
  if (mode === "custom") return t("settingsEditorStatusCustom");
  if (mode === "detected") return t("settingsEditorStatusDetected");
  return t("settingsEditorStatusDefault");
}

function getEditorPathStatusHint(mode) {
  if (mode === "custom") return t("settingsEditorHintCustom");
  if (mode === "detected") return t("settingsEditorHintDetected");
  return t("settingsEditorHintDefault");
}

function createEditorPathButton(labelKey, className, onClick) {
  const button = document.createElement("button");
  button.type = "button";
  button.className = className;
  button.textContent = t(labelKey);
  button.addEventListener("click", onClick);
  return button;
}

function createEditorPathRow(editorInfo) {
  const mode = getEditorPathMode(editorInfo);
  const row = document.createElement("div");
  row.className = "settings-row settings-row-editor";

  const card = document.createElement("div");
  card.className = "settings-editor-card";

  const header = document.createElement("div");
  header.className = "settings-editor-header";

  const titleBlock = document.createElement("div");
  titleBlock.className = "settings-editor-title-block";

  const label = document.createElement("div");
  label.className = "settings-row-label";
  label.textContent =
    currentLang === "zh"
      ? `${editorInfo.displayName} 设置`
      : `${editorInfo.displayName} settings`;

  const badge = document.createElement("span");
  badge.className = `settings-path-badge settings-path-badge-${mode}`;
  badge.textContent = getEditorPathStatusLabel(mode);

  const desc = document.createElement("div");
  desc.className = "settings-row-desc";
  desc.textContent = getEditorPathStatusHint(mode);

  titleBlock.appendChild(label);
  titleBlock.appendChild(desc);

  header.appendChild(titleBlock);
  header.appendChild(badge);

  const layout = document.createElement("div");
  layout.className = "settings-editor-layout";

  const currentPanel = document.createElement("div");
  currentPanel.className = "settings-path-panel settings-path-panel-current";

  const currentLabel = document.createElement("div");
  currentLabel.className = "settings-path-field-label";
  currentLabel.textContent = t("settingsCurrentPath");

  const input = document.createElement("input");
  input.className = "settings-path-input";
  input.type = "text";
  input.id = editorPathInputId(editorInfo.id);
  input.placeholder = t("settingsPathPlaceholder");
  input.value = editorInfo.settingsPath || "";
  input.spellcheck = false;
  input.autocomplete = "off";

  const actions = document.createElement("div");
  actions.className = "settings-editor-actions";
  actions.appendChild(
    createEditorPathButton("settingsBrowse", "btn btn-secondary btn-sm", () =>
      handleBrowseEditorPath(editorInfo.id)
    )
  );
  actions.appendChild(
    createEditorPathButton("settingsSavePath", "btn btn-primary btn-sm", () =>
      handleSaveEditorPath(editorInfo.id)
    )
  );
  actions.appendChild(
    createEditorPathButton("settingsReset", "btn btn-secondary btn-sm", () =>
      handleResetEditorPath(editorInfo.id)
    )
  );
  actions.appendChild(
    createEditorPathButton("settingsOpen", "btn btn-secondary btn-sm", () =>
      handleOpenEditorPath(editorInfo.id)
    )
  );

  currentPanel.appendChild(currentLabel);
  currentPanel.appendChild(input);
  currentPanel.appendChild(actions);

  const defaultPanel = document.createElement("div");
  defaultPanel.className = "settings-path-panel settings-path-panel-default";

  const defaultLabel = document.createElement("div");
  defaultLabel.className = "settings-path-field-label";
  defaultLabel.textContent = t("settingsDefaultPathLabel");

  const meta = document.createElement("div");
  meta.className = "settings-path-meta";
  meta.textContent = editorInfo.defaultPath || "--";

  defaultPanel.appendChild(defaultLabel);
  defaultPanel.appendChild(meta);

  layout.appendChild(currentPanel);
  layout.appendChild(defaultPanel);

  card.appendChild(header);
  card.appendChild(layout);
  row.appendChild(card);
  return row;
}

function renderSettingsEditorPaths(editorInfos) {
  const editorPathsContainer = $("settingsEditorPaths");
  editorPathsContainer.innerHTML = "";
  for (const editorInfo of editorInfos) {
    editorPathsContainer.appendChild(createEditorPathRow(editorInfo));
  }
}

async function refreshSettingsPanelData() {
  const [settings, paths] = await Promise.all([
    invoke("get_app_settings"),
    invoke("get_app_paths"),
  ]);

  appSettings = settings || {};
  appSettings.editorPaths = appSettings.editorPaths || {};
  appPaths = paths || {};

  $("settingsAutoStart").checked = !!appSettings.autoStart;
  $("settingsMinTray").checked = !!appSettings.minimizeToTray;
  $("settingsSilentStart").checked = !!appSettings.silentStartup;
  $("settingsConfigDirValue").textContent = appPaths.configDir || "--";
  $("settingsClaudePathValue").textContent = appPaths.claudeSettings || "--";
  $("settingsCodexPathValue").textContent = appPaths.codexSettings || "--";
  renderSettingsEditorPaths(getSettingsEditorPathInfos());
}

async function handleBrowseEditorPath(editorId) {
  try {
    const dialog = window.__TAURI_PLUGIN_DIALOG__;
    const editorInfo = getEditorPathInfo(editorId);
    const input = getEditorPathInput(editorId);
    const rawDefaultPath = (input?.value || editorInfo?.settingsPath || editorInfo?.defaultPath || "")
      .trim()
      .replace(/[\\/]settings\.json$/i, "");
    const selectedPath = await dialog.open({
      directory: true,
      multiple: false,
      defaultPath: rawDefaultPath || undefined,
    });
    if (!selectedPath || Array.isArray(selectedPath)) return;
    if (input) {
      input.value = selectedPath;
    }
  } catch (error) {
    showToast(String(error), "error");
  }
}

async function handleSaveEditorPath(editorId) {
  try {
    if (!appSettings) {
      await loadAppSettings();
    }
    const editorInfo = getEditorPathInfo(editorId);
    const input = getEditorPathInput(editorId);
    const draftValue = input ? input.value : "";
    const result = validateEditorPathInput(draftValue);
    if (!result.valid) {
      showToast(t("settingsPathEmpty"), "warning");
      input?.focus();
      return;
    }

    appSettings.editorPaths = appSettings.editorPaths || {};
    appSettings.editorPaths[editorId] = result.value;
    await persistAppSettings();
    await Promise.all([refreshSettingsPanelData(), loadStatus()]);
    showToast(
      t("toastEditorPathSaved", {
        name: editorInfo?.displayName || editorId,
      }),
      "success"
    );
  } catch (error) {
    showToast(String(error), "error");
  }
}

async function handleResetEditorPath(editorId) {
  try {
    if (!appSettings) {
      await loadAppSettings();
    }
    const editorInfo = getEditorPathInfo(editorId);
    appSettings.editorPaths = appSettings.editorPaths || {};
    delete appSettings.editorPaths[editorId];
    await persistAppSettings();
    await Promise.all([refreshSettingsPanelData(), loadStatus()]);
    showToast(
      t("toastEditorPathReset", {
        name: editorInfo?.displayName || editorId,
      }),
      "success"
    );
  } catch (error) {
    showToast(String(error), "error");
  }
}

function handleOpenEditorPath(editorId) {
  const input = getEditorPathInput(editorId);
  const editorInfo = getEditorPathInfo(editorId);
  const target = (input?.value || "").trim() || editorInfo?.settingsPath;
  if (!target) return;
  invoke("open_folder", { path: target }).catch((error) => {
    showToast(String(error), "error");
  });
}

async function openSettingsPanel() {
  // 设置统一进入左侧 Settings 页面，不再使用遮罩弹层作为主入口
  switchConsolePage("settings");
  try {
    await openSettingsInline();
    if (typeof refreshSettingsPanelData === "function") {
      await refreshSettingsPanelData();
    }
  } catch (e) {
    console.error("加载设置失败:", e);
    showToast("设置面板加载失败：" + String(e), "error");
  }
}

function closeSettingsPanel() {
  $("settingsOverlay").classList.remove("open");
}

async function handleSettingsToggle() {
  if (!appSettings) return;
  appSettings.autoStart = $("settingsAutoStart").checked;
  appSettings.minimizeToTray = $("settingsMinTray").checked;
  appSettings.silentStartup = $("settingsSilentStart").checked;
  syncAppSettingsAppearance();
  try {
    await invoke("save_app_settings", { settings: appSettings });
    showToast(t("toastSettingsSaved"), "success");
  } catch (e) {
    showToast(String(e), "error");
  }
}

async function handleExportProfiles() {
  try {
    const dialog = window.__TAURI_PLUGIN_DIALOG__;
    const dest = await dialog.save({
      defaultPath: "varswitch-profiles.json",
      filters: [{ name: "JSON", extensions: ["json"] }],
    });
    if (!dest) return;
    await invoke("export_profiles", { dest });
    showToast(t("toastExported"), "success");
  } catch (e) {
    showToast(String(e), "error");
  }
}

async function handleImportProfiles() {
  try {
    const dialog = window.__TAURI_PLUGIN_DIALOG__;
    const src = await dialog.open({
      filters: [{ name: "JSON", extensions: ["json"] }],
      multiple: false,
    });
    if (!src) return;
    const count = await invoke("import_profiles", { src });
    if (count > 0) {
      showToast(t("toastImported2", { count }), "success");
      await loadProfiles();
    } else {
      showToast(t("toastImportNone"), "warning");
    }
  } catch (e) {
    showToast(String(e), "error");
  }
}

// Settings event listeners
on("settingsClose", "click", closeSettingsPanel);
bindOverlayDismiss("settingsOverlay", closeSettingsPanel);
on("settingsAutoStart", "change", handleSettingsToggle);
on("settingsMinTray", "change", handleSettingsToggle);
on("settingsSilentStart", "change", handleSettingsToggle);
on("settingsOpenConfigDir", "click", () => {
  if (appPaths) invoke("open_folder", { path: appPaths.configDir });
});
on("settingsOpenClaudeDir", "click", () => {
  if (appPaths) invoke("open_folder", { path: appPaths.claudeSettings });
});
on("settingsOpenCodexDir", "click", () => {
  if (appPaths) invoke("open_folder", { path: appPaths.codexSettings });
});
on("settingsOpenLogsDir", "click", () => {
  invoke("open_logs_folder").catch((error) => showToast(String(error), "error"));
});
async function copySettingsPath(pathKey) {
  const path = pathKey === "codexSettings"
    ? appPaths?.codexDir || appPaths?.codexSettings
    : appPaths?.[pathKey];
  if (!path) return;
  try {
    await copyToolboxText(path);
    showToast(t("toastCopied"), "success");
  } catch (error) {
    showToast(String(error), "error");
  }
}
document.querySelectorAll("[data-settings-copy-path]").forEach((button) => {
  button.addEventListener("click", () => copySettingsPath(button.dataset.settingsCopyPath));
});
on("settingsExportBtn", "click", handleExportProfiles);
on("settingsImportBtn", "click", handleImportProfiles);
on("settingsOpenBackupsBtn", "click", () => {
  invoke("open_backups_folder").catch((error) => showToast(String(error), "error"));
});
on("settingsViewBackupsBtn", "click", toggleBackupList);
let backupRestoreBusy = false;
on("settingsBackupList", "click", async (event) => {
  const btn = event.target.closest("[data-restore-backup]");
  if (!btn || backupRestoreBusy) return;
  const name = btn.getAttribute("data-restore-backup");
  if (!(await appConfirm(currentLang === "zh" ? "确定用这个备份覆盖当前配置吗？当前配置会先自动备份。" : "Overwrite current profiles with this backup? A safety backup will be created first.", { title: currentLang === "zh" ? "恢复备份" : "Restore backup", danger: true, confirmText: currentLang === "zh" ? "覆盖恢复" : "Restore" }))) return;
  backupRestoreBusy = true;
  setButtonBusy(btn, true, currentLang === "zh" ? "恢复中..." : "Restoring...");
  try {
    await invoke("restore_config_backup", { name });
    showToast("已从备份恢复配置", "success");
    await loadProfiles();
    await loadCodexProfiles();
    $("settingsBackupList").style.display = "none";
  } catch (error) {
    showToast(String(error), "error");
  } finally {
    backupRestoreBusy = false;
    setButtonBusy(btn, false);
  }
});

// 把紧凑时间戳 20260624-143025 格式化为 2026-06-24 14:30:25
function formatBackupStamp(stamp) {
  const m = /^(\d{4})(\d{2})(\d{2})-(\d{2})(\d{2})(\d{2})$/.exec(stamp || "");
  if (!m) return stamp || "";
  return `${m[1]}-${m[2]}-${m[3]} ${m[4]}:${m[5]}:${m[6]}`;
}

// 展开/收起备份列表，每项可一键回滚
async function toggleBackupList() {
  const box = $("settingsBackupList");
  if (box.style.display !== "none") {
    box.style.display = "none";
    return;
  }
  try {
    const backups = await invoke("list_config_backups");
    if (!backups || backups.length === 0) {
      box.innerHTML = `<div class="mgmt-item-desc">暂无备份（首次切换配置后会自动生成）</div>`;
    } else {
      box.innerHTML = backups
        .map((b) => {
          const label = formatBackupStamp(b.stamp);
          const kindLabel = b.kind === "codex" ? "Codex" : b.kind === "grok" ? "Grok" : "Claude";
          return `<div class="settings-row">
            <div class="settings-row-info">
              <div class="settings-row-label">${kindLabel} · ${esc(label)}</div>
            </div>
            <div class="settings-row-action">
              <button class="btn btn-secondary btn-sm" data-restore-backup="${esc(b.name)}">恢复</button>
            </div>
          </div>`;
        })
        .join("");
    }
    box.style.display = "block";
  } catch (error) {
    showToast(String(error), "error");
  }
}

function withTimeout(promise, ms, label) {
  let timer = null;
  const timeout = new Promise((_, reject) => {
    timer = setTimeout(() => reject(new Error(`${label || "task"} timeout after ${ms}ms`)), ms);
  });
  return Promise.race([promise, timeout]).finally(() => {
    if (timer) clearTimeout(timer);
  });
}

async function safeLoad(label, fn, ms = 8000) {
  try {
    await withTimeout(Promise.resolve().then(fn), ms, label);
  } catch (error) {
    console.error(`[init] ${label} failed:`, error);
  }
}

(async function init() {
  const toolbar = document.querySelector(".toolbar");
  const appEl = document.querySelector(".app");
  const shell = document.querySelector("#workspaceShell");
  let revealed = false;

  function revealUi() {
    if (revealed) return;
    revealed = true;
    [toolbar, appEl, shell].forEach((el) => {
      if (!el) return;
      el.classList.remove("app-hidden");
      el.classList.add("app-reveal");
      el.style.pointerEvents = "auto";
      el.style.opacity = "1";
    });
    // 顶栏/主工作区始终可点
    if (toolbar) toolbar.style.pointerEvents = "auto";
    if (shell) shell.style.pointerEvents = "auto";
    const splash = $("splashScreen");
    if (splash) {
      splash.classList.add("fade-out");
      splash.style.pointerEvents = "none";
      setTimeout(() => splash.remove(), 400);
    }
  }

  // 1) 主题/文案：失败也不阻断
  try { applyTheme(); } catch (e) { console.error("applyTheme failed", e); }
  try { applyLanguage(); } catch (e) { console.error("applyLanguage failed", e); }

  // 2) 先绑定交互，保证侧边栏/按钮立刻可点
  try {
    bindAppDialogOnce();
    bindConsoleUiOnce();
    mountToolboxPages();
    switchConsolePage("add-provider");
    // B15: 尽早订阅后端通道状态推送，绑定流程中的状态变化不会漏收
    bindMobileChannelStatusListener();
  } catch (e) {
    console.error("console bind failed", e);
  }

  // 3) 启动动画：短隐藏 + 强制超时显示
  if (toolbar) toolbar.classList.add("app-hidden");
  if (appEl) appEl.classList.add("app-hidden");
  if (shell) shell.classList.add("app-hidden");
  const forceRevealTimer = setTimeout(revealUi, 1200);

  // 4) 数据加载互不阻塞，单项超时
  await Promise.allSettled([
    safeLoad("loadStatus", () => loadStatus()),
    safeLoad("loadProfiles", () => loadProfiles()),
    safeLoad("loadCodexProfiles", () => loadCodexProfiles()),
    safeLoad("loadCodexStatus", () => loadCodexStatus()),
    safeLoad("loadGrokProfiles", () => loadGrokProfiles()),
    safeLoad("loadGrokStatus", () => loadGrokStatus()),
    safeLoad("loadGrokDiagnostics", () => loadGrokDiagnostics()),
    safeLoad("loadGeminiProfiles", () => loadGeminiProfiles()),
    safeLoad("loadGeminiStatus", () => loadGeminiStatus()),
    safeLoad("loadCodexToolbox", () => loadCodexToolbox(), 10000),
    safeLoad("loadAppSettings", () => loadAppSettings()),
  ]);

  try {
    renderGrokPresetOptions();
    renderUpdateButton();
    enhanceAfterDataLoad();
    switchConsolePage("add-provider");
  } catch (e) {
    console.error("post-load render failed", e);
  }

  clearTimeout(forceRevealTimer);
  revealUi();

  try {
    checkForUpdatesOnStartup();
  } catch (e) {
    console.error("update check failed", e);
  }

  setTimeout(() => {
    maybeOpenUsageGuide().catch(() => {});
  }, 250);
})();
