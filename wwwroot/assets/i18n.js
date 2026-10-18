(function () {
  const FALLBACK = {
    "zh-CN": {
      "app.title": "Whisper GUI",
      "app.subtitle": "完全离线的本地语音识别",
      "nav.transcribe": "转录",
      "nav.environment": "运行环境",
      "nav.settings": "设置",
      "action.start": "开始转录",
      "action.stop": "停止",
      "action.cancel": "取消",
      "action.save": "保存设置",
      "action.import": "导入",
      "status.ready": "就绪",
      "status.error": "不可用",
      "results.empty": "还没有转写结果。",
      "logs.empty": "还没有活动记录。"
    },
    "en-US": {
      "app.title": "Whisper GUI",
      "app.subtitle": "Fully offline local speech recognition",
      "nav.transcribe": "Transcribe",
      "nav.environment": "Environment",
      "nav.settings": "Settings",
      "action.start": "Start transcription",
      "action.stop": "Stop",
      "action.cancel": "Cancel",
      "action.save": "Save settings",
      "action.import": "Import",
      "status.ready": "Ready",
      "status.error": "Unavailable",
      "results.empty": "No transcripts yet.",
      "logs.empty": "No activity yet."
    }
  };
  const LOAD_TIMEOUT_MS = 2500;
  let table = { ...FALLBACK["zh-CN"] };
  let currentLanguage = "zh-CN";

  function validTable(value) {
    if (!value || typeof value !== "object" || Array.isArray(value)) return false;
    const entries = Object.entries(value);
    return entries.length >= 50 && entries.every(([key, text]) =>
      typeof key === "string" && key.length > 0 && typeof text === "string");
  }

  async function load(language) {
    const normalized = resolveLanguage(language);
    const controller = new AbortController();
    const timer = window.setTimeout(() => controller.abort(), LOAD_TIMEOUT_MS);
    try {
      const response = await fetch(`i18n/${normalized}.json`, {
        cache: "no-store",
        signal: controller.signal
      });
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      const value = await response.json();
      if (!validTable(value)) throw new Error("Invalid translation table");
      table = value;
      currentLanguage = normalized;
      return true;
    } catch (_) {
      table = { ...FALLBACK[normalized] };
      currentLanguage = normalized;
      return false;
    } finally {
      window.clearTimeout(timer);
    }
  }

  function format(template, args) {
    let index = 0;
    return String(template).replace(/\{(\d+)\}/g, (_match, placeholder) => {
      const position = Number.parseInt(placeholder, 10);
      return position >= 0 && position < args.length
        ? String(args[position])
        : String(args[index++] ?? "");
    });
  }

  function text(key, ...args) {
    const template = table[key] ?? key;
    return args.length ? format(template, args) : template;
  }

  function resolveLanguage(setting) {
    if (setting === "en-US" || setting === "zh-CN") return setting;
    return String(navigator.language || "").toLowerCase().startsWith("zh") ? "zh-CN" : "en-US";
  }

  function apply(language) {
    currentLanguage = resolveLanguage(language);
    document.documentElement.lang = currentLanguage;
    document.title = text("app.title");
    document.querySelectorAll("[data-i18n]").forEach((node) => {
      node.textContent = text(node.dataset.i18n);
    });
    document.querySelectorAll("[data-i18n-placeholder]").forEach((node) => {
      node.setAttribute("placeholder", text(node.dataset.i18nPlaceholder));
    });
    document.querySelectorAll("[data-i18n-title]").forEach((node) => {
      node.title = text(node.dataset.i18nTitle);
      node.setAttribute("aria-label", text(node.dataset.i18nTitle));
    });
    document.dispatchEvent(new CustomEvent("i18n:applied", {
      detail: { language: currentLanguage }
    }));
  }

  window.I18n = {
    apply,
    current: () => currentLanguage,
    load,
    text
  };
})();
