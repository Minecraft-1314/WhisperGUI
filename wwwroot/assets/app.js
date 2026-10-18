const query = new URLSearchParams(window.location.search);
const isMock = query.get("mock") === "1";
const STAGGER_STEP_MS = 35;
const BRIDGE_TIMEOUT_MS = 30_000;
const ENVIRONMENT_TIMEOUT_MS = 45_000;
const TOAST_TIMEOUT_MS = 3200;
const MAX_LOG_ROWS = 500;
const MAX_RESULT_ROWS = 500;
const INSTALL_PHASE_KEYS = {
  checking: "install.phase.checking",
  runtime: "install.phase.runtime",
  "remove-torch": "install.phase.removeTorch",
  torch: "install.phase.torch",
  whisper: "install.phase.whisper",
  verify: "install.phase.verify",
  complete: "install.phase.complete"
};

const ELEMENT_IDS = [
  "headerStatus", "uiLanguageSelect", "uiThemeSelect",
  "sidebarRuntime", "sidebarCache", "openLogsButton", "summaryDevice",
  "summaryModel", "summaryFfmpeg", "summaryFiles", "taskStatus",
  "deviceSelect", "modelSelect", "languageSelect", "fileSummary",
  "browseFilesButton", "outputDirectory", "browseOutputButton",
  "ffmpegNotice", "recommendButton", "installModelButton", "startButton",
  "stopButton", "transcriptionProgress", "progressMessage", "progressPercent",
  "progressFill", "resultList", "resultEmpty", "clearResultsButton",
  "logList", "logEmpty", "clearLogsButton", "refreshEnvironmentButton",
  "hardwareStatus", "hardwareGpu", "hardwareVram", "hardwareFfmpeg",
  "hardwarePython", "dependencyStatus", "installCpuButton", "installGpuButton",
  "cancelTaskButton", "installProgress", "installProgressMessage",
  "installProgressPercent", "installProgressFill", "modelList",
  "settingsNoticeText", "settingsLanguage", "settingsTheme", "settingsDevice",
  "settingsOutput", "saveSettingsButton", "resetSettingsButton",
  "openImportButton", "serviceList", "serviceEmpty", "importOverlay",
  "importForm", "serviceName", "serviceAddress", "serviceType",
  "serviceCredential", "serviceUserId", "confirmOverlay", "confirmTitle",
  "confirmMessage", "confirmCancelButton", "confirmAcceptButton", "toastRegion",
  "aboutButton", "aboutOverlay", "aboutVersion", "aboutRuntime", "aboutInstallPath", "aboutSettingsPath"
];

const elements = {};
const pendingCalls = new Map();
const mockHistory = [];
const selectedFiles = [];
let sequence = 0;
let activeView = "transcribe";
let currentLanguage = "zh-CN";
let currentLanguagePreference = "system";
let currentTheme = "auto";

function resolveUiLanguage(preference) {
  if (preference === "en-US" || preference === "zh-CN") {
    return preference;
  }
  return String(navigator.language || "").toLowerCase().startsWith("zh") ? "zh-CN" : "en-US";
}
let isBusy = false;
let isBootstrapping = true;
let modelRenderKey = "";
let serviceRenderKey = "";
let pendingDownloadModel = "";
let confirmAccept = null;

const defaultState = () => ({
  settings: {
    language: "zh-CN",
    theme: "auto",
    device: "auto",
    outputDirectory: ""
  },
  hasSavedSettings: false,
  settingsStatus: "not-created",
  services: [],
  hardware: {
    hasCuda: false,
    hasMps: false,
    gpuName: "",
    vramGb: 0,
    hasFfmpeg: false,
    isAdministrator: false,
    lastIssue: null
  },
  environment: {
    hasSystemPython: false,
    hasLocalRuntime: false,
    hasWhisper: false,
    hasTorch: false,
    hasFfmpeg: false,
    pythonVersion: "",
    runtimeDirectory: "",
    cacheDirectory: "",
    lastIssue: null
  },
  models: [],
  task: {
    id: "",
    kind: "",
    state: "idle",
    progress: 0,
    total: 0,
    message: ""
  },
  dataDirectory: "",
  settingsPath: "",
  logPath: "",
  catalogIssue: null,
  lastError: null,
  version: "",
  runtimeVersion: "",
  installPath: ""
});

let state = defaultState();

const t = (key, ...args) => window.I18n.text(key, ...args);

function refreshIcons() {
  if (window.lucide) {
    window.lucide.createIcons({
      attrs: {
        "aria-hidden": "true",
        "stroke-width": "2"
      }
    });
  }
}

function mockState() {
  return {
    ...defaultState(),
    settings: {
      language: currentLanguage,
      theme: currentTheme,
      device: "auto",
      outputDirectory: "C:\\Users\\Public\\Documents"
    },
    services: [
      {
        id: "mock-one",
        name: "Primary endpoint",
        address: "https://example.com/primary",
        type: "api",
        userId: "10001",
        hasCredential: true
      },
      {
        id: "mock-two",
        name: "Secondary endpoint",
        address: "https://example.org/secondary",
        type: "http",
        userId: "10002",
        hasCredential: false
      },
      {
        id: "mock-three",
        name: "Archive endpoint",
        address: "https://example.net/archive",
        type: "local",
        userId: "10003",
        hasCredential: true
      }
    ],
    hardware: {
      hasCuda: true,
      hasMps: false,
      gpuName: "Mock CUDA GPU",
      vramGb: 8,
      hasFfmpeg: true,
      isAdministrator: false,
      lastIssue: null
    },
    environment: {
      hasSystemPython: true,
      hasLocalRuntime: true,
      hasWhisper: true,
      hasTorch: true,
      hasFfmpeg: true,
      pythonVersion: "Python 3.11.16",
      runtimeDirectory: "C:\\Users\\Public\\AppData\\Local\\WhisperGUI\\runtime",
      cacheDirectory: "C:\\Users\\Public\\.cache\\whisper",
      lastIssue: null
    },
    models: [
      { id: "tiny", label: "Tiny", fileName: "tiny.pt", sizeBytes: 75572083, cachedBytes: 75572083, isCached: true, sha256: "a".repeat(64) },
      { id: "base", label: "Base", fileName: "base.pt", sizeBytes: 145262807, cachedBytes: 0, isCached: false, sha256: "b".repeat(64) },
      { id: "small", label: "Small", fileName: "small.pt", sizeBytes: 483617219, cachedBytes: 0, isCached: false, sha256: "c".repeat(64) },
      { id: "medium", label: "Medium", fileName: "medium.pt", sizeBytes: 1528008539, cachedBytes: 0, isCached: false, sha256: "d".repeat(64) },
      { id: "large", label: "Large", fileName: "large-v3.pt", sizeBytes: 3087371615, cachedBytes: 0, isCached: false, sha256: "e".repeat(64) }
    ],
    dataDirectory: "C:\\Users\\Public\\AppData\\Local\\WhisperGUI",
    settingsPath: "C:\\Users\\Public\\AppData\\Local\\WhisperGUI\\settings.json",
    logPath: "C:\\Users\\Public\\AppData\\Local\\WhisperGUI\\logs\\app.log"
  };
}

function mockCall(action, payload) {
  mockHistory.push({ action, payload });
  if (action === "getState" || action === "probeEnvironment") return mockState();
  if (action === "pickFiles") {
    return [
      "C:\\Users\\Public\\Music\\sample-one.wav",
      "C:\\Users\\Public\\Music\\sample-two.mp3",
      "C:\\Users\\Public\\Music\\sample-three.flac"
    ];
  }
  if (action === "pickDirectory") return "C:\\Users\\Public\\Documents\\Transcripts";
  if (action === "saveSettings") {
    return {
      ...mockState(),
      settings: { ...state.settings, ...payload },
      hasSavedSettings: true,
      settingsStatus: "loaded"
    };
  }
  if (action === "resetSettings") {
    return { ...mockState(), hasSavedSettings: false, settingsStatus: "not-created" };
  }
  if (action === "importService") {
    const next = mockState();
    next.services = [...state.services, {
      id: `mock-${state.services.length + 1}`,
      name: payload.name,
      address: payload.address,
      type: payload.type,
      userId: payload.userId,
      hasCredential: Boolean(payload.credential)
    }];
    next.hasSavedSettings = true;
    next.settingsStatus = "loaded";
    return next;
  }
  if (action === "removeService") {
    const next = mockState();
    next.services = state.services.filter((service) => service.id !== payload.id);
    return next;
  }
  if (action === "startTranscription" || action === "installDependencies" || action === "downloadModel") {
    return { taskId: `mock-${sequence}`, started: true };
  }
  if (action === "cancel" || action === "openExternal" || action === "openDirectory") return true;
  return {};
}

async function call(action, payload = {}, timeoutMs = BRIDGE_TIMEOUT_MS) {
  if (isMock) return mockCall(action, payload);
  if (!window.chrome?.webview) {
    throw new Error("Desktop bridge is unavailable.");
  }
  const id = ++sequence;
  return new Promise((resolve, reject) => {
    const timer = window.setTimeout(() => {
      pendingCalls.delete(id);
      reject(new Error(t("msg.operationFailed", "timeout")));
    }, timeoutMs);
    pendingCalls.set(id, {
      resolve,
      reject,
      timer
    });
    window.chrome.webview.postMessage({ id, action, payload });
  });
}

function resolveBridgeMessage(raw) {
  let message;
  try {
    message = typeof raw === "string" ? JSON.parse(raw) : raw;
  } catch (_) {
    return;
  }
  if (!message || typeof message.id !== "number") {
    if (message?.type) handleEvent(message);
    return;
  }
  const pending = pendingCalls.get(message.id);
  if (!pending) return;
  pendingCalls.delete(message.id);
  window.clearTimeout(pending.timer);
  if (message.ok) {
    pending.resolve(message.result);
  } else {
    pending.reject(new Error(message.error || t("msg.operationFailed", "unknown")));
  }
}

async function initialize() {
  ELEMENT_IDS.forEach((id) => {
    elements[id] = document.getElementById(id);
  });
  if (window.chrome?.webview) {
    window.chrome.webview.addEventListener("message", (event) => {
      resolveBridgeMessage(event.data);
    });
  }
  bindEvents();
  refreshIcons();

  let loaded = true;
  try {
    state = await call("getState", {}, BRIDGE_TIMEOUT_MS);
    currentLanguagePreference = state.settings?.language || "system";
    currentLanguage = resolveUiLanguage(currentLanguagePreference);
    currentTheme = state.settings?.theme || "auto";
  } catch (error) {
    state = defaultState();
    loaded = false;
  }

  await window.I18n.load(currentLanguage);
  window.I18n.apply(currentLanguage);
  setTheme(currentTheme);
  syncSettingsForm();
  renderState(true);
  if (!loaded) showToast(t("msg.loadFailed"), "bad");

  try {
    state = await call("probeEnvironment", {}, ENVIRONMENT_TIMEOUT_MS);
  } catch (error) {
    showToast(error.message || t("msg.loadFailed"), "bad");
  } finally {
    isBootstrapping = false;
    window.__whisperGuiAudit = {
      snapshot: () => ({
        ready: !isBootstrapping,
        saveCount: mockHistory.filter((item) => item.action === "saveSettings").length,
        calls: mockHistory.map((item) => item.action)
      })
    };
    renderState(true);
  }
}

document.addEventListener("DOMContentLoaded", initialize);

function bindEvents() {
  document.querySelectorAll(".nav-button").forEach((button) => {
    button.addEventListener("click", () => setView(button.dataset.view));
  });
  elements.uiLanguageSelect.addEventListener("change", () => {
    void loadLanguage(elements.uiLanguageSelect.value);
  });
  elements.uiThemeSelect.addEventListener("change", () => {
    void applyThemePreference(elements.uiThemeSelect.value);
  });
  elements.openLogsButton.addEventListener("click", openLogDirectory);
  elements.browseFilesButton.addEventListener("click", chooseFiles);
  elements.browseOutputButton.addEventListener("click", chooseOutputDirectory);
  elements.recommendButton.addEventListener("click", applyRecommendation);
  elements.installModelButton.addEventListener("click", () => downloadSelectedModel(false));
  elements.startButton.addEventListener("click", startTranscription);
  elements.stopButton.addEventListener("click", cancelOperation);
  elements.clearResultsButton.addEventListener("click", clearResults);
  elements.clearLogsButton.addEventListener("click", clearLogs);
  elements.refreshEnvironmentButton.addEventListener("click", refreshEnvironment);
  elements.installCpuButton.addEventListener("click", () => installDependencies("cpu"));
  elements.installGpuButton.addEventListener("click", () => installDependencies("gpu"));
  elements.cancelTaskButton.addEventListener("click", cancelOperation);

  elements.deviceSelect.addEventListener("change", () => {
    state.settings.device = elements.deviceSelect.value;
    elements.settingsDevice.value = state.settings.device;
    applyRecommendation(false);
    renderSummaries();
  });
  elements.modelSelect.addEventListener("change", renderSummaries);
  elements.outputDirectory.addEventListener("input", () => {
    state.settings.outputDirectory = elements.outputDirectory.value;
    elements.settingsOutput.value = state.settings.outputDirectory;
  });
  elements.settingsLanguage.addEventListener("change", () => {
    loadLanguage(elements.settingsLanguage.value);
  });
  elements.settingsTheme.addEventListener("change", () => {
    currentTheme = elements.settingsTheme.value;
    state.settings.theme = currentTheme;
    setTheme(currentTheme);
  });
  elements.settingsDevice.addEventListener("change", () => {
    state.settings.device = elements.settingsDevice.value;
    elements.deviceSelect.value = state.settings.device;
    applyRecommendation(false);
    renderSummaries();
  });
  elements.settingsOutput.addEventListener("input", () => {
    state.settings.outputDirectory = elements.settingsOutput.value;
    elements.outputDirectory.value = state.settings.outputDirectory;
  });
  elements.saveSettingsButton.addEventListener("click", saveSettings);
  elements.resetSettingsButton.addEventListener("click", requestReset);
  elements.openImportButton.addEventListener("click", () => openOverlay("importOverlay"));
  elements.aboutButton.addEventListener("click", () => { renderAbout(); openOverlay("aboutOverlay"); });
  elements.importForm.addEventListener("submit", importService);

  elements.modelList.addEventListener("click", (event) => {
    const button = event.target.closest("[data-model-action]");
    if (!button) return;
    const model = button.dataset.model;
    if (button.dataset.modelAction === "use") {
      elements.modelSelect.value = model;
      renderSummaries();
    } else {
      downloadSelectedModel(false, model);
    }
  });
  elements.serviceList.addEventListener("click", (event) => {
    const button = event.target.closest("[data-remove-service]");
    if (!button) return;
    const service = state.services.find((item) => item.id === button.dataset.removeService);
    if (service) requestRemoveService(service);
  });

  document.querySelectorAll("[data-close-overlay]").forEach((button) => {
    button.addEventListener("click", () => closeOverlay(button.dataset.closeOverlay));
  });
  document.querySelectorAll(".overlay").forEach((overlay) => {
    overlay.addEventListener("mousedown", (event) => {
      if (event.target === overlay) closeOverlay(overlay.id);
    });
  });
  elements.confirmCancelButton.addEventListener("click", closeConfirm);
  elements.confirmAcceptButton.addEventListener("click", acceptConfirm);
  document.addEventListener("keydown", (event) => {
    if (event.key !== "Escape") return;
    if (elements.confirmOverlay.classList.contains("open")) closeConfirm();
    else if (elements.importOverlay.classList.contains("open")) closeOverlay("importOverlay");
  });
  document.addEventListener("i18n:applied", () => {
      modelRenderKey = "";
    serviceRenderKey = "";
    renderState(false);
  });
  window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
    if (currentTheme === "auto") setTheme("auto");
  });
}

function setView(view) {
  if (!["transcribe", "environment", "settings"].includes(view)) return;
  activeView = view;
  document.querySelectorAll(".nav-button").forEach((button) => {
    const active = button.dataset.view === view;
    button.classList.toggle("active", active);
    button.setAttribute("aria-current", active ? "page" : "false");
  });
  document.querySelectorAll("[data-view-panel]").forEach((panel) => {
    panel.classList.toggle("active", panel.dataset.viewPanel === view);
  });
  renderState(false);
}

async function loadLanguage(preference) {
  currentLanguagePreference = preference === "en-US" || preference === "zh-CN" ? preference : "system";
  currentLanguage = resolveUiLanguage(currentLanguagePreference);
  state.settings.language = currentLanguagePreference;
  await window.I18n.load(currentLanguage);
  window.I18n.apply(currentLanguage);
  syncSettingsForm();
  await persistUiPreferences();
}

async function applyThemePreference(theme) {
  currentTheme = ["auto", "light", "dark"].includes(theme) ? theme : "auto";
  state.settings.theme = currentTheme;
  setTheme(currentTheme);
  syncSettingsForm();
  await persistUiPreferences();
}

async function persistUiPreferences() {
  try {
    state = await call("saveSettings", {
      language: currentLanguagePreference,
      theme: currentTheme,
      device: state.settings.device || "auto",
      outputDirectory: state.settings.outputDirectory || ""
    });
    currentLanguagePreference = state.settings.language;
    currentTheme = state.settings.theme;
  } catch (error) {
    showToast(error.message, "bad");
  }
}

function setTheme(theme) {
  currentTheme = ["auto", "light", "dark"].includes(theme) ? theme : "auto";
  const dark = currentTheme === "dark"
    || (currentTheme === "auto" && window.matchMedia("(prefers-color-scheme: dark)").matches);
  document.documentElement.classList.toggle("dark", dark);
}

function syncSettingsForm() {
  elements.settingsLanguage.value = currentLanguagePreference;
  elements.settingsTheme.value = currentTheme;
  elements.uiLanguageSelect.value = currentLanguagePreference;
  elements.uiThemeSelect.value = currentTheme;
  elements.settingsDevice.value = state.settings.device || "auto";
  elements.settingsOutput.value = state.settings.outputDirectory || "";
}

function renderState(stagger) {
  syncModelOptions();
  syncSettingsForm();
  renderSummaries();
  renderEnvironment();
  renderSettingsStatus();
  renderModels(stagger);
  renderServices(stagger);
  renderTask();
  refreshIcons();
}

function syncModelOptions() {
  const previous = elements.modelSelect.value;
  const fragment = document.createDocumentFragment();
  state.models.forEach((model) => {
    const option = document.createElement("option");
    option.value = model.id;
    option.textContent = `${model.label} · ${formatMegabytes(model.sizeBytes)}`;
    fragment.append(option);
  });
  elements.modelSelect.replaceChildren(fragment);
  const available = state.models.some((model) => model.id === previous);
  elements.modelSelect.value = available
    ? previous
    : state.models.find((model) => model.id === recommendedModel())?.id
      || state.models[0]?.id
      || "";
}

function renderSummaries() {
  const model = state.models.find((item) => item.id === elements.modelSelect.value);
  elements.summaryDevice.textContent = t(`option.${state.settings.device || "auto"}`);
  elements.summaryModel.textContent = model?.label || "-";
  elements.summaryFfmpeg.textContent = state.environment.hasFfmpeg ? "OK" : t("status.error");
  elements.summaryFiles.textContent = selectedFiles.length
    ? t("format.fileCount", selectedFiles.length)
    : "0";
  elements.fileSummary.value = selectedFiles.length
    ? selectedFiles.length === 1
      ? t("format.firstFile", selectedFiles[0])
      : t("format.fileCount", selectedFiles.length)
    : t("placeholder.noFiles");
  elements.outputDirectory.value = state.settings.outputDirectory || "";
  elements.ffmpegNotice.hidden = state.environment.hasFfmpeg;
}

function renderEnvironment() {
  const environment = state.environment;
  const hardware = state.hardware;
  const pythonLabel = environment.pythonVersion || "Python 3";
  elements.sidebarRuntime.textContent = environment.hasLocalRuntime
    ? environment.pythonVersion || environment.runtimeDirectory
    : environment.hasSystemPython
      ? `${pythonLabel} · ${t("runtime.system")}`
      : t("msg.noRuntime");
  elements.sidebarCache.textContent = environment.cacheDirectory || "-";
  elements.hardwareGpu.textContent = hardware.gpuName || t("format.noGpu");
  elements.hardwareVram.textContent = hardware.vramGb > 0
    ? t("format.vram", hardware.vramGb.toFixed(1))
    : "-";
  elements.hardwareFfmpeg.textContent = environment.hasFfmpeg ? "OK" : t("status.error");
  elements.hardwarePython.textContent = environment.pythonVersion
    ? environment.hasLocalRuntime
      ? environment.pythonVersion.replace(/^Python\s*/i, "")
      : `${environment.pythonVersion.replace(/^Python\s*/i, "")} · ${t("runtime.system")}`
    : t("status.error");
  const hardwareIssue = hardware.lastIssue || environment.lastIssue;
  setPill(elements.hardwareStatus, hardwareIssue ? "bad" : "good",
    hardwareIssue ? t("status.error") : t("status.ready"));
  const ready = environment.hasLocalRuntime && environment.hasWhisper && environment.hasTorch;
  const dependencyIssue = environment.lastIssue;
  const installing = state.task.kind === "install" && state.task.state === "running";
  setPill(elements.dependencyStatus,
    installing ? "warn" : dependencyIssue ? "bad" : ready ? "good" : "warn",
    installing ? t("status.running")
      : dependencyIssue ? t("status.error") : ready ? t("status.ready") : t("status.warning"));
  setPill(elements.headerStatus,
    environment.lastIssue ? "bad" : ready ? "good" : "warn",
    environment.lastIssue ? t("status.error") : ready ? t("status.ready") : t("status.warning"));
}

function renderSettingsStatus() {
  let key = "settings.notCreated";
  if (state.settingsStatus === "loaded") key = "settings.loaded";
  else if (state.settingsStatus !== "not-created") key = "settings.ignored";
  elements.settingsNoticeText.textContent = t(key);
}

function renderModels(stagger) {
  const key = state.models.map((model) =>
    `${model.id}:${model.isCached}:${model.sizeBytes}`).join("|");
  if (key === modelRenderKey) return;
  modelRenderKey = key;
  const fragment = document.createDocumentFragment();
  state.models.forEach((model, index) => {
    const card = document.createElement("article");
    card.className = "model-card";
    if (stagger) setStagger(card, index);
    const copy = document.createElement("div");
    copy.className = "model-copy";
    const name = document.createElement("div");
    name.className = "model-name";
    name.textContent = model.label;
    const meta = document.createElement("div");
    meta.className = "model-meta";
    meta.textContent = `${formatMegabytes(model.sizeBytes)} · ${t(model.isCached ? "model.cached" : "model.missing")}`;
    copy.append(name, meta);
    const button = document.createElement("button");
    button.type = "button";
    button.className = `button ${model.isCached ? "secondary" : "primary"} compact`;
    button.dataset.modelAction = model.isCached ? "use" : "download";
    button.dataset.model = model.id;
    const icon = document.createElement("i");
    icon.dataset.lucide = model.isCached ? "check" : "download";
    const label = document.createElement("span");
    label.textContent = t(model.isCached ? "model.use" : "model.download");
    button.append(icon, label);
    card.append(copy, button);
    fragment.append(card);
  });
  elements.modelList.replaceChildren(fragment);
  refreshIcons();
}

function renderServices(stagger) {
  const key = state.services.map((service) =>
    `${service.id}:${service.name}:${service.address}`).join("|");
  if (key === serviceRenderKey) return;
  serviceRenderKey = key;
  const fragment = document.createDocumentFragment();
  state.services.forEach((service, index) => {
    const row = document.createElement("article");
    row.className = "service-row";
    if (stagger) setStagger(row, index);
    const copy = document.createElement("div");
    copy.className = "service-copy";
    const name = document.createElement("div");
    name.className = "service-name";
    name.textContent = service.name;
    const meta = document.createElement("div");
    meta.className = "service-meta";
    meta.textContent = `${service.address} · ${service.type}${service.userId ? ` · ${service.userId}` : ""}`;
    copy.append(name, meta);
    const remove = document.createElement("button");
    remove.type = "button";
    remove.className = "button ghost icon-only";
    remove.dataset.removeService = service.id;
    remove.title = t("action.remove");
    remove.setAttribute("aria-label", t("action.remove"));
    const icon = document.createElement("i");
    icon.dataset.lucide = "trash-2";
    remove.append(icon);
    row.append(copy, remove);
    fragment.append(row);
  });
  elements.serviceList.replaceChildren(fragment);
  elements.serviceEmpty.hidden = state.services.length > 0;
  refreshIcons();
}

function setStagger(node, index) {
  node.classList.add("stagger-item");
  node.dataset.staggerIndex = String(index);
  node.style.setProperty("--stagger-index", String(index * STAGGER_STEP_MS / STAGGER_STEP_MS));
}

function setPill(node, tone, label) {
  node.className = `status-pill ${tone}`;
  node.textContent = label;
}

function formatMegabytes(bytes) {
  return t("size.mb", Math.max(0, Math.round((bytes || 0) / 1024 / 1024)));
}

function recommendedModel() {
  const device = state.settings.device || "auto";
  const vram = state.hardware.hasCuda || state.hardware.hasMps ? state.hardware.vramGb : 0;
  if (device === "cpu" || vram < 2) return "tiny";
  if (vram < 4) return "base";
  if (vram < 6) return "small";
  if (vram < 10) return "medium";
  return "large";
}

function applyRecommendation(log = true) {
  const model = recommendedModel();
  if (state.models.some((item) => item.id === model)) elements.modelSelect.value = model;
  if (log) renderSummaries();
}

function renderTask() {
  const task = state.task || defaultState().task;
  isBusy = task.state === "running";
  const key = `task.${task.state}`;
  const tone = task.state === "running" ? "warn"
    : task.state === "failed" ? "bad"
      : task.state === "completed" ? "good" : "";
  setPill(elements.taskStatus, tone, t(key));
  elements.startButton.disabled = isBusy;
  elements.stopButton.disabled = !isBusy;
  elements.installCpuButton.disabled = isBusy;
  elements.installGpuButton.disabled = isBusy;
  elements.installModelButton.disabled = isBusy;
  elements.cancelTaskButton.disabled = !isBusy;
  elements.refreshEnvironmentButton.disabled = isBusy;
}

async function chooseFiles() {
  try {
    const files = await call("pickFiles");
    selectedFiles.splice(0, selectedFiles.length, ...(Array.isArray(files) ? files : []));
    renderSummaries();
    if (selectedFiles.length) showToast(t("msg.filesSelected", selectedFiles.length));
  } catch (error) {
    showToast(error.message, "bad");
  }
}

async function chooseOutputDirectory() {
  try {
    const directory = await call("pickDirectory");
    if (typeof directory === "string" && directory) {
      state.settings.outputDirectory = directory;
      renderSummaries();
      syncSettingsForm();
    }
  } catch (error) {
    showToast(error.message, "bad");
  }
}

async function openLogDirectory() {
  try {
    const directory = state.dataDirectory || "";
    await call("openDirectory", { path: directory });
  } catch (error) {
    showToast(error.message, "bad");
  }
}

async function startTranscription() {
  if (!selectedFiles.length) {
    showToast(t("msg.noFiles"), "bad");
    return;
  }
  const outputDirectory = elements.outputDirectory.value.trim();
  if (!outputDirectory) {
    showToast(t("msg.invalidOutput"), "bad");
    return;
  }
  const modelId = elements.modelSelect.value;
  const model = state.models.find((item) => item.id === modelId);
  if (!model?.isCached) {
    showConfirm(
      t("action.downloadModel"),
      t("msg.modelMissing", model?.label || modelId),
      () => downloadSelectedModel(true, modelId)
    );
    return;
  }
  await requestStart(modelId, outputDirectory);
}

async function requestStart(modelId, outputDirectory) {
  try {
    state.task = {
      id: "pending",
      kind: "transcription",
      state: "running",
      progress: 0,
      total: selectedFiles.length,
      message: ""
    };
    renderTask();
    await call("startTranscription", {
      files: [...selectedFiles],
      model: modelId,
      device: state.settings.device || "auto",
      language: elements.languageSelect.value,
      outputDirectory
    });
  } catch (error) {
    state.task = defaultState().task;
    renderTask();
    showToast(error.message, "bad");
  }
}

async function downloadSelectedModel(forStart = false, modelId = elements.modelSelect.value) {
  const model = state.models.find((item) => item.id === modelId);
  if (!model) return;
  if (model.isCached) {
    if (forStart) {
      await requestStart(model.id, elements.outputDirectory.value.trim());
    } else {
      showToast(t("model.cached"));
    }
    return;
  }
  if (forStart) {
    showConfirm(
      t("action.downloadModel"),
      t("msg.modelMissing", model.label),
      () => performDownload(model.id, true)
    );
    return;
  }
  await performDownload(model.id, false);
}

async function performDownload(modelId, forStart) {
  try {
    pendingDownloadModel = forStart ? modelId : "";
    state.task = {
      id: "pending",
      kind: "download",
      state: "running",
      progress: 0,
      total: 100,
      message: t("msg.downloadStart", modelId)
    };
    renderTask();
    showProgress(elements.installProgress, 0, t("msg.downloadStart", modelId));
    await call("downloadModel", { model: modelId });
  } catch (error) {
    pendingDownloadModel = "";
    state.task = defaultState().task;
    renderTask();
    hideProgress(elements.installProgress);
    showToast(error.message, "bad");
  }
}

async function installDependencies(target) {
  try {
    state.task = {
      id: "pending",
      kind: "install",
      state: "running",
      progress: 0,
      total: 0,
      message: t("status.loading")
    };
    renderTask();
    showProgress(elements.installProgress, 0, t("status.loading"));
    await call("installDependencies", { target });
  } catch (error) {
    state.task = defaultState().task;
    renderTask();
    hideProgress(elements.installProgress);
    showToast(error.message, "bad");
  }
}

async function refreshEnvironment() {
  try {
    elements.refreshEnvironmentButton.disabled = true;
    state = await call("probeEnvironment", {}, ENVIRONMENT_TIMEOUT_MS);
    renderState(true);
    showToast(t("msg.loaded"));
  } catch (error) {
    showToast(error.message, "bad");
  } finally {
    renderTask();
  }
}

async function cancelOperation() {
  try {
    await call("cancel");
  } catch (error) {
    showToast(error.message, "bad");
  }
}

async function saveSettings() {
  try {
    const payload = {
      language: elements.settingsLanguage.value,
      theme: elements.settingsTheme.value,
      device: elements.settingsDevice.value,
      outputDirectory: elements.settingsOutput.value.trim()
    };
    state = await call("saveSettings", payload);
    currentLanguagePreference = state.settings.language;
    currentLanguage = resolveUiLanguage(currentLanguagePreference);
    currentTheme = state.settings.theme;
    await window.I18n.load(currentLanguage);
    window.I18n.apply(currentLanguage);
    setTheme(currentTheme);
    syncSettingsForm();
    renderState(true);
    showToast(t("msg.settingsSaved"));
  } catch (error) {
    showToast(error.message, "bad");
  }
}

function requestReset() {
  showConfirm(t("msg.resetTitle"), t("msg.resetBody"), async () => {
    try {
      state = await call("resetSettings");
      currentLanguage = "zh-CN";
      currentTheme = "auto";
      await window.I18n.load(currentLanguage);
      window.I18n.apply(currentLanguage);
      setTheme(currentTheme);
      renderState(true);
      showToast(t("msg.settingsSaved"));
    } catch (error) {
      showToast(error.message, "bad");
    }
  });
}

async function importService(event) {
  event.preventDefault();
  const payload = {
    name: elements.serviceName.value.trim(),
    address: elements.serviceAddress.value.trim(),
    type: elements.serviceType.value,
    credential: elements.serviceCredential.value,
    userId: elements.serviceUserId.value.trim()
  };
  try {
    state = await call("importService", payload);
    elements.importForm.reset();
    closeOverlay("importOverlay");
    renderState(true);
    showToast(t("msg.imported", payload.name));
  } catch (error) {
    showToast(error.message, "bad");
  }
}

function requestRemoveService(service) {
  showConfirm(t("msg.removeTitle"), t("msg.removeBody", service.name), async () => {
    try {
      state = await call("removeService", { id: service.id });
      renderState(true);
      showToast(t("msg.removed", service.name));
    } catch (error) {
      showToast(error.message, "bad");
    }
  });
}

function handleEvent(message) {
  switch (message.type) {
    case "task":
      state.task = message.task || defaultState().task;
      if (state.task.state === "failed" && state.task.kind === "download") {
        pendingDownloadModel = "";
        hideProgress(elements.installProgress);
      }
      if (state.task.state === "completed" && state.task.kind === "download" && pendingDownloadModel) {
        const modelId = pendingDownloadModel;
        pendingDownloadModel = "";
        const model = state.models.find((item) => item.id === modelId);
        if (model) model.isCached = true;
        renderState(false);
        requestStart(modelId, elements.outputDirectory.value.trim());
      }
      if (state.task.state !== "running") hideProgress(elements.installProgress);
      renderTask();
      break;
    case "downloadProgress":
      showProgress(
        elements.installProgress,
        Number(message.percent) || 0,
        t("msg.downloadStart", message.model || "")
      );
      break;
    case "downloadComplete":
      showToast(t("msg.downloadDone", message.model || ""));
      {
        const model = state.models.find((item) => item.id === message.model);
        if (model) model.isCached = true;
        renderState(false);
      }
      break;
    case "installProgress":
      showProgress(
        elements.installProgress,
        Number(message.percent) || 0,
        t(INSTALL_PHASE_KEYS[message.phase] || "status.loading")
      );
      break;
    case "installComplete":
      showToast(t("msg.installDone"));
      refreshEnvironment();
      break;
    case "transcription":
      handleTranscriptionEvent(message);
      break;
    case "transcriptionComplete":
      handleTranscriptionComplete(message);
      break;
    case "operationError":
      showToast(message.message || t("msg.operationFailed", ""), "bad");
      break;
    case "log":
      appendLog(message.message || "");
      break;
    default:
      break;
  }
}

function handleTranscriptionEvent(message) {
  const total = Number(message.total) || 0;
  const current = Number(message.current) || 0;
  const percent = total > 0 ? Math.round(Math.max(0, current) * 100 / total) : 0;
  if (message.updateType === "progress") {
    showProgress(
      elements.transcriptionProgress,
      percent,
      t("format.progress", Math.max(0, current), total)
    );
  } else if (message.updateType === "result") {
    appendResult(message.path || "", message.text || "");
  } else if (message.updateType === "saved") {
    appendLog(t("msg.transcriptSaved", message.path || ""));
  } else if (message.updateType === "fileFailed") {
    appendLog(t("msg.transcriptFailed", message.error || message.text || ""), true);
  } else if (message.updateType === "language") {
    appendLog(`${t("field.language")}: ${message.text || ""}`);
  }
}

function handleTranscriptionComplete(message) {
  hideProgress(elements.transcriptionProgress);
  if (message.error) showToast(message.error, "bad");
  else if (message.cancelled) showToast(t("msg.operationCancelled"));
  else {
    const saved = Array.isArray(message.outputFiles) ? message.outputFiles.length : 0;
    const failed = Array.isArray(message.failedFiles) ? message.failedFiles.length : 0;
    showToast(failed ? t("msg.transcriptFailed", String(failed)) : t("msg.transcriptSaved", String(saved)));
  }
}

function appendLog(message, isError = false) {
  if (!message) return;
  elements.logEmpty.hidden = true;
  const row = document.createElement("div");
  row.className = "log-row stagger-item";
  row.style.setProperty("--stagger-index", "0");
  if (isError) row.style.color = "var(--bad)";
  row.textContent = message;
  elements.logList.append(row);
  while (elements.logList.children.length > MAX_LOG_ROWS) {
    elements.logList.firstElementChild.remove();
  }
  elements.logList.scrollTop = elements.logList.scrollHeight;
}

function appendResult(path, text) {
  elements.resultEmpty.hidden = true;
  const row = document.createElement("article");
  row.className = "result-row stagger-item";
  row.style.setProperty("--stagger-index", "0");
  const copy = document.createElement("div");
  copy.className = "result-copy";
  const title = document.createElement("div");
  title.className = "result-title";
  title.textContent = path.split(/[\\/]/).pop() || path;
  const meta = document.createElement("div");
  meta.className = "result-meta";
  meta.textContent = path;
  const content = document.createElement("div");
  content.className = "result-meta";
  content.textContent = text;
  copy.append(title, meta, content);
  row.append(copy);
  elements.resultList.append(row);
  while (elements.resultList.children.length > MAX_RESULT_ROWS) {
    elements.resultList.firstElementChild.remove();
  }
}

function clearLogs() {
  elements.logList.replaceChildren();
  elements.logEmpty.hidden = false;
}

function clearResults() {
  elements.resultList.replaceChildren();
  elements.resultEmpty.hidden = false;
}

function showProgress(container, percent, message) {
  const safePercent = Math.max(0, Math.min(100, Number(percent) || 0));
  container.classList.add("visible");
  const fill = container.querySelector(".progress-fill");
  const label = container.querySelector(".progress-label span:last-child");
  const text = container.querySelector(".progress-label span:first-child");
  if (fill) fill.style.transform = `scaleX(${safePercent / 100})`;
  if (label) label.textContent = `${safePercent}%`;
  if (text) text.textContent = message;
}

function hideProgress(container) {
  container.classList.remove("visible");
  const fill = container.querySelector(".progress-fill");
  if (fill) fill.style.transform = "scaleX(0)";
}

function showToast(message, tone = "good") {
  if (!message) return;
  const toast = document.createElement("div");
  toast.className = `toast ${tone}`;
  const icon = document.createElement("i");
  icon.dataset.lucide = tone === "bad" ? "triangle-alert" : "circle-check";
  const textNode = document.createElement("span");
  textNode.textContent = message;
  toast.append(icon, textNode);
  elements.toastRegion.append(toast);
  refreshIcons();
  window.setTimeout(() => toast.remove(), TOAST_TIMEOUT_MS);
}

function renderAbout() {
  if (elements.aboutVersion) elements.aboutVersion.textContent = state.version || "—";
  if (elements.aboutRuntime) elements.aboutRuntime.textContent = state.runtimeVersion || "—";
  if (elements.aboutInstallPath) elements.aboutInstallPath.textContent = state.installPath || "—";
  if (elements.aboutSettingsPath) elements.aboutSettingsPath.textContent = state.settingsPath || "—";
}

function openOverlay(id) {
  const overlay = document.getElementById(id);
  if (!overlay) return;
  overlay.classList.add("open");
  const firstInput = overlay.querySelector("input, select, button");
  if (firstInput) window.setTimeout(() => firstInput.focus(), 0);
}

function closeOverlay(id) {
  const overlay = document.getElementById(id);
  if (overlay) overlay.classList.remove("open");
}

function showConfirm(title, message, onAccept) {
  elements.confirmTitle.textContent = title;
  elements.confirmMessage.textContent = message;
  confirmAccept = onAccept;
  openOverlay("confirmOverlay");
}

function closeConfirm() {
  closeOverlay("confirmOverlay");
  confirmAccept = null;
}

async function acceptConfirm() {
  const action = confirmAccept;
  closeConfirm();
  if (action) await action();
}
