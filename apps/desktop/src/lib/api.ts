import type {
  AddResult,
  Analytics,
  BenchmarkResult,
  Category,
  Collection,
  Diagnostics,
  DownloadTask,
  DuplicateGroup,
  IntegrityReport,
  NewDownload,
  Queue,
  Rule,
  Stats,
  StoredRule,
} from "./types";

export class ApiError extends Error {
  code: string;
  status: number;
  details: unknown;

  constructor(message: string, code: string, status: number, details?: unknown) {
    super(message);
    this.name = "ApiError";
    this.code = code;
    this.status = status;
    this.details = details;
  }
}

export const isTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const storedBaseUrl =
  typeof localStorage !== "undefined" ? localStorage.getItem("nex.base") : null;
const storedToken =
  typeof localStorage !== "undefined" ? localStorage.getItem("nex.token") : null;

let baseUrl =
  storedBaseUrl ??
  (import.meta.env.VITE_API_URL as string | undefined)?.replace(/\/$/, "") ??
  "http://127.0.0.1:47821";
let token =
  storedToken ??
  (import.meta.env.VITE_API_TOKEN as string | undefined) ??
  "";
let initialized = false;

export async function initApi(): Promise<{ baseUrl: string; token: string }> {
  if (initialized) return { baseUrl, token };
  initialized = true;
  if (isTauri) {
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const config = await invoke<{ base_url: string; token: string }>("api_config");
      baseUrl = config.base_url.replace(/\/$/, "");
      token = config.token;
    } catch (error) {
      console.warn("Could not load the desktop API configuration", error);
    }
  }
  return { baseUrl, token };
}

export function getRuntimeConfig() {
  return { baseUrl, token, isTauri };
}

export function setApiToken(value: string) {
  token = value.trim();
  localStorage.setItem("nex.token", token);
}

export function getApiToken() {
  return token;
}

export function setApiBaseUrl(value: string) {
  baseUrl = value.trim().replace(/\/$/, "");
  localStorage.setItem("nex.base", baseUrl);
}

async function request<T>(path: string, init: RequestInit = {}): Promise<T> {
  const headers = new Headers(init.headers);
  if (token) headers.set("Authorization", `Bearer ${token}`);
  if (init.body && !headers.has("Content-Type")) {
    headers.set("Content-Type", "application/json");
  }
  const response = await fetch(`${baseUrl}/api/v1${path}`, { ...init, headers });
  if (!response.ok) {
    let body: { code?: string; message?: string; details?: unknown } = {};
    try {
      body = await response.json();
    } catch {
      body = {};
    }
    throw new ApiError(
      body.message ?? `Request failed with HTTP ${response.status}`,
      body.code ?? "http_error",
      response.status,
      body.details,
    );
  }
  if (response.status === 204) return undefined as T;
  const text = await response.text();
  return (text ? JSON.parse(text) : undefined) as T;
}

export const api = {
  health: () => request<{ status: string; version: string }>("/health"),

  tasks: (query: Record<string, string | number | undefined> = {}) => {
    const params = new URLSearchParams();
    for (const [key, value] of Object.entries(query)) {
      if (value !== undefined && value !== "") params.set(key, String(value));
    }
    const suffix = params.toString() ? `?${params.toString()}` : "";
    return request<DownloadTask[]>(`/tasks${suffix}`);
  },
  task: (id: string) => request<DownloadTask>(`/tasks/${id}`),
  createTask: (payload: NewDownload) =>
    request<AddResult>("/tasks", { method: "POST", body: JSON.stringify(payload) }),
  deleteTask: (id: string, deleteFile = false) =>
    request<void>(`/tasks/${id}?delete_file=${deleteFile}`, { method: "DELETE" }),
  pauseTask: (id: string) =>
    request<DownloadTask>(`/tasks/${id}/pause`, { method: "POST" }),
  resumeTask: (id: string, restart = false) =>
    request<DownloadTask>(`/tasks/${id}/resume`, {
      method: "POST",
      body: JSON.stringify({ restart }),
    }),
  retryTask: (id: string, restart = false) =>
    request<DownloadTask>(`/tasks/${id}/retry`, {
      method: "POST",
      body: JSON.stringify({ restart }),
    }),
  cancelTask: (id: string) =>
    request<DownloadTask>(`/tasks/${id}/cancel`, { method: "POST" }),
  patchTask: (id: string, patch: Record<string, unknown>) =>
    request<DownloadTask>(`/tasks/${id}`, {
      method: "PATCH",
      body: JSON.stringify(patch),
    }),
  taskEvents: (id: string) => request<unknown[]>(`/tasks/${id}/events`),
  taskIntegrity: (id: string) => request<unknown[]>(`/tasks/${id}/integrity`),
  verifyTask: (id: string) =>
    request<IntegrityReport>(`/tasks/${id}/integrity`, { method: "POST" }),
  diagnostics: (id: string) => request<Diagnostics>(`/tasks/${id}/diagnostics`),

  stats: () => request<Stats>("/stats"),
  analytics: () => request<Analytics>("/analytics"),
  speedSamples: (hours = 24) => request<unknown[]>(`/speed?hours=${hours}`),

  categories: () => request<Category[]>("/categories"),
  createCategory: (payload: Partial<Category> & { name: string }) =>
    request<Category>("/categories", { method: "POST", body: JSON.stringify(payload) }),
  deleteCategory: (id: string) =>
    request<void>(`/categories/${id}`, { method: "DELETE" }),

  queues: () => request<Queue[]>("/queues"),
  createQueue: (name: string, maxConcurrent: number) =>
    request<Queue>("/queues", {
      method: "POST",
      body: JSON.stringify({ name, max_concurrent: maxConcurrent }),
    }),
  updateQueue: (queue: Queue) =>
    request<Queue>(`/queues/${queue.id}`, {
      method: "PATCH",
      body: JSON.stringify(queue),
    }),
  deleteQueue: (id: string) => request<void>(`/queues/${id}`, { method: "DELETE" }),
  pauseQueue: (id: string) =>
    request<void>(`/queues/${id}/pause`, { method: "POST" }),
  startQueue: (id: string) =>
    request<void>(`/queues/${id}/start`, { method: "POST" }),

  collections: () => request<Collection[]>("/collections"),
  createCollection: (payload: {
    name: string;
    description?: string;
    color?: string;
  }) =>
    request<Collection>("/collections", {
      method: "POST",
      body: JSON.stringify(payload),
    }),
  deleteCollection: (id: string) =>
    request<void>(`/collections/${id}`, { method: "DELETE" }),

  rules: () => request<StoredRule[]>("/rules"),
  saveRule: (rule: Rule) =>
    request<StoredRule>("/rules", { method: "POST", body: JSON.stringify(rule) }),
  deleteRule: (id: string) => request<void>(`/rules/${id}`, { method: "DELETE" }),

  settings: () => request<Record<string, unknown>>("/settings"),
  updateSettings: (patch: Record<string, unknown>) =>
    request<Record<string, unknown>>("/settings", {
      method: "PUT",
      body: JSON.stringify(patch),
    }),

  benchmark: (url: string) =>
    request<BenchmarkResult>("/benchmark", {
      method: "POST",
      body: JSON.stringify({ url }),
    }),
  duplicates: () => request<DuplicateGroup[]>("/duplicates"),
  history: (search?: string) =>
    request<DownloadTask[]>(
      `/history${search ? `?search=${encodeURIComponent(search)}` : ""}`,
    ),
  exportHistory: async () => {
    const response = await fetch(`${baseUrl}/api/v1/history/export`, {
      headers: token ? { Authorization: `Bearer ${token}` } : {},
    });
    if (!response.ok) throw new ApiError("Export failed", "export_failed", response.status);
    return response.blob();
  },
  audit: () => request<unknown[]>("/audit"),
};

export async function revealPath(path: string) {
  if (!isTauri) {
    throw new Error("Revealing files is only available in the desktop application.");
  }
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("reveal_path", { path });
}

export async function openPath(path: string) {
  if (!isTauri) {
    throw new Error("Opening files is only available in the desktop application.");
  }
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("open_path", { path });
}
