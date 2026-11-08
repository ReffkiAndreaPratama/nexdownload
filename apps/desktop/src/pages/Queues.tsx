import { useState } from "react";
import { ListOrdered, Pause, Pencil, Play, Plus, Trash2 } from "lucide-react";
import PageHeader from "../components/PageHeader";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Dialog } from "../components/ui/dialog";
import { Input } from "../components/ui/input";
import type { Queue } from "../lib/types";
import { useApp } from "../store/app";

export default function Queues() {
  const queues = useApp((state) => state.queues);
  const analytics = useApp((state) => state.analytics);
  const createQueue = useApp((state) => state.createQueue);
  const updateQueue = useApp((state) => state.updateQueue);
  const deleteQueue = useApp((state) => state.deleteQueue);
  const pauseQueue = useApp((state) => state.pauseQueue);
  const startQueue = useApp((state) => state.startQueue);

  const [open, setOpen] = useState(false);
  const [editing, setEditing] = useState<Queue | null>(null);
  const [form, setForm] = useState({ name: "", max_concurrent: "2", priority: "0" });

  const openCreate = () => {
    setEditing(null);
    setForm({ name: "", max_concurrent: "2", priority: "0" });
    setOpen(true);
  };

  const openEdit = (queue: Queue) => {
    setEditing(queue);
    setForm({
      name: queue.name,
      max_concurrent: String(queue.max_concurrent),
      priority: String(queue.priority),
    });
    setOpen(true);
  };

  const save = async () => {
    if (!form.name.trim()) return;
    const maxConcurrent = Math.max(1, Math.min(16, Number(form.max_concurrent) || 1));
    if (editing) {
      await updateQueue({
        ...editing,
        name: form.name.trim(),
        max_concurrent: maxConcurrent,
        priority: Number(form.priority) || 0,
      });
    } else {
      await createQueue(form.name.trim(), maxConcurrent);
    }
    setOpen(false);
  };

  return (
    <div>
      <PageHeader
        title="Queues"
        description="Queues cap concurrency independently of the global scheduler limit."
        actions={
          <Button variant="primary" size="sm" onClick={openCreate}>
            <Plus className="h-3.5 w-3.5" /> New queue
          </Button>
        }
      />

      <div className="grid gap-3 lg:grid-cols-2">
        {queues.map((queue) => {
          const stat = analytics?.queues.find((item) => item.queue_id === queue.id);
          const paused = queue.status === "paused";
          return (
            <div key={queue.id} className="card p-4">
              <div className="flex items-start justify-between gap-3">
                <div className="flex min-w-0 items-center gap-2.5">
                  <div className={`flex h-9 w-9 shrink-0 items-center justify-center rounded-lg ${paused ? "bg-warning/10 text-warning" : "bg-success/10 text-success"}`}>
                    <ListOrdered className="h-4.5 w-4.5" />
                  </div>
                  <div className="min-w-0">
                    <p className="truncate text-sm font-semibold">{queue.name}</p>
                    <p className="truncate font-mono text-[10px] text-subtle">{queue.id}</p>
                  </div>
                </div>
                <Badge tone={paused ? "warning" : "success"}>{paused ? "paused" : "active"}</Badge>
              </div>

              <div className="mt-4 grid grid-cols-3 gap-2 rounded-lg border border-border/60 bg-bg/30 p-3 text-center">
                <div>
                  <p className="text-base font-semibold">{stat?.active ?? 0}</p>
                  <p className="text-[10px] uppercase tracking-wide text-subtle">Active</p>
                </div>
                <div>
                  <p className="text-base font-semibold">{stat?.queued ?? 0}</p>
                  <p className="text-[10px] uppercase tracking-wide text-subtle">Queued</p>
                </div>
                <div>
                  <p className="text-base font-semibold">{stat?.total ?? 0}</p>
                  <p className="text-[10px] uppercase tracking-wide text-subtle">Total</p>
                </div>
              </div>

              <div className="mt-3 flex flex-wrap items-center justify-between gap-2 text-[11px] text-muted">
                <span>
                  Concurrency limit <span className="font-semibold text-fg">{queue.max_concurrent}</span> ·
                  priority <span className="font-semibold text-fg">{queue.priority}</span>
                </span>
                <div className="flex items-center gap-1">
                  <button
                    type="button"
                    className="icon-btn"
                    title={paused ? "Start queue" : "Pause queue"}
                    onClick={() => void (paused ? startQueue(queue.id) : pauseQueue(queue.id))}
                  >
                    {paused ? <Play className="h-3.5 w-3.5" /> : <Pause className="h-3.5 w-3.5" />}
                  </button>
                  <button type="button" className="icon-btn" title="Edit queue" onClick={() => openEdit(queue)}>
                    <Pencil className="h-3.5 w-3.5" />
                  </button>
                  <button
                    type="button"
                    className="icon-btn hover:text-danger"
                    title="Delete queue"
                    onClick={() => {
                      if (window.confirm(`Delete the queue “${queue.name}”? Downloads keep their files but lose this queue assignment.`)) {
                        void deleteQueue(queue.id);
                      }
                    }}
                  >
                    <Trash2 className="h-3.5 w-3.5" />
                  </button>
                </div>
              </div>
            </div>
          );
        })}
      </div>

      <Dialog
        open={open}
        onClose={() => setOpen(false)}
        title={editing ? "Edit queue" : "New queue"}
        footer={
          <>
            <Button variant="ghost" onClick={() => setOpen(false)}>
              Cancel
            </Button>
            <Button variant="primary" onClick={() => void save()} disabled={!form.name.trim()}>
              {editing ? "Save changes" : "Create queue"}
            </Button>
          </>
        }
      >
        <div className="space-y-3">
          <div>
            <label className="label" htmlFor="queue-name">
              Queue name
            </label>
            <Input
              id="queue-name"
              value={form.name}
              onChange={(event) => setForm({ ...form, name: event.target.value })}
              placeholder="e.g. Night downloads"
            />
          </div>
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="label" htmlFor="queue-concurrency">
                Max concurrent
              </label>
              <Input
                id="queue-concurrency"
                type="number"
                min={1}
                max={16}
                value={form.max_concurrent}
                onChange={(event) => setForm({ ...form, max_concurrent: event.target.value })}
              />
              <p className="mt-1 text-[11px] text-muted">1–16, capped by the global limit.</p>
            </div>
            <div>
              <label className="label" htmlFor="queue-priority">
                Priority
              </label>
              <Input
                id="queue-priority"
                type="number"
                value={form.priority}
                onChange={(event) => setForm({ ...form, priority: event.target.value })}
              />
              <p className="mt-1 text-[11px] text-muted">Higher values are selected first.</p>
            </div>
          </div>
        </div>
      </Dialog>
    </div>
  );
}
