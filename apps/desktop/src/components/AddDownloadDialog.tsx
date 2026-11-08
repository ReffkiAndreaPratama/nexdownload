import { useEffect, useMemo, useState } from "react";
import {
  AlertTriangle,
  ChevronDown,
  Download,
  FolderOpen,
  Link2,
  Loader2,
  ShieldCheck,
} from "lucide-react";
import { ApiError } from "../lib/api";
import { useApp } from "../store/app";
import { Button } from "./ui/button";
import { Dialog } from "./ui/dialog";
import { Input, Select } from "./ui/input";

interface FormState {
  url: string;
  file_name: string;
  directory: string;
  category: string;
  queue_id: string;
  collection_id: string;
  connections: string;
  priority: string;
  expected_checksum: string;
}

const emptyForm: FormState = {
  url: "",
  file_name: "",
  directory: "",
  category: "",
  queue_id: "",
  collection_id: "",
  connections: "4",
  priority: "0",
  expected_checksum: "",
};

function numberSetting(settings: Record<string, unknown>, key: string, fallback: number) {
  const value = settings[key];
  return typeof value === "number" && Number.isFinite(value) ? value : fallback;
}

export default function AddDownloadDialog() {
  const open = useApp((state) => state.addOpen);
  const setAddOpen = useApp((state) => state.setAddOpen);
  const categories = useApp((state) => state.categories);
  const queues = useApp((state) => state.queues);
  const collections = useApp((state) => state.collections);
  const settings = useApp((state) => state.settings);
  const addTask = useApp((state) => state.addTask);

  const defaultConnections = numberSetting(settings, "default_connections", 4);
  const defaultDestination =
    typeof settings.default_destination === "string" ? settings.default_destination : "";

  const [form, setForm] = useState<FormState>({ ...emptyForm, connections: String(defaultConnections) });
  const [advanced, setAdvanced] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [confirmRule, setConfirmRule] = useState<string | null>(null);

  useEffect(() => {
    if (open) {
      setForm({ ...emptyForm, connections: String(defaultConnections) });
      setAdvanced(false);
      setError(null);
      setConfirmRule(null);
      setBusy(false);
    }
  }, [open, defaultConnections]);

  const update = (patch: Partial<FormState>) => {
    setForm((current) => ({ ...current, ...patch }));
    setError(null);
  };

  const checksumLooksValid = useMemo(() => {
    const value = form.expected_checksum.trim();
    return value === "" || /^[a-fA-F0-9]{64}$/.test(value);
  }, [form.expected_checksum]);

  const submit = async () => {
    if (!form.url.trim()) {
      setError("Enter an HTTP or HTTPS URL.");
      return;
    }
    if (!checksumLooksValid) {
      setError("The expected checksum must be a 64-character SHA-256 hex digest.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const result = await addTask({
        url: form.url.trim(),
        file_name: form.file_name.trim() || undefined,
        directory: form.directory.trim() || undefined,
        category: form.category || undefined,
        queue_id: form.queue_id || undefined,
        collection_id: form.collection_id || undefined,
        connections: Math.max(1, Math.min(32, Number(form.connections) || defaultConnections)),
        priority: Number(form.priority) || 0,
        expected_checksum: form.expected_checksum.trim() || undefined,
        confirmed: confirmRule !== null,
      });
      setAddOpen(false);
      if (result.duplicate_of) {
        const state = useApp.getState();
        state.pushToast(
          "warning",
          "Possible duplicate",
          "A completed download with the same size and checksum already exists.",
        );
      }
    } catch (caught) {
      if (caught instanceof ApiError && caught.code === "confirmation_required") {
        const details = caught.details as { rule?: string } | null;
        setConfirmRule(details?.rule ?? "a matching rule");
      } else {
        setError(caught instanceof Error ? caught.message : String(caught));
      }
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog
      open={open}
      onClose={() => setAddOpen(false)}
      title="New download"
      description="The engine inspects the source before creating the task, so the size and range support are real."
      width="max-w-2xl"
      footer={
        <>
          <Button variant="ghost" onClick={() => setAddOpen(false)} disabled={busy}>
            Cancel
          </Button>
          <Button variant="primary" onClick={() => void submit()} disabled={busy}>
            {busy ? <Loader2 className="h-4 w-4 animate-spin" /> : <Download className="h-4 w-4" />}
            {confirmRule ? "Add anyway" : "Add download"}
          </Button>
        </>
      }
    >
      <div className="space-y-4">
        <div>
          <label className="label" htmlFor="add-url">
            Download URL
          </label>
          <div className="relative">
            <Link2 className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-subtle" />
            <Input
              id="add-url"
              className="pl-9"
              value={form.url}
              onChange={(event) => {
                update({ url: event.target.value });
                setConfirmRule(null);
              }}
              placeholder="https://example.com/archive.zip"
              spellCheck={false}
              autoComplete="off"
              autoFocus
            />
          </div>
          <p className="mt-1 text-[11px] text-muted">
            HTTP and HTTPS only. URLs containing embedded credentials are rejected.
          </p>
        </div>

        {confirmRule ? (
          <div className="rounded-lg border border-warning/30 bg-warning/10 p-3">
            <div className="flex items-start gap-2">
              <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-warning" />
              <div className="text-xs leading-relaxed">
                <p className="font-medium text-warning">Rule confirmation required</p>
                <p className="mt-1 text-muted">
                  The rule <span className="font-semibold text-fg">{confirmRule}</span> matches this
                  download. Review the destination and queue above, then confirm if they are correct.
                </p>
              </div>
            </div>
          </div>
        ) : null}

        <div className="grid gap-3 sm:grid-cols-2">
          <div>
            <label className="label" htmlFor="add-name">
              File name <span className="font-normal text-subtle">(optional)</span>
            </label>
            <Input
              id="add-name"
              value={form.file_name}
              onChange={(event) => update({ file_name: event.target.value })}
              placeholder="Use server file name"
              spellCheck={false}
            />
          </div>
          <div>
            <label className="label" htmlFor="add-category">
              Category
            </label>
            <Select
              id="add-category"
              value={form.category}
              onChange={(event) => update({ category: event.target.value })}
            >
              <option value="">Auto-detect from extension</option>
              {categories.map((category) => (
                <option key={category.id} value={category.id}>
                  {category.name}
                </option>
              ))}
            </Select>
          </div>
          <div>
            <label className="label" htmlFor="add-queue">
              Queue
            </label>
            <Select
              id="add-queue"
              value={form.queue_id}
              onChange={(event) => update({ queue_id: event.target.value })}
            >
              <option value="">Default queue</option>
              {queues.map((queue) => (
                <option key={queue.id} value={queue.id}>
                  {queue.name}
                </option>
              ))}
            </Select>
          </div>
          <div>
            <label className="label" htmlFor="add-collection">
              Collection
            </label>
            <Select
              id="add-collection"
              value={form.collection_id}
              onChange={(event) => update({ collection_id: event.target.value })}
            >
              <option value="">No collection</option>
              {collections.map((collection) => (
                <option key={collection.id} value={collection.id}>
                  {collection.name}
                </option>
              ))}
            </Select>
          </div>
        </div>

        <div>
          <label className="label" htmlFor="add-directory">
            Destination folder <span className="font-normal text-subtle">(optional)</span>
          </label>
          <div className="relative">
            <FolderOpen className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-subtle" />
            <Input
              id="add-directory"
              className="pl-9 font-mono text-xs"
              value={form.directory}
              onChange={(event) => update({ directory: event.target.value })}
              placeholder={defaultDestination || "Server default destination"}
              spellCheck={false}
            />
          </div>
        </div>

        <button
          type="button"
          className="flex w-full items-center justify-between rounded-lg border border-border/70 px-3 py-2 text-xs font-medium text-muted transition hover:bg-raised hover:text-fg"
          onClick={() => setAdvanced((value) => !value)}
        >
          <span className="flex items-center gap-2">
            <ShieldCheck className="h-3.5 w-3.5" /> Advanced options
          </span>
          <ChevronDown className={advanced ? "h-4 w-4 rotate-180 transition" : "h-4 w-4 transition"} />
        </button>

        {advanced ? (
          <div className="grid gap-3 rounded-lg border border-border/60 bg-bg/40 p-3 sm:grid-cols-3">
            <div>
              <label className="label" htmlFor="add-connections">
                Connections
              </label>
              <Input
                id="add-connections"
                type="number"
                min={1}
                max={32}
                value={form.connections}
                onChange={(event) => update({ connections: event.target.value })}
              />
            </div>
            <div>
              <label className="label" htmlFor="add-priority">
                Priority
              </label>
              <Input
                id="add-priority"
                type="number"
                value={form.priority}
                onChange={(event) => update({ priority: event.target.value })}
              />
            </div>
            <div>
              <label className="label" htmlFor="add-checksum">
                SHA-256
              </label>
              <Input
                id="add-checksum"
                className="font-mono text-[11px]"
                value={form.expected_checksum}
                onChange={(event) => update({ expected_checksum: event.target.value })}
                placeholder="Optional expected digest"
                spellCheck={false}
              />
            </div>
            <p className="text-[11px] leading-relaxed text-muted sm:col-span-3">
              Higher connection counts are only used when the server supports byte ranges. The adaptive
              controller can lower them during a transfer; fixed ranges are never re-flowed mid-download.
            </p>
          </div>
        ) : null}

        {error ? (
          <p className="rounded-lg border border-danger/25 bg-danger/10 px-3 py-2 text-xs text-danger">
            {error}
          </p>
        ) : null}
      </div>
    </Dialog>
  );
}
