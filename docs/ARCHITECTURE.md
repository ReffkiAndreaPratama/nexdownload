# Architecture

This document describes the implemented Phase 1 architecture. It is intentionally
specific about what exists today; future work is tracked in [ROADMAP](ROADMAP.md).

## System shape

NexDownload has two hosts for the same application layer:

1. **Tauri desktop app** (`apps/desktop/src-tauri`) starts an embedded Axum API on
   loopback, creates or loads the API token, opens SQLite, and serves the React
   frontend from the Tauri bundle. The frontend receives `{ base_url, token }` through
   the `api_config` Tauri command.
2. **Headless server** (`apps/server`) starts the same `DownloadService` and API, and
   optionally serves a built web UI from `--frontend`.

Both hosts use the same crates. There is no duplicated download logic in the frontend.

```text
React UI (Tauri or browser)
        │  HTTP + bearer token
        ▼
download-api::api (Axum router)
        │
        ▼
download-api::service (DownloadService)
   ├── download-scheduler   queue admission and priority
   ├── download-rules       rule matching and confirmation
   ├── download-engine      native transfers
   ├── download-integrity   SHA-256 verification
   └── download-storage     SQLite persistence
```

## Workspace crates

| Crate | Responsibility |
| --- | --- |
| `download-core` | `DownloadTask`, `DownloadStatus`, `DownloadSegment`, `SegmentPlan`, URL and filename validation, categories. |
| `download-engine` | `DownloadEngine` trait, `NativeHttpEngine`, resource inspection, segmented transfers, adaptive connection controller, bandwidth limiter, cancellation. |
| `download-storage` | SQLite pool, migrations, task CRUD, queue/category/collection/rule metadata, settings, task events, integrity history, speed samples, audit log, analytics. |
| `download-integrity` | Streaming SHA-256 and expected-checksum/size verification. |
| `download-rules` | Deterministic conditions and actions, validation, matching. |
| `download-scheduler` | Priority ordering and concurrency decisions. |
| `download-api` | Service facade, scheduling loop, pause/resume/retry/cancel, integrity orchestration, Axum routes, auth, rate limiting, CORS, audit middleware. |
| `download-telemetry` | `tracing` initialization and URL/credential redaction helpers. |
| `nexdownload-desktop` | Tauri commands and embedded API lifecycle. |
| `nexdownload-server` | Headless CLI, data directory, token file, graceful shutdown. |

## Domain model and state machine

A task has a URL, final URL, local destination, category, optional queue and collection,
priority, connection count, byte counters, resource metadata (ETag, Last-Modified,
range support, content type), integrity fields, and a JSON segment plan.

Inclusive segment ranges use HTTP semantics:

```text
segment { start, end, downloaded }
len = end - start + 1
remaining = len - downloaded
```

The status values are:

```text
queued ─► preparing ─► connecting ─► downloading ─► verifying ─► completed
   │          │             │             │
   │          │             │             └──► pausing ─► paused
   │          │             └──► waiting_for_network
   │          └──► failed
   └──► cancelled
```

`paused`, `failed`, and `cancelled` are startable states. Retry can either continue from
saved segments or restart from byte zero. Finished states are `completed`, `failed`, and
`cancelled`; only active states occupy an engine slot.

## Segment planning

`SegmentPlan::build(total, connections)` creates contiguous, non-overlapping ranges that
cover the whole resource. The implementation:

- clamps connections to `1..=32`;
- requires at least `512 KiB` per segment;
- keeps small resources on one connection;
- rejects zero-length resources and validates the resulting plan.

The plan is persisted with the task. The total downloaded byte count is recomputed from
the segments, not trusted from a separate counter.

## Native engine flow

`NativeHttpEngine` uses `reqwest` with rustls, identity encoding, a 10-hop redirect
limit, keep-alive, and configurable read timeouts. The transfer path is:

1. **Inspect.** Send `HEAD`; if the server does not provide enough information, probe
   with a `GET` carrying `Range: bytes=0-0`. Record final URL, size, range support,
   ETag, Last-Modified, and content type.
2. **Resume validation.** If saved segments exist, compare saved total size, ETag, and
   Last-Modified with the fresh response. A mismatch returns `ResourceChanged` instead
   of writing mixed data.
3. **Plan.** Use `SegmentPlan` for range-capable resources; use a single stream for
   servers without range support or with unknown length.
4. **Prepare the file.** Create the destination and set its length. Each worker seeks to
   its own segment offset and writes only inside that segment.
5. **Transfer.** Range workers request `bytes=<offset>-<end>`, validate the returned
   `Content-Range` start, and stop at the segment boundary. If a server unexpectedly
   ignores ranges on a resumed request, the engine fails with `ResourceChanged` rather
   than corrupting the file.
6. **Progress.** Workers update atomic counters and send throttled `ProgressSnapshot`
   messages. The service persists checkpoints roughly every 900 ms.
7. **Finish.** The service verifies size and SHA-256 when configured, records an
   integrity row, and moves the task to `completed`.

Pause and cancellation are cooperative `CancellationToken`s. Workers flush and return a
paused/cancelled outcome; the service stores the final segment checkpoint and updates
the task state. In-flight cancellation is bounded by the network read timeout.

Adaptive mode can lower the number of active workers when throughput stalls. A shared
token-bucket limiter enforces an optional global bandwidth cap across workers.

## Crash recovery

On startup, `DownloadService::initialize` finds tasks that were active when the process
stopped. Disk writes may not have been flushed at the exact instant of a crash, so the
service rewinds each recovered segment by a conservative **1 MiB** (clamped to zero),
saves the rewound checkpoint, marks the task resumable, and schedules it again if auto
resume is enabled. Rewinding can re-download a small amount of data; it cannot leave a
checkpoint pointing past durable file contents.

SQLite uses WAL mode, normal synchronous durability, foreign keys, and a busy timeout.
Task progress and the segment array are updated in one row write, which keeps the
checkpoint atomic.

## Persistence schema

`crates/download-storage/migrations/0001_init.sql` creates:

| Table | Purpose |
| --- | --- |
| `tasks` | Task fields plus `segments_json`; indexes for status, category, queue, collection, checksum, and progress. |
| `task_events` | State transition history per task. |
| `integrity_checks` | Expected/actual SHA-256, size result, timestamp. |
| `queues` | Priority, per-queue concurrency, active/paused status. |
| `categories` | Extension list JSON and destination override. |
| `collections` | Optional grouping metadata. |
| `download_rules` | Conditions/actions JSON, enabled flag, priority. |
| `settings` | Validated key/value JSON settings. |
| `speed_samples` | Speed history pruned to 30 days. |
| `api_audit` | Method, path, status, remote address, timestamp. |

Times are RFC 3339 UTC strings. The database file is `nexdownload.db` in the data
directory; the API token is a separate `api-token` file.

## Application service and scheduling

`DownloadService` is the only writer of task lifecycle state. It owns:

- a `Storage` handle;
- a `DownloadEngine` trait object (native engine by default);
- a map of running task controls (pause/cancel tokens and abort handles);
- a scheduling mutex;
- a broadcast channel of service events.

`add_task` validates and normalizes the URL, runs rule matching, requires explicit
confirmation for confirming rules, computes the destination and unique filename,
classifies the category, persists the task, and asks the scheduler for a slot. The
scheduler respects the global `max_concurrent_downloads`, queue priority, queue
concurrency, per-task priority, and paused queues.

The service also provides stats, analytics, duplicate groups, history CSV, per-task
diagnostics, settings validation, integrity verification, shutdown, and startup
recovery.

## Rules

Rules are deterministic and local. A rule has conditions (URL pattern, file extension,
MIME type, source domain, and size where available) and actions (category, destination,
queue, priority, connections, and whether confirmation is required). Conditions and
actions are validated before storage. A matching confirmation rule causes `add_task` to
return a conflict with code `confirmation_required`; the client must resend with
`"confirmed": true`. The desktop UI and extension both implement this flow.

## Integrity and duplicates

`download-integrity` computes SHA-256 by streaming the completed file. The result is
compared with an optional expected checksum and with the known resource size. A mismatch
marks the task failed and records the report. Duplicate detection groups completed tasks
by identical non-null SHA-256 values; it is exposed through the API and UI.

## REST API and security

The Axum router is mounted at `/api/v1`:

- `/health` is public.
- Every other route is wrapped by authorization, rate limiting, and audit middleware.
- Authorization accepts `Authorization: Bearer <token>` or `x-api-token`.
- Token comparison is constant-time.
- A fixed-window rate limiter defaults to 900 requests per minute.
- Audit rows are written asynchronously after each protected request.
- CORS allows `http://localhost:*`, `http://127.0.0.1:*`, `chrome-extension://*`,
  `moz-extension://*`, and Tauri origins.
- The headless server refuses a non-loopback bind unless
  `NEXDOWNLOAD_TRUST_PROXY=1`; the operator is responsible for TLS termination.

The Tauri window CSP restricts scripts and styles to the app itself and permits API
connections only to loopback HTTP/WebSocket origins.

## Frontend

The React UI uses Vite, TypeScript, Tailwind, Zustand, and small local shadcn-style
components. It polls the REST API (no SSE in Phase 1), stores the token under
`nex.token`, and uses Tauri commands only for configuration discovery and opening or
revealing files. The browser build can be used against the headless server by supplying
the token through the API token gate.

## Extensibility

`DownloadEngine` is an async trait with inspect and run operations plus a cancellation
and progress context. Adding another backend means implementing the trait and injecting
it into `DownloadService`; the task model, storage, scheduler, API, and UI do not need
backend-specific code. The default distribution deliberately does not bundle aria2 or
any GPL component.
