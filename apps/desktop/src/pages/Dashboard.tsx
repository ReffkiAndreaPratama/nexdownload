import { Link } from "react-router-dom";
import {
  ArrowRight,
  CheckCircle2,
  Clock3,
  DownloadCloud,
  Gauge,
  HardDrive,
  Layers,
  Plus,
  TrendingUp,
  Zap,
} from "lucide-react";
import DownloadRow from "../components/DownloadRow";
import EmptyState from "../components/EmptyState";
import PageHeader from "../components/PageHeader";
import StatTile from "../components/StatTile";
import { DistributionBar, Sparkline } from "../components/Charts";
import { Button } from "../components/ui/button";
import { formatBytes, formatDuration, formatSpeed, relativeTime } from "../lib/format";
import { useApp } from "../store/app";

export default function Dashboard() {
  const stats = useApp((state) => state.stats);
  const analytics = useApp((state) => state.analytics);
  const tasks = useApp((state) => state.tasks);
  const queues = useApp((state) => state.queues);
  const setAddOpen = useApp((state) => state.setAddOpen);
  const pauseQueue = useApp((state) => state.pauseQueue);
  const startQueue = useApp((state) => state.startQueue);

  const active = tasks
    .filter((task) =>
      ["downloading", "connecting", "preparing", "verifying"].includes(task.status),
    )
    .slice(0, 4);
  const pending = tasks.filter((task) => task.status === "queued").slice(0, 4);
  const recent = tasks
    .filter((task) => task.status === "completed")
    .sort((a, b) => (b.completed_at ?? "").localeCompare(a.completed_at ?? ""))
    .slice(0, 5);
  const speedValues = analytics?.speed_history.map((sample) => sample.speed_bps) ?? [];

  return (
    <div>
      <PageHeader
        title="Dashboard"
        description="A live view of the native engine, queues and storage."
        actions={
          <>
            <Link to="/downloads" className="btn btn-ghost btn-sm">
              View all <ArrowRight className="h-3.5 w-3.5" />
            </Link>
            <Button variant="primary" size="sm" onClick={() => setAddOpen(true)}>
              <Plus className="h-3.5 w-3.5" /> New download
            </Button>
          </>
        }
      />

      <section className="card overflow-hidden">
        <div className="grid gap-0 lg:grid-cols-[1.4fr_1fr]">
          <div className="p-5 sm:p-6">
            <div className="flex items-center gap-2 text-xs font-medium text-accent">
              <span className="relative flex h-2 w-2">
                <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-accent opacity-60" />
                <span className="relative inline-flex h-2 w-2 rounded-full bg-accent" />
              </span>
              Engine throughput
            </div>
            <div className="mt-3 flex flex-wrap items-end gap-3">
              <p className="text-4xl font-semibold tracking-tight">
                {formatSpeed(stats?.current_speed_bps)}
              </p>
              <p className="pb-1 text-xs text-muted">
                peak {formatSpeed(stats?.peak_speed_bps)}
              </p>
            </div>
            <p className="mt-1.5 max-w-xl text-xs leading-relaxed text-muted">
              {stats?.active ?? 0} active transfer{(stats?.active ?? 0) === 1 ? "" : "s"} ·{" "}
              {stats?.queued ?? 0} queued · {stats?.paused ?? 0} paused. Ranges are planned once and
              never re-flowed mid-transfer; the adaptive gate only changes connection usage.
            </p>
            <div className="mt-4">
              <Sparkline values={speedValues} height={78} />
            </div>
            <p className="mt-1.5 text-[10px] uppercase tracking-[0.12em] text-subtle">
              Aggregated speed samples · last 24 hours
            </p>
          </div>

          <div className="border-t border-border bg-bg/30 p-5 sm:p-6 lg:border-l lg:border-t-0">
            <p className="text-[10px] font-semibold uppercase tracking-[0.14em] text-subtle">
              Storage footprint
            </p>
            <p className="mt-2 text-2xl font-semibold tracking-tight">
              {formatBytes(stats?.total_downloaded_bytes ?? 0)}
            </p>
            <p className="mt-1 text-xs text-muted">
              across {stats?.completed ?? 0} completed download{(stats?.completed ?? 0) === 1 ? "" : "s"}
            </p>
            <div className="mt-5 space-y-3 text-xs">
              <div className="flex items-center justify-between gap-3">
                <span className="flex items-center gap-2 text-muted">
                  <TrendingUp className="h-3.5 w-3.5 text-success" /> Success rate
                </span>
                <span className="font-medium">{(stats?.success_rate ?? 0).toFixed(1)}%</span>
              </div>
              <div className="flex items-center justify-between gap-3">
                <span className="flex items-center gap-2 text-muted">
                  <Clock3 className="h-3.5 w-3.5 text-info" /> Average duration
                </span>
                <span className="font-medium">{formatDuration(stats?.average_duration_secs)}</span>
              </div>
              <div className="flex items-center justify-between gap-3">
                <span className="flex items-center gap-2 text-muted">
                  <Zap className="h-3.5 w-3.5 text-warning" /> Average speed
                </span>
                <span className="font-medium">{formatSpeed(stats?.average_speed_bps)}</span>
              </div>
              <div className="flex items-center justify-between gap-3">
                <span className="flex items-center gap-2 text-muted">
                  <CheckCircle2 className="h-3.5 w-3.5 text-success" /> Completed today
                </span>
                <span className="font-medium">{stats?.completed_today ?? 0}</span>
              </div>
            </div>
          </div>
        </div>
      </section>

      <div className="mt-4 grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
        <StatTile
          label="Active"
          value={String(stats?.active ?? 0)}
          hint="Transfers using the engine"
          icon={DownloadCloud}
          tone="accent"
        />
        <StatTile
          label="Queued"
          value={String(stats?.queued ?? 0)}
          hint="Waiting for a slot"
          icon={Layers}
          tone="info"
        />
        <StatTile
          label="Completed"
          value={String(stats?.completed ?? 0)}
          hint={`${stats?.failed ?? 0} failed · ${stats?.cancelled ?? 0} cancelled`}
          icon={CheckCircle2}
          tone="success"
        />
        <StatTile
          label="Total bytes"
          value={formatBytes(stats?.total_downloaded_bytes ?? 0)}
          hint="All completed files"
          icon={HardDrive}
          tone="warning"
        />
      </div>

      <div className="mt-6 grid gap-5 xl:grid-cols-[1.6fr_1fr]">
        <section>
          <div className="mb-3 flex items-center justify-between">
            <h2 className="text-sm font-semibold">In progress</h2>
            <Link to="/downloads" className="text-xs font-medium text-accent hover:underline">
              All downloads
            </Link>
          </div>
          {active.length === 0 && pending.length === 0 ? (
            <EmptyState
              icon={Gauge}
              title="Nothing downloading"
              description="Add a URL and NexDownload will inspect the source, plan byte ranges and start the transfer."
              action={
                <Button variant="primary" size="sm" onClick={() => setAddOpen(true)}>
                  <Plus className="h-3.5 w-3.5" /> Add a download
                </Button>
              }
            />
          ) : (
            <div className="space-y-2">
              {[...active, ...pending].slice(0, 4).map((task) => (
                <DownloadRow key={task.id} task={task} compact />
              ))}
            </div>
          )}

          <div className="mb-3 mt-6 flex items-center justify-between">
            <h2 className="text-sm font-semibold">Recently completed</h2>
            <Link to="/history" className="text-xs font-medium text-accent hover:underline">
              History
            </Link>
          </div>
          {recent.length === 0 ? (
            <p className="rounded-xl border border-dashed border-border px-4 py-6 text-center text-xs text-muted">
              Completed downloads will appear here.
            </p>
          ) : (
            <div className="card divide-y divide-border/60">
              {recent.map((task) => (
                <div key={task.id} className="flex items-center gap-3 px-4 py-2.5">
                  <CheckCircle2 className="h-4 w-4 shrink-0 text-success" />
                  <div className="min-w-0 flex-1">
                    <p className="truncate text-xs font-medium">{task.file_name}</p>
                    <p className="text-[10px] text-subtle">
                      {formatBytes(task.total_bytes)} · {relativeTime(task.completed_at)}
                    </p>
                  </div>
                  <Link
                    to="/history"
                    className="shrink-0 text-[10px] font-medium text-accent hover:underline"
                  >
                    details
                  </Link>
                </div>
              ))}
            </div>
          )}
        </section>

        <aside className="space-y-5">
          <section className="card p-4">
            <div className="mb-3 flex items-center justify-between">
              <h2 className="text-sm font-semibold">Category mix</h2>
              <Link to="/categories" className="text-[11px] text-accent hover:underline">
                configure
              </Link>
            </div>
            <DistributionBar
              items={(analytics?.categories ?? [])
                .slice()
                .sort((a, b) => b.bytes - a.bytes)
                .slice(0, 5)
                .map((item) => ({ label: item.category, value: item.count }))}
            />
          </section>

          <section className="card p-4">
            <div className="mb-3 flex items-center justify-between">
              <h2 className="text-sm font-semibold">Queues</h2>
              <Link to="/queues" className="text-[11px] text-accent hover:underline">
                manage
              </Link>
            </div>
            <div className="space-y-2">
              {queues.length === 0 ? (
                <p className="text-xs text-muted">No queues configured.</p>
              ) : (
                queues.map((queue) => {
                  const stat = analytics?.queues.find((item) => item.queue_id === queue.id);
                  const paused = queue.status === "paused";
                  return (
                    <div
                      key={queue.id}
                      className="flex items-center gap-2 rounded-lg border border-border/60 px-2.5 py-2"
                    >
                      <span
                        className={`h-1.5 w-1.5 shrink-0 rounded-full ${paused ? "bg-warning" : "bg-success"}`}
                      />
                      <div className="min-w-0 flex-1">
                        <p className="truncate text-xs font-medium">{queue.name}</p>
                        <p className="text-[10px] text-subtle">
                          {stat?.active ?? 0} active · {stat?.queued ?? 0} queued · limit{" "}
                          {queue.max_concurrent}
                        </p>
                      </div>
                      <button
                        type="button"
                        className="text-[10px] font-medium text-accent hover:underline"
                        onClick={() => void (paused ? startQueue(queue.id) : pauseQueue(queue.id))}
                      >
                        {paused ? "start" : "pause"}
                      </button>
                    </div>
                  );
                })
              )}
            </div>
          </section>

          <section className="card p-4">
            <h2 className="mb-2 text-sm font-semibold">Daily completions</h2>
            <div className="space-y-1.5">
              {(analytics?.daily ?? []).slice(-7).map((day) => {
                const max = Math.max(...(analytics?.daily ?? []).map((item) => item.completed), 1);
                return (
                  <div key={day.date} className="flex items-center gap-2 text-[11px]">
                    <span className="w-14 shrink-0 text-subtle">{day.date.slice(5)}</span>
                    <div className="h-1.5 flex-1 overflow-hidden rounded-full bg-raised">
                      <div
                        className="h-full rounded-full bg-info"
                        style={{ width: `${(day.completed / max) * 100}%` }}
                      />
                    </div>
                    <span className="w-8 text-right tabular-nums text-muted">{day.completed}</span>
                  </div>
                );
              })}
              {(analytics?.daily ?? []).length === 0 ? (
                <p className="text-xs text-muted">No completions recorded yet.</p>
              ) : null}
            </div>
          </section>
        </aside>
      </div>
    </div>
  );
}
