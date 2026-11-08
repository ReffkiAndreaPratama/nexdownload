import { useState } from "react";
import {
  AlertTriangle,
  ArrowDownToLine,
  Pencil,
  Plus,
  Sparkles,
  Trash2,
} from "lucide-react";
import PageHeader from "../components/PageHeader";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Dialog } from "../components/ui/dialog";
import { Input, Select, Toggle } from "../components/ui/input";
import type { Rule, StoredRule } from "../lib/types";
import { useApp } from "../store/app";

interface FormState {
  id: string;
  name: string;
  enabled: boolean;
  priority: string;
  extensions: string;
  domains: string;
  filename_regex: string;
  url_regex: string;
  min_size_mb: string;
  max_size_mb: string;
  content_types: string;
  category: string;
  destination_dir: string;
  queue_id: string;
  action_priority: string;
  connections: string;
  require_confirmation: boolean;
}

const emptyForm: FormState = {
  id: "",
  name: "",
  enabled: true,
  priority: "0",
  extensions: "",
  domains: "",
  filename_regex: "",
  url_regex: "",
  min_size_mb: "",
  max_size_mb: "",
  content_types: "",
  category: "",
  destination_dir: "",
  queue_id: "",
  action_priority: "",
  connections: "",
  require_confirmation: false,
};

function list(value: string) {
  return value
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean);
}

function toMb(bytes: number | null | undefined) {
  return bytes ? String(Number((bytes / 1024 / 1024).toFixed(2))) : "";
}

function conditionSummary(rule: StoredRule) {
  const conditions = rule.conditions;
  const parts: string[] = [];
  if (conditions.extensions.length) parts.push(`ext: ${conditions.extensions.join(", ")}`);
  if (conditions.domains.length) parts.push(`domain: ${conditions.domains.join(", ")}`);
  if (conditions.content_types.length) parts.push(`type: ${conditions.content_types.join(", ")}`);
  if (conditions.min_size || conditions.max_size) {
    parts.push(
      `size: ${conditions.min_size ? `${toMb(conditions.min_size)} MB` : "0"} – ${
        conditions.max_size ? `${toMb(conditions.max_size)} MB` : "∞"
      }`,
    );
  }
  if (conditions.filename_regex) parts.push(`name ~ /${conditions.filename_regex}/`);
  if (conditions.url_regex) parts.push(`url ~ /${conditions.url_regex}/`);
  return parts;
}

function actionSummary(rule: StoredRule) {
  const actions = rule.actions;
  const parts: string[] = [];
  if (actions.category) parts.push(`category → ${actions.category}`);
  if (actions.destination_dir) parts.push(`folder → ${actions.destination_dir}`);
  if (actions.queue_id) parts.push(`queue → ${actions.queue_id}`);
  if (actions.priority !== null) parts.push(`priority ${actions.priority}`);
  if (actions.connections !== null) parts.push(`${actions.connections} connections`);
  if (actions.require_confirmation) parts.push("confirm before adding");
  return parts;
}

export default function Rules() {
  const rules = useApp((state) => state.rules);
  const categories = useApp((state) => state.categories);
  const queues = useApp((state) => state.queues);
  const saveRule = useApp((state) => state.saveRule);
  const deleteRule = useApp((state) => state.deleteRule);
  const [open, setOpen] = useState(false);
  const [form, setForm] = useState<FormState>(emptyForm);
  const [error, setError] = useState<string | null>(null);

  const sorted = [...rules].sort((a, b) => b.priority - a.priority);

  const openCreate = () => {
    setForm(emptyForm);
    setError(null);
    setOpen(true);
  };

  const openEdit = (rule: StoredRule) => {
    setForm({
      id: rule.id,
      name: rule.name,
      enabled: rule.enabled,
      priority: String(rule.priority),
      extensions: rule.conditions.extensions.join(", "),
      domains: rule.conditions.domains.join(", "),
      filename_regex: rule.conditions.filename_regex ?? "",
      url_regex: rule.conditions.url_regex ?? "",
      min_size_mb: toMb(rule.conditions.min_size),
      max_size_mb: toMb(rule.conditions.max_size),
      content_types: rule.conditions.content_types.join(", "),
      category: rule.actions.category ?? "",
      destination_dir: rule.actions.destination_dir ?? "",
      queue_id: rule.actions.queue_id ?? "",
      action_priority: rule.actions.priority === null ? "" : String(rule.actions.priority),
      connections: rule.actions.connections === null ? "" : String(rule.actions.connections),
      require_confirmation: rule.actions.require_confirmation,
    });
    setError(null);
    setOpen(true);
  };

  const submit = async () => {
    if (!form.name.trim()) {
      setError("A rule name is required.");
      return;
    }
    const rule: Rule = {
      id: form.id,
      name: form.name.trim(),
      enabled: form.enabled,
      priority: Number(form.priority) || 0,
      conditions: {
        extensions: list(form.extensions),
        domains: list(form.domains),
        filename_regex: form.filename_regex.trim() || null,
        url_regex: form.url_regex.trim() || null,
        min_size: form.min_size_mb ? Math.round(Number(form.min_size_mb) * 1024 * 1024) : null,
        max_size: form.max_size_mb ? Math.round(Number(form.max_size_mb) * 1024 * 1024) : null,
        content_types: list(form.content_types),
      },
      actions: {
        category: form.category || null,
        destination_dir: form.destination_dir.trim() || null,
        queue_id: form.queue_id || null,
        priority: form.action_priority === "" ? null : Number(form.action_priority),
        connections:
          form.connections === ""
            ? null
            : Math.max(1, Math.min(32, Number(form.connections) || 1)),
        require_confirmation: form.require_confirmation,
      },
    };
    try {
      await saveRule(rule);
      setOpen(false);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    }
  };

  return (
    <div>
      <PageHeader
        title="Smart rules"
        description="Rules run after source inspection and before a task is created. The first match wins."
        actions={
          <Button variant="primary" size="sm" onClick={openCreate}>
            <Plus className="h-3.5 w-3.5" /> New rule
          </Button>
        }
      />

      <div className="mb-4 grid gap-3 sm:grid-cols-3">
        <div className="card p-4">
          <p className="text-[10px] uppercase tracking-[0.12em] text-subtle">Rules</p>
          <p className="mt-1 text-xl font-semibold">{rules.length}</p>
          <p className="mt-0.5 text-[11px] text-muted">
            {rules.filter((rule) => rule.enabled).length} enabled
          </p>
        </div>
        <div className="card p-4">
          <p className="text-[10px] uppercase tracking-[0.12em] text-subtle">Ordering</p>
          <p className="mt-1 text-xl font-semibold">Priority</p>
          <p className="mt-0.5 text-[11px] text-muted">Highest priority, then stable ID</p>
        </div>
        <div className="card p-4">
          <p className="text-[10px] uppercase tracking-[0.12em] text-subtle">Conditions</p>
          <p className="mt-1 text-xl font-semibold">AND</p>
          <p className="mt-0.5 text-[11px] text-muted">Every condition must match</p>
        </div>
      </div>

      {sorted.length === 0 ? (
        <div className="card flex flex-col items-center justify-center px-6 py-14 text-center">
          <Sparkles className="h-8 w-8 text-subtle" />
          <p className="mt-3 text-sm font-semibold">No rules configured</p>
          <p className="mt-1 max-w-sm text-xs text-muted">
            Route design files into a category, send large archives to a queue, or require
            confirmation for a sensitive destination.
          </p>
          <Button className="mt-4" variant="primary" size="sm" onClick={openCreate}>
            <Plus className="h-3.5 w-3.5" /> Create the first rule
          </Button>
        </div>
      ) : (
        <div className="space-y-2">
          {sorted.map((rule) => {
            const conditions = conditionSummary(rule);
            const actions = actionSummary(rule);
            const matchesEverything = conditions.length === 0;
            return (
              <div key={rule.id} className="card p-4">
                <div className="flex flex-wrap items-start justify-between gap-3">
                  <div className="min-w-0 flex-1">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="text-sm font-semibold">{rule.name}</span>
                      <Badge tone="neutral">priority {rule.priority}</Badge>
                      {matchesEverything ? (
                        <Badge tone="danger">
                          <AlertTriangle className="h-2.5 w-2.5" /> matches everything
                        </Badge>
                      ) : null}
                      {rule.actions.require_confirmation ? (
                        <Badge tone="warning">confirmation</Badge>
                      ) : null}
                    </div>
                    <div className="mt-2 flex flex-wrap gap-1.5">
                      {conditions.length === 0 ? (
                        <span className="text-[11px] text-subtle">No conditions</span>
                      ) : (
                        conditions.map((condition) => (
                          <span
                            key={condition}
                            className="rounded bg-raised px-1.5 py-0.5 font-mono text-[10px] text-muted"
                          >
                            {condition}
                          </span>
                        ))
                      )}
                    </div>
                    <div className="mt-2 flex flex-wrap items-center gap-1.5 text-[11px] text-muted">
                      <ArrowDownToLine className="h-3 w-3 text-accent" />
                      {actions.length === 0 ? "No actions (rule only matches)" : actions.join(" · ")}
                    </div>
                  </div>
                  <div className="flex shrink-0 items-center gap-2">
                    <Toggle
                      checked={rule.enabled}
                      label={`${rule.enabled ? "Disable" : "Enable"} rule ${rule.name}`}
                      onChange={(enabled) => {
                        void saveRule({ ...rule, enabled }).catch(() => undefined);
                      }}
                    />
                    <button type="button" className="icon-btn" title="Edit rule" onClick={() => openEdit(rule)}>
                      <Pencil className="h-3.5 w-3.5" />
                    </button>
                    <button
                      type="button"
                      className="icon-btn hover:text-danger"
                      title="Delete rule"
                      onClick={() => {
                        if (window.confirm(`Delete the rule “${rule.name}”?`)) {
                          void deleteRule(rule.id);
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
      )}

      <Dialog
        open={open}
        onClose={() => setOpen(false)}
        title={form.id ? "Edit rule" : "New rule"}
        description="Conditions are matched against the inspected file name, URL, size and content type."
        width="max-w-3xl"
        footer={
          <>
            <Button variant="ghost" onClick={() => setOpen(false)}>
              Cancel
            </Button>
            <Button variant="primary" onClick={() => void submit()}>
              {form.id ? "Save rule" : "Create rule"}
            </Button>
          </>
        }
      >
        <div className="space-y-5">
          <div className="grid gap-3 sm:grid-cols-[1fr_120px_120px]">
            <div>
              <label className="label" htmlFor="rule-name">
                Rule name
              </label>
              <Input
                id="rule-name"
                value={form.name}
                onChange={(event) => setForm({ ...form, name: event.target.value })}
                placeholder="e.g. Large archives to overnight queue"
              />
            </div>
            <div>
              <label className="label" htmlFor="rule-priority">
                Priority
              </label>
              <Input
                id="rule-priority"
                type="number"
                value={form.priority}
                onChange={(event) => setForm({ ...form, priority: event.target.value })}
              />
            </div>
            <div>
              <span className="label">Enabled</span>
              <div className="flex h-9 items-center">
                <Toggle
                  checked={form.enabled}
                  label="Rule enabled"
                  onChange={(enabled) => setForm({ ...form, enabled })}
                />
              </div>
            </div>
          </div>

          <div>
            <h3 className="mb-2 text-xs font-semibold uppercase tracking-[0.08em] text-subtle">
              Conditions
            </h3>
            <div className="grid gap-3 sm:grid-cols-2">
              <div>
                <label className="label" htmlFor="rule-extensions">
                  Extensions
                </label>
                <Input
                  id="rule-extensions"
                  value={form.extensions}
                  onChange={(event) => setForm({ ...form, extensions: event.target.value })}
                  placeholder="zip, 7z, iso"
                  spellCheck={false}
                />
              </div>
              <div>
                <label className="label" htmlFor="rule-domains">
                  Domains
                </label>
                <Input
                  id="rule-domains"
                  value={form.domains}
                  onChange={(event) => setForm({ ...form, domains: event.target.value })}
                  placeholder="example.com, cdn.example.net"
                  spellCheck={false}
                />
              </div>
              <div>
                <label className="label" htmlFor="rule-content-types">
                  Content types
                </label>
                <Input
                  id="rule-content-types"
                  value={form.content_types}
                  onChange={(event) => setForm({ ...form, content_types: event.target.value })}
                  placeholder="video, application/zip"
                  spellCheck={false}
                />
              </div>
              <div className="grid grid-cols-2 gap-2">
                <div>
                  <label className="label" htmlFor="rule-min-size">
                    Min size (MB)
                  </label>
                  <Input
                    id="rule-min-size"
                    type="number"
                    min={0}
                    value={form.min_size_mb}
                    onChange={(event) => setForm({ ...form, min_size_mb: event.target.value })}
                    placeholder="0"
                  />
                </div>
                <div>
                  <label className="label" htmlFor="rule-max-size">
                    Max size (MB)
                  </label>
                  <Input
                    id="rule-max-size"
                    type="number"
                    min={0}
                    value={form.max_size_mb}
                    onChange={(event) => setForm({ ...form, max_size_mb: event.target.value })}
                    placeholder="∞"
                  />
                </div>
              </div>
              <div>
                <label className="label" htmlFor="rule-filename-regex">
                  File name regex
                </label>
                <Input
                  id="rule-filename-regex"
                  className="font-mono text-[11px]"
                  value={form.filename_regex}
                  onChange={(event) => setForm({ ...form, filename_regex: event.target.value })}
                  placeholder="^render-\\d+\\.zip$"
                  spellCheck={false}
                />
              </div>
              <div>
                <label className="label" htmlFor="rule-url-regex">
                  URL regex
                </label>
                <Input
                  id="rule-url-regex"
                  className="font-mono text-[11px]"
                  value={form.url_regex}
                  onChange={(event) => setForm({ ...form, url_regex: event.target.value })}
                  placeholder="/releases/"
                  spellCheck={false}
                />
              </div>
            </div>
            <p className="mt-1.5 text-[11px] text-muted">
              Leave all conditions empty only if the rule should intentionally match every download.
            </p>
          </div>

          <div>
            <h3 className="mb-2 text-xs font-semibold uppercase tracking-[0.08em] text-subtle">
              Actions
            </h3>
            <div className="grid gap-3 sm:grid-cols-2">
              <div>
                <label className="label" htmlFor="rule-category">
                  Category
                </label>
                <Select
                  id="rule-category"
                  value={form.category}
                  onChange={(event) => setForm({ ...form, category: event.target.value })}
                >
                  <option value="">Keep detected category</option>
                  {categories.map((category) => (
                    <option key={category.id} value={category.id}>
                      {category.name}
                    </option>
                  ))}
                </Select>
              </div>
              <div>
                <label className="label" htmlFor="rule-queue">
                  Queue
                </label>
                <Select
                  id="rule-queue"
                  value={form.queue_id}
                  onChange={(event) => setForm({ ...form, queue_id: event.target.value })}
                >
                  <option value="">Keep default queue</option>
                  {queues.map((queue) => (
                    <option key={queue.id} value={queue.id}>
                      {queue.name}
                    </option>
                  ))}
                </Select>
              </div>
              <div>
                <label className="label" htmlFor="rule-destination">
                  Destination folder
                </label>
                <Input
                  id="rule-destination"
                  className="font-mono text-[11px]"
                  value={form.destination_dir}
                  onChange={(event) => setForm({ ...form, destination_dir: event.target.value })}
                  placeholder="Optional absolute or ~ path"
                  spellCheck={false}
                />
              </div>
              <div className="grid grid-cols-2 gap-2">
                <div>
                  <label className="label" htmlFor="rule-action-priority">
                    Priority
                  </label>
                  <Input
                    id="rule-action-priority"
                    type="number"
                    value={form.action_priority}
                    onChange={(event) => setForm({ ...form, action_priority: event.target.value })}
                    placeholder="keep"
                  />
                </div>
                <div>
                  <label className="label" htmlFor="rule-connections">
                    Connections
                  </label>
                  <Input
                    id="rule-connections"
                    type="number"
                    min={1}
                    max={32}
                    value={form.connections}
                    onChange={(event) => setForm({ ...form, connections: event.target.value })}
                    placeholder="keep"
                  />
                </div>
              </div>
            </div>
            <div className="mt-3 flex items-center justify-between rounded-lg border border-border/60 px-3 py-2">
              <div>
                <p className="text-xs font-medium">Require confirmation</p>
                <p className="text-[11px] text-muted">
                  The task is only created after the user confirms in the add dialog.
                </p>
              </div>
              <Toggle
                checked={form.require_confirmation}
                label="Require confirmation"
                onChange={(require_confirmation) => setForm({ ...form, require_confirmation })}
              />
            </div>
          </div>

          {error ? (
            <p className="rounded-lg border border-danger/25 bg-danger/10 px-3 py-2 text-xs text-danger">
              {error}
            </p>
          ) : null}
        </div>
      </Dialog>
    </div>
  );
}
