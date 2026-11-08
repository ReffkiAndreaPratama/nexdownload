import { useMemo, useState } from "react";
import { Copy, Download, FileClock, Search } from "lucide-react";
import DownloadRow from "../components/DownloadRow";
import EmptyState from "../components/EmptyState";
import PageHeader from "../components/PageHeader";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Dialog } from "../components/ui/dialog";
import { Input } from "../components/ui/input";
import { api } from "../lib/api";
import { formatBytes, formatDate } from "../lib/format";
import type { DuplicateGroup } from "../lib/types";
import { useApp } from "../store/app";

export default function History() {
  const tasks = useApp((state) => state.tasks);
  const pushToast = useApp((state) => state.pushToast);
  const [search, setSearch] = useState("");
  const [duplicates, setDuplicates] = useState<DuplicateGroup[] | null>(null);
  const [duplicateOpen, setDuplicateOpen] = useState(false);
  const [loadingDuplicates, setLoadingDuplicates] = useState(false);
  const [exporting, setExporting] = useState(false);

  const terminal = useMemo(
    () =>
      tasks
        .filter((task) => ["completed", "failed", "cancelled"].includes(task.status))
        .filter((task) =>
          search.trim()
            ? `${task.file_name} ${task.url} ${task.category}`
                .toLowerCase()
                .includes(search.trim().toLowerCase())
            : true,
        )
        .sort((a, b) =>
          (b.completed_at ?? b.updated_at).localeCompare(a.completed_at ?? a.updated_at),
        ),
    [tasks, search],
  );

  const completed = terminal.filter((task) => task.status === "completed");
  const failed = terminal.filter((task) => task.status === "failed");
  const totalBytes = completed.reduce((sum, task) => sum + (task.total_bytes ?? 0), 0);

  const exportCsv = async () => {
    setExporting(true);
    try {
      const blob = await api.exportHistory();
      const url = URL.createObjectURL(blob);
      const anchor = document.createElement("a");
      anchor.href = url;
      anchor.download = `nexdownload-history-${new Date().toISOString().slice(0, 10)}.csv`;
      document.body.append(anchor);
      anchor.click();
      anchor.remove();
      URL.revokeObjectURL(url);
      pushToast("success", "History exported", "CSV download started.");
    } catch (error) {
      pushToast("error", "Export failed", error instanceof Error ? error.message : String(error));
    } finally {
      setExporting(false);
    }
  };

  const loadDuplicates = async () => {
    setLoadingDuplicates(true);
    setDuplicateOpen(true);
    try {
      setDuplicates(await api.duplicates());
    } catch (error) {
      pushToast("error", "Could not load duplicates", error instanceof Error ? error.message : String(error));
      setDuplicateOpen(false);
    } finally {
      setLoadingDuplicates(false);
    }
  };

  return (
    <div>
      <PageHeader
        title="History"
        description="Completed, failed and cancelled transfers with integrity and duplicate checks."
        actions={
          <>
            <Button size="sm" onClick={() => void loadDuplicates()}>
              <Copy className="h-3.5 w-3.5" /> Duplicates
            </Button>
            <Button size="sm" variant="primary" onClick={() => void exportCsv()} disabled={exporting}>
              <Download className="h-3.5 w-3.5" /> Export CSV
            </Button>
          </>
        }
      />

      <div className="mb-4 grid gap-3 sm:grid-cols-3">
        <div className="card p-4">
          <p className="text-[10px] uppercase tracking-[0.12em] text-subtle">Completed</p>
          <p className="mt-1 text-xl font-semibold">{completed.length}</p>
          <p className="mt-0.5 text-[11px] text-muted">{formatBytes(totalBytes)} stored</p>
        </div>
        <div className="card p-4">
          <p className="text-[10px] uppercase tracking-[0.12em] text-subtle">Failed</p>
          <p className="mt-1 text-xl font-semibold">{failed.length}</p>
          <p className="mt-0.5 text-[11px] text-muted">Diagnostics are available per task</p>
        </div>
        <div className="card p-4">
          <p className="text-[10px] uppercase tracking-[0.12em] text-subtle">Terminal tasks</p>
          <p className="mt-1 text-xl font-semibold">{terminal.length}</p>
          <p className="mt-0.5 text-[11px] text-muted">Retained until you remove them</p>
        </div>
      </div>

      <div className="relative mb-4">
        <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-subtle" />
        <Input
          value={search}
          onChange={(event) => setSearch(event.target.value)}
          className="pl-9"
          placeholder="Search history…"
          aria-label="Search history"
        />
      </div>

      {terminal.length === 0 ? (
        <EmptyState
          icon={FileClock}
          title={tasks.length === 0 ? "No history yet" : "No matching history"}
          description={
            tasks.length === 0
              ? "Completed and failed downloads will collect here with checksum records and diagnostics."
              : "Try a different search term."
          }
        />
      ) : (
        <div className="space-y-2">
          {terminal.map((task) => (
            <DownloadRow key={task.id} task={task} />
          ))}
        </div>
      )}

      <Dialog
        open={duplicateOpen}
        onClose={() => setDuplicateOpen(false)}
        title="Duplicate candidates"
        description="Groups of completed tasks that share a SHA-256 digest."
        width="max-w-2xl"
      >
        {loadingDuplicates ? (
          <p className="py-8 text-center text-xs text-muted">Comparing stored checksums…</p>
        ) : !duplicates || duplicates.length === 0 ? (
          <div className="py-8 text-center">
            <Copy className="mx-auto h-8 w-8 text-subtle" />
            <p className="mt-2 text-xs text-muted">
              No duplicate groups found. Only completed files with a recorded checksum are compared.
            </p>
          </div>
        ) : (
          <div className="space-y-3">
            {duplicates.map((group) => (
              <div key={group.checksum} className="rounded-lg border border-border/70 p-3">
                <div className="flex items-center justify-between gap-2">
                  <Badge tone="warning">{group.tasks.length} copies</Badge>
                  <span className="truncate font-mono text-[10px] text-subtle" title={group.checksum}>
                    {group.checksum}
                  </span>
                </div>
                <div className="mt-2 space-y-1.5">
                  {group.tasks.map((task) => (
                    <div key={task.id} className="flex items-center justify-between gap-2 text-xs">
                      <span className="min-w-0 truncate">{task.file_name}</span>
                      <span className="shrink-0 text-[10px] text-subtle">
                        {formatBytes(task.total_bytes)} · {formatDate(task.completed_at)}
                      </span>
                    </div>
                  ))}
                </div>
              </div>
            ))}
          </div>
        )}
      </Dialog>
    </div>
  );
}
