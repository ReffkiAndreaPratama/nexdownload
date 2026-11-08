import { useEffect, useMemo, useState } from "react";
import {
  Activity,
  AlertTriangle,
  Ban,
  Clipboard,
  FileWarning,
  FolderOpen,
  Pause,
  Play,
  RotateCcw,
  ShieldCheck,
  Trash2,
} from "lucide-react";
import { api, isTauri, openPath, revealPath } from "../lib/api";
import {
  domainOf,
  formatBytes,
  formatDate,
  formatSpeed,
  percent,
  statusLabel,
  truncateMiddle,
} from "../lib/format";
import type {
  Diagnostics,
  DownloadTask,
  IntegrityRecord,
  TaskEvent,
} from "../lib/types";
import { cn } from "../lib/utils";
import { useApp } from "../store/app";
import { Badge } from "./ui/badge";
import { Button } from "./ui/button";
import { Dialog } from "./ui/dialog";
import { Input, Select } from "./ui/input";
import { ProgressBar } from "./ui/progress";

type Tab = "overview" | "events" | "integrity" | "diagnostics";

const tabs: { id: Tab; label: string }[] = [
  { id: "overview", label: "Overview" },
  { id: "events", label: "History" },
  { id: "integrity", label: "Integrity" },
  { id: "diagnostics", label: "Diagnostics" },
];

function InfoRow({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="grid grid-cols-[130px_1fr] gap-3 py-1.5 text-xs">
      <span className="text-subtle">{label}</span>
      <span className="min-w-0 break-words text-fg">{children}</span>
    </div>
  );
}

export default function TaskDetail() {
  const detailId = useApp((state) => state.detailId);
  const setDetailId = useApp((state) => state.setDetailId);
  const storeTask = useApp((state) => state.tasks.find((task) => task.id === state.detailId));
  const categories = useApp((state) => state.categories);
  const queues = useApp((state) => state.queues);
  const collections = useApp((state) => state.collections);
  const pauseTask = useApp((state) => state.pauseTask);
  const resumeTask = useApp((state) => state.resumeTask);
  const retryTask = useApp((state) => state.retryTask);
  const cancelTask = useApp((state) => state.cancelTask);
  const removeTask = useApp((state) => state.removeTask);
  const verifyTask = useApp((state) => state.verifyTask);
  const refreshAll = useApp((state) => state.refreshAll);
  const pushToast = useApp((state) => state.pushToast);

  const [task, setTask] = useState<DownloadTask | null>(null);
  const [tab, setTab] = useState<Tab>("overview");
  const [events, setEvents] = useState<TaskEvent[]>([]);
  const [integrity, setIntegrity] = useState<IntegrityRecord[]>([]);
  const [diagnostics, setDiagnostics] = useState<Diagnostics | null>(null);
  const [loadingDiagnostics, setLoadingDiagnostics] = useState(false);
  const [organize, setOrganize] = useState({
    category: "",
    queue_id: "",
    collection_id: "",
    priority: "0",
    connections: "4",
  });
  const [savingOrganize, setSavingOrganize] = useState(false);

  useEffect(() => {
    if (!detailId) {
      setTask(null);
      return;
    }
    setTask(storeTask ?? null);
    setTab("overview");
    setEvents([]);
    setIntegrity([]);
    setDiagnostics(null);
    let cancelled = false;
    void api
      .task(detailId)
      .then((fresh) => {
        if (!cancelled) setTask(fresh);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
    // storeTask intentionally not a dependency: it changes on every poll.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [detailId]);

  useEffect(() => {
    if (!task) return;
    setOrganize({
      category: task.category,
      queue_id: task.queue_id ?? "",
      collection_id: task.collection_id ?? "",
      priority: String(task.priority),
      connections: String(task.connections),
    });
  }, [task?.id, task?.category, task?.queue_id, task?.collection_id, task?.priority, task?.connections]);

  useEffect(() => {
    if (!detailId) return;
    if (tab === "events" && events.length === 0) {
      void api.taskEvents(detailId).then((rows) => setEvents(rows as TaskEvent[])).catch(() => undefined);
    }
    if (tab === "integrity" && integrity.length === 0) {
      void api
        .taskIntegrity(detailId)
        .then((rows) => setIntegrity(rows as IntegrityRecord[]))
        .catch(() => undefined);
    }
    if (tab === "diagnostics" && !diagnostics) {
      void loadDiagnostics();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tab, detailId]);

  const open = detailId !== null;
  const progress = task ? percent(task) : 0;
  const active = task ? ["downloading", "connecting", "preparing"].includes(task.status) : false;
  const pending = task ? ["queued", "pausing", "waiting_for_network"].includes(task.status) : false;
  const failed = task ? ["failed", "cancelled"].includes(task.status) : false;

  const segmentTotal = useMemo(
    () => task?.segments.reduce((sum, segment) => sum + (segment.end - segment.start + 1), 0) ?? 0,
    [task?.segments],
  );

  async function loadDiagnostics() {
    if (!detailId) return;
    setLoadingDiagnostics(true);
    try {
      const report = await api.diagnostics(detailId);
      setDiagnostics(report);
      setEvents(report.events);
      setIntegrity(report.integrity);
    } catch (error) {
      pushToast("error", "Could not load diagnostics", error instanceof Error ? error.message : String(error));
    } finally {
      setLoadingDiagnostics(false);
    }
  }

  async function saveOrganize() {
    if (!task) return;
    setSavingOrganize(true);
    try {
      await api.patchTask(task.id, {
        category: organize.category,
        queue_id: organize.queue_id || undefined,
        collection_id: organize.collection_id || undefined,
        priority: Number(organize.priority) || 0,
        connections: Math.max(1, Math.min(32, Number(organize.connections) || 1)),
      });
      await refreshAll();
      const fresh = await api.task(task.id);
      setTask(fresh);
      pushToast("success", "Download updated");
    } catch (error) {
      pushToast("error", "Could not update download", error instanceof Error ? error.message : String(error));
    } finally {
      setSavingOrganize(false);
    }
  }

  async function copy(value: string, label: string) {
    try {
      await navigator.clipboard.writeText(value);
      pushToast("success", `${label} copied`);
    } catch {
      pushToast("error", "Clipboard unavailable");
    }
  }

  return (
    <Dialog
      open={open}
      onClose={() => setDetailId(null)}
      width="max-w-4xl"
      title={task ? task.file_name : "Download details"}
      description={task ? `${domainOf(task.url)} · ${statusLabel(task.status)}` : undefined}
      footer={
        task ? (
          <>
            <Button variant="ghost" onClick={() => setDetailId(null)}>
              Close
            </Button>
            {active || pending ? (
              <Button onClick={() => void pauseTask(task.id)}>
                <Pause className="h-4 w-4" /> Pause
              </Button>
            ) : null}
            {task.status === "paused" ? (
              <Button onClick={() => void resumeTask(task.id)}>
                <Play className="h-4 w-4" /> Resume
              </Button>
            ) : null}
            {failed ? (
              <Button onClick={() => void retryTask(task.id, task.status === "cancelled")}>
                <RotateCcw className="h-4 w-4" /> {task.status === "failed" ? "Retry" : "Restart"}
              </Button>
            ) : null}
            {task.status === "completed" ? (
              <Button variant="primary" onClick={() => void verifyTask(task.id)}>
                <ShieldCheck className="h-4 w-4" /> Verify
              </Button>
            ) : null}
            {active || pending || task.status === "paused" ? (
              <Button variant="danger" onClick={() => void cancelTask(task.id)}>
                <Ban className="h-4 w-4" /> Cancel
              </Button>
            ) : null}
          </>
        ) : null
      }
    >
      {!task ? (
        <p className="py-8 text-center text-sm text-muted">Loading task…</p>
      ) : (
        <div className="space-y-5">
          <div className="rounded-xl border border-border/70 bg-bg/40 p-4">
            <div className="flex flex-wrap items-center justify-between gap-3">
              <div className="flex flex-wrap items-center gap-2">
                <Badge tone={task.status === "completed" ? "success" : task.status === "failed" ? "danger" : "accent"}>
                  {statusLabel(task.status)}
                </Badge>
                <span className="text-xs text-muted">
                  {formatBytes(task.downloaded_bytes)} of {formatBytes(task.total_bytes)}
                </span>
                {task.total_bytes ? (
                  <span className="text-xs font-medium text-accent">{progress.toFixed(2)}%</span>
                ) : null}
              </div>
              <div className="flex items-center gap-2">
                {isTauri && task.status === "completed" ? (
                  <>
                    <Button size="sm" onClick={() => void openPath(task.file_path).catch((error) => pushToast("error", "Could not open file", String(error)))}>
                      <FolderOpen className="h-3.5 w-3.5" /> Open
                    </Button>
                    <Button size="sm" onClick={() => void revealPath(task.file_path).catch((error) => pushToast("error", "Could not reveal file", String(error)))}>
                      Show in folder
                    </Button>
                  </>
                ) : null}
                <Button
                  size="sm"
                  variant="danger"
                  onClick={() => {
                    const deleteFile = window.confirm(
                      `Remove "${task.file_name}" from the list?\n\nClick OK to also delete the downloaded file.`,
                    );
                    void removeTask(task.id, deleteFile);
                    setDetailId(null);
                  }}
                >
                  <Trash2 className="h-3.5 w-3.5" /> Remove
                </Button>
              </div>
            </div>
            <div className="mt-3">
              <ProgressBar value={progress} segments={task.segments} indeterminate={pending} />
            </div>
            <div className="mt-2 flex flex-wrap justify-between gap-2 text-[11px] text-muted">
              <span>{formatSpeed(task.speed_bps)} · {task.connections} connection{task.connections === 1 ? "" : "s"}</span>
              <span>{task.error ? task.error : `Updated ${formatDate(task.updated_at)}`}</span>
            </div>
          </div>

          <div className="flex flex-wrap gap-1 border-b border-border pb-2">
            {tabs.map((item) => (
              <button
                key={item.id}
                type="button"
                onClick={() => setTab(item.id)}
                className={cn(
                  "rounded-md px-3 py-1.5 text-xs font-medium transition",
                  tab === item.id ? "bg-accent/10 text-accent" : "text-muted hover:bg-raised hover:text-fg",
                )}
              >
                {item.label}
              </button>
            ))}
          </div>

          {tab === "overview" ? (
            <div className="grid gap-5 lg:grid-cols-2">
              <div>
                <h3 className="mb-2 text-xs font-semibold uppercase tracking-[0.08em] text-subtle">Resource</h3>
                <div className="rounded-lg border border-border/60 px-3 py-1">
                  <InfoRow label="Source">
                    <a className="break-all text-accent hover:underline" href={task.url} target="_blank" rel="noreferrer">
                      {truncateMiddle(task.url, 110)}
                    </a>
                  </InfoRow>
                  {task.final_url && task.final_url !== task.url ? (
                    <InfoRow label="Final URL">
                      <span className="break-all">{truncateMiddle(task.final_url, 110)}</span>
                    </InfoRow>
                  ) : null}
                  <InfoRow label="Category">{task.category}</InfoRow>
                  <InfoRow label="Queue">{queues.find((queue) => queue.id === task.queue_id)?.name ?? task.queue_id ?? "—"}</InfoRow>
                  <InfoRow label="Collection">
                    {collections.find((collection) => collection.id === task.collection_id)?.name ?? task.collection_id ?? "—"}
                  </InfoRow>
                  <InfoRow label="Content type">{task.content_type ?? "—"}</InfoRow>
                  <InfoRow label="Ranges">{task.accepts_ranges ? "Supported" : "Not supported"}</InfoRow>
                  <InfoRow label="ETag">{task.etag ?? "—"}</InfoRow>
                  <InfoRow label="Last modified">{task.last_modified ?? "—"}</InfoRow>
                </div>
              </div>
              <div>
                <h3 className="mb-2 text-xs font-semibold uppercase tracking-[0.08em] text-subtle">Storage</h3>
                <div className="rounded-lg border border-border/60 px-3 py-1">
                  <InfoRow label="Destination">
                    <span className="break-all font-mono text-[11px]">{task.destination_dir}</span>
                  </InfoRow>
                  <InfoRow label="File path">
                    <span className="break-all font-mono text-[11px]">{task.file_path}</span>
                  </InfoRow>
                  <InfoRow label="Created">{formatDate(task.created_at)}</InfoRow>
                  <InfoRow label="Started">{formatDate(task.started_at)}</InfoRow>
                  <InfoRow label="Completed">{formatDate(task.completed_at)}</InfoRow>
                  <InfoRow label="SHA-256">
                    {task.checksum_sha256 ? (
                      <button type="button" className="inline-flex items-center gap-1.5 break-all text-left font-mono text-[11px] text-accent" onClick={() => void copy(task.checksum_sha256 ?? "", "Checksum")}>
                        {task.checksum_sha256} <Clipboard className="h-3 w-3 shrink-0" />
                      </button>
                    ) : (
                      "Not verified yet"
                    )}
                  </InfoRow>
                  <InfoRow label="Expected">
                    {task.expected_checksum ? (
                      <span className="break-all font-mono text-[11px]">{task.expected_checksum}</span>
                    ) : (
                      "—"
                    )}
                  </InfoRow>
                </div>
              </div>

              <div className="lg:col-span-2">
                <h3 className="mb-2 text-xs font-semibold uppercase tracking-[0.08em] text-subtle">
                  Segments ({task.segments.length})
                </h3>
                {task.segments.length === 0 ? (
                  <p className="rounded-lg border border-dashed border-border px-3 py-4 text-xs text-muted">
                    This transfer has not produced a segment plan yet.
                  </p>
                ) : (
                  <div className="max-h-44 space-y-1.5 overflow-y-auto pr-1 scroll-thin">
                    {task.segments.map((segment) => {
                      const length = segment.end - segment.start + 1;
                      const filled = length > 0 ? Math.min(100, (segment.downloaded / length) * 100) : 0;
                      return (
                        <div key={segment.index} className="flex items-center gap-3 text-[11px]">
                          <span className="w-6 text-right text-subtle">#{segment.index + 1}</span>
                          <span className="w-48 shrink-0 font-mono text-muted">
                            {formatBytes(segment.start)}–{formatBytes(segment.end)}
                          </span>
                          <div className="h-1.5 flex-1 overflow-hidden rounded-full bg-raised">
                            <div className="h-full rounded-full bg-accent" style={{ width: `${filled}%` }} />
                          </div>
                          <span className="w-14 text-right tabular-nums text-muted">{filled.toFixed(0)}%</span>
                        </div>
                      );
                    })}
                    <p className="pt-1 text-[10px] text-subtle">
                      {(segmentTotal / 1024 / 1024).toFixed(2)} MiB planned across {task.segments.length} range
                      {task.segments.length === 1 ? "" : "s"}.
                    </p>
                  </div>
                )}
              </div>

              <div className="lg:col-span-2">
                <h3 className="mb-2 text-xs font-semibold uppercase tracking-[0.08em] text-subtle">Organize</h3>
                <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-5">
                  <div>
                    <label className="label" htmlFor="detail-category">Category</label>
                    <Select id="detail-category" value={organize.category} onChange={(event) => setOrganize({ ...organize, category: event.target.value })}>
                      {categories.map((category) => (
                        <option key={category.id} value={category.id}>{category.name}</option>
                      ))}
                    </Select>
                  </div>
                  <div>
                    <label className="label" htmlFor="detail-queue">Queue</label>
                    <Select id="detail-queue" value={organize.queue_id} onChange={(event) => setOrganize({ ...organize, queue_id: event.target.value })}>
                      <option value="">Unassigned</option>
                      {queues.map((queue) => (
                        <option key={queue.id} value={queue.id}>{queue.name}</option>
                      ))}
                    </Select>
                  </div>
                  <div>
                    <label className="label" htmlFor="detail-collection">Collection</label>
                    <Select id="detail-collection" value={organize.collection_id} onChange={(event) => setOrganize({ ...organize, collection_id: event.target.value })}>
                      <option value="">Unassigned</option>
                      {collections.map((collection) => (
                        <option key={collection.id} value={collection.id}>{collection.name}</option>
                      ))}
                    </Select>
                  </div>
                  <div>
                    <label className="label" htmlFor="detail-priority">Priority</label>
                    <Input id="detail-priority" type="number" value={organize.priority} onChange={(event) => setOrganize({ ...organize, priority: event.target.value })} />
                  </div>
                  <div>
                    <label className="label" htmlFor="detail-connections">Connections</label>
                    <div className="flex gap-2">
                      <Input id="detail-connections" type="number" min={1} max={32} value={organize.connections} onChange={(event) => setOrganize({ ...organize, connections: event.target.value })} />
                      <Button size="sm" variant="primary" disabled={savingOrganize} onClick={() => void saveOrganize()}>
                        Save
                      </Button>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          ) : null}

          {tab === "events" ? (
            <div className="space-y-2">
              {events.length === 0 ? (
                <p className="rounded-lg border border-dashed border-border px-3 py-6 text-center text-xs text-muted">No state changes recorded.</p>
              ) : (
                events.map((event) => (
                  <div key={event.id} className="flex items-start gap-3 rounded-lg border border-border/60 px-3 py-2 text-xs">
                    <Activity className="mt-0.5 h-3.5 w-3.5 shrink-0 text-accent" />
                    <div className="min-w-0 flex-1">
                      <p>
                        {event.from_state ? `${event.from_state} → ` : ""}
                        <span className="font-medium text-fg">{event.to_state}</span>
                      </p>
                      {event.note ? <p className="mt-0.5 text-muted">{event.note}</p> : null}
                    </div>
                    <span className="shrink-0 text-[10px] text-subtle">{formatDate(event.created_at)}</span>
                  </div>
                ))
              )}
            </div>
          ) : null}

          {tab === "integrity" ? (
            <div className="space-y-3">
              <div className="flex items-center justify-between gap-3">
                <p className="text-xs text-muted">Integrity checks are stored for every completed transfer.</p>
                <Button size="sm" onClick={() => void verifyTask(task.id).then(() => {
                  if (tab === "integrity") {
                    void api.taskIntegrity(task.id).then((rows) => setIntegrity(rows as IntegrityRecord[]));
                  }
                })}>
                  <ShieldCheck className="h-3.5 w-3.5" /> Run check
                </Button>
              </div>
              {integrity.length === 0 ? (
                <p className="rounded-lg border border-dashed border-border px-3 py-6 text-center text-xs text-muted">No integrity records yet.</p>
              ) : (
                integrity.map((record) => (
                  <div key={record.id} className="rounded-lg border border-border/60 p-3 text-xs">
                    <div className="flex items-center justify-between gap-2">
                      <span className="font-medium uppercase">{record.algorithm}</span>
                      <Badge tone={record.status === "verified" ? "success" : record.status === "not_configured" ? "neutral" : "danger"}>
                        {record.status.replaceAll("_", " ")}
                      </Badge>
                    </div>
                    <p className="mt-2 break-all font-mono text-[10px] text-muted">actual: {record.actual ?? "—"}</p>
                    <p className="break-all font-mono text-[10px] text-muted">expected: {record.expected ?? "—"}</p>
                    <p className="mt-1 text-[10px] text-subtle">
                      {formatBytes(record.actual_size)} · {formatDate(record.checked_at)}
                    </p>
                  </div>
                ))
              )}
            </div>
          ) : null}

          {tab === "diagnostics" ? (
            <div className="space-y-3">
              <div className="flex items-center justify-between gap-3">
                <p className="text-xs text-muted">Re-inspect the source and collect local evidence.</p>
                <Button size="sm" onClick={() => void loadDiagnostics()} disabled={loadingDiagnostics}>
                  <Activity className={cn("h-3.5 w-3.5", loadingDiagnostics && "animate-pulse")} /> Refresh
                </Button>
              </div>
              {!diagnostics ? (
                <p className="rounded-lg border border-dashed border-border px-3 py-6 text-center text-xs text-muted">
                  {loadingDiagnostics ? "Inspecting source…" : "No diagnostic report loaded."}
                </p>
              ) : (
                <>
                  <div className="rounded-lg border border-border/60 p-3">
                    <div className="flex items-start gap-2">
                      {diagnostics.error_kind ? (
                        <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-warning" />
                      ) : (
                        <ShieldCheck className="mt-0.5 h-4 w-4 shrink-0 text-success" />
                      )}
                      <div>
                        <p className="text-sm font-medium">{diagnostics.summary}</p>
                        {diagnostics.error ? <p className="mt-1 break-words text-xs text-danger">{diagnostics.error}</p> : null}
                        <p className="mt-1 text-[11px] text-muted">
                          Source reachable: {diagnostics.source_reachable === null ? "not checked" : diagnostics.source_reachable ? "yes" : "no"}
                        </p>
                      </div>
                    </div>
                    {diagnostics.hints.length > 0 ? (
                      <ul className="mt-3 list-disc space-y-1 pl-5 text-xs text-muted">
                        {diagnostics.hints.map((hint) => <li key={hint}>{hint}</li>)}
                      </ul>
                    ) : null}
                    {diagnostics.suggested_actions.length > 0 ? (
                      <div className="mt-3 flex flex-wrap gap-1.5">
                        {diagnostics.suggested_actions.map((action) => (
                          <Badge key={action} tone="info">{action}</Badge>
                        ))}
                      </div>
                    ) : null}
                  </div>
                  {diagnostics.resource ? (
                    <div className="rounded-lg border border-border/60 p-3 text-xs">
                      <p className="mb-2 flex items-center gap-1.5 font-medium"><FileWarning className="h-3.5 w-3.5" /> Fresh resource view</p>
                      <InfoRow label="Final URL"><span className="break-all">{truncateMiddle(diagnostics.resource.final_url, 120)}</span></InfoRow>
                      <InfoRow label="Content type">{diagnostics.resource.content_type ?? "—"}</InfoRow>
                      <InfoRow label="Size">{formatBytes(diagnostics.resource.total_bytes)}</InfoRow>
                      <InfoRow label="Ranges">{diagnostics.resource.accepts_ranges ? "Supported" : "Not supported"}</InfoRow>
                      <InfoRow label="ETag">{diagnostics.resource.etag ?? "—"}</InfoRow>
                      <InfoRow label="Server">{diagnostics.resource.server ?? "—"}</InfoRow>
                    </div>
                  ) : null}
                </>
              )}
            </div>
          ) : null}
        </div>
      )}
    </Dialog>
  );
}
