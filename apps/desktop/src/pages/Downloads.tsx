import { useMemo, useState } from "react";
import { Download, Search, SlidersHorizontal, X } from "lucide-react";
import DownloadRow from "../components/DownloadRow";
import EmptyState from "../components/EmptyState";
import PageHeader from "../components/PageHeader";
import { Button } from "../components/ui/button";
import { Input, Select } from "../components/ui/input";
import { cn } from "../lib/utils";
import { useApp } from "../store/app";

const filters = [
  { id: "all", label: "All", statuses: null },
  { id: "active", label: "Active", statuses: ["downloading", "connecting", "preparing", "verifying"] },
  { id: "queued", label: "Queued", statuses: ["queued", "waiting_for_network"] },
  { id: "paused", label: "Paused", statuses: ["paused", "pausing"] },
  { id: "completed", label: "Completed", statuses: ["completed"] },
  { id: "failed", label: "Failed", statuses: ["failed", "cancelled"] },
] as const;

type FilterId = (typeof filters)[number]["id"];

export default function Downloads() {
  const tasks = useApp((state) => state.tasks);
  const categories = useApp((state) => state.categories);
  const setAddOpen = useApp((state) => state.setAddOpen);
  const [filter, setFilter] = useState<FilterId>("all");
  const [search, setSearch] = useState("");
  const [category, setCategory] = useState("");
  const [sort, setSort] = useState("newest");

  const filtered = useMemo(() => {
    const current = filters.find((item) => item.id === filter);
    const statuses = current?.statuses as readonly string[] | null | undefined;
    const query = search.trim().toLowerCase();
    return tasks
      .filter((task) => (statuses ? statuses.includes(task.status) : true))
      .filter((task) => (category ? task.category === category : true))
      .filter((task) =>
        query
          ? `${task.file_name} ${task.url} ${task.category}`.toLowerCase().includes(query)
          : true,
      )
      .sort((a, b) => {
        switch (sort) {
          case "oldest":
            return a.created_at.localeCompare(b.created_at);
          case "name":
            return a.file_name.localeCompare(b.file_name);
          case "size":
            return (b.total_bytes ?? 0) - (a.total_bytes ?? 0);
          case "priority":
            return b.priority - a.priority;
          default:
            return b.created_at.localeCompare(a.created_at);
        }
      });
  }, [tasks, filter, category, search, sort]);

  const hasFilters = search !== "" || category !== "" || filter !== "all";

  return (
    <div>
      <PageHeader
        title="Downloads"
        description={`${tasks.length} task${tasks.length === 1 ? "" : "s"} tracked by the local engine.`}
        actions={
          <Button variant="primary" size="sm" onClick={() => setAddOpen(true)}>
            <Download className="h-3.5 w-3.5" /> New download
          </Button>
        }
      />

      <div className="card mb-4 p-3">
        <div className="flex flex-col gap-3">
          <div className="flex flex-wrap items-center gap-1">
            {filters.map((item) => {
              const count = item.statuses
                ? tasks.filter((task) => (item.statuses as readonly string[]).includes(task.status)).length
                : tasks.length;
              return (
                <button
                  key={item.id}
                  type="button"
                  onClick={() => setFilter(item.id)}
                  className={cn(
                    "rounded-lg px-3 py-1.5 text-xs font-medium transition",
                    filter === item.id
                      ? "bg-accent/10 text-accent"
                      : "text-muted hover:bg-raised hover:text-fg",
                  )}
                >
                  {item.label}
                  <span className="ml-1.5 text-[10px] text-subtle">{count}</span>
                </button>
              );
            })}
          </div>

          <div className="flex flex-col gap-2 sm:flex-row">
            <div className="relative flex-1">
              <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-subtle" />
              <Input
                value={search}
                onChange={(event) => setSearch(event.target.value)}
                className="pl-9"
                placeholder="Search file name, URL or category…"
                aria-label="Search downloads"
              />
            </div>
            <div className="flex gap-2">
              <Select
                className="w-40"
                value={category}
                onChange={(event) => setCategory(event.target.value)}
                aria-label="Filter by category"
              >
                <option value="">All categories</option>
                {categories.map((item) => (
                  <option key={item.id} value={item.id}>
                    {item.name}
                  </option>
                ))}
              </Select>
              <div className="relative">
                <SlidersHorizontal className="pointer-events-none absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-subtle" />
                <Select
                  className="w-36 pl-8"
                  value={sort}
                  onChange={(event) => setSort(event.target.value)}
                  aria-label="Sort downloads"
                >
                  <option value="newest">Newest first</option>
                  <option value="oldest">Oldest first</option>
                  <option value="name">Name</option>
                  <option value="size">Largest size</option>
                  <option value="priority">Priority</option>
                </Select>
              </div>
              {hasFilters ? (
                <button
                  type="button"
                  className="icon-btn"
                  title="Clear filters"
                  onClick={() => {
                    setSearch("");
                    setCategory("");
                    setFilter("all");
                  }}
                >
                  <X className="h-4 w-4" />
                </button>
              ) : null}
            </div>
          </div>
        </div>
      </div>

      <div className="mb-2 flex items-center justify-between px-1 text-[11px] text-subtle">
        <span>
          Showing {filtered.length} of {tasks.length}
        </span>
        <span>Foreground transfers are polled every 1.2 seconds</span>
      </div>

      {filtered.length === 0 ? (
        <EmptyState
          icon={Download}
          title={tasks.length === 0 ? "No downloads yet" : "No downloads match these filters"}
          description={
            tasks.length === 0
              ? "Add an HTTP or HTTPS URL. The engine will inspect the resource, decide whether ranges are usable and create a persistent task."
              : "Try a different status, category or search term."
          }
          action={
            tasks.length === 0 ? (
              <Button variant="primary" size="sm" onClick={() => setAddOpen(true)}>
                <Download className="h-3.5 w-3.5" /> Add a download
              </Button>
            ) : (
              <Button
                size="sm"
                onClick={() => {
                  setSearch("");
                  setCategory("");
                  setFilter("all");
                }}
              >
                Clear filters
              </Button>
            )
          }
        />
      ) : (
        <div className="space-y-2">
          {filtered.map((task) => (
            <DownloadRow key={task.id} task={task} />
          ))}
        </div>
      )}
    </div>
  );
}
