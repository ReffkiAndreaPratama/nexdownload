const DEFAULT_SETTINGS = {
  baseUrl: "http://127.0.0.1:47821",
  token: "",
  intercept: true,
  notify: true,
};

const form = document.getElementById("settings");
const baseUrlInput = document.getElementById("base-url");
const tokenInput = document.getElementById("token");
const interceptInput = document.getElementById("intercept");
const notifyInput = document.getElementById("notify");
const status = document.getElementById("status");

function describeError(error) {
  return error && error.message ? error.message : String(error);
}

function setStatus(kind, text) {
  status.className = `status${kind ? ` ${kind}` : ""}`;
  status.textContent = text;
}

function normalizeBase(raw) {
  const url = new URL(String(raw || "").trim());
  if (url.protocol !== "http:" && url.protocol !== "https:") {
    throw new Error("The API URL must use http:// or https://.");
  }
  return `${url.origin}${url.pathname.replace(/\/+$/, "")}`;
}

function originPattern(baseUrl) {
  const url = new URL(baseUrl);
  return `${url.protocol}//${url.host}/*`;
}

async function requestHostPermission(baseUrl) {
  try {
    const granted = await chrome.permissions.request({
      origins: [originPattern(baseUrl)],
    });
    if (!granted) {
      throw new Error(
        "The browser did not grant access to this API host. Allow it and try again.",
      );
    }
  } catch (error) {
    throw new Error(
      `Cannot access ${originPattern(baseUrl)}: ${describeError(error)}`,
    );
  }
}

async function send(action, payload = {}) {
  const response = await chrome.runtime.sendMessage({
    type: "nex:request",
    action,
    ...payload,
  });
  if (!response) {
    throw new Error("The extension background worker did not respond.");
  }
  if (!response.ok) {
    const details = response.error || {};
    const error = new Error(details.message || "NexDownload request failed.");
    error.code = details.code || "extension_error";
    error.status = details.status || 0;
    throw error;
  }
  return response.data;
}

async function load() {
  const stored = await chrome.storage.local.get(DEFAULT_SETTINGS);
  baseUrlInput.value = stored.baseUrl || DEFAULT_SETTINGS.baseUrl;
  tokenInput.value = stored.token || "";
  interceptInput.checked = Boolean(stored.intercept);
  notifyInput.checked = Boolean(stored.notify);
}

async function save(event) {
  event.preventDefault();
  setStatus("", "Saving…");
  try {
    const baseUrl = normalizeBase(baseUrlInput.value);
    const token = tokenInput.value.trim();
    if (!token) {
      throw new Error("The API token is required.");
    }
    await requestHostPermission(baseUrl);
    await chrome.storage.local.set({
      baseUrl,
      token,
      intercept: interceptInput.checked,
      notify: notifyInput.checked,
    });
    baseUrlInput.value = baseUrl;
    setStatus("success", "Settings saved.");
  } catch (error) {
    setStatus("error", describeError(error));
  }
}

async function test() {
  setStatus("", "Testing…");
  try {
    const baseUrl = normalizeBase(baseUrlInput.value);
    const token = tokenInput.value.trim();
    if (!token) {
      throw new Error("Enter the API token before testing the connection.");
    }
    await requestHostPermission(baseUrl);
    const result = await send("test", { baseUrl, token });
    setStatus(
      "success",
      `Connected to NexDownload ${result.version}. ${result.totalTasks} task(s), ${result.active} active.`,
    );
  } catch (error) {
    setStatus("error", describeError(error));
  }
}

async function openDashboard() {
  try {
    const stored = await chrome.storage.local.get(DEFAULT_SETTINGS);
    const baseUrl = normalizeBase(stored.baseUrl || DEFAULT_SETTINGS.baseUrl);
    await chrome.tabs.create({ url: `${baseUrl}/` });
  } catch (error) {
    setStatus("error", describeError(error));
  }
}

form.addEventListener("submit", (event) => void save(event));
document.getElementById("test").addEventListener("click", () => void test());
document.getElementById("dashboard").addEventListener("click", () => void openDashboard());

void load();
