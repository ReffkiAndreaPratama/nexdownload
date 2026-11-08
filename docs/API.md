# Local API

The API is served by both the desktop shell and `nexdownloadd`.

- Default base URL: `http://127.0.0.1:47821`
- Prefix: `/api/v1`
- Desktop binds to `127.0.0.1:47821`, or an ephemeral loopback port if that port is
  occupied. The UI receives the actual URL from the Tauri `api_config` command.
- Content type: `application/json` unless noted otherwise.
- `/api/v1/health` is public. **Every other route requires authentication.**
- Rate limit: 900 requests per minute per client by default.
- Every protected request is recorded in the audit log.

## Authentication

Use either header:

```http
Authorization: Bearer <token>
```

```http
x-api-token: <token>
```

The token is generated on first launch and stored in the application data directory as
`api-token`. It can be supplied explicitly with `NEXDOWNLOAD_API_TOKEN` or
`nexdownloadd --token <token>` (minimum 16 characters). Desktop Settings can reveal and
copy the current token.

An unauthenticated protected request returns HTTP `401`:

```json
{
  "code": "unauthorized",
  "message": "missing or invalid API token",
  "details": null
}
```

## Error format

All API errors use the same shape:

```json
{
  "code": "validation_error",
  "message": "connections must be between 1 and 32",
  "details": null
}
```

Common codes: `validation_error` (400), `not_found` (404), `conflict` (409),
`confirmation_required` (409), `unauthorized` (401), `rate_limited` (429),
`storage_error` / `io_error` (500), and engine-related codes such as
`http_error`, `timeout`, `dns`, `tls`, or `resource_changed` (usually 502).

## Health

### `GET /api/v1/health`

Public.

```json
{
  "status": "ok",
  "name": "NexDownload",
  "version": "0.1.0",
  "uptime_secs": 42,
  "time": "2026-11-07T08:00:00+00:00"
}
```

## Tasks

### `GET /api/v1/tasks`

Returns tasks, newest first by the storage query order. Query parameters (all optional):

| Parameter | Meaning |
| --- | --- |
| `status` | Comma-separated statuses, e.g. `queued,downloading,paused`. |
| `category` | Exact category id, e.g. `videos`. |
| `queue_id` | Exact queue id. |
| `collection_id` | Exact collection id. |
| `search` | Case-insensitive search over task fields supported by storage. |
| `limit` | Maximum rows, default `500`. |
| `offset` | Row offset, default `0`. |

### `POST /api/v1/tasks`

Creates and queues a task. The server inspects the resource before persisting it, so
this call can take as long as a HEAD/range probe.

```json
{
  "url": "https://example.com/archive.zip",
  "file_name": "archive.zip",
  "directory": "C:\\Users\\me\\Downloads",
  "category": "archives",
  "queue_id": "default",
  "collection_id": null,
  "connections": 4,
  "priority": 0,
  "expected_checksum": "e3b0c44298fc1c149afbf4c8996fb924...",
  "confirmed": false
}
```

Every field except `url` is optional. `connections` is clamped to the supported range
(1–32); the default comes from settings. `expected_checksum` must be a SHA-256 hex
digest. `directory` must be a valid destination accepted by the domain layer.

Response: HTTP `201`

```json
{
  "task": {
    "id": "1f0f6f9d-...",
    "url": "https://example.com/archive.zip",
    "final_url": "https://cdn.example.com/archive.zip",
    "file_name": "archive.zip",
    "destination_dir": "C:\\Users\\me\\Downloads",
    "file_path": "C:\\Users\\me\\Downloads\\archive.zip",
    "category": "archives",
    "queue_id": "default",
    "collection_id": null,
    "status": "connecting",
    "priority": 0,
    "total_bytes": 10485760,
    "downloaded_bytes": 0,
    "connections": 4,
    "speed_bps": 0.0,
    "error": null,
    "error_kind": null,
    "etag": "\"abc123\"",
    "last_modified": null,
    "accepts_ranges": true,
    "content_type": "application/zip",
    "checksum_sha256": null,
    "expected_checksum": null,
    "source_domain": "cdn.example.com",
    "created_at": "2026-11-07T08:00:00+00:00",
    "updated_at": "2026-11-07T08:00:00+00:00",
    "started_at": null,
    "completed_at": null,
    "segments": []
  },
  "duplicate_of": null,
  "rule_applied": null
}
```

`duplicate_of` is set when the same resource is already known. `rule_applied` names the
rule that changed the request.

#### Rule confirmation

If a matching rule requires confirmation, the server returns HTTP `409` and does not
create the task:

```json
{
  "code": "confirmation_required",
  "message": "rule 'ask-before-video' requires confirmation before this download can be added",
  "details": { "rule": "ask-before-video" }
}
```

Repeat the request with `"confirmed": true` to accept the rule action. The desktop UI and
browser extension implement this flow. Do not set `confirmed: true` automatically.

### `GET /api/v1/tasks/{id}`

Returns one `DownloadTask`, or `404`.

### `PATCH /api/v1/tasks/{id}`

Updates organization fields. Omitted fields are left unchanged; this does not change a
running transfer's byte position.

```json
{
  "category": "videos",
  "queue_id": "night",
  "collection_id": "tutorial-series",
  "priority": 5,
  "connections": 8
}
```

### `DELETE /api/v1/tasks/{id}?delete_file=true`

Deletes the task. `delete_file` defaults to `false`; when `true`, the completed file is
also removed. Returns HTTP `204`.

### Lifecycle actions

| Method and path | Body | Result |
| --- | --- | --- |
| `POST /api/v1/tasks/{id}/pause` | none | Returns the task, initially `pausing`; workers stop cooperatively and the task becomes `paused`. |
| `POST /api/v1/tasks/{id}/resume` | `{"restart":false}` | Resumes from saved segments. `restart:true` discards the checkpoint. Returns the task. |
| `POST /api/v1/tasks/{id}/retry` | `{"restart":false}` | Retries a failed/cancelled task. Same restart semantics. |
| `POST /api/v1/tasks/{id}/cancel` | none | Cancels and returns the task. |

For the JSON actions, send `Content-Type: application/json` and a body such as `{}` to
avoid extractor ambiguity. The desktop client always sends an explicit body.

### Per-task inspection

| Method and path | Result |
| --- | --- |
| `GET /api/v1/tasks/{id}/events` | State-transition event history. |
| `GET /api/v1/tasks/{id}/integrity` | Integrity-check history. |
| `POST /api/v1/tasks/{id}/integrity` | Re-hash the completed file now and return the report. |
| `GET /api/v1/tasks/{id}/diagnostics` | Live source probe plus error hints, events, integrity rows, and suggested actions. |

Diagnostics performs network I/O and can be slow for an unreachable source.

## Statistics and analytics

### `GET /api/v1/stats`

Aggregate counts, downloaded/known bytes, current/peak/average speed, success rate, and
today's completed count. The exact shape is the `Stats` structure in
`crates/download-storage/src/stats.rs`.

### `GET /api/v1/analytics`

Returns stats, category totals, 14-day daily history, queue totals, and 24-hour speed
samples.

### `GET /api/v1/duplicates`

Groups completed tasks by an identical non-empty SHA-256 checksum:

```json
[
  {
    "checksum": "8f14e45f...",
    "tasks": [ { "id": "..." }, { "id": "..." } ]
  }
]
```

### `POST /api/v1/benchmark`

Downloads up to 2 MiB from a URL with a range request and measures connection/header and
transfer timing.

```json
{ "url": "https://example.com/large.bin" }
```

Response fields: `url`, `status`, `connect_and_headers_ms`, `download_ms`, `bytes`,
`throughput_bps`, `dns_ms`, `tls_ms`, and `note`.

## History

### `GET /api/v1/history`

Returns completed, failed, and cancelled tasks. Supports `search`, `limit`, and `offset`.

### `GET /api/v1/history/export`

Returns `text/csv` as an attachment named `nexdownload-history.csv`. Fields are escaped
for CSV, including quotes and commas.

## Categories

### `GET /api/v1/categories`

Returns built-in and user categories. A category has `id`, `name`, `extensions`, and
optional `destination_dir`.

### `POST /api/v1/categories`

Create or update a category. If `id` is omitted it is derived from the name; ids must be
lowercase ASCII letters, digits, `-`, or `_`.

```json
{
  "id": "images",
  "name": "Images",
  "extensions": ["png", "jpg", "jpeg", "webp"],
  "destination_dir": "C:\\Users\\me\\Pictures"
}
```

### `DELETE /api/v1/categories/{id}`

Deletes a category. Built-in behavior is controlled by the service/storage layer.

## Queues

### `GET /api/v1/queues`

Returns queues with id, name, priority, concurrency limit, and active/paused status.

### `POST /api/v1/queues`

```json
{ "name": "Night queue", "max_concurrent": 2 }
```

### `PATCH /api/v1/queues/{id}`

Accepts a full queue object; the path id overrides the body id. Fields include `name`,
`priority`, `max_concurrent`, and `status` (`active` or `paused`).

### `DELETE /api/v1/queues/{id}`

Deletes a queue. Tasks referencing it are handled by the storage foreign-key policy.

### `POST /api/v1/queues/{id}/pause` and `POST /api/v1/queues/{id}/start`

Pause or resume admission for a queue. Both return HTTP `204`.

## Collections

### `GET /api/v1/collections`

Returns collections.

### `POST /api/v1/collections`

```json
{
  "name": "Tutorial series",
  "description": "Downloaded course",
  "color": "#4f8cff"
}
```

### `DELETE /api/v1/collections/{id}`

Returns `204`, or `404` if it does not exist.

## Rules

Rules are evaluated in descending priority, then stable id. Every configured condition
must match; within a list, any entry may match. The first matching rule wins.

Conditions:

```json
{
  "extensions": ["mp4", "mkv"],
  "domains": ["example.com"],
  "filename_regex": "(?i)lecture-\\d+",
  "min_size": 1048576,
  "max_size": null,
  "content_types": ["video/"],
  "url_regex": null
}
```

Actions:

```json
{
  "category": "videos",
  "destination_dir": "D:\\Media",
  "queue_id": "night",
  "priority": 10,
  "connections": 8,
  "require_confirmation": true
}
```

### `GET /api/v1/rules`

Returns stored rules.

### `POST /api/v1/rules`

Creates or updates a rule. The server validates regexes, name, and size bounds.

```json
{
  "id": "ask-before-video",
  "name": "Ask before video",
  "enabled": true,
  "priority": 100,
  "conditions": { "extensions": ["mp4", "mkv"] },
  "actions": { "category": "videos", "require_confirmation": true }
}
```

### `DELETE /api/v1/rules/{id}`

Returns `204`.

## Settings

### `GET /api/v1/settings`

Returns the settings map.

### `PUT /api/v1/settings`

Accepts a JSON object containing only the keys to change. Unknown keys are persisted but
known keys are validated:

| Key | Type | Validation |
| --- | --- | --- |
| `theme` | string | `dark`, `light`, or `black` (UI preference). |
| `compact_rows` | boolean | UI preference. |
| `max_concurrent_downloads` | number | 1–16. |
| `default_connections` | number | 1–32. |
| `default_destination` | string | Destination path validated by the domain layer. |
| `bandwidth_limit_bps` | number | `0` means unlimited; positive value is bytes/second. |
| `adaptive_connections` | boolean | Enable adaptive worker count. |
| `auto_resume` | boolean | Resume recoverable tasks on startup. |
| `verify_integrity` | boolean | Verify SHA-256 after completion. |
| `notifications` | boolean | Desktop notification preference. |
| `intercept_downloads` | boolean | Browser-extension preference exposed to clients. |
| `lan_access` | boolean | UI/API policy preference; binding still requires TLS proxy opt-in. |
| `retry_limit` | number | 0–20. |
| `timeout_secs` | number | 5–600. |

Example:

```json
{
  "max_concurrent_downloads": 4,
  "default_connections": 8,
  "bandwidth_limit_bps": 0,
  "auto_resume": true,
  "verify_integrity": true
}
```

## Audit and shutdown

### `GET /api/v1/audit`

Returns the 100 most recent audit rows. Each row includes method, path, HTTP status,
remote address, and timestamp.

### `POST /api/v1/shutdown`

Requests a graceful service shutdown and process exit after a short delay. This endpoint
is protected but should still be treated as privileged.

## Client examples

### PowerShell

```powershell
$base = "http://127.0.0.1:47821/api/v1"
$headers = @{ Authorization = "Bearer $env:NEXDOWNLOAD_API_TOKEN" }

Invoke-RestMethod "$base/health"
Invoke-RestMethod "$base/tasks?status=downloading,paused" -Headers $headers

$body = @{
  url = "https://example.com/file.iso"
  connections = 8
} | ConvertTo-Json

$created = Invoke-RestMethod "$base/tasks" -Method Post -Headers $headers `
  -ContentType "application/json" -Body $body
$created.task.id
```

### JavaScript

```js
const base = "http://127.0.0.1:47821/api/v1";
const headers = {
  Authorization: `Bearer ${token}`,
  "Content-Type": "application/json",
};

const response = await fetch(`${base}/tasks`, {
  method: "POST",
  headers,
  body: JSON.stringify({ url: "https://example.com/file.zip", connections: 4 }),
});
if (!response.ok) throw new Error(await response.text());
console.log(await response.json());
```
