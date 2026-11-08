import { useEffect, useMemo, useState } from "react";
import {
  Activity,
  AlertTriangle,
  Gauge,
  RefreshCw,
  Search,
  ShieldCheck,
  Stethoscope,
  Timer,
} from "lucide-react";
import PageHeader from "../components/PageHeader";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Input, Select } from "../components/ui/input";
import { api } from "../lib/api";
import { formatBytes, formatDate, formatSpeed, statusLabel, truncateMiddle } from "../lib/format";
import type { BenchmarkResult, Diagnostics } from "../lib/types";
import { useApp } from "../store/app";

interface AuditRow {
  method: string;
  path: string;
  status: number;
  remote: string | null;
  created_at: string;
}

function Info({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-start justify-between gap-3 py-1 text-xs">
      <span className="shrink-0 text-subtle">{label}</span>
      <span className="min-w-0 break-all text-right text-fg">{value}</span>
    </div>
  );
}

export default function Diagnostics() {
  const tasks = useApp((state) => state.tasks);
  const pushToast = useApp((state) => state.pushToast);
  const [selected, setSelected] = useState("");
  const [report, setReport] = useState<Diagnostics | null>(null);
  const [loadingReport, setLoadingReport] = useState(false);

  const [benchmarkUrl, setBenchmarkUrl] = useState("");
  const [benchmark, setBenchmark] = useState<BenchmarkResult | null>(null);
  const [benchmarking, setBenchmarking] = useState(false);

  const [audit, setAudit] = useState<AuditRow[]>([]);
  const [loadingAudit, setLoadingAudit] = useState(false);

  const options = useMemo(
    () => [...tasks].sort((a, b) => b.updated_at.localeCompare(a.updated_at)),
    [tasks],
  );

  useEffect(() => {
    if (!selected && options.length > 0) {
      const failed = options.find((task) => task.status === "failed");
      setSelected(failed?.id ?? options[0].id);
    }
  }, [options, selected]);

  const loadReport = async () => {
    if (!selected) return;
    setLoadingReport(true);
    try {
      setReport(await api.diagnostics(selected));
    } catch (error) {
      pushToast("error", "Diagnostics failed", error instanceof Error ? error.message : String(error));
    } finally {
      setLoadingReport(false);
    }
  };

  const loadAudit = async () => {
    setLoadingAudit(true);
    try {
      setAudit((await api.audit()) as AuditRow[]);
    } catch (error) {
      pushToast("error", "Could not load audit log", error instanceof Error ? error.message : String(error));
    } finally {
      setLoadingAudit(false);
    }
  };

  useEffect(() => {
    void loadAudit();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const runBenchmark = async () => {
    if (!benchmarkUrl.trim()) {
      pushToast("warning", "Enter a URL to benchmark");
      return;
    }
    setBenchmarking(true);
    try {
      setBenchmark(await api.benchmark(benchmarkUrl.trim()));
      pushToast("success", "Benchmark complete");
    } catch (error) {
      pushToast("error", "Benchmark failed", error instanceof Error ? error.message : String(error));
    } finally {
      setBenchmarking(false);
    }
  };

  return (
    <div>
      <PageHeader
        title="Diagnostics"
        description="Inspect a task, benchmark a source and audit recent authenticated API calls."
        actions={
          <Button size="sm" onClick={() => void loadAudit()} disabled={loadingAudit}>
            <RefreshCw className={loadingAudit ? "h-3.5 w-3.5 animate-spin" : "h-3.5 w-3.5"} /> Refresh audit
          </Button>
        }
      />

      <div className="grid gap-5 xl:grid-cols-[1.4fr_1fr]">
        <section className="card p-5">
          <div className="mb-3 flex items-center gap-2">
            <Stethoscope className="h-4 w-4 text-accent" />
            <h2 className="text-sm font-semibold">Task diagnostics</h2>
          </div>
          <div className="flex flex-col gap-2 sm:flex-row">
            <Select
              value={selected}
              onChange={(event) => {
                setSelected(event.target.value);
                setReport(null);
              }}
              aria-label="Select a task"
            >
              {options.length === 0 ? <option value="">No tasks available</option> : null}
              {options.map((task) => (
                <option key={task.id} value={task.id}>
                  {statusLabel(task.status)} · {task.file_name}
                </option>
              ))}
            </Select>
            <Button onClick={() => void loadReport()} disabled={!selected || loadingReport}>
              <Search className="h-3.5 w-3.5" /> {loadingReport ? "Inspecting…" : "Inspect"}
            </Button>
          </div>

          {!report ? (
            <div className="mt-4 rounded-lg border border-dashed border-border px-4 py-10 text-center">
              <Activity className="mx-auto h-7 w-7 text-subtle" />
              <p className="mt-2 text-xs text-muted">
                Choose a task and inspect it. The server re-checks the source without modifying the
                download.
              </p>
            </div>
          ) : (
            <div className="mt-4 space-y-3">
              <div className="rounded-lg border border-border/60 p-3">
                <div className="flex items-start gap-2">
                  {report.error_kind ? (
                    <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-warning" />
                  ) : (
                    <ShieldCheck className="mt-0.5 h-4 w-4 shrink-0 text-success" />
                  )}
                  <div className="min-w-0">
                    <p className="text-sm font-medium">{report.summary}</p>
                    {report.error ? (
                      <p className="mt-1 break-words text-xs text-danger">{report.error}</p>
                    ) : null}
                    <p className="mt-1 text-[11px] text-muted">
                      Source reachable:{" "}
                      {report.source_reachable === null
                        ? "not checked"
                        : report.source_reachable
                          ? "yes"
                          : "no"}
                    </p>
                  </div>
                </div>
                {report.hints.length > 0 ? (
                  <ul className="mt-3 list-disc space-y-1 pl-5 text-xs text-muted">
                    {report.hints.map((hint) => (
                      <li key={hint}>{hint}</li>
                    ))}
                  </ul>
                ) : null}
                {report.suggested_actions.length > 0 ? (
                  <div className="mt-3 flex flex-wrap gap-1.5">
                    {report.suggested_actions.map((action) => (
                      <Badge key={action} tone="info">
                        {action}
                      </Badge>
                    ))}
                  </div>
                ) : null}
              </div>

              {report.resource ? (
                <div className="rounded-lg border border-border/60 p-3">
                  <p className="mb-1 text-[10px] font-semibold uppercase tracking-[0.1em] text-subtle">
                    Fresh source view
                  </p>
                  <Info label="Final URL" value={truncateMiddle(report.resource.final_url, 90)} />
                  <Info label="Content type" value={report.resource.content_type ?? "—"} />
                  <Info label="Size" value={formatBytes(report.resource.total_bytes)} />
                  <Info label="Ranges" value={report.resource.accepts_ranges ? "supported" : "not supported"} />
                  <Info label="ETag" value={report.resource.etag ?? "—"} />
                  <Info label="Server" value={report.resource.server ?? "—"} />
                </div>
              ) : null}

              <div className="grid gap-3 sm:grid-cols-2">
                <div className="rounded-lg border border-border/60 p-3">
                  <p className="text-[10px] font-semibold uppercase tracking-[0.1em] text-subtle">
                    Recent events ({report.events.length})
                  </p>
                  <div className="mt-1.5 max-h-40 space-y-1 overflow-y-auto pr-1 scroll-thin">
                    {report.events.slice(-8).reverse().map((event) => (
                      <p key={event.id} className="text-[11px] text-muted">
                        <span className="text-fg">{event.to_state}</span>{" "}
                        <span className="text-subtle">{formatDate(event.created_at)}</span>
                      </p>
                    ))}
                    {report.events.length === 0 ? <p className="text-[11px] text-subtle">None</p> : null}
                  </div>
                </div>
                <div className="rounded-lg border border-border/60 p-3">
                  <p className="text-[10px] font-semibold uppercase tracking-[0.1em] text-subtle">
                    Integrity records ({report.integrity.length})
                  </p>
                  <div className="mt-1.5 max-h-40 space-y-1.5 overflow-y-auto pr-1 scroll-thin">
                    {report.integrity.map((record) => (
                      <div key={record.id} className="text-[11px]">
                        <p className="flex items-center justify-between gap-2">
                          <span className="text-fg">{record.algorithm.toUpperCase()}</span>
                          <Badge tone={record.status === "verified" ? "success" : "danger"}>
                            {record.status.replaceAll("_", " ")}
                          </Badge>
                        </p>
                        <p className="truncate font-mono text-[10px] text-subtle" title={record.actual ?? ""}>
                          {record.actual ?? "no digest"}
                        </p>
                      </div>
                    ))}
                    {report.integrity.length === 0 ? (
                      <p className="text-[11px] text-subtle">None</p>
                    ) : null}
                  </div>
                </div>
              </div>
            </div>
          )}
        </section>

        <section className="card p-5">
          <div className="mb-3 flex items-center gap-2">
            <Gauge className="h-4 w-4 text-accent" />
            <h2 className="text-sm font-semibold">Source benchmark</h2>
          </div>
          <p className="text-[11px] leading-relaxed text-muted">
            Measures DNS, TLS, time-to-first-byte and a short bounded transfer. Only the configured
            HTTP(S) source is contacted.
          </p>
          <div className="mt-3 space-y-2">
            <Input
              value={benchmarkUrl}
              onChange={(event) => setBenchmarkUrl(event.target.value)}
              placeholder="https://example.com/file.bin"
              spellCheck={false}
            />
            <Button variant="primary" className="w-full" onClick={() => void runBenchmark()} disabled={benchmarking}>
              <Timer className="h-3.5 w-3.5" /> {benchmarking ? "Benchmarking…" : "Run benchmark"}
            </Button>
          </div>

          {benchmark ? (
            <div className="mt-4 space-y-2">
              <div className="flex items-center justify-between">
                <Badge tone={benchmark.status >= 200 && benchmark.status < 400 ? "success" : "danger"}>
                  HTTP {benchmark.status}
                </Badge>
                <span className="font-mono text-[10px] text-subtle">
                  {truncateMiddle(benchmark.url, 44)}
                </span>
              </div>
              <div className="rounded-lg border border-border/60 px-3 py-1">
                <Info label="Headers" value={`${benchmark.connect_and_headers_ms.toFixed(1)} ms`} />
                <Info label="Download" value={`${benchmark.download_ms.toFixed(1)} ms`} />
                <Info label="Bytes" value={formatBytes(benchmark.bytes)} />
                <Info label="Throughput" value={formatSpeed(benchmark.throughput_bps)} />
                <Info
                  label="DNS"
                  value={benchmark.dns_ms === null ? "—" : `${benchmark.dns_ms.toFixed(1)} ms`}
                />
                <Info
                  label="TLS"
                  value={benchmark.tls_ms === null ? "—" : `${benchmark.tls_ms.toFixed(1)} ms`}
                />
              </div>
              <p className="text-[11px] leading-relaxed text-muted">{benchmark.note}</p>
            </div>
          ) : null}
        </section>
      </div>

      <section className="card mt-5 overflow-hidden">
        <div className="flex flex-wrap items-center justify-between gap-2 border-b border-border px-5 py-4">
          <div>
            <h2 className="text-sm font-semibold">Recent API audit</h2>
            <p className="text-[11px] text-muted">
              Authenticated requests recorded by the local middleware (newest first).
            </p>
          </div>
          <Badge tone="neutral">{audit.length} rows</Badge>
        </div>
        <div className="max-h-96 overflow-auto scroll-thin">
          <table className="w-full text-left text-xs">
            <thead className="sticky top-0 bg-surface text-[10px] uppercase tracking-[0.08em] text-subtle">
              <tr className="border-b border-border/60">
                <th className="px-5 py-2 font-medium">Method</th>
                <th className="px-3 py-2 font-medium">Path</th>
                <th className="px-3 py-2 text-right font-medium">Status</th>
                <th className="px-3 py-2 font-medium">Remote</th>
                <th className="px-5 py-2 text-right font-medium">Time</th>
              </tr>
            </thead>
            <tbody>
              {audit.length === 0 ? (
                <tr>
                  <td className="px-5 py-8 text-center text-muted" colSpan={5}>
                    No audit entries yet.
                  </td>
                </tr>
              ) : (
                audit.map((row, index) => (
                  <tr key={`${row.created_at}-${index}`} className="border-b border-border/40 last:border-0">
                    <td className="px-5 py-2 font-mono text-[11px]">{row.method}</td>
                    <td className="max-w-[420px] truncate px-3 py-2 font-mono text-[11px] text-muted" title={row.path}>
                      {row.path}
                    </td>
                    <td className="px-3 py-2 text-right">
                      <span className={row.status >= 400 ? "text-danger" : "text-success"}>{row.status}</span>
                    </td>
                    <td className="px-3 py-2 font-mono text-[10px] text-subtle">{row.remote ?? "—"}</td>
                    <td className="px-5 py-2 text-right text-[10px] text-subtle">{formatDate(row.created_at)}</td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>
      </section>
    </div>
  );
}
