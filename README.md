# NexDownload

**An independent, local-first download manager with a native segmented HTTP engine,
an authenticated local API, and a Tauri desktop UI.**

[![CI](https://github.com/ReffkiAndreaPratama/nexdownload/actions/workflows/ci.yml/badge.svg)](https://github.com/ReffkiAndreaPratama/nexdownload/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-4f8cff.svg)](LICENSE)

NexDownload is an original MIT-licensed project. It is not affiliated with, endorsed
by, or derived from Internet Download Manager, AB Download Manager, aria2, or any other
download manager. See [NOTICE](NOTICE) and [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

## Why NexDownload

- **Native engine, not a wrapper.** Rust/Tokio performs HTTP/HTTPS transfers directly
  with range detection, parallel segments, redirects, resumable checkpoints, and
  integrity verification.
- **Local-first by design.** The desktop app and headless server bind to loopback by
  default. Every protected API endpoint requires a bearer token.
- **Crash-aware persistence.** Segment checkpoints and task state are stored atomically
  in SQLite, so a restart does not lose queued work.
- **Real workflow features.** Queues, categories, deterministic rules, collections,
  pause/resume/retry, history export, duplicate detection, analytics, diagnostics, and
  an audit log are implemented, not mocked.
- **Extensible.** The engine is behind a trait, so an aria2 adapter or another backend
  can be added without changing the domain model or UI.

## Status

Version **0.1.0** is the first Phase 1 release. The native HTTP/HTTPS path is working and
covered by unit, storage, API, and live network tests. The desktop shell, browser
extension, documentation, and CI are part of this repository.

Not implemented yet: BitTorrent/magnet links, FTP/SFTP, HLS/DASH, media-site
extraction, scheduled time windows, and cloud sync. See [ROADMAP](docs/ROADMAP.md).

## Features

| Area | What works |
| --- | --- |
| Transfers | Segmented HTTP/HTTPS downloads, range detection, redirects, single-stream fallback, unknown-length streaming, adaptive connections, bandwidth limiting, retry with backoff |
| Lifecycle | Queued, preparing, connecting, downloading, pausing, paused, verifying, completed, failed, cancelled, waiting-for-network states |
| Resume | Per-segment checkpoints persisted to SQLite; size, ETag, and Last-Modified checks reject changed resources |
| Integrity | SHA-256 verification, expected checksum support, size checks, integrity history |
| Organization | Queues with priority/concurrency, categories by extension and destination, collections, smart rules with confirmation, history, duplicate groups |
| Observability | Speed/ETA, progress events, analytics, per-task diagnostics, structured logs, API audit log, CSV history export |
| Interfaces | Tauri desktop app, React UI, authenticated REST API, Manifest V3 browser extension |
| Verification | Rust unit/integration tests, Vitest component/helper tests, Playwright browser smoke test, CI desktop check |
| Security | Bearer-token authentication, constant-time token comparison, request rate limiting, CORS allow-list, loopback default, explicit TLS-proxy opt-in for LAN access |

## Architecture at a glance

```text
┌──────────────────────────────┐      ┌──────────────────────────────┐
│ Tauri desktop shell          │      │ nexdownloadd headless server │
│  React UI + embedded API     │      │  Axum API + SQLite           │
└──────────────┬───────────────┘      └──────────────┬───────────────┘
               │                                     │
               └───────────────┬─────────────────────┘
                               ▼
                    ┌─────────────────────┐
                    │ download-api        │
                    │ service + REST      │
                    └──────────┬──────────┘
          ┌────────────────────┼────────────────────┐
          ▼                    ▼                    ▼
   ┌──────────────┐     ┌──────────────┐     ┌──────────────┐
   │ core model   │     │ engine       │     │ storage      │
   │ + rules      │     │ HTTP/HTTPS   │     │ SQLite       │
   │ + scheduler  │     │ + adaptive   │     │ + analytics  │
   └──────────────┘     └──────────────┘     └──────────────┘
```

See [ARCHITECTURE](docs/ARCHITECTURE.md) for the state machine, persistence layout,
engine internals, and security model.

## Quick start

### Desktop app

Prerequisites: Rust 1.82+, Node.js 22.12+, npm, and the platform dependencies required
by Tauri 2 (WebView2 on Windows; WebKitGTK/GTK development packages on Linux; Xcode
command-line tools on macOS).

```powershell
npm --prefix apps/desktop install
npm --prefix apps/desktop run tauri dev
```

The desktop shell starts an embedded API on `127.0.0.1:47821` (or an ephemeral port if
that port is busy), generates an API token on first launch, and stores it in the
application data directory. The UI discovers the URL and token through Tauri commands.

### Headless server and web UI

```powershell
npm --prefix apps/desktop install
npm --prefix apps/desktop run build
cargo run -p nexdownload-server -- --frontend apps/desktop/dist
```

The server prints the UI URL, data directory, download directory, and API token. To set
them explicitly:

```powershell
cargo run -p nexdownload-server -- `
  --host 127.0.0.1 `
  --port 47821 `
  --data-dir .\data `
  --frontend .\apps\desktop\dist
```

Environment variables are also supported: `NEXDOWNLOAD_HOST`, `NEXDOWNLOAD_PORT`,
`NEXDOWNLOAD_DATA_DIR`, `NEXDOWNLOAD_FRONTEND`, `NEXDOWNLOAD_API_TOKEN`, and
`NEXDOWNLOAD_TRUST_PROXY`.

### Browser extension

Build/install the desktop app, copy the API token from **Settings → Local API**, then
load `apps/browser-extension` as an unpacked extension in `chrome://extensions`. See
the [extension README](apps/browser-extension/README.md).

## API in 30 seconds

Every endpoint except `/api/v1/health` requires one of:

```http
Authorization: Bearer <token>
x-api-token: <token>
```

Examples:

```powershell
$headers = @{ Authorization = "Bearer $env:NEXDOWNLOAD_API_TOKEN" }
Invoke-RestMethod http://127.0.0.1:47821/api/v1/health
Invoke-RestMethod http://127.0.0.1:47821/api/v1/tasks -Headers $headers
Invoke-RestMethod http://127.0.0.1:47821/api/v1/tasks -Method Post -Headers $headers `
  -ContentType application/json `
  -Body '{"url":"https://example.com/file.zip","connections":4}'
```

The complete route table, payloads, confirmation flow, and error format are in
[API](docs/API.md).

## Security model

- The API binds to `127.0.0.1` by default.
- Protected routes authenticate with a 64-character token generated with UUID entropy
  and compared in constant time.
- The token is written to `<data-dir>/api-token` with `0600` permissions on Unix.
- Requests are rate-limited and recorded in an audit table.
- Binding a non-loopback address is refused unless `NEXDOWNLOAD_TRUST_PROXY=1` is set.
  That flag is an assertion that a trusted TLS reverse proxy terminates HTTPS; it does
  not add TLS itself.
- CORS is limited to localhost, Tauri, and browser-extension origins.
- The Tauri window uses a restrictive Content Security Policy.

See [SECURITY](SECURITY.md) for reporting and operational guidance.

## Development

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
npm --prefix apps/desktop run build
npm --prefix apps/desktop test
cargo check -p nexdownload-desktop
```

The live Playwright smoke test is opt-in because it needs a running server and token:

```powershell
$env:NEXDOWNLOAD_E2E_TOKEN = "<token>"
$env:NEXDOWNLOAD_E2E_URL = "http://127.0.0.1:47821"
npm --prefix apps/desktop run test:e2e
```

`cargo test` runs the default workspace members (core, engine, storage, API, headless
server). The Tauri desktop crate is checked separately because its test binary links a
GUI shell. On some minimal Windows toolchains the desktop test DLL can exceed the
portable linker export limit; `cargo check -p nexdownload-desktop` is the supported
verification path there.

See [DEVELOPMENT](docs/DEVELOPMENT.md) for platform setup, test design, and release
builds.

## Repository layout

```text
apps/
  desktop/            React + Vite + Tailwind UI and Tauri 2 shell
  server/             nexdownloadd headless server
  browser-extension/  Manifest V3 browser integration
crates/
  download-core/      Domain model, state machine, validation, segment plan
  download-engine/    Native HTTP/HTTPS segmented engine and adaptive limiter
  download-storage/   SQLite persistence, migrations, analytics, audit
  download-integrity/ SHA-256 and file verification helpers
  download-rules/     Deterministic URL/file rules and validation
  download-scheduler/ Priority and concurrency scheduling
  download-api/       Service facade, queue orchestration, Axum REST API
  download-telemetry/ Structured logging and redaction
docs/                 Architecture, API, development, roadmap
```

## Documentation

- [Architecture](docs/ARCHITECTURE.md)
- [API reference](docs/API.md)
- [Development guide](docs/DEVELOPMENT.md)
- [Roadmap](docs/ROADMAP.md)
- [Changelog](CHANGELOG.md)
- [Contributing](CONTRIBUTING.md)
- [Code of conduct](CODE_OF_CONDUCT.md)

## License

MIT. See [LICENSE](LICENSE).
