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
    importBtn: "Import Current",
    addBtn: "+ Add Config",
    statusTitle: "Current Status",
    statusHint: "Restart terminal and VSCode after switching to apply environment variables.",
    profilesTitle: "Config List",
    addConfig: "Add Config",
    editConfig: "Edit Config",
    nameLabel: "Config Name",
    keyLabel: "Key",
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
    synced: "✓ Synced",
    unsynced: "⚠ Not Synced",
    noConfigs: "No configs yet",
    addFirstConfig: "Add your first config",
    inUse: "In Use",
    switchUse: "Switch",
    edit: "Edit",
    delete: "Delete",
    toastUpdated: "Config updated",
    toastAdded: "Config added",
    toastDeleted: "Config deleted",
    toastImported: "Current config imported",
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
    switchToDark: "Dark",
    switchToLight: "Light",
    langZhButton: "中文",
    langEnButton: "EN",
    placeholderName: "e.g. Production",
    placeholderApiKey: "sk-...",
    placeholderBaseUrl: "https://api.example.com"
  },
  zh: {
    appTitle: "VarSwitch",
    importBtn: "导入当前配置",
    addBtn: "+ 添加配置",
    statusTitle: "当前配置状态",
    statusHint: "切换后请重启终端和 VSCode，使环境变量生效。",
    profilesTitle: "配置列表",
    addConfig: "添加配置",
    editConfig: "编辑配置",
    nameLabel: "配置名称",
    keyLabel: "密钥",
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
    synced: "✓ 已同步",
    unsynced: "⚠ 未同步",
    noConfigs: "暂无配置",
    addFirstConfig: "添加第一个配置",
    inUse: "使用中",
    switchUse: "切换使用",
    edit: "编辑",
    delete: "删除",
    toastUpdated: "配置已更新",
    toastAdded: "配置已添加",
    toastDeleted: "配置已删除",
    toastImported: "当前配置已导入",
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
    switchToDark: "夜间",
    switchToLight: "白天",
    langZhButton: "中文",
    langEnButton: "en",
    placeholderName: "例如：生产环境",
    placeholderApiKey: "sk-...",
    placeholderBaseUrl: "https://api.example.com"
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

function updateThemeButtonText() {
  $("themeBtn").textContent = currentTheme === "light" ? t("switchToDark") : t("switchToLight");
}

function applyTheme() {
  document.documentElement.setAttribute("data-theme", currentTheme);
  updateThemeButtonText();
}

function applyLanguage() {
  document.documentElement.lang = currentLang === "zh" ? "zh-CN" : "en";
  document.title = t("appTitle");

  $("appTitle").textContent = t("appTitle");
  $("importBtn").textContent = t("importBtn");
  $("addBtn").textContent = t("addBtn");
  $("statusSectionTitle").textContent = t("statusTitle");
  $("statusHint").textContent = t("statusHint");
  $("profilesSectionTitle").textContent = t("profilesTitle");
  $("profileNameLabel").textContent = t("nameLabel");
  $("profileApiKeyLabel").textContent = t("keyLabel");
  $("profileBaseUrlLabel").textContent = t("urlLabel");
  $("cancelBtn").textContent = t("cancel");
  $("submitBtn").textContent = t("save");
  $("switchPanelTitle").textContent = t("switchingTo");
  $("switchStep1Text").textContent = t("stepSystem");
  $("switchStep2Text").textContent = t("stepVscode");
  $("switchStep3Text").textContent = t("stepClaude");
  $("switchCancelBtn").textContent = t("cancelSwitch");
  $("switchStepLabel").textContent = t("preparing");

  $("profileName").placeholder = t("placeholderName");
  $("profileApiKey").placeholder = t("placeholderApiKey");
  $("profileBaseUrl").placeholder = t("placeholderBaseUrl");

  $("langBtn").textContent = currentLang === "en" ? t("langZhButton") : t("langEnButton");
  updateThemeButtonText();

  if ($("modalOverlay").classList.contains("open")) {
    $("modalTitle").textContent = editingId ? t("editConfig") : t("addConfig");
  }
}

function toggleLanguage() {
  currentLang = currentLang === "en" ? "zh" : "en";
  localStorage.setItem(LANG_STORAGE_KEY, currentLang);
  applyLanguage();
  renderProfiles();
  loadStatus();
}

function toggleTheme() {
  currentTheme = currentTheme === "light" ? "dark" : "light";
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

    grid.innerHTML = locations.map((loc) => {
      const item = status[loc.key];
      if (!item) {
        return `
          <div class="status-card error-card">
            <div class="status-card-title">${loc.title}</div>
            <div style="font-size:13px;color:var(--error-text)">${t("readFailed")}</div>
          </div>`;
      }

      return `
        <div class="status-card">
          <div class="status-card-title">${loc.title}</div>
          <div class="status-item">
            <span class="status-label">${t("keyLabel")}</span>
            <span class="status-value">${maskKey(item.apiKey)}</span>
          </div>
          <div class="status-item">
            <span class="status-label">${t("urlLabel")}</span>
            <span class="status-value">${truncUrl(item.baseUrl)}</span>
          </div>
          <span class="status-badge ${synced ? "synced" : "unsynced"}">
            ${synced ? t("synced") : t("unsynced")}
          </span>
        </div>`;
    }).join("");
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
        <p>${t("noConfigs")}</p>
        <button class="btn btn-primary" id="addFirstBtn" type="button">${t("addFirstConfig")}</button>
      </div>`;
    const addFirstBtn = $("addFirstBtn");
    if (addFirstBtn) {
      addFirstBtn.addEventListener("click", () => $("addBtn").click());
    }
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
          <span class="field-label">${t("keyLabel")}</span>
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

  if (!window.confirm(t("confirmDelete", { name: profile.name }))) {
    return;
  }

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
$("langBtn").addEventListener("click", toggleLanguage);
$("themeBtn").addEventListener("click", toggleTheme);
$("cancelBtn").addEventListener("click", closeModal);
$("modalClose").addEventListener("click", closeModal);
$("switchCancelBtn").addEventListener("click", handleCancelSwitch);
$("profileForm").addEventListener("submit", handleSubmit);
$("modalOverlay").addEventListener("click", (event) => {
  if (event.target === $("modalOverlay")) {
    closeModal();
  }
});

(async function init() {
  applyTheme();
  applyLanguage();
  await Promise.all([loadStatus(), loadProfiles()]);
})();
