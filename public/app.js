const $ = (id) => document.getElementById(id);
const tauriApi = window.__TAURI__;

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

const LANG_STORAGE_KEY = "varswitch.lang";
const THEME_STORAGE_KEY = "varswitch.theme";

const I18N = {
  en: {
    appTitle: "VarSwitch",
    appSubtitle: "Environment Sync Manager",
    importBtn: "Import Current",
    addBtn: "+ Add Config",
    statusTitle: "Current Status",
    statusHint: "Restart terminal and VSCode after switching to apply environment variables.",
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
    stepVscode: "VSCode",
    stepClaude: "Claude",
    progressSystem: "Updating system environment variables...",
    progressVscode: "Updating VSCode settings...",
    progressClaude: "Updating Claude settings...",
    progressFinalize: "Finalizing switch...",
    progressDone: "Done",
    progressCancelling: "Cancelling...",
    statusSystemEnv: "System Environment",
    statusVscode: "VSCode Settings",
    statusClaude: "Claude Settings",
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
    settingsVscodePath: "VSCode settings",
    settingsOpen: "Open",
    settingsExport: "Export Profiles",
    settingsImport: "Import Profiles",
    toastSettingsSaved: "Settings saved",
    toastExported: "Profiles exported",
    toastImported2: "{count} profile(s) imported",
    toastImportNone: "No new profiles to import",
    settingsSilentStart: "Silent startup",
    settingsSilentStartDesc: "Start minimized to system tray",
    settingsLang: "Language",
    settingsTheme: "Theme"
  },
  zh: {
    appTitle: "VarSwitch",
    appSubtitle: "环境变量同步工具",
    importBtn: "导入当前配置",
    addBtn: "+ 添加配置",
    statusTitle: "当前配置状态",
    statusHint: "切换后请重启终端和 VSCode，使环境变量生效。",
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
    stepVscode: "VSCode",
    stepClaude: "Claude",
    progressSystem: "正在更新系统环境变量...",
    progressVscode: "正在更新 VSCode 设置...",
    progressClaude: "正在更新 Claude 设置...",
    progressFinalize: "正在收尾...",
    progressDone: "完成",
    progressCancelling: "正在取消...",
    statusSystemEnv: "系统环境变量",
    statusVscode: "VSCode 设置",
    statusClaude: "Claude 设置",
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
    settingsVscodePath: "VSCode 设置",
    settingsOpen: "打开",
    settingsExport: "导出配置",
    settingsImport: "导入配置",
    toastSettingsSaved: "设置已保存",
    toastExported: "配置已导出",
    toastImported2: "已导入 {count} 个配置",
    toastImportNone: "没有新配置可导入",
    settingsSilentStart: "静默启动",
    settingsSilentStartDesc: "启动时最小化到系统托盘",
    settingsLang: "语言",
    settingsTheme: "主题"
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
let editingId = null;
let switchingSnapshot = null;
let progressUnlisten = null;
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
let isDiscovering = false;
let promptTemplates = [];
let activePromptTab = "editor";
let isShowingGithubSkills = false;

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

function maskKey(key) {
  if (!key || key.length < 12) return key || "--";
  return `${key.slice(0, 6)}****${key.slice(-4)}`;
}

function truncUrl(url, max = 40) {
  if (!url) return "--";
  return url.length > max ? `${url.slice(0, max)}...` : url;
}

function updateThemeSegControl() {
  const lightBtn = $("themeLightBtn");
  const darkBtn = $("themeDarkBtn");
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
  document.documentElement.lang = currentLang === "zh" ? "zh-CN" : "en";
  document.title = t("appTitle");

  $("appTitle").textContent = t("appTitle");
  $("appSubtitle").textContent = t("appSubtitle");
  $("importBtnText").textContent = t("importBtn");
  $("addBtn").textContent = t("addBtn");
  $("statusSectionTitle").textContent = t("statusTitle");
  $("statusHint").textContent = t("statusHint");
  $("profilesSectionTitle").textContent = t("profilesTitle");
  $("profileNameLabel").textContent = t("nameLabel");
  $("profileApiKeyLabel").textContent = t("tokenLabel");
  $("profileBaseUrlLabel").textContent = t("urlLabel");
  $("cancelBtn").textContent = t("cancel");
  $("submitBtn").textContent = t("save");
  $("switchPanelTitle").textContent = t("switchingTo");
  $("switchStep1Text").textContent = t("stepSystem");
  $("switchStep2Text").textContent = t("stepVscode");
  $("switchStep3Text").textContent = t("stepClaude");
  $("switchCancelBtn").textContent = t("cancelSwitch");
  $("switchStepLabel").textContent = t("preparing");
  $("activeConfigLabel").textContent = t("activeConfigLabel");
  $("syncNowBtnText").textContent = t("syncNow");

  $("profileName").placeholder = t("placeholderName");
  $("profileApiKey").placeholder = t("placeholderApiKey");
  $("profileBaseUrl").placeholder = t("placeholderBaseUrl");

  // Management panel labels
  $("skillsBtn").title = t("skillsManage");
  $("promptsBtn").title = t("promptsManage");
  $("mcpBtn").title = t("mcpManage");
  $("skillsTitle").textContent = t("skillsTitle");
  $("addSkillBtn").textContent = t("addSkill");
  $("skillNameLabel2").textContent = t("skillName");
  $("skillContentLabel").textContent = t("skillContent");
  $("skillNameInput").placeholder = t("skillNamePlaceholder");
  $("skillCancelBtn").textContent = t("cancel");
  $("skillSaveBtn").textContent = t("save");
  $("promptsTitle2").textContent = t("promptsTitle");
  $("promptsPath").textContent = t("promptsPathLabel");
  $("promptSaveBtn").textContent = t("save");
  $("mcpTitle2").textContent = t("mcpTitle");
  $("mcpTabInstalled").textContent = t("mcpTabInstalled");
  $("mcpTabPresets").textContent = t("mcpTabPresets");
  $("mcpPresetSearch").placeholder = t("mcpSearchPlaceholder");
  $("mcpPresetLoadingText").textContent = t("mcpSearchLoading");
  $("mcpPath").textContent = t("mcpPathLabel");
  $("addMcpBtn").textContent = t("addMcp");
  $("mcpNameLabel2").textContent = t("mcpName");
  $("mcpConfigLabel").textContent = t("mcpConfig");
  $("mcpNameInput").placeholder = t("mcpNamePlaceholder");
  $("mcpCancelBtn").textContent = t("cancel");
  $("mcpSaveBtn").textContent = t("save");

  // Skills Discovery labels
  $("skillsTabInstalled").textContent = t("skillsTabInstalled");
  $("skillsTabDiscover").textContent = t("skillsTabDiscover");
  $("discoverSearch").placeholder = t("discoverSearchPlaceholder");
  $("discoverLoadingText").textContent = t("discoverLoading");
  $("manageReposBtn").textContent = t("manageRepos");
  $("repoManagerTitle").textContent = t("repoManagerTitle");
  $("addRepoLabel").textContent = t("addRepoLabel");
  $("repoUrlInput").placeholder = t("addRepoPlaceholder");
  $("searchGithubSkillsBtnText").textContent = t("skillsSearchGithub");
  $("discoverGithubBannerText").textContent = t("skillsGithubResults");
  $("backToCatalogBtnText").textContent = t("skillsBackToCatalog");

  // Prompt tabs
  $("promptTabEditor").textContent = t("promptTabEditor");
  $("promptTabTemplates").textContent = t("promptTabTemplates");
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
  $("settingsBtn").title = t("settingsTitle");
  $("settingsTitle2").textContent = t("settingsTitle");
  $("settingsGroupGeneral").textContent = t("settingsGroupGeneral");
  $("settingsGroupPaths").textContent = t("settingsGroupPaths");
  $("settingsGroupBackup").textContent = t("settingsGroupBackup");
  $("settingsAutoStartLabel").textContent = t("settingsAutoStart");
  $("settingsAutoStartDesc").textContent = t("settingsAutoStartDesc");
  $("settingsMinTrayLabel").textContent = t("settingsMinTray");
  $("settingsMinTrayDesc").textContent = t("settingsMinTrayDesc");
  $("settingsConfigDirLabel").textContent = t("settingsConfigDir");
  $("settingsClaudePathLabel").textContent = t("settingsClaudePath");
  $("settingsVscodePathLabel").textContent = t("settingsVscodePath");
  $("settingsOpenConfigDir").textContent = t("settingsOpen");
  $("settingsOpenClaudeDir").textContent = t("settingsOpen");
  $("settingsOpenVscodeDir").textContent = t("settingsOpen");
  $("settingsExportBtn").textContent = t("settingsExport");
  $("settingsImportBtn").textContent = t("settingsImport");
  $("settingsSilentStartLabel").textContent = t("settingsSilentStart");
  $("settingsSilentStartDesc").textContent = t("settingsSilentStartDesc");

  updateLangSegControl();
  updateThemeSegControl();

  if ($("modalOverlay").classList.contains("open")) {
    $("modalTitle").textContent = editingId ? t("editConfig") : t("addConfig");
  }
}

function setLanguage(lang) {
  currentLang = lang;
  localStorage.setItem(LANG_STORAGE_KEY, currentLang);
  applyLanguage();
  renderProfiles();
  loadStatus();
}

function setTheme(theme) {
  currentTheme = theme;
  localStorage.setItem(THEME_STORAGE_KEY, currentTheme);
  applyTheme();
}

function showSwitchOverlay(profileName) {
  $("switchProfileName").textContent = profileName;
  $("switchProgressBar").style.width = "0%";
  $("switchStepLabel").textContent = t("preparing");
  $("switchProgressPercent").textContent = "0%";
  $("switchStep1").className = "switch-step";
  $("switchStep2").className = "switch-step";
  $("switchStep3").className = "switch-step";
  $("switchCancelBtn").disabled = false;
  $("switchOverlay").classList.add("open");
}

function hideSwitchOverlay() {
  $("switchOverlay").classList.remove("open");
}

function switchProgressLabel(step) {
  if (step <= 1) return t("preparing");
  if (step === 2) return t("progressSystem");
  if (step === 3) return t("progressVscode");
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
    vscode: t("progressVscode"),
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

async function loadStatus() {
  try {
    const status = await invoke("get_status");
    const grid = $("statusGrid");

    const locations = [
      { key: "envVars", title: t("statusSystemEnv") },
      { key: "vscode", title: t("statusVscode") },
      { key: "claude", title: t("statusClaude") }
    ];

    const keys = locations.map((l) => status[l.key]?.apiKey).filter(Boolean);
    const urls = locations.map((l) => status[l.key]?.baseUrl).filter(Boolean);
    const synced = keys.length > 0 && new Set(keys).size <= 1 && new Set(urls).size <= 1;

    const COPY_ICON = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>`;

    grid.innerHTML = locations.map((loc) => {
      const item = status[loc.key];
      if (!item) {
        return `
          <div class="status-card error-card">
            <div class="status-card-title">
              <span class="status-card-title-text">${loc.title}</span>
            </div>
            <div style="font-size:13px;color:var(--error-text)">${t("readFailed")}</div>
          </div>`;
      }

      const badgeClass = synced ? "synced" : "unsynced";
      const badgeText = synced ? t("synced") : t("unsynced");
      const dotColor = synced ? "var(--success-text)" : "var(--warning-text)";

      return `
        <div class="status-card">
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
    }).join("");

    grid.querySelectorAll(".copy-btn").forEach((btn) => {
      btn.addEventListener("click", () => {
        const text = btn.getAttribute("data-copy");
        if (text) {
          navigator.clipboard.writeText(text).then(() => {
            showToast(t("toastCopied"), "success");
          });
        }
      });
    });
  } catch (error) {
    showToast(t("loadStatusFailed", { error: String(error) }), "error");
  }
}

async function loadProfiles() {
  try {
    const data = await invoke("get_profiles");
    profiles = data.profiles || [];
    renderProfiles();
  } catch (error) {
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
        <div class="empty-state-actions">
          <button class="btn btn-primary" id="addFirstBtn" type="button">${t("addFirstConfig")}</button>
          <button class="btn btn-secondary" id="importFirstBtn" type="button">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/></svg>
            ${t("importBtn")}
          </button>
        </div>
      </div>`;
    const addFirstBtn = $("addFirstBtn");
    if (addFirstBtn) {
      addFirstBtn.addEventListener("click", () => $("addBtn").click());
    }
    const importFirstBtn = $("importFirstBtn");
    if (importFirstBtn) {
      importFirstBtn.addEventListener("click", handleImport);
    }
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

function openModal(profile) {
  editingId = profile ? profile.id : null;
  $("modalTitle").textContent = profile ? t("editConfig") : t("addConfig");
  $("profileId").value = editingId || "";
  $("profileName").value = profile ? profile.name : "";
  $("profileApiKey").value = profile ? profile.apiKey : "";
  $("profileBaseUrl").value = profile ? profile.baseUrl : "";
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

  try {
    if (editingId) {
      await invoke("update_profile", { id: editingId, name, apiKey, baseUrl });
      showToast(t("toastUpdated"), "success");
    } else {
      await invoke("add_profile", { name, apiKey, baseUrl });
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
  const profile = profiles.find((item) => item.id === id);
  if (!profile) return;

  showSwitchOverlay(profile.name);

  try {
    switchingSnapshot = await invoke("snapshot_config");
  } catch (error) {
    hideSwitchOverlay();
    showToast(t("snapshotFailed", { error: String(error) }), "error");
    return;
  }

  progressUnlisten = await listen("switch-progress", (event) => {
    updateSwitchProgress(event.payload);
  });

  try {
    const result = await invoke("switch_profile", { id });

    if (result.success) {
      $("switchProgressBar").style.width = "100%";
      $("switchProgressPercent").textContent = "100%";
      $("switchStep1").className = "switch-step done";
      $("switchStep2").className = "switch-step done";
      $("switchStep3").className = "switch-step done";
      $("switchStepLabel").textContent = t("switchDone");
      await new Promise((resolve) => setTimeout(resolve, 350));
    }

    hideSwitchOverlay();

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
      const locationNames = {
        envVars: t("statusSystemEnv"),
        vscode: t("statusVscode"),
        claude: t("statusClaude")
      };
      const ok = Object.entries(result.results || {})
        .filter(([, success]) => Boolean(success))
        .map(([name]) => locationNames[name] || name);
      showToast(
        t("partialSuccess", {
          ok: ok.join(", ") || "--",
          errors: (result.errors || []).join("; ") || "--"
        }),
        "warning"
      );
    }
  } catch (error) {
    hideSwitchOverlay();
    showToast(t("switchFailed", { error: String(error) }), "error");
  } finally {
    if (progressUnlisten) {
      progressUnlisten();
      progressUnlisten = null;
    }
    switchingSnapshot = null;
    await loadProfiles();
    await loadStatus();
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

  const dialog = window.__TAURI_PLUGIN_DIALOG__;
  const confirmed = await dialog.ask(t("confirmDelete", { name: profile.name }), {
    title: t("delete"),
    kind: "warning",
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
  const input = window.prompt(t("importPrompt"), t("importDefaultName"));
  if (input === null) return;

  const name = input.trim() || t("importDefaultName");
  try {
    await invoke("import_current", { name });
    showToast(t("toastImported"), "success");
    await loadProfiles();
    await loadStatus();
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
  const dialog = window.__TAURI_PLUGIN_DIALOG__;
  const confirmed = await dialog.ask(t("confirmDeleteSkill", { name }), {
    title: t("delete"),
    kind: "warning",
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
  const dialog = window.__TAURI_PLUGIN_DIALOG__;
  const confirmed = await dialog.ask(t("confirmDeleteMcp", { name }), {
    title: t("delete"),
    kind: "warning",
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
  const container = $("toastContainer");
  const toast = document.createElement("div");
  toast.className = `toast ${type}`;
  toast.textContent = String(message);
  container.appendChild(toast);

  setTimeout(() => {
    toast.style.opacity = "0";
    setTimeout(() => toast.remove(), 300);
  }, 3200);
}

$("addBtn").addEventListener("click", () => openModal(null));
$("importBtn").addEventListener("click", handleImport);
$("langZhBtn").addEventListener("click", () => setLanguage("zh"));
$("langEnBtn").addEventListener("click", () => setLanguage("en"));
$("themeLightBtn").addEventListener("click", () => setTheme("light"));
$("themeDarkBtn").addEventListener("click", () => setTheme("dark"));
$("cancelBtn").addEventListener("click", closeModal);
$("modalClose").addEventListener("click", closeModal);
$("switchCancelBtn").addEventListener("click", handleCancelSwitch);
$("profileForm").addEventListener("submit", handleSubmit);
$("syncNowBtn").addEventListener("click", handleSyncNow);
$("modalOverlay").addEventListener("click", (event) => {
  if (event.target === $("modalOverlay")) {
    closeModal();
  }
});

// ── Management Panel Event Listeners ────────────────

$("skillsBtn").addEventListener("click", openSkillsPanel);
$("skillsClose").addEventListener("click", closeSkillsPanel);
$("addSkillBtn").addEventListener("click", () => showSkillsEdit(null, "command"));
$("skillCancelBtn").addEventListener("click", hideSkillsEdit);
$("skillSaveBtn").addEventListener("click", handleSaveSkill);
$("skillsOverlay").addEventListener("click", (event) => {
  if (event.target === $("skillsOverlay")) closeSkillsPanel();
});

// Skills tabs
$("skillsTabInstalled").addEventListener("click", () => switchSkillsTab("installed"));
$("skillsTabDiscover").addEventListener("click", () => switchSkillsTab("discover"));

// Skills discover search

// Discover search and filters
$("discoverSearch").addEventListener("input", (e) => {
  discoverSearchQuery = e.target.value;
  // Local filter for catalog mode
  renderDiscoverGrid();
});
$("discoverSearch").addEventListener("keydown", (e) => {
  if (e.key === "Enter") {
    e.preventDefault();
    searchGitHubSkills();
  }
});
$("searchGithubSkillsBtn").addEventListener("click", searchGitHubSkills);
$("backToCatalogBtn").addEventListener("click", backToCatalog);
$("discoverRepoFilter").addEventListener("change", (e) => {
  discoverRepoFilter = e.target.value;
  renderDiscoverGrid();
});
$("discoverStatusFilter").addEventListener("change", (e) => {
  discoverStatusFilter = e.target.value;
  renderDiscoverGrid();
});

// Repo manager
$("manageReposBtn").addEventListener("click", openRepoManager);
$("refreshDiscoverBtn").addEventListener("click", () => {
  backToCatalog();
});
$("repoManagerClose").addEventListener("click", closeRepoManager);
$("repoManagerOverlay").addEventListener("click", (event) => {
  if (event.target === $("repoManagerOverlay")) closeRepoManager();
});
$("addRepoBtn").addEventListener("click", handleAddRepo);
$("repoUrlInput").addEventListener("keydown", (e) => {
  if (e.key === "Enter") handleAddRepo();
});

$("promptsBtn").addEventListener("click", openPromptsPanel);
$("promptsClose").addEventListener("click", closePromptsPanel);
$("promptSaveBtn").addEventListener("click", handleSavePrompt);
$("promptsOverlay").addEventListener("click", (event) => {
  if (event.target === $("promptsOverlay")) closePromptsPanel();
});
$("promptTabEditor").addEventListener("click", () => switchPromptTab("editor"));
$("promptTabTemplates").addEventListener("click", () => switchPromptTab("templates"));
$("promptInsertSelect").addEventListener("change", (e) => {
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

$("mcpBtn").addEventListener("click", openMcpPanel);
$("mcpClose").addEventListener("click", closeMcpPanel);
$("addMcpBtn").addEventListener("click", () => showMcpEdit(null));
$("mcpCancelBtn").addEventListener("click", hideMcpEdit);
$("mcpSaveBtn").addEventListener("click", handleSaveMcp);
$("mcpOverlay").addEventListener("click", (event) => {
  if (event.target === $("mcpOverlay")) closeMcpPanel();
});
$("mcpTabInstalled").addEventListener("click", () => switchMcpTab("installed"));
$("mcpTabPresets").addEventListener("click", () => switchMcpTab("presets"));
$("mcpPresetSearch").addEventListener("keydown", (e) => {
  if (e.key === "Enter") {
    e.preventDefault();
    searchGitHubMcp();
  }
});

// ── Settings Panel ──────────────────────────────────

let appSettings = null;
let appPaths = null;

async function openSettingsPanel() {
  $("settingsOverlay").classList.add("open");
  // 加载设置和路径
  try {
    const [settings, paths] = await Promise.all([
      invoke("get_app_settings"),
      invoke("get_app_paths"),
    ]);
    appSettings = settings;
    appPaths = paths;
    $("settingsAutoStart").checked = settings.autoStart;
    $("settingsMinTray").checked = settings.minimizeToTray;
    $("settingsSilentStart").checked = settings.silentStartup;
    $("settingsConfigDirValue").textContent = paths.configDir;
    $("settingsClaudePathValue").textContent = paths.claudeSettings;
    $("settingsVscodePathValue").textContent = paths.vscodeSettings;
  } catch (e) {
    console.error("加载设置失败:", e);
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
  appSettings.language = currentLang;
  appSettings.theme = currentTheme;
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
$("settingsBtn").addEventListener("click", openSettingsPanel);
$("settingsClose").addEventListener("click", closeSettingsPanel);
$("settingsOverlay").addEventListener("click", (event) => {
  if (event.target === $("settingsOverlay")) closeSettingsPanel();
});
$("settingsAutoStart").addEventListener("change", handleSettingsToggle);
$("settingsMinTray").addEventListener("change", handleSettingsToggle);
$("settingsSilentStart").addEventListener("change", handleSettingsToggle);
$("settingsOpenConfigDir").addEventListener("click", () => {
  if (appPaths) invoke("open_folder", { path: appPaths.configDir });
});
$("settingsOpenClaudeDir").addEventListener("click", () => {
  if (appPaths) invoke("open_folder", { path: appPaths.claudeSettings });
});
$("settingsOpenVscodeDir").addEventListener("click", () => {
  if (appPaths) invoke("open_folder", { path: appPaths.vscodeSettings });
});
$("settingsExportBtn").addEventListener("click", handleExportProfiles);
$("settingsImportBtn").addEventListener("click", handleImportProfiles);

(async function init() {
  applyTheme();
  applyLanguage();

  // 隐藏主内容，等启动动画结束后再显示
  const toolbar = document.querySelector('.toolbar');
  const appEl = document.querySelector('.app');
  if (toolbar) toolbar.classList.add('app-hidden');
  if (appEl) appEl.classList.add('app-hidden');

  await Promise.all([loadStatus(), loadProfiles()]);

  // 启动动画：等加载条填满后淡出
  const splash = $('splashScreen');
  if (splash) {
    // 等待加载条动画完成（0.5s 延迟 + 1.2s 填充）
    await new Promise((r) => setTimeout(r, 1800));
    splash.classList.add('fade-out');
    // 淡出后显示主内容
    setTimeout(() => {
      if (toolbar) {
        toolbar.classList.remove('app-hidden');
        toolbar.classList.add('app-reveal');
      }
      if (appEl) {
        appEl.classList.remove('app-hidden');
        appEl.classList.add('app-reveal');
      }
    }, 150);
    // 完全移除 splash DOM
    setTimeout(() => splash.remove(), 600);
  }
})();
