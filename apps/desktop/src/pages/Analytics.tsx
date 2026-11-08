import {
  BarChart3,
  CheckCircle2,
  Clock3,
  DownloadCloud,
  Gauge,
  HardDrive,
  Layers,
  Zap,
} from "lucide-react";
import PageHeader from "../components/PageHeader";
import StatTile from "../components/StatTile";
import { BarChart, Sparkline } from "../components/Charts";
import { Badge } from "../components/ui/badge";
import { formatBytes, formatDuration, formatSpeed } from "../lib/format";
import { useApp } from "../store/app";

export default function Analytics() {
  const analytics = useApp((state) => state.analytics);
  const stats = analytics?.stats;
  const speedValues = analytics?.speed_history.map((sample) => sample.speed_bps) ?? [];
  const daily = (analytics?.daily ?? []).slice(-14);
  const categories = [...(analytics?.categories ?? [])].sort((a, b) => b.bytes - a.bytes);
  const maxCategoryBytes = Math.max(...categories.map((item) => item.bytes), 1);
  const queues = analytics?.queues ?? [];

  return (
    <div>
      <PageHeader
        title="Analytics"
        description="Persisted speed samples, completion data and category usage. Refreshed every 15 seconds."
      />

      <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
        <StatTile
          label="Current speed"
          value={formatSpeed(stats?.current_speed_bps)}
          hint={`Peak ${formatSpeed(stats?.peak_speed_bps)}`}
          icon={Gauge}
          tone="accent"
        />
        <StatTile
          label="Average speed"
          value={formatSpeed(stats?.average_speed_bps)}
          hint={`Average duration ${formatDuration(stats?.average_duration_secs)}`}
          icon={Zap}
          tone="info"
        />
        <StatTile
          label="Success rate"
          value={`${(stats?.success_rate ?? 0).toFixed(1)}%`}
          hint={`${stats?.completed ?? 0} completed · ${stats?.failed ?? 0} failed`}
          icon={CheckCircle2}
          tone="success"
        />
        <StatTile
          label="Transferred"
          value={formatBytes(stats?.total_downloaded_bytes ?? 0)}
          hint={`${stats?.total_tasks ?? 0} total tasks`}
          icon={HardDrive}
          tone="warning"
        />
      </div>

      <div className="mt-5 grid gap-5 xl:grid-cols-[1.5fr_1fr]">
        <section className="card p-5">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <div>
              <h2 className="text-sm font-semibold">Speed history</h2>
              <p className="text-[11px] text-muted">Aggregated engine samples retained for 30 days.</p>
            </div>
            <Badge tone="accent">{speedValues.length} samples</Badge>
          </div>
          <div className="mt-4">
            <Sparkline values={speedValues} height={180} />
          </div>
          <div className="mt-3 flex flex-wrap items-center gap-4 border-t border-border/60 pt-3 text-[11px] text-muted">
            <span className="flex items-center gap-1.5">
              <span className="h-2 w-2 rounded-full bg-accent" /> Download speed
            </span>
            <span>Peak {formatSpeed(stats?.peak_speed_bps)}</span>
            <span>Average {formatSpeed(stats?.average_speed_bps)}</span>
          </div>
        </section>

        <section className="card p-5">
          <h2 className="text-sm font-semibold">Task mix</h2>
          <p className="text-[11px] text-muted">Current state distribution.</p>
          <div className="mt-4 space-y-3">
            {[
              { label: "Active", value: stats?.active ?? 0, tone: "bg-accent" },
              { label: "Queued", value: stats?.queued ?? 0, tone: "bg-info" },
              { label: "Paused", value: stats?.paused ?? 0, tone: "bg-warning" },
              { label: "Completed", value: stats?.completed ?? 0, tone: "bg-success" },
              { label: "Failed", value: stats?.failed ?? 0, tone: "bg-danger" },
              { label: "Cancelled", value: stats?.cancelled ?? 0, tone: "bg-muted" },
            ].map((item) => {
              const total = Math.max(stats?.total_tasks ?? 0, 1);
              return (
                <div key={item.label}>
                  <div className="mb-1 flex items-center justify-between text-[11px]">
                    <span className="text-muted">{item.label}</span>
                    <span className="font-medium">{item.value}</span>
                  </div>
                  <div className="h-1.5 overflow-hidden rounded-full bg-raised">
                    <div
                      className={`h-full rounded-full ${item.tone}`}
                      style={{ width: `${(item.value / total) * 100}%` }}
                    />
                  </div>
                </div>
              );
            })}
          </div>
        </section>
      </div>

      <div className="mt-5 grid gap-5 xl:grid-cols-2">
        <section className="card p-5">
          <div className="flex items-center justify-between gap-2">
            <div>
              <h2 className="text-sm font-semibold">Completions by day</h2>
              <p className="text-[11px] text-muted">Last 14 days of completed tasks.</p>
            </div>
            <BarChart3 className="h-4 w-4 text-subtle" />
          </div>
          <div className="mt-4">
            <BarChart
              data={daily.map((day) => ({
                label: day.date.slice(5),
                value: day.completed,
                hint: `${day.date}: ${day.completed} completed · ${formatBytes(day.bytes)}`,
              }))}
            />
          </div>
        </section>

        <section className="card p-5">
          <div className="flex items-center justify-between gap-2">
            <div>
              <h2 className="text-sm font-semibold">Bytes by category</h2>
              <p className="text-[11px] text-muted">Based on completed files with known sizes.</p>
            </div>
            <Layers className="h-4 w-4 text-subtle" />
          </div>
          <div className="mt-4 space-y-2.5">
            {categories.length === 0 ? (
              <p className="text-xs text-muted">No completed downloads yet.</p>
            ) : (
              categories.slice(0, 8).map((item) => (
                <div key={item.category}>
                  <div className="mb-1 flex items-center justify-between gap-2 text-[11px]">
                    <span className="truncate text-muted">{item.category}</span>
                    <span className="shrink-0 text-subtle">
                      {item.count} · {formatBytes(item.bytes)}
                    </span>
                  </div>
                  <div className="h-1.5 overflow-hidden rounded-full bg-raised">
                    <div
                      className="h-full rounded-full bg-info"
                      style={{ width: `${(item.bytes / maxCategoryBytes) * 100}%` }}
                    />
                  </div>
                </div>
              ))
            )}
          </div>
        </section>
      </div>

      <section className="card mt-5">
        <div className="flex flex-wrap items-center justify-between gap-2 border-b border-border px-5 py-4">
          <div>
            <h2 className="text-sm font-semibold">Queue performance</h2>
            <p className="text-[11px] text-muted">Current queue occupancy; scheduling limits live in Queues.</p>
          </div>
          <div className="flex items-center gap-1.5 text-[11px] text-muted">
            <DownloadCloud className="h-3.5 w-3.5 text-accent" /> {stats?.active ?? 0} active transfers
          </div>
        </div>
        <div className="overflow-x-auto">
          <table className="w-full text-left text-xs">
            <thead className="text-[10px] uppercase tracking-[0.08em] text-subtle">
              <tr className="border-b border-border/60">
                <th className="px-5 py-2 font-medium">Queue</th>
                <th className="px-3 py-2 text-right font-medium">Active</th>
                <th className="px-3 py-2 text-right font-medium">Queued</th>
                <th className="px-5 py-2 text-right font-medium">Total</th>
              </tr>
            </thead>
            <tbody>
              {queues.length === 0 ? (
                <tr>
                  <td className="px-5 py-6 text-center text-muted" colSpan={4}>
                    No queue data available.
                  </td>
                </tr>
              ) : (
                queues.map((queue) => (
                  <tr key={queue.queue_id} className="border-b border-border/40 last:border-0">
                    <td className="px-5 py-2.5 font-medium">{queue.name}</td>
                    <td className="px-3 py-2.5 text-right tabular-nums">{queue.active}</td>
                    <td className="px-3 py-2.5 text-right tabular-nums">{queue.queued}</td>
                    <td className="px-5 py-2.5 text-right tabular-nums">{queue.total}</td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>
      </section>

      <div className="mt-5 flex flex-wrap items-center gap-4 rounded-xl border border-border/60 bg-surface/40 px-4 py-3 text-[11px] text-muted">
        <span className="flex items-center gap-1.5">
          <Clock3 className="h-3.5 w-3.5" /> Samples older than 30 days are pruned by the server.
        </span>
        <span className="flex items-center gap-1.5">
          <Gauge className="h-3.5 w-3.5" /> Speed is sampled from the segmented engine, not estimated.
        </span>
      </div>
    </div>
  );
}
