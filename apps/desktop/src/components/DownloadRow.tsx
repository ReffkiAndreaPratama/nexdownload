import {
  Ban,
  CheckCircle2,
  FolderOpen,
  Pause,
  Play,
  RotateCcw,
  ShieldCheck,
  Trash2,
} from "lucide-react";
import { isTauri, revealPath, openPath } from "../lib/api";
import {
  domainOf,
  formatBytes,
  formatEta,
  formatSpeed,
  percent,
  relativeTime,
  statusLabel,
  statusTone,
  truncateMiddle,
} from "../lib/format";
import { cn } from "../lib/utils";
import { useApp } from "../store/app";
import type { DownloadTask } from "../lib/types";
import { Badge } from "./ui/badge";
import { ProgressBar } from "./ui/progress";

const activeStates = new Set(["preparing", "connecting", "downloading"]);
const pendingStates = new Set(["queued", "waiting_for_network", "pausing"]);
const failedStates = new Set(["failed", "cancelled"]);

function statusToneFor(status: string): "accent" | "success" | "warning" | "danger" | "info" | "neutral" {
  switch (status) {
    case "downloading":
      return "accent";
    case "completed":
      return "success";
    case "failed":
      return "danger";
    case "paused":
    case "pausing":
      return "warning";
    case "queued":
    case "preparing":
    case "connecting":
    case "verifying":
      return "info";
    default:
      return "neutral";
  }
}

export default function DownloadRow({
  task,
  compact = false,
}: {
  task: DownloadTask;
  compact?: boolean;
}) {
  const setDetailId = useApp((state) => state.setDetailId);
  const pauseTask = useApp((state) => state.pauseTask);
  const resumeTask = useApp((state) => state.resumeTask);
  const retryTask = useApp((state) => state.retryTask);
  const cancelTask = useApp((state) => state.cancelTask);
  const removeTask = useApp((state) => state.removeTask);
  const verifyTask = useApp((state) => state.verifyTask);
  const pushToast = useApp((state) => state.pushToast);

  const progress = percent(task);
  const remaining =
    task.speed_bps > 0 && task.total_bytes
      ? Math.max(0, (task.total_bytes - task.downloaded_bytes) / task.speed_bps)
      : null;
  const busy = activeStates.has(task.status) || pendingStates.has(task.status);

  const handleOpen = async () => {
    try {
      await openPath(task.file_path);
    } catch (error) {
      pushToast("error", "Could not open file", error instanceof Error ? error.message : String(error));
    }
  };

  const handleReveal = async () => {
    try {
      await revealPath(task.file_path);
    } catch (error) {
      pushToast("error", "Could not reveal file", error instanceof Error ? error.message : String(error));
    }
  };

  const handleRemove = () => {
    const deleteFile = window.confirm(
      `Remove "${task.file_name}" from the list?\n\nClick OK to also delete the downloaded file from disk. Click Cancel to keep the file.`,
    );
    void removeTask(task.id, deleteFile);
  };

  return (
    <div
      className={cn(
        "group rounded-xl border border-border/70 bg-surface/60 transition hover:border-border hover:bg-raised/40",
        compact ? "p-3" : "p-4",
      )}
    >
      <div className="flex flex-col gap-3 sm:flex-row sm:items-start">
        <button
          type="button"
          className="min-w-0 flex-1 text-left"
          onClick={() => setDetailId(task.id)}
          title={task.file_name}
        >
          <div className="flex flex-wrap items-center gap-2">
            <span className="truncate text-sm font-medium">{task.file_name}</span>
            <Badge tone={statusToneFor(task.status)}>
              {activeStates.has(task.status) ? (
                <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-current" />
              ) : null}
              {statusLabel(task.status)}
            </Badge>
            {task.expected_checksum ? (
              <Badge tone="neutral" title="Expected SHA-256 checksum recorded">
                <ShieldCheck className="h-3 w-3" /> checksum
              </Badge>
            ) : null}
          </div>
          <div className="mt-1 flex flex-wrap items-center gap-x-2 gap-y-1 text-[11px] text-muted">
            <span className={statusTone(task.status)}>{task.category}</span>
            <span className="text-subtle">·</span>
            <span title={task.url}>{domainOf(task.url)}</span>
            <span className="text-subtle">·</span>
            <span>{formatBytes(task.downloaded_bytes)} of {formatBytes(task.total_bytes)}</span>
            {!compact ? (
              <>
                <span className="text-subtle">·</span>
                <span>{task.connections} connection{task.connections === 1 ? "" : "s"}</span>
                <span className="text-subtle">·</span>
                <span>{relativeTime(task.updated_at)}</span>
              </>
            ) : null}
          </div>
          {!compact ? (
            <p className="mt-1 truncate font-mono text-[10px] text-subtle" title={task.url}>
              {truncateMiddle(task.url, 90)}
            </p>
          ) : null}
        </button>

        <div className="flex shrink-0 flex-wrap items-center gap-1">
          {busy ? (
            <button
              type="button"
              className="icon-btn"
              title="Pause download"
              onClick={() => void pauseTask(task.id)}
            >
              <Pause className="h-4 w-4" />
            </button>
          ) : null}
          {task.status === "paused" || task.status === "queued" ? (
            <button
              type="button"
              className="icon-btn"
              title="Resume download"
              onClick={() => void resumeTask(task.id)}
            >
              <Play className="h-4 w-4" />
            </button>
          ) : null}
          {failedStates.has(task.status) ? (
            <button
              type="button"
              className="icon-btn"
              title={task.status === "failed" ? "Retry download" : "Restart download"}
              onClick={() => void retryTask(task.id, task.status === "cancelled")}
            >
              <RotateCcw className="h-4 w-4" />
            </button>
          ) : null}
          {busy || task.status === "paused" ? (
            <button
              type="button"
              className="icon-btn"
              title="Cancel download"
              onClick={() => void cancelTask(task.id)}
            >
              <Ban className="h-4 w-4" />
            </button>
          ) : null}
          {task.status === "completed" ? (
            <>
              <button
                type="button"
                className="icon-btn text-success"
                title="Verify file integrity"
                onClick={() => void verifyTask(task.id)}
              >
                <ShieldCheck className="h-4 w-4" />
              </button>
              {isTauri ? (
                <>
                  <button type="button" className="icon-btn" title="Open file" onClick={() => void handleOpen()}>
                    <CheckCircle2 className="h-4 w-4" />
                  </button>
                  <button
                    type="button"
                    className="icon-btn"
                    title="Show in folder"
                    onClick={() => void handleReveal()}
                  >
                    <FolderOpen className="h-4 w-4" />
                  </button>
                </>
              ) : null}
            </>
          ) : null}
          <button
            type="button"
            className="icon-btn hover:text-danger"
            title="Remove from list"
            onClick={handleRemove}
          >
            <Trash2 className="h-4 w-4" />
          </button>
        </div>
      </div>

      <div className="mt-3 flex items-center gap-3">
        <ProgressBar
          value={progress}
          segments={task.segments}
          indeterminate={pendingStates.has(task.status) || task.status === "preparing"}
        />
        <span className="w-24 shrink-0 text-right text-[11px] tabular-nums text-muted">
          {task.total_bytes ? `${progress.toFixed(1)}%` : formatBytes(task.downloaded_bytes)}
        </span>
      </div>
      <div className="mt-1.5 flex items-center justify-between text-[11px] text-muted">
        <span>
          {task.status === "downloading" ? formatSpeed(task.speed_bps) : busy ? "Waiting…" : statusLabel(task.status)}
        </span>
        <span>
          {task.error
            ? truncateMiddle(task.error, 100)
            : remaining !== null
              ? `${formatEta(remaining)} remaining`
              : task.completed_at
                ? relativeTime(task.completed_at)
                : ""}
        </span>
      </div>
    </div>
  );
}
