const elements = {
  status: document.getElementById("status"),
  setup: document.getElementById("setup"),
  errorPanel: document.getElementById("error-panel"),
  errorText: document.getElementById("error-text"),
  overview: document.getElementById("overview"),
  activeCount: document.getElementById("active-count"),
  speed: document.getElementById("speed"),
  completedCount: document.getElementById("completed-count"),
  recent: document.getElementById("recent"),
  form: document.getElementById("add-form"),
  url: document.getElementById("url"),
  fileName: document.getElementById("file-name"),
  add: document.getElementById("add"),
  formMessage: document.getElementById("form-message"),
  dashboard: document.getElementById("dashboard"),
  options: document.getElementById("options"),
  openOptions: document.getElementById("open-options"),
  retry: document.getElementById("retry"),
  version: document.getElementById("version"),
};

let dashboardUrl = "";

function describeError(error) {
  if (!error) return "Unknown error";
  return error.message || String(error);
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
    error.details = details.details || null;
    throw error;
  }
  return response.data;
}

function setStatus(kind, text) {
  elements.status.className = `status${kind ? ` ${kind}` : ""}`;
  elements.status.textContent = text;
}

function show(element, visible) {
  element.classList.toggle("hidden", !visible);
}

function formatBytes(value) {
  const bytes = Number(value || 0);
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KiB", "MiB", "GiB", "TiB"];
  const power = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const scaled = bytes / 1024 ** power;
  return `${scaled >= 10 || power === 0 ? scaled.toFixed(0) : scaled.toFixed(1)} ${units[power]}`;
}

function formatSpeed(value) {
  return `${formatBytes(value)}/s`;
}

function renderTasks(tasks) {
  elements.recent.replaceChildren();
  for (const task of tasks.slice(0, 6)) {
    const item = document.createElement("li");
    const button = document.createElement("button");
    button.type = "button";
    button.title = task.file_name || task.url || "Download";

    const name = document.createElement("span");
    name.className = "task-name";
    name.textContent = task.file_name || task.url || "Download";

    const status = document.createElement("span");
    status.className = "task-status";
    status.textContent = task.status || "unknown";

    button.append(name, status);
    button.addEventListener("click", () => {
      if (dashboardUrl) {
        void chrome.tabs.create({ url: dashboardUrl });
      }
    });
    item.append(button);
    elements.recent.append(item);
  }
}

async function load() {
  setStatus("", "Checking local API…");
  show(elements.setup, false);
  show(elements.errorPanel, false);
  show(elements.overview, false);
  try {
    const state = await send("dashboard");
    dashboardUrl = state.baseUrl ? `${state.baseUrl.replace(/\/+$/, "")}/` : "";
    if (!state.configured) {
      setStatus("error", "Not configured");
      show(elements.setup, true);
      show(elements.overview, false);
      return;
    }
    if (state.error) {
      setStatus("error", "API unavailable");
      elements.errorText.textContent = state.error;
      show(elements.errorPanel, true);
      return;
    }
    setStatus("connected", "Connected");
    elements.activeCount.textContent = String(state.stats?.active ?? 0);
    elements.speed.textContent = formatSpeed(state.stats?.current_speed_bps);
    elements.completedCount.textContent = String(state.stats?.completed ?? 0);
    renderTasks(state.tasks || []);
    show(elements.overview, true);
  } catch (error) {
    setStatus("error", "Extension error");
    elements.errorText.textContent = describeError(error);
    show(elements.errorPanel, true);
  }
}

async function submitDownload(event) {
  event.preventDefault();
  elements.formMessage.className = "form-message";
  elements.formMessage.textContent = "";
  const url = elements.url.value.trim();
  if (!url) return;
  const fileName = elements.fileName.value.trim();
  elements.add.disabled = true;
  elements.add.textContent = "Sending…";
  try {
    let result;
    try {
      result = await send("add", { url, fileName, confirmed: false });
    } catch (error) {
      if (error.code !== "confirmation_required") {
        throw error;
      }
      const approved = window.confirm(
        "A NexDownload rule requires confirmation for this download. Add it anyway?",
      );
      if (!approved) {
        elements.formMessage.textContent = "Download was not added.";
        return;
      }
      result = await send("add", { url, fileName, confirmed: true });
    }
    const name = result?.task?.file_name || "Download";
    elements.formMessage.className = "form-message success";
    elements.formMessage.textContent = `Queued ${name}.`;
    elements.url.value = "";
    elements.fileName.value = "";
    await load();
  } catch (error) {
    elements.formMessage.className = "form-message error";
    elements.formMessage.textContent = describeError(error);
  } finally {
    elements.add.disabled = false;
    elements.add.textContent = "Send to NexDownload";
  }
}

elements.form.addEventListener("submit", submitDownload);
elements.retry.addEventListener("click", () => void load());
elements.options.addEventListener("click", () => void chrome.runtime.openOptionsPage());
elements.openOptions.addEventListener("click", () => void chrome.runtime.openOptionsPage());
elements.dashboard.addEventListener("click", () => {
  if (dashboardUrl) {
    void chrome.tabs.create({ url: dashboardUrl });
  } else {
    void chrome.runtime.openOptionsPage();
  }
});

void load();
