import { create } from "zustand";
import { api, ApiError, getRuntimeConfig, initApi, setApiToken } from "../lib/api";
import type {
  AddResult,
  Analytics,
  Category,
  Collection,
  DownloadTask,
  NewDownload,
  Queue,
  Rule,
  Stats,
  StoredRule,
} from "../lib/types";

export type Theme = "dark" | "light" | "black";
export type ToastKind = "success" | "error" | "info" | "warning";

export interface Toast {
  id: string;
  kind: ToastKind;
  title: string;
  message?: string;
}

interface AppState {
  baseUrl: string;
  token: string;
  authorized: boolean;
  ready: boolean;
  loading: boolean;
  error: string | null;
  theme: Theme;
  compact: boolean;

  tasks: DownloadTask[];
  stats: Stats | null;
  analytics: Analytics | null;
  categories: Category[];
  queues: Queue[];
  collections: Collection[];
  rules: StoredRule[];
  settings: Record<string, unknown>;
  toasts: Toast[];
  addOpen: boolean;
  detailId: string | null;

  init: () => Promise<void>;
  bootstrap: () => Promise<void>;
  refreshLive: () => Promise<void>;
  refreshMeta: () => Promise<void>;
  refreshAll: () => Promise<void>;
  setToken: (token: string) => Promise<void>;
  setTheme: (theme: Theme) => void;
  setCompact: (compact: boolean) => void;
  setAddOpen: (open: boolean) => void;
  setDetailId: (id: string | null) => void;
  pushToast: (kind: ToastKind, title: string, message?: string) => void;
  dismissToast: (id: string) => void;

  addTask: (payload: NewDownload) => Promise<AddResult>;
  pauseTask: (id: string) => Promise<void>;
  resumeTask: (id: string, restart?: boolean) => Promise<void>;
  retryTask: (id: string, restart?: boolean) => Promise<void>;
  cancelTask: (id: string) => Promise<void>;
  removeTask: (id: string, deleteFile?: boolean) => Promise<void>;
  verifyTask: (id: string) => Promise<void>;
  updateSettings: (patch: Record<string, unknown>) => Promise<void>;
  createCategory: (payload: Partial<Category> & { name: string }) => Promise<void>;
  deleteCategory: (id: string) => Promise<void>;
  createQueue: (name: string, maxConcurrent: number) => Promise<void>;
  updateQueue: (queue: Queue) => Promise<void>;
  deleteQueue: (id: string) => Promise<void>;
  pauseQueue: (id: string) => Promise<void>;
  startQueue: (id: string) => Promise<void>;
  createCollection: (payload: {
    name: string;
    description?: string;
    color?: string;
  }) => Promise<void>;
  deleteCollection: (id: string) => Promise<void>;
  saveRule: (rule: Rule) => Promise<void>;
  deleteRule: (id: string) => Promise<void>;
}

const storedTheme = localStorage.getItem("nex.theme") as Theme | null;
const storedCompact = localStorage.getItem("nex.compact") === "1";

export function applyTheme(theme: Theme) {
  document.documentElement.dataset.theme = theme;
  document.documentElement.style.colorScheme =
    theme === "light" ? "light" : "dark";
}

function messageOf(error: unknown): string {
  if (error instanceof ApiError) return error.message;
  if (error instanceof Error) return error.message;
  return String(error);
}

function toastId() {
  return `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

let initStarted = false;

export const useApp = create<AppState>((set, get) => {
  const notifyError = (error: unknown, title = "Something went wrong") => {
    if (error instanceof ApiError && error.status === 401) {
      set({ authorized: false, ready: true });
    }
    const message = messageOf(error);
    set({ error: message });
    get().pushToast("error", title, message);
  };

  const reload = async () => {
    await Promise.all([get().refreshLive(), get().refreshMeta()]);
  };

  return {
    baseUrl: getRuntimeConfig().baseUrl,
    token: getRuntimeConfig().token,
    authorized: false,
    ready: false,
    loading: true,
    error: null,
    theme: storedTheme ?? "dark",
    compact: storedCompact,

    tasks: [],
    stats: null,
    analytics: null,
    categories: [],
    queues: [],
    collections: [],
    rules: [],
    settings: {},
    toasts: [],
    addOpen: false,
    detailId: null,

    init: async () => {
      if (initStarted) return;
      initStarted = true;
      applyTheme(get().theme);
      const config = await initApi();
      set({ baseUrl: config.baseUrl, token: config.token });
      if (!config.token) {
        set({ ready: true, loading: false, authorized: false });
        return;
      }
      await get().bootstrap();
    },

    bootstrap: async () => {
      if (!get().token) {
        set({ ready: true, loading: false, authorized: false });
        return;
      }
      set({ loading: true, error: null });
      try {
        const [tasks, stats, analytics, categories, queues, collections, rules, settings] =
          await Promise.all([
            api.tasks({ limit: 500 }),
            api.stats(),
            api.analytics(),
            api.categories(),
            api.queues(),
            api.collections(),
            api.rules(),
            api.settings(),
          ]);
        const settingsTheme = settings.theme;
        const theme =
          (localStorage.getItem("nex.theme") as Theme | null) ??
          (settingsTheme === "light" || settingsTheme === "black" || settingsTheme === "dark"
            ? settingsTheme
            : get().theme);
        applyTheme(theme);
        set({
          tasks,
          stats,
          analytics,
          categories,
          queues,
          collections,
          rules,
          settings,
          theme,
          authorized: true,
          ready: true,
          loading: false,
          error: null,
        });
      } catch (error) {
        if (error instanceof ApiError && error.status === 401) {
          set({ authorized: false, ready: true, loading: false, error: null });
          return;
        }
        set({
          ready: true,
          loading: false,
          error: messageOf(error),
          authorized: false,
        });
      }
    },

    refreshLive: async () => {
      if (!get().authorized) return;
      try {
        const [tasks, stats] = await Promise.all([api.tasks({ limit: 500 }), api.stats()]);
        set({ tasks, stats });
      } catch (error) {
        if (error instanceof ApiError && error.status === 401) {
          set({ authorized: false });
        }
      }
    },

    refreshMeta: async () => {
      if (!get().authorized) return;
      try {
        const [analytics, categories, queues, collections, rules] = await Promise.all([
          api.analytics(),
          api.categories(),
          api.queues(),
          api.collections(),
          api.rules(),
        ]);
        set({ analytics, categories, queues, collections, rules });
      } catch (error) {
        if (error instanceof ApiError && error.status === 401) {
          set({ authorized: false });
        }
      }
    },

    refreshAll: reload,

    setToken: async (token: string) => {
      setApiToken(token);
      set({ token, authorized: false, ready: false });
      await get().bootstrap();
    },

    setTheme: (theme) => {
      applyTheme(theme);
      localStorage.setItem("nex.theme", theme);
      set({ theme });
      void api
        .updateSettings({ theme })
        .then((settings) => set({ settings }))
        .catch(() => undefined);
    },

    setCompact: (compact) => {
      localStorage.setItem("nex.compact", compact ? "1" : "0");
      set({ compact });
      void api
        .updateSettings({ compact_rows: compact })
        .then((settings) => set({ settings }))
        .catch(() => undefined);
    },

    setAddOpen: (open) => set({ addOpen: open }),
    setDetailId: (id) => set({ detailId: id }),

    pushToast: (kind, title, message) => {
      const toast: Toast = { id: toastId(), kind, title, message };
      set((state) => ({ toasts: [...state.toasts.slice(-4), toast] }));
      window.setTimeout(() => get().dismissToast(toast.id), 5200);
    },

    dismissToast: (id) =>
      set((state) => ({ toasts: state.toasts.filter((toast) => toast.id !== id) })),

    addTask: async (payload) => {
      const result = await api.createTask(payload);
      await reload();
      const warnings = [
        result.rule_applied ? `Rule “${result.rule_applied}” applied.` : null,
        result.duplicate_of ? "A matching completed download already exists." : null,
      ].filter(Boolean);
      get().pushToast(
        "success",
        "Download added",
        warnings.length ? warnings.join(" ") : result.task.file_name,
      );
      return result;
    },

    pauseTask: async (id) => {
      try {
        await api.pauseTask(id);
        await reload();
      } catch (error) {
        notifyError(error, "Could not pause download");
      }
    },

    resumeTask: async (id, restart = false) => {
      try {
        await api.resumeTask(id, restart);
        await reload();
      } catch (error) {
        notifyError(error, "Could not resume download");
      }
    },

    retryTask: async (id, restart = false) => {
      try {
        await api.retryTask(id, restart);
        await reload();
      } catch (error) {
        notifyError(error, "Could not retry download");
      }
    },

    cancelTask: async (id) => {
      try {
        await api.cancelTask(id);
        await reload();
        get().pushToast("info", "Download cancelled");
      } catch (error) {
        notifyError(error, "Could not cancel download");
      }
    },

    removeTask: async (id, deleteFile = false) => {
      try {
        await api.deleteTask(id, deleteFile);
        await reload();
        get().pushToast("info", "Download removed");
      } catch (error) {
        notifyError(error, "Could not remove download");
      }
    },

    verifyTask: async (id) => {
      try {
        get().pushToast("info", "Verifying file", "Computing SHA-256 checksum…");
        const report = await api.verifyTask(id);
        await reload();
        if (report.status === "checksum_mismatch" || report.status === "size_mismatch") {
          get().pushToast("error", "Integrity check failed", report.status.replaceAll("_", " "));
        } else {
          get().pushToast(
            "success",
            "Integrity verified",
            report.status === "not_configured"
              ? `SHA-256: ${report.actual_sha256}`
              : "The file matches the expected SHA-256 digest.",
          );
        }
      } catch (error) {
        notifyError(error, "Could not verify file");
      }
    },

    updateSettings: async (patch) => {
      try {
        const settings = await api.updateSettings(patch);
        set({ settings });
        if (typeof patch.theme === "string") {
          const theme = patch.theme as Theme;
          applyTheme(theme);
          localStorage.setItem("nex.theme", theme);
          set({ theme });
        }
        if (typeof patch.compact_rows === "boolean") {
          set({ compact: patch.compact_rows });
          localStorage.setItem("nex.compact", patch.compact_rows ? "1" : "0");
        }
        await get().refreshLive();
        get().pushToast("success", "Settings saved");
      } catch (error) {
        notifyError(error, "Could not save settings");
        throw error;
      }
    },

    createCategory: async (payload) => {
      try {
        await api.createCategory(payload);
        await get().refreshMeta();
        get().pushToast("success", "Category created", payload.name);
      } catch (error) {
        notifyError(error, "Could not create category");
      }
    },

    deleteCategory: async (id) => {
      try {
        await api.deleteCategory(id);
        await get().refreshMeta();
        get().pushToast("info", "Category deleted");
      } catch (error) {
        notifyError(error, "Could not delete category");
      }
    },

    createQueue: async (name, maxConcurrent) => {
      try {
        await api.createQueue(name, maxConcurrent);
        await get().refreshMeta();
        get().pushToast("success", "Queue created", name);
      } catch (error) {
        notifyError(error, "Could not create queue");
      }
    },

    updateQueue: async (queue) => {
      try {
        await api.updateQueue(queue);
        await get().refreshMeta();
        get().pushToast("success", "Queue updated", queue.name);
      } catch (error) {
        notifyError(error, "Could not update queue");
      }
    },

    deleteQueue: async (id) => {
      try {
        await api.deleteQueue(id);
        await get().refreshMeta();
        get().pushToast("info", "Queue deleted");
      } catch (error) {
        notifyError(error, "Could not delete queue");
      }
    },

    pauseQueue: async (id) => {
      try {
        await api.pauseQueue(id);
        await reload();
        get().pushToast("info", "Queue paused");
      } catch (error) {
        notifyError(error, "Could not pause queue");
      }
    },

    startQueue: async (id) => {
      try {
        await api.startQueue(id);
        await reload();
        get().pushToast("success", "Queue started");
      } catch (error) {
        notifyError(error, "Could not start queue");
      }
    },

    createCollection: async (payload) => {
      try {
        await api.createCollection(payload);
        await get().refreshMeta();
        get().pushToast("success", "Collection created", payload.name);
      } catch (error) {
        notifyError(error, "Could not create collection");
      }
    },

    deleteCollection: async (id) => {
      try {
        await api.deleteCollection(id);
        await get().refreshMeta();
        get().pushToast("info", "Collection deleted");
      } catch (error) {
        notifyError(error, "Could not delete collection");
      }
    },

    saveRule: async (rule) => {
      try {
        const saved = await api.saveRule(rule);
        await get().refreshMeta();
        get().pushToast("success", rule.id ? "Rule updated" : "Rule created", saved.name);
      } catch (error) {
        notifyError(error, "Could not save rule");
        throw error;
      }
    },

    deleteRule: async (id) => {
      try {
        await api.deleteRule(id);
        await get().refreshMeta();
        get().pushToast("info", "Rule deleted");
      } catch (error) {
        notifyError(error, "Could not delete rule");
      }
    },
  };
});
