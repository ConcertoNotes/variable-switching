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

function mobileDebug(label, payload = {}) {
  console.log(`[mobile-control] ${label}`, {
    selectedMobileChannel,
    selectedCodexThreadId,
    ...payload,
  });
}

function mobileDebugError(label, error, payload = {}) {
  console.error(`[mobile-control] ${label}`, {
    selectedMobileChannel,
    selectedCodexThreadId,
    error,
    ...payload,
  });
}

const LANG_STORAGE_KEY = "varswitch.lang";
const THEME_STORAGE_KEY = "varswitch.theme";
const APP_REPOSITORY_URL = "https://github.com/ConcertoNotes/variable-switching";
const APP_DOWNLOAD_PAGE_URL = "https://download.varswitch.strova.top/";
const CODEX_PRESETS = [
  {
    id: "openai",
    name: "OpenAI Responses",
    baseUrl: "https://api.openai.com/v1",
    model: "gpt-5.5",
    providerName: "custom",
  },
  {
    id: "deepseek",
    name: "DeepSeek",
    baseUrl: "https://api.deepseek.com",
    model: "deepseek-v4-flash",
    providerName: "deepseek",
  },
  {
    id: "kimi",
    name: "Kimi",
    baseUrl: "https://api.moonshot.cn/v1",
    model: "kimi-k2.6",
    providerName: "kimi",
  },
  {
    id: "zhipu_glm",
    name: "Zhipu GLM",
    baseUrl: "https://open.bigmodel.cn/api/coding/paas/v4",
    model: "glm-5.1",
    providerName: "zhipu_glm",
  },
  {
    id: "minimax",
    name: "MiniMax",
    baseUrl: "https://api.minimaxi.com/v1",
    model: "MiniMax-M2.7",
    providerName: "minimax",
  },
  {
    id: "siliconflow",
    name: "SiliconFlow",
    baseUrl: "https://api.siliconflow.cn/v1",
    model: "Pro/MiniMaxAI/MiniMax-M2.7",
    providerName: "siliconflow",
  },
  {
    id: "openrouter",
    name: "OpenRouter",
    baseUrl: "https://openrouter.ai/api/v1",
    model: "openai/gpt-5.3-codex",
    providerName: "openrouter",
  },
];

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

const I18N = {
  en: {
    appTitle: "VarSwitch",
    appSubtitle: "Environment Sync Manager",
    importBtn: "Import Current",
    addBtn: "+ Add Config",
    statusTitle: "Current Status",
    statusHint: "Restart terminal and editor after switching to apply environment variables.",
    profilesTitle: "Config List",
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
    codexPageTab: "Codex CLI",
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
    codexImageSectionTitle: "GPT Image 2",
    codexImageSectionHint: "Optional image-only API. It is written to the same ~/.codex/config.toml used by Codex.",
    codexImageApiKeyLabel: "Image API Key",
    codexImageBaseUrlLabel: "Image Base URL",
    codexAuthModeLabel: "Write Mode",
    codexAuthModeDefaultTitle: "Default write",
    codexAuthModeDefaultHint: "~/.codex/auth.json + ~/.codex/config.toml",
    codexAuthModeOfficialTitle: "Official account login, API quota",
    codexAuthModeOfficialHint: "Only write ~/.codex/config.toml",
    codexOfficialConfigLabel: "Official account API quota config",
    copy: "Copy",
    codexActiveConfigLabel: "Active Codex Config",
    codexSwitching: "Writing Codex configuration...",
    codexSwitchedTo: "Codex switched to {name}",
    codexImportPrompt: "Name for the imported Codex config:",
    codexImportDefaultName: "Current Codex Config",
    codexToastImported: "Current Codex config imported",
    codexToastAdded: "Codex config added",
    codexToastUpdated: "Codex config updated",
    codexToastDeleted: "Codex config deleted",
    codexNoConfigsTitle: "No Codex configs yet",
    codexNoConfigsDesc: "Create a config to sync Codex CLI settings in one click.",
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
    grokAddFirstConfig: "Add your first Grok config",
    codexToolbox: "Toolbox",
    codexToolboxTitle: "Codex Toolbox",
    toolboxTabMarket: "Plugin Market",
    toolboxTabSession: "Session Sync",
    toolboxTabRemote: "Mobile Control",
    toolboxMarketHint: "Choose a plugin marketplace and install it into Codex. Other marketplace entries are removed when applying.",
    toolboxMarketInputLabel: "Plugin Marketplace",
    toolboxMarketApply: "Install to Codex",
    toolboxMarketplaceInstalling: "Installing plugin marketplace...",
    toolboxMarketplaceProgressPrepare: "Preparing Codex config...",
    toolboxMarketplaceProgressInstall: "Installing marketplace with Codex CLI...",
    toolboxMarketplaceProgressVerify: "Verifying marketplace...",
    toolboxMarketplaceProgressDone: "Installation complete",
    toolboxCurrentMarket: "Current",
    toolboxMarketType: "Type",
    toolboxMarketSource: "Source",
    toolboxMarketRemove: "Remove",
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
    toolboxMarketplaceApplied: "Plugin marketplace installed",
    toolboxMarketplaceRemoved: "Plugin marketplace removed",
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
    placeholderBaseUrl: "https://api.example.com",
    modelIdLabel: "Model ID",
    placeholderModelId: "e.g. opus, sonnet",
    modelIdHint: "Optional. Sets model in editor and Claude settings.",
    endpointTest: "Test Speed",
    endpointTesting: "Testing...",
    endpointUse: "Use",
    endpointEmpty: "Enter a Base URL first.",
    endpointFailed: "Failed",
    endpointNoResults: "No endpoint results",
    endpointSelected: "Endpoint selected",
    modelFetch: "Fetch Models",
    modelFetching: "Fetching...",
    modelFetchMissing: "Enter Base URL and API Key first.",
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
    usageGuideIntro: "VarSwitch centralizes Claude Code, Codex CLI, plugins, prompts, MCP servers, mobile control, settings, and backups.",
    usageGuideStep1Title: "Claude Code configs",
    usageGuideStep1Desc: "Add or import Token/Base URL configs, test endpoint speed, then switch to sync system env, supported editors, and ~/.claude/settings.json.",
    usageGuideStep2Title: "Codex CLI configs",
    usageGuideStep2Desc: "Manage Codex API Key, Base URL, model, provider, auth mode, diagnostics, and backups for ~/.codex/config.toml and auth.json.",
    usageGuideStep3Title: "Codex Toolbox",
    usageGuideStep3Desc: "Install plugin marketplaces, repair bundled plugins, enable important plugins, sync Codex sessions, and bind mobile control.",
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
    statusTitle: "当前配置状态",
    statusHint: "切换后请重启终端和编辑器，使环境变量生效。",
    profilesTitle: "配置列表",
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
    codexPageTab: "Codex CLI",
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
    codexImageSectionTitle: "GPT Image 2",
    codexImageSectionHint: "可选的图片专用 API，会写入 Codex 使用的同一份 ~/.codex/config.toml。",
    codexImageApiKeyLabel: "图片 API Key",
    codexImageBaseUrlLabel: "图片 Base URL",
    codexAuthModeLabel: "写入方式",
    codexAuthModeDefaultTitle: "默认写入",
    codexAuthModeDefaultHint: "~/.codex/auth.json + ~/.codex/config.toml",
    codexAuthModeOfficialTitle: "官方账号登录，api额度消耗",
    codexAuthModeOfficialHint: "只写 ~/.codex/config.toml",
    codexOfficialConfigLabel: "官方账号登录，api额度消耗配置",
    copy: "复制",
    codexActiveConfigLabel: "当前 Codex 配置",
    codexSwitching: "正在写入 Codex 配置...",
    codexSwitchedTo: "Codex 已切换到 {name}",
    codexImportPrompt: "请输入导入的 Codex 配置名称：",
    codexImportDefaultName: "当前 Codex 配置",
    codexToastImported: "当前 Codex 配置已导入",
    codexToastAdded: "Codex 配置已添加",
    codexToastUpdated: "Codex 配置已更新",
    codexToastDeleted: "Codex 配置已删除",
    codexNoConfigsTitle: "暂无 Codex 配置",
    codexNoConfigsDesc: "创建一个配置，一键同步 Codex CLI 设置。",
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
    grokAddFirstConfig: "添加第一个 Grok 配置",
    codexToolbox: "工具箱",
    codexToolboxTitle: "Codex 工具箱",
    toolboxTabMarket: "插件市场",
    toolboxTabSession: "会话同步",
    toolboxTabRemote: "手机控制",
    toolboxMarketHint: "选择一个插件市场并安装到 Codex；安装时会移除其它插件市场源。",
    toolboxMarketInputLabel: "插件市场",
    toolboxMarketApply: "安装到 Codex",
    toolboxMarketplaceInstalling: "正在安装插件市场...",
    toolboxMarketplaceProgressPrepare: "正在准备 Codex 配置...",
    toolboxMarketplaceProgressInstall: "正在通过 Codex CLI 安装插件市场...",
    toolboxMarketplaceProgressVerify: "正在校验插件市场...",
    toolboxMarketplaceProgressDone: "安装完成",
    toolboxCurrentMarket: "当前",
    toolboxMarketType: "类型",
    toolboxMarketSource: "地址",
    toolboxMarketRemove: "移除",
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
    toolboxMarketplaceApplied: "插件市场已安装",
    toolboxMarketplaceRemoved: "插件市场已移除",
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
    placeholderBaseUrl: "https://api.example.com",
    modelIdLabel: "模型 ID",
    placeholderModelId: "如 opus, sonnet",
    modelIdHint: "可选。设置编辑器和 Claude 系统设置中的模型。",
    endpointTest: "测速",
    endpointTesting: "测速中...",
    endpointUse: "使用",
    endpointEmpty: "请先输入 Base URL。",
    endpointFailed: "失败",
    endpointNoResults: "暂无测速结果",
    endpointSelected: "已选择端点",
    modelFetch: "获取模型",
    modelFetching: "获取中...",
    modelFetchMissing: "请先填写 Base URL 和 API Key。",
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
    usageGuideIntro: "VarSwitch 可集中管理 Claude Code、Codex CLI、插件市场、提示词、MCP Server、移动端控制、设置与备份。",
    usageGuideStep1Title: "Claude Code 配置",
    usageGuideStep1Desc: "添加或导入 Token/Base URL 配置，可测试接口速度；切换后会同步系统环境变量、已支持编辑器和 ~/.claude/settings.json。",
    usageGuideStep2Title: "Codex CLI 配置",
    usageGuideStep2Desc: "管理 Codex API Key、Base URL、模型、Provider 和写入方式，并提供诊断与 ~/.codex/config.toml、auth.json 备份。",
    usageGuideStep3Title: "Codex Toolbox",
    usageGuideStep3Desc: "安装插件市场、修复内置插件、启用关键插件、同步 Codex 会话，并绑定飞书/Lark、QQ、微信移动端控制。",
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

let currentLang = localStorage.getItem(LANG_STORAGE_KEY) || "en";
if (!I18N[currentLang]) {
  currentLang = "en";
}

let currentTheme = localStorage.getItem(THEME_STORAGE_KEY) || "light";
if (currentTheme !== "light" && currentTheme !== "dark") {
  currentTheme = "light";
}

let profiles = [];
let codexProfiles = [];
let grokProfiles = [];
let grokDiagnostics = null;
let codexToolbox = null;
let codexDiagnostics = null;
let currentPage = "overview";
let activeConsolePage = "overview";
let codexWizardStep = 1;
let codexWizardEnableAfterSave = false;
let overviewEvents = [];
let configurationFilter = "all";
let configurationSearch = "";
let pluginFilter = "all";
let pluginSearch = "";
let codexDiagExpanded = false;
let lastClaudeStatus = null;
let lastCodexStatus = null;
let lastGrokStatus = null;
let editingGrokId = null;
let detectedEditors = {}; // { id: displayName }
let editingId = null;
let switchingSnapshot = null;
let progressUnlisten = null;
let isSwitchingProfile = false;
let skillsData = [];
let editingSkillName = null;
let mcpServers = {};
let editingMcpName = null;
let discoverSkills = [];
let skillRepos = [];
let activeSkillsTab = "installed";
let discoverSearchQuery = "";
let discoverRepoFilter = "all";
let discoverStatusFilter = "all";
let editorCarouselIndex = 0;
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
let activeToolboxTab = "market";
let selectedCodexThreadId = "";
let selectedMobileChannel = "wechat";
let toolboxRefreshTimer = null;
let toolboxRefreshBusy = false;
let larkCredentialSaveTimer = null;
let toolboxRemoteBusy = false;
let marketplaceInstallBusy = false;
let marketplaceProgressUnlisten = null;
let toolboxSessionSyncBusy = false;
let toolboxSessionProgressTimer = null;
let toolboxSessionProgressValue = 0;
let toolboxSessionSearchQuery = "";
let toolboxSelectedSessionIds = new Set();
let toolboxSelectedTrashSessionIds = new Set();
let toolboxSessionTrashOpen = false;
let toolboxCopiedSessionId = "";
let mobileBindBusyAction = "";

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
      <svg viewBox="0 0 64 64" fill="none" xmlns="http://www.w3.org/2000/svg">
        <path d="M38.7 10h-8.9L13 54h8.5l3.6-9.7h18.1L46.9 54H56L38.7 10Zm-11 27.2 6.4-17.1 6.5 17.1H27.7Z" fill="currentColor"/>
      </svg>
    </span>`;
  }
  if (kind === "codex") {
    return `<span class="product-icon product-icon-codex" aria-hidden="true">
      <svg viewBox="0 0 64 64" fill="none" xmlns="http://www.w3.org/2000/svg">
        <path d="M32 6 54.5 19v26L32 58 9.5 45V19L32 6Z" stroke="currentColor" stroke-width="5" stroke-linejoin="round"/>
        <path d="M22 25.5 32 19l10 6.5v13L32 45l-10-6.5v-13Z" stroke="currentColor" stroke-width="4" stroke-linejoin="round"/>
        <path d="M32 19v26M22 25.5l20 13M42 25.5l-20 13" stroke="currentColor" stroke-width="3" stroke-linecap="round"/>
      </svg>
    </span>`;
  }
  if (kind === "grok") {
    return `<span class="product-icon product-icon-grok" aria-hidden="true">
      <img src="grok-color.svg" width="16" height="16" alt="">
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

async function downloadAndOpenUpdate() {
  return installAppUpdate();
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

function updateThemeSegControl() {
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
  setText("statusSectionTitle", t("statusTitle"));
  setText("statusHint", t("statusHint"));
  setText("profilesSectionTitle", t("profilesTitle"));
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

  setPlaceholder("profileName", t("placeholderName"));
  setPlaceholder("profileApiKey", t("placeholderApiKey"));
  setPlaceholder("profileBaseUrl", t("placeholderBaseUrl"));
  setText("profileModelIdLabel", t("modelIdLabel"));
  setPlaceholder("profileModelId", t("placeholderModelId"));
  setText("profileModelIdHint", t("modelIdHint"));

  // Codex page labels
  setText("codexStatusSectionTitle", t("codexStatusTitle"));
  setText("codexProfilesSectionTitle", t("codexProfilesTitle"));
  setText("codexActiveConfigLabel", t("codexActiveConfigLabel"));
  setText("codexCardSyncBtn", t("syncNow"));
  setText("codexPageImportBtn", t("importBtn"));
  setText("codexPresetLabel", t("codexPresetLabel"));
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
  if ($("grokStatusSectionTitle")) setText("grokStatusSectionTitle", t("grokStatusTitle"));
  if ($("grokProfilesSectionTitle")) setText("grokProfilesSectionTitle", t("grokProfilesTitle"));
  if ($("grokActiveConfigLabel")) setText("grokActiveConfigLabel", t("grokActiveConfigLabel"));
  if ($("grokSyncNowBtnText")) setText("grokSyncNowBtnText", t("syncNow"));
  if ($("grokStatusHint")) setText("grokStatusHint", t("grokStatusHint"));
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
  if ($("grokRefreshBtn")) setText("grokRefreshBtn", currentLang === "zh" ? "刷新状态" : "Refresh");
  if ($("grokOpenFolderBtn")) setText("grokOpenFolderBtn", t("grokOpenFolder"));
  if ($("grokBackupRuntimeBtn")) setText("grokBackupRuntimeBtn", t("grokBackupRuntime"));
  if ($("grokPageImportBtn")) setText("grokPageImportBtn", t("grokImportCurrent"));
  if ($("grokProfileName")) setPlaceholder("grokProfileName", t("placeholderName"));
  if ($("grokApiKey")) setPlaceholder("grokApiKey", "xai-...");
  if ($("grokBaseUrl")) setPlaceholder("grokBaseUrl", "https://api.x.ai/v1");
  if ($("grokModel")) setPlaceholder("grokModel", "e.g. grok-4");
  renderGrokPresetOptions();
  const codexToolboxBtn = $("codexToolboxBtn");
  if (codexToolboxBtn) codexToolboxBtn.textContent = t("codexToolbox");
  setText("codexToolboxTitle", t("codexToolboxTitle"));
  setText("toolboxTabMarket", t("toolboxTabMarket"));
  setText("toolboxTabSession", t("toolboxTabSession"));
  setText("toolboxTabRemote", t("toolboxTabRemote"));
  setText("toolboxMarketHint", t("toolboxMarketHint"));
  setText("toolboxMarketInputLabel", t("toolboxMarketInputLabel"));
  setText("toolboxMarketApplyBtn", t("toolboxMarketApply"));
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
  if (codexToolbox) {
    renderCodexToolbox();
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
    if (typeof renderOverviewDashboard === "function") renderOverviewDashboard();
    if (typeof applyPluginFilters === "function") applyPluginFilters();
  } catch (error) {
    console.error("loadCodexToolbox failed:", error);
    if (typeof showToast === "function") showToast(String(error), "error");
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
  $("toolboxTabMarket")?.classList.toggle("active", tab === "market");
  $("toolboxTabSession")?.classList.toggle("active", tab === "session");
  $("toolboxTabRemote")?.classList.toggle("active", tab === "remote");
  // 控制台一级页面会把内容挂到 host 中，始终显示对应区块
  const market = $("toolboxMarketContent");
  const session = $("toolboxSessionContent");
  const remote = $("toolboxRemoteContent");
  if (market) market.style.display = tab === "market" || activeConsolePage === "plugins" ? "" : "none";
  if (session) session.style.display = tab === "session" || activeConsolePage === "sessions" ? "" : "none";
  if (remote) remote.style.display = tab === "remote" || activeConsolePage === "mobile" ? "" : "none";
  // 在独立页面中强制显示
  if (activeConsolePage === "plugins" && market) market.style.display = "";
  if (activeConsolePage === "sessions" && session) session.style.display = "";
  if (activeConsolePage === "mobile" && remote) remote.style.display = "";
}

function openCodexToolbox() {
  // Toolbox 已拆分为一级导航页面，默认进入插件市场
  const tab = activeToolboxTab || "market";
  if (tab === "session") switchConsolePage("sessions");
  else if (tab === "remote") switchConsolePage("mobile");
  else switchConsolePage("plugins");
  loadCodexToolbox();
}

function closeCodexToolbox() {
  $("codexToolboxOverlay").classList.remove("open");
  stopToolboxRefresh();
}

function startToolboxRefresh(ticks = 8) {
  stopToolboxRefresh();
  let remaining = ticks;
  toolboxRefreshTimer = setInterval(async () => {
    if (toolboxRefreshBusy) return;
    if (!$("codexToolboxOverlay").classList.contains("open") || remaining <= 0) {
      stopToolboxRefresh();
      return;
    }
    remaining -= 1;
    toolboxRefreshBusy = true;
    try {
      const lark = codexToolbox?.mobileChannels?.find((binding) => binding.channel === "lark");
      const wechat = codexToolbox?.mobileChannels?.find((binding) => binding.channel === "wechat");
      if (lark?.qrDeviceCode && (!lark?.appId || !lark?.appSecret)) {
        codexToolbox = await invoke("poll_lark_bot_registration", {});
      } else if (wechat?.qrDeviceCode && !wechat?.botToken) {
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
  const marketplaceValue =
    helpers.normalizeCodexPluginMarketplaceInput?.(codexToolbox.pluginMarketplaceInput) ||
    helpers.CODEX_PLUGIN_MARKETPLACE_URL ||
    "https://gitcode.com/2301_79703673/codex-plugins.git";
  const marketplaceSelect = $("toolboxMarketplaceInput");
  const marketplaces = helpers.CODEX_PLUGIN_MARKETPLACES || [
    {
      name: "VarSwitch 插件合集",
      url: marketplaceValue,
      count: 189,
      zh: "覆盖官方常用服务和桌面能力，适合默认安装。",
      en: "Broad official-style service and desktop coverage; best default choice.",
    },
  ];
  marketplaceSelect.innerHTML = marketplaces
    .map((item) => {
      const label = `${item.name} · ${item.count || "--"} plugins`;
      return `<option value="${esc(item.url)}">${esc(label)}</option>`;
    })
    .join("");
  marketplaceSelect.value = marketplaceValue;
  const option =
    helpers.getCodexPluginMarketplaceOption?.(marketplaceValue) ||
    marketplaces.find((item) => item.url === marketplaceValue) ||
    marketplaces[0];
  $("toolboxMarketplaceDesc").textContent =
    currentLang === "zh" ? option.zh || "" : option.en || option.zh || "";
  renderBuiltinPlugins();
  renderToolboxSessionSync();
  renderToolboxSyncedThreads();
  renderToolboxMobileControl();
  renderToolboxRemote();
  applyPluginFilters();
}

function builtinPluginStatusText(status) {
  if (!status?.available) {
    return status?.lastError || "未找到 Codex App 自带插件目录";
  }
  const market = status.marketplaceConfigured ? "已挂载 openai-bundled" : "未挂载 openai-bundled";
  const important = `${status.importantEnabledCount || 0}/${status.importantTotalCount || 0} 个关键插件已启用`;
  const total = `${status.enabledCount || 0}/${status.totalCount || 0} 个内置插件已启用`;
  return `${market} · ${important} · ${total}`;
}

function renderBuiltinPlugins() {
  const summary = $("builtinPluginSummary");
  const list = $("builtinPluginList");
  if (!summary || !list) return;
  const status = codexToolbox?.builtinPlugins || {};
  summary.textContent = builtinPluginStatusText(status);
  if (!status.available) {
    list.innerHTML = `<div class="builtin-plugin-empty">${esc(status.lastError || "未发现 Codex App 内置插件")}</div>`;
    return;
  }
  const plugins = status.plugins || [];
  if (plugins.length === 0) {
    list.innerHTML = `<div class="builtin-plugin-empty">没有可展示的内置插件</div>`;
    return;
  }
  list.innerHTML = plugins.map((plugin) => {
    const skills = (plugin.skills || []).slice(0, 4).map((skill) => `<span>${esc(skill.name)}</span>`).join("");
    return `<div class="builtin-plugin-card ${plugin.important ? "important" : ""}">
      <div class="builtin-plugin-card-main">
        <div class="builtin-plugin-title-row">
          <strong>${esc(plugin.displayName || plugin.name)}</strong>
          ${plugin.important ? `<span class="builtin-plugin-badge">关键</span>` : ""}
          ${plugin.enabled ? `<span class="builtin-plugin-badge enabled">已启用</span>` : ""}
        </div>
        <div class="builtin-plugin-desc">${esc(plugin.description || plugin.id)}</div>
        <div class="builtin-plugin-skills">${skills}</div>
      </div>
      <div class="builtin-plugin-card-actions">
        <button class="btn ${plugin.enabled ? "btn-secondary" : "btn-primary"} btn-sm" data-enable-builtin-plugin="${esc(plugin.id)}" ${plugin.enabled ? "disabled" : ""} type="button">
          ${plugin.enabled ? "已启用" : "启用"}
        </button>
      </div>
    </div>`;
  }).join("");
  list.querySelectorAll("[data-enable-builtin-plugin]").forEach((btn) => {
    btn.addEventListener("click", async () => {
      const pluginId = btn.getAttribute("data-enable-builtin-plugin");
      if (!pluginId) return;
      btn.disabled = true;
      try {
        codexToolbox = await invoke("enable_codex_builtin_plugin", { pluginId });
        showToast("Codex 内置插件已启用", "success");
        renderCodexToolbox();
        await loadCodexDiagnostics();
      } catch (error) {
        showToast(String(error), "error");
        btn.disabled = false;
      }
    });
  });
}

async function handleRepairOpenAiBundledPlugins() {
  const btn = $("builtinPluginRepairBtn");
  if (btn) btn.disabled = true;
  try {
    codexToolbox = await invoke("repair_openai_bundled_plugins");
    showToast("已修复 Codex 内置插件市场", "success");
    renderCodexToolbox();
    await loadCodexDiagnostics();
  } catch (error) {
    showToast(String(error), "error");
  } finally {
    if (btn) btn.disabled = false;
  }
}

async function handleEnableImportantBuiltinPlugins() {
  const btn = $("builtinPluginEnableImportantBtn");
  if (btn) btn.disabled = true;
  try {
    codexToolbox = await invoke("enable_important_codex_builtin_plugins");
    showToast("Computer Use / Chrome 等关键插件已启用", "success");
    renderCodexToolbox();
    await loadCodexDiagnostics();
  } catch (error) {
    showToast(String(error), "error");
  } finally {
    if (btn) btn.disabled = false;
  }
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

  appSelect.innerHTML = [
    `<option value="wechat">${channelLabel("wechat")}</option>`,
    `<option value="qq">${channelLabel("qq")}</option>`,
    `<option value="lark">${channelLabel("lark")}</option>`,
  ].join("");
  appSelect.value = selectedMobileChannel;

  if (codexToolbox?.selectedMobileThreadId) {
    selectedCodexThreadId = codexToolbox.selectedMobileThreadId;
  } else if (!selectedCodexThreadId && threads[0]) {
    selectedCodexThreadId = threads[0].id;
  }
  threadSelect.innerHTML = threads.length
    ? threads.map((thread) => `<option value="${esc(thread.id)}">${esc(thread.threadName || thread.lastUserMessage || thread.id)}</option>`).join("")
    : `<option value="">${t("toolboxMobileNoThreadOption")}</option>`;
  threadSelect.value = threads.some((thread) => thread.id === selectedCodexThreadId) ? selectedCodexThreadId : "";

  $("toolboxMobileBindPanel").innerHTML = renderSelectedMobileBinding(getSelectedMobileBinding());
  bindMobileControlEvents();
}

function bindMobileControlEvents() {
  $("toolboxMobileAppSelect").onchange = () => {
    selectedMobileChannel = $("toolboxMobileAppSelect").value || "wechat";
    renderCodexToolbox();
  };

  $("toolboxMobileThreadSelect").onchange = async () => {
    selectedCodexThreadId = $("toolboxMobileThreadSelect").value || "";
    if (!selectedCodexThreadId) return;
    try {
      await bindCurrentMobileSelection();
      showToast(t("toolboxThreadSelected"), "success");
      renderCodexToolbox();
    } catch (error) {
      showToast(String(error), "error");
    }
  };

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
  mobileBindBusyAction = action || "";
  setButtonBusy(btn, true, getMobileBindingLoadingText(selectedMobileChannel, mobileBindBusyAction));
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
      invoke("start_qq_qr_binding", {})
        .then((snapshot) => {
          codexToolbox = snapshot;
          renderCodexToolbox();
          startToolboxRefresh(120);
        })
        .catch((error) => {
          mobileDebugError("start_qq_qr_binding invoke:failed", error);
          showToast(String(error), "error");
          loadCodexToolbox();
        });
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
  } finally {
    mobileBindBusyAction = "";
    setButtonBusy(btn, false);
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
  let body = "";
  let actions = "";

  if (channel === "wechat") {
    body = `${qr}${loading}<div class="mgmt-item-desc">${esc(status.detail || "点击绑定微信后，用手机微信扫码并确认绑定。")}</div>`;
    actions = `
      <button class="btn btn-primary btn-sm" data-action="toolbox-start-wechat-qr">${t("toolboxStartWechatQr")}</button>
      <button class="btn btn-secondary btn-sm danger-text" data-action="toolbox-clear-wechat-binding">清除绑定</button>
    `;
  } else if (channel === "qq") {
    body = `${qr}${loading}<div class="mgmt-item-desc">${esc(status.detail || "点击 QQ 扫码绑定后，用 QQ 扫描二维码即可保存机器人凭据。")}</div>`;
    actions = `
      <button class="btn btn-primary btn-sm" data-action="toolbox-start-qq-qr">${t("toolboxStartQqQr")}</button>
      <button class="btn btn-secondary btn-sm danger-text" data-action="toolbox-clear-qq-binding">清除绑定</button>
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
          <input type="password" data-channel-field="appSecret" autocomplete="off" value="${esc(binding.appSecret || "")}">
        </label>
      </div>
    `;
    actions = `
      <button class="btn btn-success btn-sm" data-action="toolbox-open-lark-existing">${t("toolboxRebindLarkBot")}</button>
      <button class="btn btn-secondary btn-sm" data-action="toolbox-open-lark-create">${t("toolboxCreateLarkBot")}</button>
      <button class="btn btn-secondary btn-sm danger-text" data-action="toolbox-unbind-lark-session">${t("toolboxUnbindLarkSession")}</button>
      <button class="btn btn-secondary btn-sm danger-text" data-action="toolbox-stop-lark-listen">${t("toolboxStopLarkListen")}</button>
      <button class="btn btn-secondary btn-sm danger-text" data-action="toolbox-clear-lark-binding">${t("toolboxClearLarkBinding")}</button>
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
      return `<div class="smart-control-approval-card">
        <div class="smart-control-approval-card-title">${esc(approval.title || approval.method || "Approval")}</div>
        <div class="smart-control-approval-card-body">${esc(approval.body || approval.rawPreview || "")}</div>
        <div class="smart-control-approval-actions">
          ${options.map((option) => `<button class="btn btn-secondary btn-sm" type="button" data-approval-id="${esc(approval.requestId || "")}" data-approval-decision="${esc(option)}">${esc(option)}</button>`).join("")}
        </div>
      </div>`;
    }).join("")}`;
  panel.querySelectorAll("[data-approval-id]").forEach((btn) => {
    btn.addEventListener("click", async () => {
      const requestId = btn.getAttribute("data-approval-id");
      const decision = btn.getAttribute("data-approval-decision");
      if (!requestId || !decision) return;
      btn.disabled = true;
      try {
        await invoke("submit_smart_control_approval", { requestId, decision });
        showToast(t("toolboxSmartControlApprovalSubmitted"), "success");
        await loadSmartControlDebug(true);
        renderSmartControlApprovals();
      } catch (error) {
        showToast(String(error), "error");
        btn.disabled = false;
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

function marketplaceProgressLabel(label, step) {
  const labelMap = {
    prepare: t("toolboxMarketplaceProgressPrepare"),
    install: t("toolboxMarketplaceProgressInstall"),
    verify: t("toolboxMarketplaceProgressVerify"),
    done: t("toolboxMarketplaceProgressDone"),
  };
  if (label && labelMap[label]) return labelMap[label];
  if (step <= 1) return t("toolboxMarketplaceProgressPrepare");
  if (step === 2) return t("toolboxMarketplaceProgressInstall");
  if (step === 3) return t("toolboxMarketplaceProgressVerify");
  return t("toolboxMarketplaceProgressDone");
}

function showMarketplaceProgress() {
  const progress = $("toolboxMarketProgress");
  progress.hidden = false;
  $("toolboxMarketProgressBar").style.width = "0%";
  $("toolboxMarketProgressPercent").textContent = "0%";
  $("toolboxMarketProgressLabel").textContent = t("toolboxMarketplaceProgressPrepare");
}

function updateMarketplaceProgress(payload) {
  const step = Math.max(1, Number(payload?.step || 1));
  const total = Math.max(1, Number(payload?.total || 4));
  const pct = Math.min(100, Math.round((step / total) * 100));
  $("toolboxMarketProgressBar").style.width = `${pct}%`;
  $("toolboxMarketProgressPercent").textContent = `${pct}%`;
  $("toolboxMarketProgressLabel").textContent = marketplaceProgressLabel(payload?.label, step);
}

function setMarketplaceInstallBusy(isBusy) {
  marketplaceInstallBusy = isBusy;
  $("toolboxMarketApplyBtn").disabled = isBusy;
  $("toolboxMarketplaceInput").disabled = isBusy;
  $("toolboxMarketApplyBtn").textContent = isBusy
    ? t("toolboxMarketplaceInstalling")
    : t("toolboxMarketApply");
}

async function loadStatus() {
  try {
    // 同时获取状态和检测到的编辑器列表
    const [status, editors] = await Promise.all([
      invoke("get_status"),
      invoke("get_detected_editors"),
    ]);
    detectedEditors = editors || {};
    lastClaudeStatus = status;
    const grid = $("statusGrid");

    // 构建编辑器列表
    const editorLocations = [];
    for (const [editorId, displayName] of Object.entries(detectedEditors)) {
      editorLocations.push({
        key: `editor_${editorId}`,
        title: `${displayName} ${t("settingsOpen") === "打开" ? "设置" : "Settings"}`,
        data: (status.editors || {})[editorId] || null
      });
    }

    // 固定三列：系统环境变量、编辑器轮播、Claude
    const systemLoc = { key: "envVars", title: t("statusSystemEnv"), data: status.envVars };
    const claudeLoc = { key: "claude", title: t("statusClaude"), data: status.claude, icon: "anthropic" };

    // 计算同步状态
    const allLocations = [systemLoc, ...editorLocations, claudeLoc];
    const allKeys = allLocations.map((l) => l.data?.apiKey).filter(Boolean);
    const allUrls = allLocations.map((l) => l.data?.baseUrl).filter(Boolean);
    const synced = allKeys.length > 0 && new Set(allKeys).size <= 1 && new Set(allUrls).size <= 1;

    const COPY_ICON = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>`;

    // 渲染单张状态卡片
    function renderCard(loc, extraClass) {
      const item = loc.data;
      if (!item) {
        return `
          <div class="status-card error-card ${extraClass || ""}">
            <div class="status-card-title">
              <span class="status-card-title-text">${productIcon(loc.icon)}${loc.title}</span>
              <span class="status-badge-console error">${currentLang === "zh" ? "读取失败" : "Failed"}</span>
            </div>
            <div style="font-size:13px;color:var(--console-muted)">${t("readFailed")}</div>
            <div style="margin-top:10px;display:flex;gap:8px;">
              <button class="btn btn-secondary btn-sm" type="button" data-action="claude-fix">${currentLang === "zh" ? "修复" : "Fix"}</button>
              <button class="btn btn-ghost btn-sm" type="button" data-action="claude-detail">${currentLang === "zh" ? "查看详情" : "Details"}</button>
            </div>
          </div>`;
      }
      const badgeClass = synced ? "synced" : "unsynced";
      const badgeText = synced ? t("synced") : t("unsynced");
      const dotColor = synced ? "var(--success-text)" : "var(--warning-text)";
      return `
        <div class="status-card ${extraClass || ""}">
          <div class="status-card-title">
            <span class="status-card-title-text">${loc.title}</span>
            <span class="status-badge ${badgeClass}">
              <span style="width:6px;height:6px;border-radius:50%;background:${dotColor};flex-shrink:0;"></span>
              ${badgeText}
            </span>
          </div>
          <div class="status-item">
            <span class="status-label">${t("tokenLabel")}</span>
            <div class="status-value-wrapper">
              <span class="status-value">${maskKey(item.apiKey)}</span>
              <button class="copy-btn" type="button" data-copy="${esc(item.apiKey || "")}" title="Copy">${COPY_ICON}</button>
            </div>
          </div>
          <div class="status-item">
            <span class="status-label">${t("urlLabel")}</span>
            <div class="status-value-wrapper">
              <span class="status-value has-tooltip" data-tooltip="${esc(item.baseUrl || "")}">${truncUrl(item.baseUrl)}</span>
              <button class="copy-btn" type="button" data-copy="${esc(item.baseUrl || "")}" title="Copy">${COPY_ICON}</button>
            </div>
          </div>
        </div>`;
    }

    // 列1：系统环境变量
    let html = renderCard(systemLoc);

    // 列2：编辑器轮播
    if (editorLocations.length === 0) {
      html += `<div class="status-card" style="display:flex;align-items:center;justify-content:center;color:var(--text-muted);font-size:13px;">${t("stepEditors")}: --</div>`;
    } else if (editorLocations.length === 1) {
      html += `<div class="editor-carousel single">${renderCard(editorLocations[0])}</div>`;
    } else {
      if (editorCarouselIndex >= editorLocations.length) editorCarouselIndex = 0;
      const cards = editorLocations.map((loc, i) => {
        let cls = "carousel-hidden";
        if (i === editorCarouselIndex) cls = "carousel-active";
        else if (i === (editorCarouselIndex + 1) % editorLocations.length) cls = "carousel-next";
        return renderCard(loc, cls);
      }).join("");
      const dots = editorLocations.map((_, i) =>
        `<span class="carousel-dot ${i === editorCarouselIndex ? "active" : ""}"></span>`
      ).join("");
      html += `<div class="editor-carousel" id="editorCarousel">${cards}<div class="carousel-indicator">${dots}</div></div>`;
    }

    // 列3：Claude
    html += renderCard(claudeLoc);

    grid.innerHTML = html;

    // 绑定轮播点击
    const carousel = $("editorCarousel");
    if (carousel && editorLocations.length > 1) {
      carousel.addEventListener("click", () => {
        editorCarouselIndex = (editorCarouselIndex + 1) % editorLocations.length;
        const cards = carousel.querySelectorAll(".status-card");
        const dots = carousel.querySelectorAll(".carousel-dot");
        cards.forEach((card, i) => {
          card.classList.remove("carousel-active", "carousel-next", "carousel-hidden");
          if (i === editorCarouselIndex) card.classList.add("carousel-active");
          else if (i === (editorCarouselIndex + 1) % editorLocations.length) card.classList.add("carousel-next");
          else card.classList.add("carousel-hidden");
        });
        dots.forEach((dot, i) => {
          dot.classList.toggle("active", i === editorCarouselIndex);
        });
      });
    }

    // 绑定复制按钮
    grid.querySelectorAll(".copy-btn").forEach((btn) => {
      btn.addEventListener("click", (e) => {
        e.stopPropagation();
        const text = btn.getAttribute("data-copy");
        if (text) {
          navigator.clipboard.writeText(text).then(() => {
            showToast(t("toastCopied"), "success");
          });
        }
      });
    });

    // 错误卡片：修复 / 查看详情
    grid.querySelectorAll("button[data-action='claude-fix']").forEach((btn) => {
      btn.addEventListener("click", (e) => {
        e.stopPropagation();
        switchConsolePage("settings");
        showToast(currentLang === "zh" ? "请在设置中检查编辑器路径后重试同步" : "Check editor paths in Settings, then sync again", "warning");
      });
    });
    grid.querySelectorAll("button[data-action='claude-detail']").forEach((btn) => {
      btn.addEventListener("click", async (e) => {
        e.stopPropagation();
        await appConfirm(
          currentLang === "zh"
            ? "错误原因：配置文件读取失败。\n建议：确认 Cursor / 编辑器已安装，或在设置中手动指定配置路径。"
            : "Reason: failed to read editor settings.\nSuggestion: confirm the editor is installed or set a custom path in Settings.",
          {
            title: currentLang === "zh" ? "读取失败详情" : "Read failure details",
            confirmText: currentLang === "zh" ? "知道了" : "OK",
            cancelText: currentLang === "zh" ? "关闭" : "Close",
          }
        );
      });
    });

    // 环境状态矩阵（精简版）
    const matrix = $("claudeEnvMatrix");
    if (matrix) {
      const items = [
        { title: currentLang === "zh" ? "系统环境变量" : "System Env", ok: !!status.envVars, detail: status.envVars ? (currentLang === "zh" ? "已同步" : "Synced") : (currentLang === "zh" ? "读取失败" : "Failed") },
        { title: currentLang === "zh" ? "编辑器配置" : "Editor Config", ok: editorLocations.some((l) => l.data), detail: editorLocations.some((l) => l.data) ? (currentLang === "zh" ? "可读取" : "Readable") : (currentLang === "zh" ? "读取失败" : "Failed") },
        { title: currentLang === "zh" ? "Claude 设置" : "Claude Settings", ok: !!status.claude, detail: status.claude ? (currentLang === "zh" ? "已同步" : "Synced") : (currentLang === "zh" ? "读取失败" : "Failed") },
      ];
      matrix.innerHTML = items
        .map((item) => `<div class="env-matrix-item"><span class="status-badge-console ${item.ok ? "healthy" : "error"}">${item.ok ? (currentLang === "zh" ? "正常" : "OK") : (currentLang === "zh" ? "异常" : "Error")}</span><strong>${esc(item.title)}</strong><small>${esc(item.detail)}</small></div>`)
        .join("");
    }

    if (typeof renderOverviewDashboard === "function") renderOverviewDashboard();
  } catch (error) {
    console.error("loadStatus failed:", error);
    showToast(t("loadStatusFailed", { error: String(error) }), "error");
    if (typeof renderOverviewDashboard === "function") renderOverviewDashboard();
  }
}

async function loadProfiles() {
  try {
    const data = await invoke("get_profiles");
    profiles = data.profiles || [];
    renderProfiles();
    if (typeof renderOverviewDashboard === "function") renderOverviewDashboard();
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
          <span class="field-label">${t("tokenLabel")}</span>
          <span class="field-value">${maskKey(profile.apiKey)}</span>
        </div>
        <div class="profile-field">
          <span class="field-label">${t("urlLabel")}</span>
          <span class="field-value">${truncUrl(profile.baseUrl, 50)}</span>
        </div>
        ${profile.modelId ? `<div class="profile-field">
          <span class="field-label">${t("modelIdLabel")}</span>
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

  grid.querySelectorAll("button[data-action]").forEach((btn) => {
    const action = btn.getAttribute("data-action");
    const id = btn.getAttribute("data-id");
    if (!id) return;

    btn.addEventListener("click", () => {
      if (action === "switch") handleSwitch(id);
      if (action === "edit") handleEdit(id);
      if (action === "delete") handleDelete(id);
    });
  });

  updateActiveConfigBar();
}

function updateActiveConfigBar() {
  const section = $("activeConfigSection");
  const nameEl = $("activeConfigName");
  const activeProfile = profiles.find((p) => p.isActive);

  if (activeProfile) {
    nameEl.textContent = activeProfile.name;
    section.style.display = "";
  } else {
    section.style.display = "none";
  }
}

function handleSyncNow() {
  const activeProfile = profiles.find((p) => p.isActive);
  if (activeProfile) {
    handleSwitch(activeProfile.id);
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

function normalizeEndpointCandidate(url) {
  return String(url || "").trim().replace(/\/+$/, "");
}

function uniqueEndpointCandidates(values) {
  const seen = new Set();
  const result = [];
  for (const value of values) {
    const normalized = normalizeEndpointCandidate(value);
    if (!normalized || seen.has(normalized)) continue;
    seen.add(normalized);
    result.push(normalized);
  }
  return result;
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

function getEndpointCandidates(kind) {
  const inputId = productFieldMap(kind).baseUrl;
  return uniqueEndpointCandidates([$(inputId)?.value]);
}

function endpointLatencyClass(result) {
  if (typeof result.latency !== "number") return "failed";
  if (result.latency < 400) return "fast";
  if (result.latency >= 1000) return "slow";
  return "";
}

function endpointMetaText(result) {
  if (typeof result.latency === "number") {
    return `${Math.round(result.latency)}ms${result.status ? ` · ${result.status}` : ""}`;
  }
  return result.error || t("endpointFailed");
}

function renderEndpointResults(kind, results) {
  const fields = productFieldMap(kind);
  const inputId = fields.baseUrl;
  const resultsId = fields.endpointResults;
  const container = $(resultsId);
  if (!container) return;

  const sorted = results.slice().sort((a, b) => {
    const left = typeof a.latency === "number" ? a.latency : Number.POSITIVE_INFINITY;
    const right = typeof b.latency === "number" ? b.latency : Number.POSITIVE_INFINITY;
    return left === right ? a.url.localeCompare(b.url) : left - right;
  });

  if (sorted.length === 0) {
    container.innerHTML = `<div class="endpoint-row"><span class="endpoint-url">${t("endpointNoResults")}</span></div>`;
    container.classList.add("open");
    return;
  }

  container.innerHTML = sorted.map((result) => `
    <div class="endpoint-row">
      <span class="endpoint-url" title="${esc(result.url)}">${esc(result.url)}</span>
      <span class="endpoint-meta ${endpointLatencyClass(result)}">${esc(endpointMetaText(result))}</span>
      <button class="btn btn-secondary btn-sm endpoint-use-btn" data-url="${esc(result.url)}" type="button">${t("endpointUse")}</button>
    </div>
  `).join("");
  container.classList.add("open");

  container.querySelectorAll(".endpoint-use-btn").forEach((button) => {
    button.addEventListener("click", () => {
      const url = button.getAttribute("data-url");
      if (!url) return;
      $(inputId).value = url;
      showToast(t("endpointSelected"), "success");
    });
  });
}

function clearEndpointResults(kind) {
  const resultsId = productFieldMap(kind).endpointResults;
  const container = $(resultsId);
  if (!container) return;
  container.innerHTML = "";
  container.classList.remove("open");
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
    <button class="model-row model-use-btn" data-model="${esc(model)}" type="button" title="${esc(model)}">
      <span class="model-id">${esc(model)}</span>
      <span class="model-use">${t("endpointUse")}</span>
    </button>
  `).join("");
  container.classList.add("open");
  container.querySelectorAll(".model-use-btn").forEach((button) => {
    button.addEventListener("click", () => {
      const model = button.getAttribute("data-model");
      if (!model) return;
      $(modelInputId(kind)).value = model;
      updateCodexOfficialConfigPreview();
      showToast(t("modelSelected"), "success");
    });
  });
}

async function handleModelFetch(kind) {
  const fields = productFieldMap(kind);
  const buttonId = fields.modelFetchBtn;
  const baseUrlId = fields.baseUrl;
  const apiKeyId = fields.apiKey;
  const button = $(buttonId);
  const baseUrl = $(baseUrlId).value.trim();
  const apiKey = $(apiKeyId).value.trim();
  if (!baseUrl || !apiKey) {
    showToast(t("modelFetchMissing"), "warning");
    return;
  }

  const previousText = button.textContent;
  button.disabled = true;
  button.textContent = t("modelFetching");
  try {
    const models = await invoke("fetch_available_models", { baseUrl, apiKey, timeoutSecs: 12 });
    renderModelResults(kind, models || []);
  } catch (error) {
    showToast(String(error), "error");
  } finally {
    button.disabled = false;
    button.textContent = previousText || t("modelFetch");
  }
}

async function handleEndpointTest(kind) {
  const buttonId = productFieldMap(kind).endpointTestBtn;
  const button = $(buttonId);
  const urls = getEndpointCandidates(kind);
  if (urls.length === 0) {
    showToast(t("endpointEmpty"), "warning");
    return;
  }

  const previousText = button.textContent;
  button.disabled = true;
  button.textContent = t("endpointTesting");
  try {
    const results = await invoke("test_api_endpoints", { urls, timeoutSecs: 8 });
    renderEndpointResults(kind, results || []);
  } catch (error) {
    showToast(String(error), "error");
  } finally {
    button.disabled = false;
    button.textContent = previousText || t("endpointTest");
  }
}

function openModal(profile) {
  editingId = profile ? profile.id : null;
  $("modalTitle").textContent = profile ? t("editConfig") : t("addConfig");
  $("profileId").value = editingId || "";
  $("profileName").value = profile ? profile.name : "";
  $("profileApiKey").value = profile ? profile.apiKey : "";
  $("profileBaseUrl").value = profile ? profile.baseUrl : "";
  $("profileModelId").value = profile ? (profile.modelId || "") : "";
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

  const name = $("profileName").value.trim();
  const apiKey = $("profileApiKey").value.trim();
  const baseUrl = $("profileBaseUrl").value.trim();
  const modelId = $("profileModelId").value.trim();

  try {
    if (editingId) {
      await invoke("update_profile", { id: editingId, name, apiKey, baseUrl, modelId });
      showToast(t("toastUpdated"), "success");
    } else {
      await invoke("add_profile", { name, apiKey, baseUrl, modelId: modelId || null });
      showToast(t("toastAdded"), "success");
    }

    closeModal();
    await loadProfiles();
    await loadStatus();
  } catch (error) {
    showToast(String(error), "error");
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

function handleEdit(id) {
  const profile = profiles.find((item) => item.id === id);
  if (profile) {
    openModal(profile);
  }
}

async function handleDelete(id) {
  const profile = profiles.find((item) => item.id === id);
  if (!profile) return;

  const confirmed = await appConfirm(t("confirmDelete", { name: profile.name }), {
    title: t("delete"),
    confirmText: currentLang === "zh" ? "删除配置" : "Delete",
    danger: true,
  });
  if (!confirmed) return;

  try {
    await invoke("delete_profile", { id });
    showToast(t("toastDeleted"), "success");
    await loadProfiles();
    await loadStatus();
  } catch (error) {
    showToast(String(error), "error");
  }
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
  switchConsolePage(map[page] || page || "overview");
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

async function loadGrokProfiles() {
  try {
    const data = await invoke("get_grok_profiles");
    grokProfiles = data.profiles || [];
    renderGrokProfiles();
    if (typeof renderOverviewDashboard === "function") renderOverviewDashboard();
  } catch (error) {
    showToast(String(error), "error");
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
          <span class="field-value">${maskKey(profile.apiKey)}</span>
        </div>
        <div class="profile-field">
          <span class="field-label">${t("grokBaseUrlLabel")}</span>
          <span class="field-value">${truncUrl(profile.baseUrl, 50)}</span>
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

  grid.querySelectorAll("button[data-action]").forEach((btn) => {
    const action = btn.getAttribute("data-action");
    const id = btn.getAttribute("data-id");
    if (!id) return;
    btn.addEventListener("click", () => {
      if (action === "grok-switch") handleGrokSwitch(id);
      if (action === "grok-edit") {
        const p = grokProfiles.find((x) => x.id === id);
        if (p) openGrokModal(p);
      }
      if (action === "grok-delete") handleGrokDelete(id);
    });
  });

  updateGrokActiveConfigBar();
}

function updateGrokActiveConfigBar() {
  const section = $("grokActiveConfigSection");
  const nameEl = $("grokActiveConfigName");
  if (!section || !nameEl) return;
  const active = grokProfiles.find((p) => p.isActive);
  if (active) {
    nameEl.textContent = active.name;
    section.style.display = "";
  } else {
    section.style.display = "none";
  }
}

async function loadGrokStatus() {
  try {
    const status = await invoke("get_grok_status");
    lastGrokStatus = status;
    const grid = $("grokStatusGrid");
    if (!grid) {
      if (typeof renderOverviewDashboard === "function") renderOverviewDashboard();
      return;
    }
    if (!status || (!status.apiKey && !status.configExists)) {
      grid.innerHTML = `<div class="status-card" style="display:flex;align-items:center;justify-content:center;color:var(--text-muted);font-size:13px;">Grok (~/.grok/config.toml): --</div>`;
      if (typeof renderOverviewDashboard === "function") renderOverviewDashboard();
      return;
    }
    const COPY_ICON = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>`;
    grid.innerHTML = `
      <div class="status-card">
        <div class="status-card-title">
          <span class="status-card-title-text">${productIcon("grok")}Grok CLI · ~/.grok/config.toml</span>
        </div>
        <div class="status-item">
          <span class="status-label">${t("grokApiKeyLabel")}</span>
          <div class="status-value-wrapper">
            <span class="status-value">${maskKey(status.apiKey)}</span>
            <button class="copy-btn" type="button" data-copy="${esc(status.apiKey || "")}" title="Copy">${COPY_ICON}</button>
          </div>
        </div>
        <div class="status-item">
          <span class="status-label">${t("grokBaseUrlLabel")}</span>
          <div class="status-value-wrapper">
            <span class="status-value has-tooltip" data-tooltip="${esc(status.baseUrl || "")}">${truncUrl(status.baseUrl)}</span>
            <button class="copy-btn" type="button" data-copy="${esc(status.baseUrl || "")}" title="Copy">${COPY_ICON}</button>
          </div>
        </div>
        ${status.model ? `<div class="status-item">
          <span class="status-label">${t("grokModelLabel")}</span>
          <div class="status-value-wrapper">
            <span class="status-value">${esc(status.model)}</span>
            <button class="copy-btn" type="button" data-copy="${esc(status.model || "")}" title="Copy">${COPY_ICON}</button>
          </div>
        </div>` : ""}
        ${status.defaultModelId ? `<div class="status-item">
          <span class="status-label">default</span>
          <div class="status-value-wrapper">
            <span class="status-value">${esc(status.defaultModelId)}</span>
          </div>
        </div>` : ""}
        ${status.apiBackend ? `<div class="status-item">
          <span class="status-label">${t("grokApiBackendLabel")}</span>
          <div class="status-value-wrapper">
            <span class="status-value">${esc(status.apiBackend)}</span>
          </div>
        </div>` : ""}
        <div class="status-item">
          <span class="status-label">source</span>
          <div class="status-value-wrapper">
            <span class="status-value">${esc(status.source || "--")}</span>
          </div>
        </div>
      </div>`;
    grid.querySelectorAll(".copy-btn").forEach((btn) => {
      btn.addEventListener("click", (e) => {
        e.stopPropagation();
        const text = btn.getAttribute("data-copy");
        if (text) navigator.clipboard.writeText(text).then(() => showToast(t("toastCopied"), "success"));
      });
    });
    if (typeof renderOverviewDashboard === "function") renderOverviewDashboard();
  } catch (error) {
    console.error("Failed to load grok status:", error);
    if (typeof renderOverviewDashboard === "function") renderOverviewDashboard();
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

async function loadGrokDiagnostics() {
  try {
    grokDiagnostics = await invoke("get_grok_diagnostics");
    renderGrokDiagnostics();
  } catch (error) {
    const panel = $("grokDiagnosticsPanel");
    if (panel) {
      panel.innerHTML = `<div class="diagnostics-card warning">${currentLang === "zh" ? "诊断加载失败" : "Diagnostics failed"}：${esc(String(error))}</div>`;
    }
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
  $("grokBaseUrl").value = profile ? profile.baseUrl : "https://api.x.ai/v1";
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
  const name = $("grokProfileName").value.trim();
  const apiKey = $("grokApiKey").value.trim();
  const baseUrl = $("grokBaseUrl").value.trim() || "https://api.x.ai/v1";
  const model = $("grokModel").value.trim();
  const apiBackend = $("grokApiBackend")?.value || "chat_completions";

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
  } catch (error) {
    showToast(String(error), "error");
  }
}

async function handleGrokSwitch(id) {
  if (isSwitchingProfile) return;
  const profile = grokProfiles.find((item) => item.id === id);
  if (!profile) return;

  isSwitchingProfile = true;
  showSwitchOverlay(profile.name, "grok");
  try {
    await waitForNextPaint();
    await invoke("switch_grok_profile", { id });
    completeSwitchOverlay();
    await new Promise((resolve) => setTimeout(resolve, 250));
    showToast(t("grokSwitchedTo", { name: profile?.name || "" }), "success");
  } catch (error) {
    showToast(String(error), "error");
  } finally {
    hideSwitchOverlay();
    isSwitchingProfile = false;
    await Promise.all([loadGrokProfiles(), loadGrokStatus(), loadGrokDiagnostics()]);
  }
}

async function handleGrokDelete(id) {
  const profile = grokProfiles.find((x) => x.id === id);
  if (!profile) return;
  const confirmed = await appConfirm(t("confirmDelete", { name: profile.name }), {
    title: t("delete"),
    confirmText: currentLang === "zh" ? "删除配置" : "Delete",
    danger: true,
  });
  if (!confirmed) return;
  try {
    await invoke("delete_grok_profile", { id });
    showToast(t("grokToastDeleted"), "success");
    await Promise.all([loadGrokProfiles(), loadGrokStatus(), loadGrokDiagnostics()]);
  } catch (error) {
    showToast(String(error), "error");
  }
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

// ── Codex Profile Management ────────────────────────

let editingCodexId = null;

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
  const imageApiKey = $("codexImageApiKey").value.trim().replace(/\\/g, "\\\\").replace(/"/g, '\\"');
  const imageBaseUrl = ($("codexImageBaseUrl").value.trim() || "https://hk.getelucid.com/v1").replace(/\\/g, "\\\\").replace(/"/g, '\\"');
  const imageConfig = imageApiKey ? `

[gpt_image_2]
api_key = "${imageApiKey}"
base_url = "${imageBaseUrl}"
model = "gpt-image-2"` : "";
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
experimental_bearer_token = "${apiKey}"${imageConfig}`;
}

function updateCodexOfficialConfigPreview() {
  const preview = $("codexOfficialConfig");
  if (preview) preview.value = buildCodexOfficialConfig();
}

function updateCodexAuthModeUi() {
  const mode = getCodexAuthMode();
  const isOfficialMode = mode === "official_account_api_quota";
  const saveOnly = mode === "save_only";
  if ($("codexOfficialConfigGroup")) {
    $("codexOfficialConfigGroup").style.display = isOfficialMode ? "block" : "none";
  }
  // 官方账号模式 / 仅保存：API Key 可不填
  if ($("codexApiKey")) {
    $("codexApiKey").required = !(isOfficialMode || saveOnly);
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
    if (typeof renderOverviewDashboard === "function") renderOverviewDashboard();
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
    updateCodexActiveConfigBar();
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
          <span class="field-value">${maskKey(profile.apiKey)}</span>
        </div>
        <div class="profile-field">
          <span class="field-label">${t("codexBaseUrlLabel")}</span>
          <span class="field-value">${truncUrl(profile.baseUrl, 50)}</span>
        </div>
        ${profile.model ? `<div class="profile-field">
          <span class="field-label">${t("codexModelLabel")}</span>
          <span class="field-value">${esc(profile.model)}</span>
        </div>` : ""}
        ${profile.providerName ? `<div class="profile-field">
          <span class="field-label">${t("codexProviderLabel")}</span>
          <span class="field-value">${esc(profile.providerName)}</span>
        </div>` : ""}
        ${profile.imageApiKey ? `<div class="profile-field">
          <span class="field-label">${t("codexImageApiKeyLabel")}</span>
          <span class="field-value">${maskKey(profile.imageApiKey)}</span>
        </div>` : ""}
        ${profile.imageApiKey && profile.imageBaseUrl ? `<div class="profile-field">
          <span class="field-label">${t("codexImageBaseUrlLabel")}</span>
          <span class="field-value">${truncUrl(profile.imageBaseUrl, 50)}</span>
        </div>` : ""}
      </div>
      <div class="profile-actions">
        ${profile.isActive ? "" : `<button class="btn btn-switch btn-sm" data-action="codex-switch" data-id="${profile.id}" type="button">${t("switchUse")}</button>`}
        <button class="btn btn-secondary btn-sm" data-action="codex-edit" data-id="${profile.id}" type="button">${t("edit")}</button>
        <button class="btn btn-danger btn-sm" data-action="codex-delete" data-id="${profile.id}" type="button">${t("delete")}</button>
      </div>
    </div>
  `).join("");

  grid.querySelectorAll("button[data-action]").forEach((btn) => {
    const action = btn.getAttribute("data-action");
    const id = btn.getAttribute("data-id");
    if (!id) return;
    btn.addEventListener("click", () => {
      if (action === "codex-switch") handleCodexSwitch(id);
      if (action === "codex-edit") {
        const p = codexProfiles.find((x) => x.id === id);
        if (p) openCodexModal(p);
      }
      if (action === "codex-delete") handleCodexDelete(id);
    });
  });

  updateCodexActiveConfigBar();
}

function updateCodexActiveConfigBar() {
  const section = $("codexActiveConfigSection");
  const nameEl = $("codexActiveConfigName");
  const active = codexProfiles.find((p) => p.isActive);
  if (active) {
    nameEl.textContent = active.name;
    section.style.display = "";
  } else {
    section.style.display = "none";
  }
}

function renderCodexDiagnostics() {
  const panel = $("codexDiagnosticsPanel");
  if (!panel) return;
  const d = codexDiagnostics;
  if (!d) {
    panel.innerHTML = `<div class="diagnostics-card muted">Codex diagnostics: --</div>`;
    return;
  }
  const healthy = Array.isArray(d.issues) && d.issues.length === 0;
  const issueItems = (d.issues || []).map((item) => `<li>${esc(item)}</li>`).join("");
  const suggestionItems = (d.suggestions || []).map((item) => `<li>${esc(item)}</li>`).join("");
  const markets = (d.pluginMarketplaces || []).length
    ? d.pluginMarketplaces.map((item) => `<span class="diagnostics-pill">${esc(truncUrl(item, 34))}</span>`).join("")
    : `<span class="diagnostics-pill muted">未安装插件市场</span>`;
  panel.innerHTML = `
    <div class="diagnostics-card ${healthy ? "healthy" : "warning"}">
      <div class="diagnostics-head">
        <div>
          <div class="diagnostics-kicker">Codex Health</div>
          <div class="diagnostics-title">${healthy ? "配置健康" : "需要处理"}</div>
        </div>
        <span class="diagnostics-state ${healthy ? "ok" : "warn"}">${healthy ? "OK" : `${d.issues.length} issues`}</span>
      </div>
      <div class="diagnostics-grid">
        <div><span>Active</span><strong>${esc(d.activeProfileName || "--")}</strong></div>
        <div><span>Provider</span><strong>${esc(d.providerName || "--")}</strong></div>
        <div><span>Model</span><strong>${esc(d.model || "--")}</strong></div>
        <div><span>Auth</span><strong>${esc(d.authMode || "--")}</strong></div>
      </div>
      <div class="diagnostics-paths">
        <div title="${esc(d.configPath || "")}">config: ${esc(d.configExists ? d.configPath : "未找到")}</div>
        <div title="${esc(d.authPath || "")}">auth: ${esc(d.authExists ? d.authPath : "未找到")}</div>
      </div>
      <div class="diagnostics-marketplaces">${markets}</div>
      ${issueItems ? `<div class="diagnostics-list"><strong>问题</strong><ul>${issueItems}</ul></div>` : ""}
      ${suggestionItems ? `<div class="diagnostics-list"><strong>建议</strong><ul>${suggestionItems}</ul></div>` : ""}
      <div class="diagnostics-footer">Last checked: ${esc(d.lastCheckedAt || "--")}</div>
    </div>`;
}

async function loadCodexDiagnostics() {
  try {
    codexDiagnostics = await invoke("get_codex_diagnostics");
    renderCodexDiagnostics();
  } catch (error) {
    const panel = $("codexDiagnosticsPanel");
    if (panel) {
      panel.innerHTML = `<div class="diagnostics-card warning">诊断加载失败：${esc(String(error))}</div>`;
    }
  }
}

async function handleCodexRuntimeBackup() {
  try {
    const result = await invoke("backup_codex_runtime");
    const files = [result.configBackup, result.authBackup].filter(Boolean).length;
    showToast(files ? `已备份 ${files} 个 Codex 文件` : "没有可备份的 Codex 文件", files ? "success" : "warning");
    await loadCodexDiagnostics();
  } catch (error) {
    showToast(String(error), "error");
  }
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
      if (typeof renderCodexCurrentCard === "function") renderCodexCurrentCard();
      if (typeof renderOverviewDashboard === "function") renderOverviewDashboard();
      return;
    }
    const COPY_ICON = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>`;
    grid.innerHTML = `
      <div class="status-card">
        <div class="status-card-title">
          <span class="status-card-title-text">${productIcon("codex")}Codex CLI</span>
        </div>
        <div class="status-item">
          <span class="status-label">${t("codexApiKeyLabel")}</span>
          <div class="status-value-wrapper">
            <span class="status-value">${maskKey(status.apiKey)}</span>
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
        ${status.imageApiKey ? `<div class="status-item">
          <span class="status-label">${t("codexImageApiKeyLabel")}</span>
          <div class="status-value-wrapper">
            <span class="status-value">${maskKey(status.imageApiKey)}</span>
            <button class="copy-btn" type="button" data-copy="${esc(status.imageApiKey || "")}" title="Copy">${COPY_ICON}</button>
          </div>
        </div>
        <div class="status-item">
          <span class="status-label">${t("codexImageBaseUrlLabel")}</span>
          <div class="status-value-wrapper">
            <span class="status-value has-tooltip" data-tooltip="${esc(status.imageBaseUrl || "")}" title="${esc(status.imageBaseUrl || "")}">${esc(status.imageBaseUrl || "--")}</span>
            <button class="copy-btn" type="button" data-copy="${esc(status.imageBaseUrl || "")}" title="Copy">${COPY_ICON}</button>
          </div>
        </div>` : ""}
      </div>`;
    if (grid) {
      grid.querySelectorAll(".copy-btn").forEach((btn) => {
        btn.addEventListener("click", (e) => {
          e.stopPropagation();
          const text = btn.getAttribute("data-copy");
          if (text) navigator.clipboard.writeText(text).then(() => showToast(t("toastCopied"), "success"));
        });
      });
    }
    if (typeof renderCodexCurrentCard === "function") renderCodexCurrentCard();
    if (typeof renderOverviewDashboard === "function") renderOverviewDashboard();
  } catch (error) {
    console.error("Failed to load codex status:", error);
    if (typeof renderOverviewDashboard === "function") renderOverviewDashboard();
  }
}

function openCodexModal(profile) {
  editingCodexId = profile ? profile.id : null;
  codexWizardEnableAfterSave = false;
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
  if ($("codexProvider")) $("codexProvider").value = profile ? (profile.providerName || "") : "";
  if ($("codexImageApiKey")) $("codexImageApiKey").value = profile ? (profile.imageApiKey || "") : "";
  if ($("codexImageBaseUrl")) $("codexImageBaseUrl").value = profile ? (profile.imageBaseUrl || "https://hk.getelucid.com/v1") : "https://hk.getelucid.com/v1";
  setCodexAuthMode(profile ? (profile.authMode || "auth_json") : "auth_json");
  updateCodexPresetHint();
  clearEndpointResults("codex");
  clearModelResults("codex");
  setCodexWizardStep(profile ? 3 : 1);
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
  if (!validateCodexWizardStep(3)) {
    setCodexWizardStep(3);
    return;
  }
  const name = $("codexProfileName").value.trim();
  const apiKey = $("codexApiKey").value.trim();
  const baseUrl = $("codexBaseUrl").value.trim();
  const model = $("codexModel").value.trim();
  const providerName = $("codexProvider").value.trim();
  const imageApiKey = $("codexImageApiKey").value.trim();
  const imageBaseUrl = $("codexImageBaseUrl").value.trim();
  let authMode = getCodexAuthMode();
  // save_only：前端仅保存配置，后端仍用 auth_json 字段存储，不自动切换
  const saveOnly = authMode === "save_only";
  if (saveOnly) authMode = "auth_json";
  const enableAfter = codexWizardEnableAfterSave && !saveOnly;
  codexWizardEnableAfterSave = false;

  try {
    let savedId = editingCodexId;
    if (editingCodexId) {
      await invoke("update_codex_profile", { id: editingCodexId, name, apiKey, baseUrl, model: model || null, providerName: providerName || null, authMode, imageApiKey: imageApiKey || null, imageBaseUrl: imageBaseUrl || null });
      showToast(t("codexToastUpdated"), "success");
    } else {
      const created = await invoke("add_codex_profile", { name, apiKey, baseUrl, model: model || null, providerName: providerName || null, authMode, imageApiKey: imageApiKey || null, imageBaseUrl: imageBaseUrl || null });
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
      await loadCodexDiagnostics();
    }
    renderOverviewDashboard();
  } catch (error) {
    showToast(String(error), "error");
  }
}

async function handleCodexSwitch(id) {
  if (isSwitchingProfile) return;
  const profile = codexProfiles.find((item) => item.id === id);
  if (!profile) return;

  isSwitchingProfile = true;
  showSwitchOverlay(profile.name, "codex");
  try {
    await waitForNextPaint();
    await invoke("switch_codex_profile", { id });
    completeSwitchOverlay();
    await new Promise((resolve) => setTimeout(resolve, 250));
    showToast(t("codexSwitchedTo", { name: profile?.name || "" }), "success");
  } catch (error) {
    showToast(String(error), "error");
  } finally {
    hideSwitchOverlay();
    isSwitchingProfile = false;
    await Promise.all([loadCodexProfiles(), loadCodexStatus(), loadCodexDiagnostics()]);
  }
}

async function handleCodexDelete(id) {
  const profile = codexProfiles.find((x) => x.id === id);
  if (!profile) return;
  const confirmed = await appConfirm(
    currentLang === "zh"
      ? `删除配置 “${profile.name}”？\n\n删除后将无法从 VarSwitch 中恢复，但不会自动删除远程 API 服务。`
      : t("confirmDelete", { name: profile.name }),
    {
      title: t("delete"),
      confirmText: currentLang === "zh" ? "删除配置" : "Delete",
      danger: true,
    }
  );
  if (!confirmed) return;
  try {
    await invoke("delete_codex_profile", { id });
    showToast(t("codexToastDeleted"), "success");
    await loadCodexProfiles();
    await loadCodexStatus();
  } catch (error) {
    showToast(String(error), "error");
  }
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
  $("skillsOverlay").classList.add("open");
  hideSkillsEdit();
  switchSkillsTab("installed");
  loadSkills();
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

  list.querySelectorAll("button[data-action]").forEach((btn) => {
    const action = btn.getAttribute("data-action");
    const name = btn.getAttribute("data-name");
    const sourceType = btn.getAttribute("data-source-type") || "command";
    btn.addEventListener("click", () => {
      if (action === "edit-skill") showSkillsEdit(name, sourceType);
      if (action === "delete-skill") handleDeleteSkill(name, sourceType);
    });
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
  const name = $("skillNameInput").value.trim();
  const content = $("skillContentInput").value;
  const sourceType = $("skillsEdit").dataset.sourceType || "command";
  if (!name) return;

  try {
    await invoke("save_skill", { name, content, sourceType });
    showToast(t("toastSkillSaved"), "success");
    hideSkillsEdit();
    await loadSkills();
  } catch (error) {
    showToast(String(error), "error");
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
    const starsHtml = skill.stars ? `<span class="skill-card-badge">\u2605 ${skill.stars}</span>` : "";
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

  grid.querySelectorAll("button[data-action='install-catalog']").forEach((btn) => {
    btn.addEventListener("click", async () => {
      const name = btn.getAttribute("data-name");
      const url = btn.getAttribute("data-url");
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
  });

  grid.querySelectorAll("button[data-action='open-skill-url']").forEach((btn) => {
    btn.addEventListener("click", () => {
      const url = btn.getAttribute("data-url");
      if (url) window.__TAURI__?.shell?.open(url);
    });
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

  list.querySelectorAll("button[data-action='remove-repo']").forEach((btn) => {
    btn.addEventListener("click", () => handleRemoveRepo(btn.getAttribute("data-url")));
  });
}

async function handleAddRepo() {
  const url = $("repoUrlInput").value.trim();
  if (!url) return;

  try {
    await invoke("add_skill_repo", { url, branch: "main" });
    $("repoUrlInput").value = "";
    showToast(t("toastRepoAdded"), "success");
    await loadSkillRepos();
    renderRepoList();
  } catch (error) {
    showToast(String(error), "error");
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
  $("promptsOverlay").classList.add("open");
  switchPromptTab("editor");
  loadClaudeMd();
  loadPromptTemplates();
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

  grid.querySelectorAll("button[data-action]").forEach((btn) => {
    const action = btn.getAttribute("data-action");
    const id = btn.getAttribute("data-id");
    btn.addEventListener("click", (e) => {
      e.stopPropagation();
      const tpl = promptTemplates.find((t) => t.id === id);
      if (!tpl) return;
      if (action === "append-template") {
        const current = $("promptContentInput").value;
        $("promptContentInput").value = current
          ? current + "\n\n" + tpl.content
          : tpl.content;
        switchPromptTab("editor");
        showToast(t("toastSnippetInserted"), "success");
      } else if (action === "replace-template") {
        $("promptContentInput").value = tpl.content;
        switchPromptTab("editor");
        showToast(t("toastTemplateApplied"), "success");
      }
    });
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
  $("mcpOverlay").classList.add("open");
  switchMcpTab("installed");
  hideMcpEdit();
  loadMcpServers();
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

  list.querySelectorAll("button[data-action]").forEach((btn) => {
    const action = btn.getAttribute("data-action");
    const name = btn.getAttribute("data-name");
    btn.addEventListener("click", () => {
      if (action === "edit-mcp") showMcpEdit(name);
      if (action === "delete-mcp") handleDeleteMcp(name);
    });
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

  try {
    await invoke("save_mcp_server", { name, config });
    showToast(t("toastMcpSaved"), "success");
    hideMcpEdit();
    await loadMcpServers();
  } catch (error) {
    showToast(String(error), "error");
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
  const query = $("mcpPresetSearch").value.trim();
  if (!query) {
    mcpGitHubResults = [];
    renderMcpPresets();
    return;
  }

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
    const stars = preset.stars ? `<span class="skill-card-badge">\u2605 ${preset.stars}</span>` : "";
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

  grid.querySelectorAll("button[data-action='install-mcp-preset']").forEach((btn) => {
    btn.addEventListener("click", async () => {
      const id = btn.getAttribute("data-id");
      const allItems = [...mcpPresets, ...mcpGitHubResults];
      const preset = allItems.find((p) => p.id === id);
      if (!preset) return;

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
  });

  grid.querySelectorAll("button[data-action='open-mcp-url']").forEach((btn) => {
    btn.addEventListener("click", () => {
      const url = btn.getAttribute("data-url");
      if (url) window.__TAURI__?.shell?.open(url);
    });
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
    if (typeof pushOverviewEvent === "function" && Array.isArray(overviewEvents)) {
      // 直接写入，避免 toast->event->render 链路异常阻断
      overviewEvents.unshift({
        message: String(message || ""),
        level: type === "error" ? "error" : type === "warning" ? "warning" : "info",
        time: new Date(),
      });
      overviewEvents = overviewEvents.slice(0, 12);
      if (typeof renderOverviewEvents === "function") renderOverviewEvents();
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
    appDialogResolver = resolve;
    const overlay = $("appDialogOverlay");
    const dialog = overlay?.querySelector(".app-dialog");
    if (!overlay || !dialog) {
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
  $("appDialogOverlay")?.addEventListener("click", (event) => {
    if (event.target === $("appDialogOverlay")) closeAppDialog(null);
  });
  $("appDialogInput")?.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      event.preventDefault();
      $("appDialogConfirm")?.click();
    }
  });
}

// ── Overview 事件与控制台导航 ──────────────────────────────
function pushOverviewEvent(message, level = "info") {
  overviewEvents.unshift({
    message: String(message || ""),
    level,
    time: new Date(),
  });
  overviewEvents = overviewEvents.slice(0, 12);
  renderOverviewEvents();
}

function formatRelativeTime(date) {
  if (!date) return "--";
  const diff = Math.max(0, Date.now() - date.getTime());
  if (diff < 60_000) return currentLang === "zh" ? "刚刚" : "just now";
  if (diff < 3_600_000) {
    const m = Math.floor(diff / 60_000);
    return currentLang === "zh" ? `${m} 分钟前` : `${m}m ago`;
  }
  if (diff < 86_400_000) {
    const h = Math.floor(diff / 3_600_000);
    return currentLang === "zh" ? `${h} 小时前` : `${h}h ago`;
  }
  const d = Math.floor(diff / 86_400_000);
  return currentLang === "zh" ? `${d} 天前` : `${d}d ago`;
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
  const market = $("toolboxMarketContent");
  const session = $("toolboxSessionContent");
  const remote = $("toolboxRemoteContent");
  const pluginHost = $("pluginPageHost");
  const sessionHost = $("sessionPageHost");
  const mobileHost = $("mobilePageHost");
  if (market && pluginHost && market.parentElement !== pluginHost) {
    pluginHost.appendChild(market);
    market.style.display = "";
  }
  if (session && sessionHost && session.parentElement !== sessionHost) {
    sessionHost.appendChild(session);
    session.style.display = "";
  }
  if (remote && mobileHost && remote.parentElement !== mobileHost) {
    mobileHost.appendChild(remote);
    remote.style.display = "";
  }

  // 设置页：把设置面板 body 挂入页面
  const settingsBody = document.querySelector("#settingsOverlay .settings-body");
  const settingsHost = $("settingsPageHost");
  if (settingsBody && settingsHost && settingsBody.parentElement !== settingsHost) {
    settingsHost.appendChild(settingsBody);
  }
}

function switchConsolePage(page) {
  const next = page || "overview";
  activeConsolePage = next;
  // 与旧 page 切换兼容
  if (next === "claude" || next === "codex" || next === "grok") {
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
    loadCodexDiagnostics();
  } else if (next === "grok") {
    loadGrokProfiles();
    loadGrokStatus();
  } else if (next === "configurations") {
    renderConfigurationList();
  } else if (next === "plugins" || next === "sessions" || next === "mobile") {
    mountToolboxPages();
    loadCodexToolbox();
    if (next === "plugins") switchToolboxTab("market");
    if (next === "sessions") switchToolboxTab("session");
    if (next === "mobile") switchToolboxTab("remote");
  } else if (next === "settings") {
    mountToolboxPages();
    openSettingsInline();
  } else if (next === "overview") {
    renderOverviewDashboard();
  }

  renderGlobalConfigStatus();

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
    // 同步设置页语言/主题按钮
    $("settingsLangZh")?.classList.toggle("active", currentLang === "zh");
    $("settingsLangEn")?.classList.toggle("active", currentLang === "en");
    $("settingsThemeLight")?.classList.toggle("active", currentTheme === "light");
    $("settingsThemeDark")?.classList.toggle("active", currentTheme === "dark");
  } catch (error) {
    showToast(String(error), "error");
  }
}

function renderOverviewEvents() {
  const list = $("overviewEventList");
  if (!list) return;
  if (!overviewEvents.length) {
    list.innerHTML = `<div class="event-item"><span class="event-dot"></span><div><strong>${currentLang === "zh" ? "暂无事件" : "No events yet"}</strong><small>${currentLang === "zh" ? "同步、导入、切换后会出现在这里" : "Sync, import and switch actions will appear here"}</small></div><span class="event-time">--</span></div>`;
    return;
  }
  list.innerHTML = overviewEvents
    .map((item) => {
      const tone = item.level === "error" ? "warning" : item.level === "warning" ? "warning" : "";
      return `<div class="event-item"><span class="event-dot ${tone}"></span><div><strong>${esc(item.message)}</strong></div><span class="event-time">${formatRelativeTime(item.time)}</span></div>`;
    })
    .join("");
}

function renderOverviewRecentConfigs() {
  const host = $("overviewRecentConfigs");
  if (!host) return;
  const items = [];
  const activeClaude = profiles.find((p) => p.isActive);
  const activeCodex = codexProfiles.find((p) => p.isActive);
  const activeGrok = grokProfiles.find((p) => p.isActive);
  if (activeClaude) items.push({ type: "claude", profile: activeClaude });
  if (activeCodex) items.push({ type: "codex", profile: activeCodex });
  if (activeGrok) items.push({ type: "grok", profile: activeGrok });
  // 补充最近非激活配置
  [...profiles, ...codexProfiles, ...grokProfiles]
    .filter((p) => !p.isActive)
    .slice(0, 4)
    .forEach((p) => {
      const type = profiles.includes(p) ? "claude" : codexProfiles.includes(p) ? "codex" : "grok";
      if (!items.find((x) => x.profile.id === p.id && x.type === type)) {
        items.push({ type, profile: p });
      }
    });

  if (!items.length) {
    host.innerHTML = `<div class="empty-inline">${currentLang === "zh" ? "还没有配置，点击右上角添加。" : "No configs yet. Add one from the top bar."}</div>`;
    return;
  }

  host.innerHTML = items
    .slice(0, 6)
    .map(({ type, profile }) => {
      const icon = type === "claude" ? "anthropic-color.svg" : type === "codex" ? "OpenAI-black-monoblossom.svg" : "grok-color.svg";
      const typeLabel = type === "claude" ? "Claude" : type === "codex" ? "Codex" : "Grok";
      const badge = profile.isActive
        ? `<span class="status-badge-console healthy">${currentLang === "zh" ? "使用中" : "Active"}</span>`
        : `<span class="status-badge-console">${currentLang === "zh" ? "可切换" : "Ready"}</span>`;
      return `<div class="configuration-row configuration-row-overview">
        <div class="configuration-data">
          <div class="configuration-main"><img src="${icon}" alt=""><div><strong>${esc(profile.name)}</strong><small>${typeLabel} · ${esc(truncUrl(profile.baseUrl || "", 36))}</small></div></div>
          <div class="configuration-meta">${esc(maskKey(profile.apiKey || ""))}</div>
          <div class="configuration-state">${badge}</div>
        </div>
        <div class="configuration-actions">
          ${profile.isActive
            ? `<span class="configuration-action-slot" aria-hidden="true"></span>`
            : `<button class="btn btn-secondary btn-sm" data-overview-switch="${type}:${esc(profile.id)}" type="button">${currentLang === "zh" ? "切换" : "Switch"}</button>`}
          <button class="btn btn-ghost btn-sm" data-console-page="${type === "claude" ? "claude" : type === "codex" ? "codex" : "grok"}" type="button">${currentLang === "zh" ? "查看" : "Open"}</button>
        </div>
      </div>`;
    })
    .join("");

  host.querySelectorAll("[data-overview-switch]").forEach((btn) => {
    btn.addEventListener("click", async () => {
      const [type, id] = (btn.getAttribute("data-overview-switch") || "").split(":");
      if (type === "claude") await handleSwitch(id);
      else if (type === "codex") await handleCodexSwitch(id);
      else if (type === "grok") await handleGrokSwitch(id);
      renderOverviewDashboard();
    });
  });
  host.querySelectorAll("[data-console-page]").forEach((btn) => {
    btn.addEventListener("click", () => switchConsolePage(btn.getAttribute("data-console-page")));
  });
}

function renderConfigurationList() {
  const host = $("configurationList");
  if (!host) return;
  const q = (configurationSearch || "").trim().toLowerCase();
  const rows = [];
  profiles.forEach((p) => rows.push({ type: "claude", profile: p }));
  codexProfiles.forEach((p) => rows.push({ type: "codex", profile: p }));
  grokProfiles.forEach((p) => rows.push({ type: "grok", profile: p }));

  const filtered = rows.filter(({ type, profile }) => {
    if (configurationFilter === "claude" && type !== "claude") return false;
    if (configurationFilter === "codex" && type !== "codex") return false;
    if (configurationFilter === "grok" && type !== "grok") return false;
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
      const icon = type === "claude" ? "anthropic-color.svg" : type === "codex" ? "OpenAI-black-monoblossom.svg" : "grok-color.svg";
      const typeLabel = type === "claude" ? "Claude" : type === "codex" ? "Codex" : "Grok";
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

  host.querySelectorAll(".configuration-row").forEach((row) => {
    const type = row.getAttribute("data-config-type");
    const id = row.getAttribute("data-config-id");
    row.querySelectorAll("button[data-action]").forEach((btn) => {
      btn.addEventListener("click", async () => {
        const action = btn.getAttribute("data-action");
        if (action === "switch") {
          if (type === "claude") await handleSwitch(id);
          else if (type === "codex") await handleCodexSwitch(id);
          else await handleGrokSwitch(id);
        } else if (action === "edit") {
          if (type === "claude") handleEdit(id);
          else if (type === "codex") {
            const p = codexProfiles.find((x) => x.id === id);
            if (p) openCodexModal(p);
          } else {
            const p = grokProfiles.find((x) => x.id === id);
            if (p) openGrokModal(p);
          }
        } else if (action === "copy") {
          duplicateConfiguration(type, id);
        } else if (action === "delete") {
          if (type === "claude") await handleDelete(id);
          else if (type === "codex") await handleCodexDelete(id);
          else await handleGrokDelete(id);
        }
        renderConfigurationList();
        renderOverviewDashboard();
      });
    });
  });
}

function renderOverviewDashboard() {
  try {
  const activeClaude = profiles.find((p) => p.isActive);
  const activeCodex = codexProfiles.find((p) => p.isActive);
  const activeGrok = grokProfiles.find((p) => p.isActive);
  const mainName = activeCodex?.name || activeClaude?.name || activeGrok?.name || (currentLang === "zh" ? "尚未启用配置" : "No active config");
  if ($("overviewActiveName")) $("overviewActiveName").textContent = mainName;
  renderGlobalConfigStatus();

  // Claude：只要状态已读取就不再显示“检查中”
  const claudeLoaded = lastClaudeStatus != null || profiles.length > 0;
  const claudeOk = !!(lastClaudeStatus?.envVars || lastClaudeStatus?.claude || activeClaude);
  setBadge(
    $("overviewClaudeBadge"),
    !claudeLoaded ? (currentLang === "zh" ? "检查中" : "...") : claudeOk ? (currentLang === "zh" ? "正常" : "OK") : (currentLang === "zh" ? "待配置" : "Idle"),
    !claudeLoaded ? "" : claudeOk ? "healthy" : "warning"
  );
  if ($("overviewClaudeStatus")) {
    $("overviewClaudeStatus").textContent = !claudeLoaded
      ? (currentLang === "zh" ? "检查中" : "Checking")
      : (activeClaude?.name || (currentLang === "zh" ? "未启用" : "Inactive"));
  }
  setNavDot("claudeNavState", !claudeLoaded ? "" : claudeOk ? "healthy" : "warning");

  // Codex
  const codexLoaded = lastCodexStatus != null || codexProfiles.length > 0;
  const codexOk = !!(lastCodexStatus?.apiKey || activeCodex);
  setBadge(
    $("overviewCodexBadge"),
    !codexLoaded ? (currentLang === "zh" ? "检查中" : "...") : codexOk ? (currentLang === "zh" ? "正常" : "OK") : (currentLang === "zh" ? "待配置" : "Idle"),
    !codexLoaded ? "" : codexOk ? "healthy" : "warning"
  );
  if ($("overviewCodexStatus")) {
    $("overviewCodexStatus").textContent = !codexLoaded
      ? (currentLang === "zh" ? "检查中" : "Checking")
      : (activeCodex?.name || (currentLang === "zh" ? "未启用" : "Inactive"));
  }
  setNavDot("codexNavState", !codexLoaded ? "" : codexOk ? "healthy" : "warning");

  // Grok
  const grokLoaded = lastGrokStatus != null || grokProfiles.length > 0;
  const grokOk = !!(lastGrokStatus?.apiKey || activeGrok);
  setBadge(
    $("overviewGrokBadge"),
    !grokLoaded ? (currentLang === "zh" ? "检查中" : "...") : grokOk ? (currentLang === "zh" ? "正常" : "OK") : (currentLang === "zh" ? "待配置" : "Idle"),
    !grokLoaded ? "" : grokOk ? "healthy" : ""
  );
  if ($("overviewGrokStatus")) {
    $("overviewGrokStatus").textContent = !grokLoaded
      ? (currentLang === "zh" ? "检查中" : "Checking")
      : (activeGrok?.name || (currentLang === "zh" ? "未启用" : "Inactive"));
  }
  setNavDot("grokNavState", !grokLoaded ? "" : grokOk ? "healthy" : "");

  // Plugins / sessions / mobile from toolbox
  const plugins = codexToolbox?.builtinPlugins?.plugins || codexToolbox?.plugins || [];
  const pluginEnabled = Array.isArray(plugins) ? plugins.filter((p) => p.enabled || p.isEnabled).length : 0;
  const pluginTotal = Array.isArray(plugins) ? plugins.length : 0;
  if ($("overviewPluginStatus")) $("overviewPluginStatus").textContent = pluginTotal ? `${pluginEnabled}/${pluginTotal}` : (currentLang === "zh" ? "未加载" : "N/A");
  setBadge($("overviewPluginBadge"), pluginTotal ? (currentLang === "zh" ? "已就绪" : "Ready") : "--", pluginTotal ? "healthy" : "");
  setNavDot("pluginNavState", pluginTotal ? "healthy" : "");

  const sessionMetrics = helpers.getCodexSessionMetrics?.(codexToolbox) || { count: 0 };
  const sessionCount = sessionMetrics.count;
  if ($("overviewSessionStatus")) $("overviewSessionStatus").textContent = sessionCount ? `${sessionCount}` : (currentLang === "zh" ? "0" : "0");
  setBadge($("overviewSessionBadge"), sessionCount ? (currentLang === "zh" ? "已同步" : "Synced") : (currentLang === "zh" ? "空" : "Empty"), sessionCount ? "healthy" : "");
  if ($("sessionNavCount")) $("sessionNavCount").textContent = String(sessionCount || 0);

  const channels = codexToolbox?.mobileChannels || [];
  const bound = Array.isArray(channels) ? channels.some((c) => c.bound || c.appId || c.botToken) : false;
  setNavDot("mobileNavState", bound ? "healthy" : "");

  const healthTone = claudeOk || codexOk ? "healthy" : "warning";
  setBadge($("overviewHealthBadge"), healthTone === "healthy" ? (currentLang === "zh" ? "环境健康" : "Healthy") : (currentLang === "zh" ? "需要关注" : "Needs attention"), healthTone);
  if ($("overviewHealthSummary")) {
    $("overviewHealthSummary").textContent = currentLang === "zh"
      ? `Claude ${claudeOk ? "正常" : "待配置"} · Codex ${codexOk ? "正常" : "待配置"} · 会话 ${sessionCount || 0}`
      : `Claude ${claudeOk ? "OK" : "idle"} · Codex ${codexOk ? "OK" : "idle"} · Sessions ${sessionCount || 0}`;
  }
  if ($("globalStatusText")) {
    $("globalStatusText").textContent = currentLang === "zh"
      ? `Codex ${codexOk ? "正常" : "待配置"} · Claude ${claudeOk ? "正常" : "待配置"}`
      : `Codex ${codexOk ? "OK" : "idle"} · Claude ${claudeOk ? "OK" : "idle"}`;
  }
  $("globalStatusDot")?.classList.toggle("healthy", healthTone === "healthy");
  $("globalStatusDot")?.classList.toggle("warning", healthTone !== "healthy");

  renderOverviewRecentConfigs();
  renderOverviewEvents();
  renderConfigurationList();
  renderCodexCurrentCard();
  renderSessionStatusCard();
  renderMobileTimeline();
  } catch (error) {
    console.error("renderOverviewDashboard failed:", error);
  }
}

function renderCodexCurrentCard() {
  const grid = $("codexCurrentGrid");
  if (!grid) return;
  const active = codexProfiles.find((p) => p.isActive);
  const status = lastCodexStatus || {};
  const healthy = !!(status.apiKey || active);
  setBadge($("codexCurrentHealthBadge"), healthy ? (currentLang === "zh" ? "正常" : "Healthy") : (currentLang === "zh" ? "未启用" : "Idle"), healthy ? "healthy" : "warning");
  const rows = [
    [currentLang === "zh" ? "当前配置" : "Active", active?.name || "--"],
    ["Provider", status.providerName || active?.providerName || "--"],
    ["Model", status.model || active?.model || "--"],
    ["Auth", status.authMode || active?.authMode || "--"],
    ["API Key", maskKey(status.apiKey || active?.apiKey || "")],
    ["Base URL", truncUrl(status.baseUrl || active?.baseUrl || "", 48)],
  ];
  grid.innerHTML = rows
    .map(([k, v]) => `<div class="wizard-review-item"><small>${esc(k)}</small><strong title="${esc(String(v))}">${esc(String(v))}</strong></div>`)
    .join("");

  const panel = $("codexDiagnosticsPanel");
  if (panel) panel.style.display = codexDiagExpanded ? "" : "none";
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

function renderGlobalConfigStatus() {
  const activeClaude = profiles.find((profile) => profile.isActive);
  const activeCodex = codexProfiles.find((profile) => profile.isActive);
  const activeGrok = grokProfiles.find((profile) => profile.isActive);
  const display = helpers.getGlobalConfigDisplay?.(
    activeConsolePage,
    {
      claude: activeClaude?.name || "",
      codex: activeCodex?.name || "",
      grok: activeGrok?.name || "",
      total: profiles.length + codexProfiles.length + grokProfiles.length,
    },
    currentLang
  );
  if (!display) return;
  if ($("globalConfigLabel")) $("globalConfigLabel").textContent = display.label;
  if ($("globalConfigName")) $("globalConfigName").textContent = display.name;
  if ($("globalConfigPill")) $("globalConfigPill").title = display.title;
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

function applyPluginFilters() {
  const q = (pluginSearch || "").trim().toLowerCase();
  const cards = document.querySelectorAll("#builtinPluginList .builtin-plugin-card, #builtinPluginList .builtin-plugin-item, #builtinPluginList .plugin-card, #pluginPageHost .builtin-plugin-item");
  cards.forEach((card) => {
    const text = (card.textContent || "").toLowerCase();
    const enabled = /enabled|已启用|active/i.test(text);
    const needsRepair = /repair|修复|broken|error|异常/i.test(text);
    const installed = true;
    const show = helpers.matchesPluginFilter
      ? helpers.matchesPluginFilter({ text, enabled, needsRepair, installed }, pluginFilter, q)
      : (!q || text.includes(q));
    card.style.display = show ? "" : "none";
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
    $("codexProvider").value = profile.providerName || "";
    $("codexImageApiKey").value = profile.imageApiKey || "";
    $("codexImageBaseUrl").value = profile.imageBaseUrl || "https://hk.getelucid.com/v1";
    setCodexAuthMode(profile.authMode || "auth_json");
    setCodexWizardStep(3);
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

// ── Codex 分步向导 ────────────────────────────────────────
function setCodexWizardStep(step) {
  codexWizardStep = Math.min(5, Math.max(1, step));
  document.querySelectorAll("#codexWizardStepper .wizard-step-indicator").forEach((el) => {
    const n = Number(el.getAttribute("data-wizard-step"));
    el.classList.toggle("active", n === codexWizardStep);
    el.classList.toggle("done", n < codexWizardStep);
  });
  document.querySelectorAll("#codexProfileForm .wizard-step-panel").forEach((panel) => {
    panel.classList.toggle("active", Number(panel.getAttribute("data-wizard-panel")) === codexWizardStep);
  });
  const back = $("codexWizardBackBtn");
  const next = $("codexWizardNextBtn");
  const submit = $("codexSubmitBtn");
  const enable = $("codexSaveEnableBtn");
  if (back) back.style.visibility = codexWizardStep === 1 ? "hidden" : "visible";
  if (next) next.style.display = codexWizardStep < 5 ? "" : "none";
  if (submit) submit.style.display = codexWizardStep === 5 ? "" : "none";
  if (enable) enable.style.display = codexWizardStep === 5 ? "" : "none";
  if (codexWizardStep === 5) renderCodexWizardReview();
}

function validateCodexWizardStep(step) {
  if (step === 3) {
    const name = $("codexProfileName")?.value.trim();
    const baseUrl = $("codexBaseUrl")?.value.trim();
    const apiKey = $("codexApiKey")?.value.trim();
    const mode = getCodexAuthMode();
    if (!name) {
      showToast(currentLang === "zh" ? "请填写配置名称" : "Config name is required", "warning");
      return false;
    }
    if (!baseUrl) {
      showToast(currentLang === "zh" ? "请填写 Base URL" : "Base URL is required", "warning");
      return false;
    }
    if (mode !== "official_account_api_quota" && mode !== "save_only" && !apiKey) {
      showToast(currentLang === "zh" ? "当前写入方式需要 API Key" : "API Key is required for this write mode", "warning");
      return false;
    }
  }
  return true;
}

function renderCodexWizardReview() {
  const host = $("codexWizardReview");
  if (!host) return;
  const mode = getCodexAuthMode();
  const modeLabel =
    mode === "official_account_api_quota"
      ? (currentLang === "zh" ? "仅配置文件" : "Config only")
      : mode === "save_only"
        ? (currentLang === "zh" ? "仅保存" : "Save only")
        : (currentLang === "zh" ? "默认写入" : "Default write");
  const rows = [
    [currentLang === "zh" ? "名称" : "Name", $("codexProfileName")?.value || "--"],
    ["API Key", maskKey($("codexApiKey")?.value || "")],
    ["Base URL", $("codexBaseUrl")?.value || "--"],
    ["Model", $("codexModel")?.value || "--"],
    ["Provider", $("codexProvider")?.value || "--"],
    [currentLang === "zh" ? "写入方式" : "Write mode", modeLabel],
  ];
  host.innerHTML = rows
    .map(([k, v]) => `<div class="wizard-review-item"><small>${esc(k)}</small><strong>${esc(String(v))}</strong></div>`)
    .join("");
}

async function runCodexWizardTests() {
  const apiKey = $("codexApiKey")?.value.trim() || "";
  const baseUrl = $("codexBaseUrl")?.value.trim() || "";
  const model = $("codexModel")?.value.trim() || "";
  const keyOk = apiKey.length >= 8 || getCodexAuthMode() === "official_account_api_quota" || getCodexAuthMode() === "save_only";
  setBadge($("codexTestKey"), keyOk ? (currentLang === "zh" ? "正常" : "OK") : (currentLang === "zh" ? "异常" : "Fail"), keyOk ? "healthy" : "error");

  let urlOk = false;
  try {
    if (baseUrl) {
      setBadge($("codexTestUrl"), currentLang === "zh" ? "检测中" : "Testing", "warning");
      await handleEndpointTest("codex");
      urlOk = true;
    }
  } catch (_) {
    urlOk = false;
  }
  setBadge($("codexTestUrl"), urlOk ? (currentLang === "zh" ? "正常" : "OK") : (currentLang === "zh" ? "异常" : "Fail"), urlOk ? "healthy" : "error");
  setBadge($("codexTestModel"), model ? (currentLang === "zh" ? "已填写" : "Set") : (currentLang === "zh" ? "可选" : "Optional"), model ? "healthy" : "");
  setBadge($("codexTestPerm"), currentLang === "zh" ? "正常" : "OK", "healthy");
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
      nav.classList.contains("metric-card") ||
      nav.classList.contains("sidebar-item") ||
      nav.classList.contains("quick-action") ||
      nav.classList.contains("sidebar-settings")
    ) {
      event.preventDefault();
      switchConsolePage(page);
    }
  });

  $("overviewAddBtn")?.addEventListener("click", () => openConfigTypeDialog("add"));
  $("overviewImportBtn")?.addEventListener("click", () => openConfigTypeDialog("import"));
  $("overviewSyncBtn")?.addEventListener("click", async () => {
    const activeCodex = codexProfiles.find((p) => p.isActive);
    const activeClaude = profiles.find((p) => p.isActive);
    if (activeCodex) await handleCodexSwitch(activeCodex.id);
    else if (activeClaude) await handleSwitch(activeClaude.id);
    else showToast(currentLang === "zh" ? "没有可同步的活动配置" : "No active config to sync", "warning");
    renderOverviewDashboard();
  });
  $("quickClaudeAdd")?.addEventListener("click", () => {
    switchConsolePage("claude");
    openModal(null);
  });
  $("quickCodexAdd")?.addEventListener("click", () => {
    switchConsolePage("codex");
    openCodexModal(null);
  });
  $("configAddBtn")?.addEventListener("click", () => openConfigTypeDialog("add"));
  $("configImportBtn")?.addEventListener("click", () => openConfigTypeDialog("import"));
  $("claudePageAddBtn")?.addEventListener("click", () => openModal(null));
  $("claudePageImportBtn")?.addEventListener("click", handleImport);
  $("claudeRefreshBtn")?.addEventListener("click", async () => {
    await Promise.all([loadStatus(), loadProfiles()]);
    showToast(currentLang === "zh" ? "Claude 状态已刷新" : "Claude status refreshed", "success");
  });
  $("claudeSyncBtn")?.addEventListener("click", handleSyncNow);
  $("codexPageAddBtn")?.addEventListener("click", () => openCodexModal(null));
  $("codexPageImportBtn")?.addEventListener("click", handleCodexImport);
  $("codexCardSyncBtn")?.addEventListener("click", () => {
    const active = codexProfiles.find((profile) => profile.isActive);
    if (active) handleCodexSwitch(active.id);
  });
  $("codexOpenDirBtn")?.addEventListener("click", async () => {
    try {
      await invoke("open_codex_dir");
    } catch (error) {
      // 回退到设置里的路径打开
      try {
        await invoke("open_path", { pathType: "codex" });
      } catch (e2) {
        showToast(String(error || e2), "error");
      }
    }
  });
  $("codexToggleDiagBtn")?.addEventListener("click", () => {
    codexDiagExpanded = !codexDiagExpanded;
    $("codexToggleDiagBtn").textContent = codexDiagExpanded
      ? (currentLang === "zh" ? "收起诊断详情" : "Hide diagnostics")
      : (currentLang === "zh" ? "查看诊断详情" : "View diagnostics");
    renderCodexCurrentCard();
  });
  $("sessionPageSyncBtn")?.addEventListener("click", handleToolboxSessionSync);

  $("configurationSearch")?.addEventListener("input", (event) => {
    configurationSearch = event.target.value || "";
    renderConfigurationList();
  });
  $("configurationFilter")?.querySelectorAll("button[data-config-filter]").forEach((btn) => {
    btn.addEventListener("click", () => {
      configurationFilter = btn.getAttribute("data-config-filter") || "all";
      $("configurationFilter").querySelectorAll("button").forEach((b) => b.classList.toggle("active", b === btn));
      renderConfigurationList();
    });
  });
  $("pluginSearchInput")?.addEventListener("input", (event) => {
    pluginSearch = event.target.value || "";
    applyPluginFilters();
  });
  $("pluginFilter")?.querySelectorAll("button[data-plugin-filter]").forEach((btn) => {
    btn.addEventListener("click", () => {
      pluginFilter = btn.getAttribute("data-plugin-filter") || "all";
      $("pluginFilter").querySelectorAll("button").forEach((b) => b.classList.toggle("active", b === btn));
      applyPluginFilters();
    });
  });

  $("settingsLangZh")?.addEventListener("click", () => setLanguage("zh"));
  $("settingsLangEn")?.addEventListener("click", () => setLanguage("en"));
  $("settingsThemeLight")?.addEventListener("click", () => setTheme("light"));
  $("settingsThemeDark")?.addEventListener("click", () => setTheme("dark"));
  $("settingsGuideBtn")?.addEventListener("click", () => openUsageGuide());
  $("settingsUpdateBtn")?.addEventListener("click", () => handleUpdateButton());
  $("settingsDownloadBtn")?.addEventListener("click", () => openUpdateReleasePage());
  $("settingsGithubBtn")?.addEventListener("click", () => openGitHubRepo());
  $("settingsSkillsBtn")?.addEventListener("click", () => openSkillsPanel());
  $("settingsPromptsBtn")?.addEventListener("click", () => openPromptsPanel());
  $("settingsMcpBtn")?.addEventListener("click", () => openMcpPanel());

  // Codex wizard controls
  $("codexWizardNextBtn")?.addEventListener("click", async () => {
    if (!validateCodexWizardStep(codexWizardStep)) return;
    if (codexWizardStep === 4) await runCodexWizardTests();
    setCodexWizardStep(codexWizardStep + 1);
  });
  $("codexWizardBackBtn")?.addEventListener("click", () => setCodexWizardStep(codexWizardStep - 1));
  $("codexWizardRunTestBtn")?.addEventListener("click", () => runCodexWizardTests());
  $("codexSaveEnableBtn")?.addEventListener("click", async () => {
    codexWizardEnableAfterSave = true;
    $("codexSubmitBtn")?.click();
  });
  $("codexApiKeyToggle")?.addEventListener("click", () => {
    const input = $("codexApiKey");
    if (!input) return;
    const show = input.type === "password";
    input.type = show ? "text" : "password";
    $("codexApiKeyToggle").textContent = show ? (currentLang === "zh" ? "隐藏" : "Hide") : (currentLang === "zh" ? "显示" : "Show");
  });
  document.querySelectorAll("#codexWizardStepper .wizard-step-indicator").forEach((el) => {
    el.addEventListener("click", () => {
      const step = Number(el.getAttribute("data-wizard-step"));
      if (step < codexWizardStep) setCodexWizardStep(step);
    });
  });
}

function enhanceAfterDataLoad() {
  mountToolboxPages();
  renderOverviewDashboard();
  applyPluginFilters();
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
  $("configTypeOverlay")?.addEventListener("click", (event) => {
    if (event.target === $("configTypeOverlay")) closeConfigTypeDialog();
  });
  $("configTypeOverlay")?.querySelectorAll("[data-config-kind]").forEach((button) => {
    button.addEventListener("click", () => runConfigTypeAction(button.getAttribute("data-config-kind")));
  });
}


function triggerCurrentAdd() {
  if (activeConsolePage === "codex") openCodexModal(null);
  else if (activeConsolePage === "grok") openGrokModal(null);
  else if (activeConsolePage === "claude") openModal(null);
  else openConfigTypeDialog("add");
}

function triggerCurrentImport() {
  if (activeConsolePage === "codex") handleCodexImport();
  else if (activeConsolePage === "grok") handleGrokImport();
  else if (activeConsolePage === "claude") handleImport();
  else openConfigTypeDialog("import");
}

$("codexRefreshDiagnosticsBtn")?.addEventListener("click", loadCodexDiagnostics);
$("codexBackupRuntimeBtn")?.addEventListener("click", handleCodexRuntimeBackup);
$("heroToolboxBtn")?.addEventListener("click", openCodexToolbox);
$("codexToolboxOpenBtn")?.addEventListener("click", openCodexToolbox);
on("langZhBtn", "click", () => setLanguage("zh"));
on("langEnBtn", "click", () => setLanguage("en"));
on("themeLightBtn", "click", () => setTheme("light"));
on("themeDarkBtn", "click", () => setTheme("dark"));
on("cancelBtn", "click", closeModal);
on("modalClose", "click", closeModal);
on("switchCancelBtn", "click", handleCancelSwitch);
on("profileForm", "submit", handleSubmit);
on("profileBaseUrl", "focus", () => tryClipboardAutoFill("url", "profileBaseUrl"));
on("profileApiKey", "focus", () => tryClipboardAutoFill("key", "profileApiKey"));
on("profileModelFetchBtn", "click", () => handleModelFetch("claude"));
on("profileEndpointTestBtn", "click", () => handleEndpointTest("claude"));
on("modalOverlay", "click", (event) => {
  if (event.target === $("modalOverlay")) {
    closeModal();
  }
});

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
on("codexModalOverlay", "click", (event) => {
  if (event.target === $("codexModalOverlay")) closeCodexModal();
});

// ── Grok Modal Event Listeners ──────────────────────

$("grokCancelBtn")?.addEventListener("click", closeGrokModal);
$("grokModalClose")?.addEventListener("click", closeGrokModal);
$("grokProfileForm")?.addEventListener("submit", handleGrokSubmit);
$("grokPresetSelect")?.addEventListener("change", () => applyGrokPreset(getSelectedGrokPreset()));
$("grokBaseUrl")?.addEventListener("focus", () => tryClipboardAutoFill("url", "grokBaseUrl"));
$("grokApiKey")?.addEventListener("focus", () => tryClipboardAutoFill("key", "grokApiKey"));
$("grokModelFetchBtn")?.addEventListener("click", () => handleModelFetch("grok"));
$("grokEndpointTestBtn")?.addEventListener("click", () => handleEndpointTest("grok"));
$("grokModalOverlay")?.addEventListener("click", (event) => {
  if (event.target === $("grokModalOverlay")) closeGrokModal();
});
$("grokSyncNowBtn")?.addEventListener("click", () => {
  const active = grokProfiles.find((p) => p.isActive);
  if (active) handleGrokSwitch(active.id);
});
$("grokPageAddBtn")?.addEventListener("click", () => openGrokModal(null));
$("grokRefreshBtn")?.addEventListener("click", async () => {
  await Promise.all([loadGrokProfiles(), loadGrokStatus(), loadGrokDiagnostics()]);
  showToast(currentLang === "zh" ? "Grok 状态已刷新" : "Grok status refreshed", "success");
});
$("grokOpenFolderBtn")?.addEventListener("click", handleOpenGrokFolder);
$("grokBackupRuntimeBtn")?.addEventListener("click", handleGrokRuntimeBackup);
$("grokPageImportBtn")?.addEventListener("click", handleGrokImport);
$("quickGrokAdd")?.addEventListener("click", () => {
  switchPage("grok");
  openGrokModal(null);
});
$("codexToolboxBtn")?.addEventListener("click", openCodexToolbox);
on("codexToolboxClose", "click", closeCodexToolbox);
on("codexToolboxOverlay", "click", (event) => {
  if (event.target === $("codexToolboxOverlay")) closeCodexToolbox();
});
on("toolboxTabMarket", "click", () => switchToolboxTab("market"));
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
    renderOverviewDashboard();
  } catch (error) {
    finishToolboxSessionProgress(false);
    showToast(String(error), "error");
  }
}
$("builtinPluginRepairBtn")?.addEventListener("click", handleRepairOpenAiBundledPlugins);
$("builtinPluginEnableImportantBtn")?.addEventListener("click", handleEnableImportantBuiltinPlugins);
on("toolboxMarketApplyBtn", "click", async () => {
  if (marketplaceInstallBusy) return;
  setMarketplaceInstallBusy(true);
  showMarketplaceProgress();
  try {
    if (marketplaceProgressUnlisten) {
      marketplaceProgressUnlisten();
      marketplaceProgressUnlisten = null;
    }
    marketplaceProgressUnlisten = await listen("plugin-marketplace-progress", (event) => {
      updateMarketplaceProgress(event.payload);
    });
    codexToolbox = await invoke("apply_plugin_marketplace", {
      source: $("toolboxMarketplaceInput").value.trim(),
    });
    updateMarketplaceProgress({ step: 6, total: 6, label: "done" });
    showToast(t("toolboxMarketplaceApplied"), "success");
    renderCodexToolbox();
  } catch (error) {
    showToast(String(error), "error");
  } finally {
    if (marketplaceProgressUnlisten) {
      marketplaceProgressUnlisten();
      marketplaceProgressUnlisten = null;
    }
    setMarketplaceInstallBusy(false);
  }
});
on("toolboxMarketplaceInput", "change", () => {
  const option =
    helpers.getCodexPluginMarketplaceOption?.($("toolboxMarketplaceInput").value) ||
    {};
  $("toolboxMarketplaceDesc").textContent =
    currentLang === "zh" ? option.zh || "" : option.en || option.zh || "";
});
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
on("skillsOverlay", "click", (event) => {
  if (event.target === $("skillsOverlay")) closeSkillsPanel();
});

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
on("repoManagerOverlay", "click", (event) => {
  if (event.target === $("repoManagerOverlay")) closeRepoManager();
});
on("addRepoBtn", "click", handleAddRepo);
on("repoUrlInput", "keydown", (e) => {
  if (e.key === "Enter") handleAddRepo();
});

on("promptsBtn", "click", openPromptsPanel);
on("promptsClose", "click", closePromptsPanel);
on("promptSaveBtn", "click", handleSavePrompt);
on("promptsOverlay", "click", (event) => {
  if (event.target === $("promptsOverlay")) closePromptsPanel();
});
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
on("mcpOverlay", "click", (event) => {
  if (event.target === $("mcpOverlay")) closeMcpPanel();
});
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
on("usageGuideOverlay", "click", (event) => {
  if (event.target === $("usageGuideOverlay")) closeUsageGuide();
});

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
on("settingsOverlay", "click", (event) => {
  if (event.target === $("settingsOverlay")) closeSettingsPanel();
});
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
on("settingsExportBtn", "click", handleExportProfiles);
on("settingsImportBtn", "click", handleImportProfiles);
on("settingsOpenBackupsBtn", "click", () => {
  invoke("open_backups_folder").catch((error) => showToast(String(error), "error"));
});
on("settingsViewBackupsBtn", "click", toggleBackupList);
on("settingsBackupList", "click", async (event) => {
  const btn = event.target.closest("[data-restore-backup]");
  if (!btn) return;
  const name = btn.getAttribute("data-restore-backup");
  if (!(await appConfirm(currentLang === "zh" ? "确定用这个备份覆盖当前配置吗？当前配置会先自动备份。" : "Overwrite current profiles with this backup? A safety backup will be created first.", { title: currentLang === "zh" ? "恢复备份" : "Restore backup", danger: true, confirmText: currentLang === "zh" ? "覆盖恢复" : "Restore" }))) return;
  try {
    await invoke("restore_config_backup", { name });
    showToast("已从备份恢复配置", "success");
    await loadProfiles();
    await loadCodexProfiles();
    $("settingsBackupList").style.display = "none";
  } catch (error) {
    showToast(String(error), "error");
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
    bindConfigTypeDialogOnce();
    bindConsoleUiOnce();
    mountToolboxPages();
    switchConsolePage("overview");
    // 先渲染一次“空状态”，避免一直停在 HTML 初始的“检查中”
    if (typeof renderOverviewDashboard === "function") renderOverviewDashboard();
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
    safeLoad("loadCodexDiagnostics", () => loadCodexDiagnostics()),
    safeLoad("loadGrokProfiles", () => loadGrokProfiles()),
    safeLoad("loadGrokStatus", () => loadGrokStatus()),
    safeLoad("loadGrokDiagnostics", () => loadGrokDiagnostics()),
    safeLoad("loadCodexToolbox", () => loadCodexToolbox(), 10000),
    safeLoad("loadAppSettings", () => loadAppSettings()),
  ]);

  try {
    renderGrokPresetOptions();
    renderUpdateButton();
    enhanceAfterDataLoad();
    switchConsolePage("overview");
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
