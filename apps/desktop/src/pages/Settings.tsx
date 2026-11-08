import { useEffect, useMemo, useState } from "react";
import {
  Activity,
  Bell,
  Cpu,
  Eye,
  EyeOff,
  FolderOpen,
  HardDrive,
  Palette,
  Save,
  Server,
  ShieldCheck,
  Wifi,
} from "lucide-react";
import PageHeader from "../components/PageHeader";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Input, Toggle } from "../components/ui/input";
import { api, getRuntimeConfig } from "../lib/api";
import { cn } from "../lib/utils";
import { useApp, type Theme } from "../store/app";

const themes: { id: Theme; label: string; description: string; swatch: string }[] = [
  { id: "dark", label: "Midnight", description: "Deep slate with teal accents", swatch: "bg-[#0b1017]" },
  { id: "black", label: "OLED", description: "Pure black for dark rooms", swatch: "bg-black" },
  { id: "light", label: "Daylight", description: "Bright neutral workspace", swatch: "bg-[#f6f7f9]" },
];

interface FormState {
  max_concurrent: string;
  default_connections: string;
  retry_limit: string;
  timeout_secs: string;
  bandwidth_mbps: string;
  default_destination: string;
  adaptive_connections: boolean;
  auto_resume: boolean;
  verify_integrity: boolean;
  notifications: boolean;
  intercept_downloads: boolean;
}

function integerSetting(settings: Record<string, unknown>, key: string, fallback: number) {
  const value = settings[key];
  return typeof value === "number" && Number.isFinite(value) ? String(value) : String(fallback);
}

function boolSetting(settings: Record<string, unknown>, key: string, fallback: boolean) {
  return typeof settings[key] === "boolean" ? (settings[key] as boolean) : fallback;
}

export default function Settings() {
  const settings = useApp((state) => state.settings);
  const theme = useApp((state) => state.theme);
  const compact = useApp((state) => state.compact);
  const setTheme = useApp((state) => state.setTheme);
  const setCompact = useApp((state) => state.setCompact);
  const updateSettings = useApp((state) => state.updateSettings);
  const pushToast = useApp((state) => state.pushToast);
  const baseUrl = useApp((state) => state.baseUrl);

  const [form, setForm] = useState<FormState>(() => ({
    max_concurrent: integerSetting(settings, "max_concurrent_downloads", 3),
    default_connections: integerSetting(settings, "default_connections", 4),
    retry_limit: integerSetting(settings, "retry_limit", 3),
    timeout_secs: integerSetting(settings, "timeout_secs", 30),
    bandwidth_mbps:
      typeof settings.bandwidth_limit_bps === "number" && settings.bandwidth_limit_bps > 0
        ? String(Number(((settings.bandwidth_limit_bps as number) / 1024 / 1024).toFixed(2)))
        : "0",
    default_destination:
      typeof settings.default_destination === "string" ? settings.default_destination : "",
    adaptive_connections: boolSetting(settings, "adaptive_connections", true),
    auto_resume: boolSetting(settings, "auto_resume", true),
    verify_integrity: boolSetting(settings, "verify_integrity", true),
    notifications: boolSetting(settings, "notifications", true),
    intercept_downloads: boolSetting(settings, "intercept_downloads", true),
  }));
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [version, setVersion] = useState<string | null>(null);
  const [showToken, setShowToken] = useState(false);
  const token = getRuntimeConfig().token;

  useEffect(() => {
    setForm({
      max_concurrent: integerSetting(settings, "max_concurrent_downloads", 3),
      default_connections: integerSetting(settings, "default_connections", 4),
      retry_limit: integerSetting(settings, "retry_limit", 3),
      timeout_secs: integerSetting(settings, "timeout_secs", 30),
      bandwidth_mbps:
        typeof settings.bandwidth_limit_bps === "number" && settings.bandwidth_limit_bps > 0
          ? String(Number(((settings.bandwidth_limit_bps as number) / 1024 / 1024).toFixed(2)))
          : "0",
      default_destination:
        typeof settings.default_destination === "string" ? settings.default_destination : "",
      adaptive_connections: boolSetting(settings, "adaptive_connections", true),
      auto_resume: boolSetting(settings, "auto_resume", true),
      verify_integrity: boolSetting(settings, "verify_integrity", true),
      notifications: boolSetting(settings, "notifications", true),
      intercept_downloads: boolSetting(settings, "intercept_downloads", true),
    });
  }, [settings]);

  useEffect(() => {
    void api
      .health()
      .then((health) => setVersion(health.version))
      .catch(() => undefined);
  }, []);

  const maskedToken = useMemo(() => {
    if (!token) return "not loaded";
    if (token.length <= 10) return token;
    return `${token.slice(0, 6)}…${token.slice(-4)}`;
  }, [token]);

  const validate = () => {
    const maxConcurrent = Number(form.max_concurrent);
    const connections = Number(form.default_connections);
    const retries = Number(form.retry_limit);
    const timeout = Number(form.timeout_secs);
    const bandwidth = Number(form.bandwidth_mbps);
    if (!Number.isInteger(maxConcurrent) || maxConcurrent < 1 || maxConcurrent > 16) {
      return "Maximum concurrent downloads must be an integer from 1 to 16.";
    }
    if (!Number.isInteger(connections) || connections < 1 || connections > 32) {
      return "Default connections must be an integer from 1 to 32.";
    }
    if (!Number.isInteger(retries) || retries < 0 || retries > 20) {
      return "Retry limit must be an integer from 0 to 20.";
    }
    if (!Number.isInteger(timeout) || timeout < 5 || timeout > 600) {
      return "Request timeout must be an integer from 5 to 600 seconds.";
    }
    if (!Number.isFinite(bandwidth) || bandwidth < 0) {
      return "Bandwidth limit must be zero (unlimited) or a positive number.";
    }
    return null;
  };

  const save = async () => {
    const message = validate();
    if (message) {
      setError(message);
      return;
    }
    setError(null);
    setSaving(true);
    try {
      await updateSettings({
        max_concurrent_downloads: Number(form.max_concurrent),
        default_connections: Number(form.default_connections),
        retry_limit: Number(form.retry_limit),
        timeout_secs: Number(form.timeout_secs),
        bandwidth_limit_bps: Math.round(Number(form.bandwidth_mbps) * 1024 * 1024),
        default_destination: form.default_destination.trim(),
        adaptive_connections: form.adaptive_connections,
        auto_resume: form.auto_resume,
        verify_integrity: form.verify_integrity,
        notifications: form.notifications,
        intercept_downloads: form.intercept_downloads,
      });
    } catch {
      // The store already emits a toast with the server message.
    } finally {
      setSaving(false);
    }
  };

  const testConnection = async () => {
    try {
      const health = await api.health();
      setVersion(health.version);
      pushToast("success", "API reachable", `NexDownload ${health.version}`);
    } catch (caught) {
      pushToast("error", "API unreachable", caught instanceof Error ? caught.message : String(caught));
    }
  };

  return (
    <div>
      <PageHeader
        title="Settings"
        description="Every setting below is persisted in the local SQLite database."
        actions={
          <Button variant="primary" size="sm" onClick={() => void save()} disabled={saving}>
            <Save className="h-3.5 w-3.5" /> Save changes
          </Button>
        }
      />

      <div className="grid gap-5 xl:grid-cols-2">
        <section className="card p-5">
          <div className="mb-4 flex items-center gap-2">
            <Palette className="h-4 w-4 text-accent" />
            <h2 className="text-sm font-semibold">Appearance</h2>
          </div>
          <div className="grid gap-2 sm:grid-cols-3">
            {themes.map((item) => (
              <button
                key={item.id}
                type="button"
                onClick={() => setTheme(item.id)}
                className={cn(
                  "rounded-lg border p-3 text-left transition",
                  theme === item.id
                    ? "border-accent/50 bg-accent/5"
                    : "border-border hover:border-border/80 hover:bg-raised/50",
                )}
              >
                <span className={cn("mb-2 block h-8 w-full rounded border border-white/10", item.swatch)} />
                <span className="block text-xs font-medium">{item.label}</span>
                <span className="mt-0.5 block text-[10px] text-muted">{item.description}</span>
              </button>
            ))}
          </div>
          <div className="mt-4 flex items-center justify-between rounded-lg border border-border/60 px-3 py-2.5">
            <div>
              <p className="text-xs font-medium">Compact rows</p>
              <p className="text-[11px] text-muted">Fit more transfers on screen.</p>
            </div>
            <Toggle checked={compact} label="Compact rows" onChange={setCompact} />
          </div>
        </section>

        <section className="card p-5">
          <div className="mb-4 flex items-center gap-2">
            <Cpu className="h-4 w-4 text-accent" />
            <h2 className="text-sm font-semibold">Engine</h2>
          </div>
          <div className="grid gap-3 sm:grid-cols-2">
            <div>
              <label className="label" htmlFor="set-concurrent">
                Max concurrent downloads
              </label>
              <Input
                id="set-concurrent"
                type="number"
                min={1}
                max={16}
                value={form.max_concurrent}
                onChange={(event) => setForm({ ...form, max_concurrent: event.target.value })}
              />
            </div>
            <div>
              <label className="label" htmlFor="set-connections">
                Default connections
              </label>
              <Input
                id="set-connections"
                type="number"
                min={1}
                max={32}
                value={form.default_connections}
                onChange={(event) => setForm({ ...form, default_connections: event.target.value })}
              />
            </div>
            <div>
              <label className="label" htmlFor="set-retries">
                Retry limit
              </label>
              <Input
                id="set-retries"
                type="number"
                min={0}
                max={20}
                value={form.retry_limit}
                onChange={(event) => setForm({ ...form, retry_limit: event.target.value })}
              />
            </div>
            <div>
              <label className="label" htmlFor="set-timeout">
                Request timeout (seconds)
              </label>
              <Input
                id="set-timeout"
                type="number"
                min={5}
                max={600}
                value={form.timeout_secs}
                onChange={(event) => setForm({ ...form, timeout_secs: event.target.value })}
              />
            </div>
            <div className="sm:col-span-2">
              <label className="label" htmlFor="set-bandwidth">
                Bandwidth limit (MiB/s)
              </label>
              <Input
                id="set-bandwidth"
                type="number"
                min={0}
                step="0.1"
                value={form.bandwidth_mbps}
                onChange={(event) => setForm({ ...form, bandwidth_mbps: event.target.value })}
              />
              <p className="mt-1 text-[11px] text-muted">
                Zero means unlimited. The limiter applies per engine instance, not per connection.
              </p>
            </div>
          </div>
          <div className="mt-4 space-y-2">
            <div className="flex items-center justify-between rounded-lg border border-border/60 px-3 py-2.5">
              <div>
                <p className="text-xs font-medium">Adaptive connections</p>
                <p className="text-[11px] text-muted">
                  Lower active connections when the source is unstable.
                </p>
              </div>
              <Toggle
                checked={form.adaptive_connections}
                label="Adaptive connections"
                onChange={(adaptive_connections) => setForm({ ...form, adaptive_connections })}
              />
            </div>
            <div className="flex items-center justify-between rounded-lg border border-border/60 px-3 py-2.5">
              <div>
                <p className="text-xs font-medium">Auto resume on startup</p>
                <p className="text-[11px] text-muted">
                  Requeue interrupted transfers after a crash or restart.
                </p>
              </div>
              <Toggle
                checked={form.auto_resume}
                label="Auto resume"
                onChange={(auto_resume) => setForm({ ...form, auto_resume })}
              />
            </div>
            <div className="flex items-center justify-between rounded-lg border border-border/60 px-3 py-2.5">
              <div>
                <p className="text-xs font-medium">Verify integrity after completion</p>
                <p className="text-[11px] text-muted">
                  Hash completed files and compare expected checksums.
                </p>
              </div>
              <Toggle
                checked={form.verify_integrity}
                label="Verify integrity"
                onChange={(verify_integrity) => setForm({ ...form, verify_integrity })}
              />
            </div>
          </div>
        </section>

        <section className="card p-5">
          <div className="mb-4 flex items-center gap-2">
            <FolderOpen className="h-4 w-4 text-accent" />
            <h2 className="text-sm font-semibold">Storage</h2>
          </div>
          <label className="label" htmlFor="set-destination">
            Default destination
          </label>
          <Input
            id="set-destination"
            className="font-mono text-xs"
            value={form.default_destination}
            onChange={(event) => setForm({ ...form, default_destination: event.target.value })}
            placeholder="e.g. C:\Users\you\Downloads"
            spellCheck={false}
          />
          <p className="mt-1 text-[11px] text-muted">
            Categories and rules can override this. Relative paths and paths escaping the destination
            root are rejected by the domain layer.
          </p>
          <div className="mt-4 space-y-2">
            <div className="flex items-center justify-between rounded-lg border border-border/60 px-3 py-2.5">
              <div>
                <p className="text-xs font-medium">Desktop notifications</p>
                <p className="text-[11px] text-muted">Notify when a transfer finishes or fails.</p>
              </div>
              <Toggle
                checked={form.notifications}
                label="Notifications"
                onChange={(notifications) => setForm({ ...form, notifications })}
              />
            </div>
            <div className="flex items-center justify-between rounded-lg border border-border/60 px-3 py-2.5">
              <div>
                <p className="text-xs font-medium">Browser hand-off</p>
                <p className="text-[11px] text-muted">
                  Remember this client&apos;s preference for extension-added downloads.
                </p>
              </div>
              <Toggle
                checked={form.intercept_downloads}
                label="Browser hand-off"
                onChange={(intercept_downloads) => setForm({ ...form, intercept_downloads })}
              />
            </div>
          </div>
        </section>

        <section className="card p-5">
          <div className="mb-4 flex items-center gap-2">
            <Server className="h-4 w-4 text-accent" />
            <h2 className="text-sm font-semibold">Local API</h2>
          </div>
          <div className="space-y-2 text-xs">
            <div className="flex items-center justify-between gap-3 rounded-lg border border-border/60 px-3 py-2">
              <span className="flex items-center gap-2 text-muted">
                <Wifi className="h-3.5 w-3.5" /> Base URL
              </span>
              <span className="truncate font-mono text-[11px]">{baseUrl}</span>
            </div>
            <div className="flex items-center justify-between gap-3 rounded-lg border border-border/60 px-3 py-2">
              <span className="flex items-center gap-2 text-muted">
                <ShieldCheck className="h-3.5 w-3.5" /> Bearer token
              </span>
              <div className="flex items-center gap-2">
                <span className="font-mono text-[11px]">{showToken ? token : maskedToken}</span>
                <button
                  type="button"
                  className="icon-btn"
                  onClick={() => setShowToken((value) => !value)}
                  aria-label={showToken ? "Hide token" : "Show token"}
                  title={showToken ? "Hide token" : "Show token"}
                >
                  {showToken ? <EyeOff className="h-3.5 w-3.5" /> : <Eye className="h-3.5 w-3.5" />}
                </button>
              </div>
            </div>
            <div className="flex items-center justify-between gap-3 rounded-lg border border-border/60 px-3 py-2">
              <span className="flex items-center gap-2 text-muted">
                <Activity className="h-3.5 w-3.5" /> Version
              </span>
              <span className="font-mono text-[11px]">{version ?? "—"}</span>
            </div>
            <div className="flex items-center justify-between gap-3 rounded-lg border border-border/60 px-3 py-2">
              <span className="flex items-center gap-2 text-muted">
                <Bell className="h-3.5 w-3.5" /> Auth scope
              </span>
              <Badge tone="success">every protected route</Badge>
            </div>
          </div>
          <div className="mt-4 flex gap-2">
            <Button size="sm" onClick={() => void testConnection()}>
              <Activity className="h-3.5 w-3.5" /> Test connection
            </Button>
            <Button
              size="sm"
              onClick={() => {
                void navigator.clipboard
                  .writeText(token)
                  .then(() => pushToast("success", "Token copied"))
                  .catch(() => pushToast("error", "Clipboard unavailable"));
              }}
            >
              Copy token
            </Button>
          </div>
          <div className="mt-4 rounded-lg border border-warning/25 bg-warning/5 p-3 text-[11px] leading-relaxed text-muted">
            <p className="flex items-center gap-1.5 font-medium text-warning">
              <ShieldCheck className="h-3.5 w-3.5" /> LAN access is a server-side decision
            </p>
            <p className="mt-1">
              NexDownload binds to loopback by default. Binding to a non-loopback address requires{" "}
              <code className="font-mono">NEXDOWNLOAD_TRUST_PROXY=1</code> and a TLS reverse proxy;
              the API never offers unauthenticated remote access.
            </p>
          </div>
        </section>
      </div>

      <div className="mt-4 flex flex-wrap items-center justify-between gap-3 rounded-xl border border-border/60 bg-surface/40 px-4 py-3">
        <div className="flex items-center gap-3 text-[11px] text-muted">
          <HardDrive className="h-4 w-4 text-subtle" />
          Settings are validated by the API before they are written to SQLite.
        </div>
        <div className="flex items-center gap-2">
          {error ? <span className="text-xs text-danger">{error}</span> : null}
          <Button variant="primary" size="sm" onClick={() => void save()} disabled={saving}>
            <Save className="h-3.5 w-3.5" /> {saving ? "Saving…" : "Save changes"}
          </Button>
        </div>
      </div>
    </div>
  );
}
