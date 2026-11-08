const DEFAULT_SETTINGS = Object.freeze({
  baseUrl: "http://127.0.0.1:47821",
  token: "",
  intercept: true,
  notify: true,
});

const API_PREFIX = "/api/v1";
const ICON_URL = chrome.runtime.getURL("icons/icon128.png");
let warnedMissingToken = false;

function normalizeBase(raw) {
  const value = String(raw || DEFAULT_SETTINGS.baseUrl).trim();
  const url = new URL(value);
  if (url.protocol !== "http:" && url.protocol !== "https:") {
    throw new Error("The API URL must use http:// or https://");
  }
  return `${url.origin}${url.pathname.replace(/\/+$/, "")}`;
}

function originPattern(baseUrl) {
  const url = new URL(baseUrl);
  return `${url.protocol}//${url.host}/*`;
}

async function readSettings() {
  const stored = await chrome.storage.local.get(DEFAULT_SETTINGS);
  return {
    ...DEFAULT_SETTINGS,
    ...stored,
    baseUrl: normalizeBase(stored.baseUrl || DEFAULT_SETTINGS.baseUrl),
  };
}

async function hasHostPermission(baseUrl) {
  try {
    return await chrome.permissions.contains({ origins: [originPattern(baseUrl)] });
  } catch {
    return false;
  }
}

function makeApiError(payload, status) {
  const message =
    payload && typeof payload.message === "string"
      ? payload.message
      : `NexDownload API returned HTTP ${status}`;
  const error = new Error(message);
  error.code = payload && payload.code ? payload.code : "http_error";
  error.status = status;
  error.details = payload ? payload.details : null;
  return error;
}

async function requestWithConfig(config, path, options = {}) {
  if (!config.token) {
    throw new Error("NexDownload API token is not configured.");
  }
  const method = options.method || "GET";
  const controller = new AbortController();
  const timeout = setTimeout(
    () => controller.abort(),
    Number(options.timeoutMs) || 10_000,
  );
  try {
    const response = await fetch(`${config.baseUrl}${API_PREFIX}${path}`, {
      method,
      headers: {
        Authorization: `Bearer ${config.token}`,
        "Content-Type": "application/json",
      },
      body:
        options.body === undefined ? undefined : JSON.stringify(options.body),
      signal: controller.signal,
    });
    const text = await response.text();
    let payload = null;
    if (text) {
      try {
        payload = JSON.parse(text);
      } catch {
        payload = null;
      }
    }
    if (!response.ok) {
      throw makeApiError(payload, response.status);
    }
    return payload;
  } catch (error) {
    if (error && error.name === "AbortError") {
      throw new Error("NexDownload API request timed out.");
    }
    throw error;
  } finally {
    clearTimeout(timeout);
  }
}

async function apiRequest(path, options) {
  const config = await readSettings();
  return requestWithConfig(config, path, options);
}

async function addDownload(payload) {
  return apiRequest("/tasks", {
    method: "POST",
    body: {
      url: payload.url,
      file_name: payload.fileName || undefined,
      directory: payload.directory || undefined,
      category: payload.category || undefined,
      connections: payload.connections || undefined,
      confirmed: Boolean(payload.confirmed),
    },
  });
}

async function testConnection(config) {
  const normalized = {
    baseUrl: normalizeBase(config.baseUrl),
    token: String(config.token || "").trim(),
  };
  if (!normalized.token) {
    throw new Error("Enter the API token before testing the connection.");
  }
  const healthResponse = await fetch(
    `${normalized.baseUrl}${API_PREFIX}/health`,
    { method: "GET" },
  );
  if (!healthResponse.ok) {
    throw new Error(`Health check failed with HTTP ${healthResponse.status}`);
  }
  const health = await healthResponse.json();
  const stats = await requestWithConfig(normalized, "/stats");
  return {
    version: health.version || "unknown",
    totalTasks: Number(stats.total_tasks || 0),
    active: Number(stats.active || 0),
  };
}

async function dashboardState() {
  const config = await readSettings();
  const state = {
    configured: Boolean(config.token),
    baseUrl: config.baseUrl,
    intercept: config.intercept,
    error: null,
    stats: null,
    tasks: [],
  };
  if (!state.configured) {
    return state;
  }
  try {
    const [stats, tasks] = await Promise.all([
      requestWithConfig(config, "/stats"),
      requestWithConfig(config, "/tasks?limit=6"),
    ]);
    state.stats = stats;
    state.tasks = Array.isArray(tasks) ? tasks : [];
  } catch (error) {
    state.error = error.message || String(error);
    state.errorCode = error.code || "request_failed";
  }
  return state;
}

async function showNotification(title, message) {
  try {
    await chrome.notifications.create({
      type: "basic",
      iconUrl: ICON_URL,
      title,
      message,
    });
  } catch {
    // Notifications are best effort; interception must still work without them.
  }
}

async function setBadge(text, color) {
  try {
    await chrome.action.setBadgeBackgroundColor({ color: color || "#2563eb" });
    await chrome.action.setBadgeText({ text });
    if (text) {
      setTimeout(() => {
        void chrome.action.setBadgeText({ text: "" });
      }, 5000);
    }
  } catch {
    // Badge updates are cosmetic.
  }
}

function fileNameFromDownload(item) {
  const candidate = String(item.filename || "")
    .split(/[\\/]/)
    .filter(Boolean)
    .pop();
  return candidate || undefined;
}

async function interceptDownload(item) {
  const settings = await readSettings();
  if (!settings.intercept) {
    return;
  }
  if (!/^https?:/i.test(item.url || "")) {
    return;
  }
  if (!settings.token) {
    if (!warnedMissingToken) {
      warnedMissingToken = true;
      await setBadge("!", "#f59e0b");
      await showNotification(
        "NexDownload is not configured",
        "Open the extension options and enter the local API token.",
      );
    }
    return;
  }

  try {
    const result = await addDownload({
      url: item.finalUrl || item.url,
      fileName: fileNameFromDownload(item),
      confirmed: false,
    });
    await chrome.downloads.cancel(item.id).catch(() => undefined);
    await chrome.downloads.erase({ id: item.id }).catch(() => undefined);
    await setBadge("OK", "#16a34a");
    if (settings.notify) {
      const name = result && result.task ? result.task.file_name : "download";
      await showNotification("Sent to NexDownload", name);
    }
  } catch (error) {
    if (error.code === "confirmation_required") {
      await showNotification(
        "Confirmation required",
        "A NexDownload rule needs confirmation. Open the app to approve this download.",
      );
    } else if (error.code === "unauthorized" || error.status === 401) {
      await showNotification(
        "NexDownload authentication failed",
        "Update the API token in the extension options.",
      );
    } else if (settings.notify) {
      await showNotification(
        "Browser download kept",
        `NexDownload could not queue it: ${error.message || error}`,
      );
    }
    // The browser download is deliberately not cancelled when the API fails.
  }
}

chrome.runtime.onInstalled.addListener(async () => {
  const stored = await chrome.storage.local.get(DEFAULT_SETTINGS);
  await chrome.storage.local.set({ ...DEFAULT_SETTINGS, ...stored });
  await chrome.contextMenus.removeAll();
  chrome.contextMenus.create({
    id: "nexdownload-link",
    title: "Download with NexDownload",
    contexts: ["link"],
  });
});

chrome.contextMenus.onClicked.addListener((info) => {
  if (info.menuItemId !== "nexdownload-link" || !info.linkUrl) {
    return;
  }
  void (async () => {
    try {
      const result = await addDownload({
        url: info.linkUrl,
        confirmed: false,
      });
      const name = result && result.task ? result.task.file_name : "download";
      await setBadge("OK", "#16a34a");
      await showNotification("Sent to NexDownload", name);
    } catch (error) {
      if (error.code === "confirmation_required") {
        await showNotification(
          "Confirmation required",
          "Open NexDownload to approve this download.",
        );
      } else {
        await showNotification(
          "NexDownload could not queue the link",
          error.message || String(error),
        );
      }
    }
  })();
});

chrome.downloads.onCreated.addListener((item) => {
  void interceptDownload(item);
});

chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  if (!message || message.type !== "nex:request" || sender.id !== chrome.runtime.id) {
    return false;
  }
  void (async () => {
    switch (message.action) {
      case "dashboard":
        return { ok: true, data: await dashboardState() };
      case "add":
        return {
          ok: true,
          data: await addDownload({
            url: message.url,
            fileName: message.fileName,
            confirmed: Boolean(message.confirmed),
          }),
        };
      case "test":
        return {
          ok: true,
          data: await testConnection({
            baseUrl: message.baseUrl,
            token: message.token,
          }),
        };
      case "permission": {
        const baseUrl = normalizeBase(message.baseUrl);
        const granted = await chrome.permissions.request({
          origins: [originPattern(baseUrl)],
        });
        return { ok: true, data: { granted } };
      }
      case "openOptions":
        await chrome.runtime.openOptionsPage();
        return { ok: true, data: null };
      default:
        throw new Error(`Unknown extension action: ${message.action}`);
    }
  })()
    .then(sendResponse)
    .catch((error) =>
      sendResponse({
        ok: false,
        error: {
          message: error.message || String(error),
          code: error.code || "extension_error",
          status: error.status || 0,
          details: error.details || null,
        },
      }),
    );
  return true;
});
