import { useMemo, useState } from "react";
import { FolderTree, Lock, Pencil, Plus, Trash2 } from "lucide-react";
import PageHeader from "../components/PageHeader";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Dialog } from "../components/ui/dialog";
import { Input } from "../components/ui/input";
import { formatBytes } from "../lib/format";
import type { Category } from "../lib/types";
import { useApp } from "../store/app";

const emptyForm = { id: "", name: "", extensions: "", destination_dir: "" };

export default function Categories() {
  const categories = useApp((state) => state.categories);
  const tasks = useApp((state) => state.tasks);
  const analytics = useApp((state) => state.analytics);
  const createCategory = useApp((state) => state.createCategory);
  const deleteCategory = useApp((state) => state.deleteCategory);
  const [open, setOpen] = useState(false);
  const [form, setForm] = useState(emptyForm);
  const [editing, setEditing] = useState(false);

  const usage = useMemo(() => {
    const map = new Map<string, { count: number; bytes: number }>();
    for (const item of analytics?.categories ?? []) {
      map.set(item.category, { count: item.count, bytes: item.bytes });
    }
    return map;
  }, [analytics]);

  const startCreate = () => {
    setForm(emptyForm);
    setEditing(false);
    setOpen(true);
  };

  const startEdit = (category: Category) => {
    setForm({
      id: category.id,
      name: category.name,
      extensions: category.extensions.join(", "),
      destination_dir: category.destination_dir ?? "",
    });
    setEditing(true);
    setOpen(true);
  };

  const save = async () => {
    if (!form.name.trim()) return;
    await createCategory({
      id: form.id || undefined,
      name: form.name.trim(),
      extensions: form.extensions
        .split(",")
        .map((extension) => extension.trim())
        .filter(Boolean),
      destination_dir: form.destination_dir.trim() || null,
    } as Partial<Category> & { name: string });
    setOpen(false);
  };

  return (
    <div>
      <PageHeader
        title="Categories"
        description="Extension-based classification with optional per-category destination folders."
        actions={
          <Button variant="primary" size="sm" onClick={startCreate}>
            <Plus className="h-3.5 w-3.5" /> New category
          </Button>
        }
      />

      <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
        {categories.map((category) => {
          const stat = usage.get(category.id);
          return (
            <div key={category.id} className="card flex flex-col p-4">
              <div className="flex items-start justify-between gap-2">
                <div className="flex min-w-0 items-center gap-2">
                  <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-accent/10 text-accent">
                    <FolderTree className="h-4.5 w-4.5" />
                  </div>
                  <div className="min-w-0">
                    <p className="truncate text-sm font-semibold">{category.name}</p>
                    <p className="truncate font-mono text-[10px] text-subtle">{category.id}</p>
                  </div>
                </div>
                {category.builtin ? (
                  <Badge tone="neutral">
                    <Lock className="h-2.5 w-2.5" /> built-in
                  </Badge>
                ) : (
                  <Badge tone="accent">custom</Badge>
                )}
              </div>

              <div className="mt-3 flex min-h-[24px] flex-wrap gap-1">
                {category.extensions.length === 0 ? (
                  <span className="text-[11px] text-subtle">No extensions mapped</span>
                ) : (
                  category.extensions.map((extension) => (
                    <span
                      key={extension}
                      className="rounded bg-raised px-1.5 py-0.5 font-mono text-[10px] text-muted"
                    >
                      .{extension}
                    </span>
                  ))
                )}
              </div>

              <p className="mt-3 truncate font-mono text-[10px] text-subtle" title={category.destination_dir ?? ""}>
                {category.destination_dir ?? "Inherits default destination"}
              </p>

              <div className="mt-3 flex items-center justify-between border-t border-border/60 pt-3">
                <span className="text-[11px] text-muted">
                  {stat?.count ?? 0} file{(stat?.count ?? 0) === 1 ? "" : "s"} ·{" "}
                  {formatBytes(stat?.bytes ?? 0)}
                </span>
                <div className="flex items-center gap-1">
                  {!category.builtin ? (
                    <>
                      <button
                        type="button"
                        className="icon-btn"
                        title="Edit category"
                        onClick={() => startEdit(category)}
                      >
                        <Pencil className="h-3.5 w-3.5" />
                      </button>
                      <button
                        type="button"
                        className="icon-btn hover:text-danger"
                        title="Delete category"
                        onClick={() => {
                          if (window.confirm(`Delete the category “${category.name}”?`)) {
                            void deleteCategory(category.id);
                          }
                        }}
                      >
                        <Trash2 className="h-3.5 w-3.5" />
                      </button>
                    </>
                  ) : (
                    <span className="text-[10px] text-subtle">
                      {tasks.filter((task) => task.category === category.id).length} tracked
                    </span>
                  )}
                </div>
              </div>
            </div>
          );
        })}
      </div>

      <Dialog
        open={open}
        onClose={() => setOpen(false)}
        title={editing ? "Edit category" : "New category"}
        description="Extensions are normalized to lowercase and stored without a leading dot."
        footer={
          <>
            <Button variant="ghost" onClick={() => setOpen(false)}>
              Cancel
            </Button>
            <Button variant="primary" onClick={() => void save()} disabled={!form.name.trim()}>
              {editing ? "Save changes" : "Create category"}
            </Button>
          </>
        }
      >
        <div className="space-y-3">
          <div>
            <label className="label" htmlFor="category-id">
              Identifier <span className="font-normal text-subtle">(optional)</span>
            </label>
            <Input
              id="category-id"
              value={form.id}
              onChange={(event) => setForm({ ...form, id: event.target.value })}
              placeholder="Auto-generated from the name"
              disabled={editing}
              spellCheck={false}
            />
            <p className="mt-1 text-[11px] text-muted">
              Lowercase letters, digits, hyphens and underscores only.
            </p>
          </div>
          <div>
            <label className="label" htmlFor="category-name">
              Display name
            </label>
            <Input
              id="category-name"
              value={form.name}
              onChange={(event) => setForm({ ...form, name: event.target.value })}
              placeholder="e.g. Design assets"
            />
          </div>
          <div>
            <label className="label" htmlFor="category-extensions">
              Extensions
            </label>
            <Input
              id="category-extensions"
              value={form.extensions}
              onChange={(event) => setForm({ ...form, extensions: event.target.value })}
              placeholder="zip, 7z, tar.gz"
              spellCheck={false}
            />
          </div>
          <div>
            <label className="label" htmlFor="category-destination">
              Destination folder <span className="font-normal text-subtle">(optional)</span>
            </label>
            <Input
              id="category-destination"
              className="font-mono text-xs"
              value={form.destination_dir}
              onChange={(event) => setForm({ ...form, destination_dir: event.target.value })}
              placeholder="Inherit the global default"
              spellCheck={false}
            />
          </div>
        </div>
      </Dialog>
    </div>
  );
}
