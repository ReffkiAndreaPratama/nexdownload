import { useMemo, useState } from "react";
import { BookMarked, Plus, Trash2 } from "lucide-react";
import PageHeader from "../components/PageHeader";
import { Button } from "../components/ui/button";
import { Dialog } from "../components/ui/dialog";
import { Input } from "../components/ui/input";
import { formatBytes } from "../lib/format";
import { useApp } from "../store/app";

const colors = ["#2DD4BF", "#818CF8", "#38BDF8", "#FBBF24", "#F472B6", "#A3E635"];

export default function Collections() {
  const collections = useApp((state) => state.collections);
  const tasks = useApp((state) => state.tasks);
  const createCollection = useApp((state) => state.createCollection);
  const deleteCollection = useApp((state) => state.deleteCollection);
  const [open, setOpen] = useState(false);
  const [form, setForm] = useState({
    name: "",
    description: "",
    color: colors[0],
  });

  const usage = useMemo(() => {
    const map = new Map<string, { count: number; bytes: number }>();
    for (const task of tasks) {
      if (!task.collection_id) continue;
      const current = map.get(task.collection_id) ?? { count: 0, bytes: 0 };
      map.set(task.collection_id, {
        count: current.count + 1,
        bytes: current.bytes + (task.total_bytes ?? 0),
      });
    }
    return map;
  }, [tasks]);

  const save = async () => {
    if (!form.name.trim()) return;
    await createCollection({
      name: form.name.trim(),
      description: form.description.trim() || undefined,
      color: form.color,
    });
    setOpen(false);
    setForm({ name: "", description: "", color: colors[0] });
  };

  return (
    <div>
      <PageHeader
        title="Collections"
        description="Group related downloads across categories and queues. Files keep their own paths."
        actions={
          <Button variant="primary" size="sm" onClick={() => setOpen(true)}>
            <Plus className="h-3.5 w-3.5" /> New collection
          </Button>
        }
      />

      {collections.length === 0 ? (
        <div className="card flex flex-col items-center justify-center px-6 py-14 text-center">
          <BookMarked className="h-8 w-8 text-subtle" />
          <p className="mt-3 text-sm font-semibold">No collections yet</p>
          <p className="mt-1 max-w-sm text-xs text-muted">
            Collections are a light-weight way to keep a project together, such as a game build,
            dataset or course download.
          </p>
          <Button className="mt-4" variant="primary" size="sm" onClick={() => setOpen(true)}>
            <Plus className="h-3.5 w-3.5" /> Create a collection
          </Button>
        </div>
      ) : (
        <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
          {collections.map((collection) => {
            const stat = usage.get(collection.id);
            const color = collection.color ?? colors[0];
            return (
              <div key={collection.id} className="card p-4">
                <div className="flex items-start justify-between gap-3">
                  <div className="flex min-w-0 items-center gap-2.5">
                    <span
                      className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg"
                      style={{ backgroundColor: `${color}22`, color }}
                    >
                      <BookMarked className="h-4.5 w-4.5" />
                    </span>
                    <div className="min-w-0">
                      <p className="truncate text-sm font-semibold">{collection.name}</p>
                      <p className="text-[10px] text-subtle">
                        {stat?.count ?? 0} download{(stat?.count ?? 0) === 1 ? "" : "s"} ·{" "}
                        {formatBytes(stat?.bytes ?? 0)}
                      </p>
                    </div>
                  </div>
                  <button
                    type="button"
                    className="icon-btn hover:text-danger"
                    title="Delete collection"
                    onClick={() => {
                      if (window.confirm(`Delete the collection “${collection.name}”? Downloads are kept.`)) {
                        void deleteCollection(collection.id);
                      }
                    }}
                  >
                    <Trash2 className="h-3.5 w-3.5" />
                  </button>
                </div>
                <p className="mt-3 min-h-[32px] text-xs leading-relaxed text-muted">
                  {collection.description || "No description."}
                </p>
                <div className="mt-3 border-t border-border/60 pt-2 text-[10px] text-subtle">
                  Created {new Date(collection.created_at).toLocaleDateString()}
                </div>
              </div>
            );
          })}
        </div>
      )}

      <Dialog
        open={open}
        onClose={() => setOpen(false)}
        title="New collection"
        footer={
          <>
            <Button variant="ghost" onClick={() => setOpen(false)}>
              Cancel
            </Button>
            <Button variant="primary" onClick={() => void save()} disabled={!form.name.trim()}>
              Create collection
            </Button>
          </>
        }
      >
        <div className="space-y-3">
          <div>
            <label className="label" htmlFor="collection-name">
              Name
            </label>
            <Input
              id="collection-name"
              value={form.name}
              onChange={(event) => setForm({ ...form, name: event.target.value })}
              placeholder="e.g. Blender assets"
            />
          </div>
          <div>
            <label className="label" htmlFor="collection-description">
              Description <span className="font-normal text-subtle">(optional)</span>
            </label>
            <textarea
              id="collection-description"
              className="input min-h-20 resize-y"
              value={form.description}
              onChange={(event) => setForm({ ...form, description: event.target.value })}
              placeholder="What belongs in this collection?"
            />
          </div>
          <div>
            <span className="label">Accent colour</span>
            <div className="flex flex-wrap gap-2">
              {colors.map((color) => (
                <button
                  key={color}
                  type="button"
                  aria-label={`Use colour ${color}`}
                  className={`h-7 w-7 rounded-full border-2 transition ${form.color === color ? "border-fg" : "border-transparent"}`}
                  style={{ backgroundColor: color }}
                  onClick={() => setForm({ ...form, color })}
                />
              ))}
            </div>
          </div>
        </div>
      </Dialog>
    </div>
  );
}
