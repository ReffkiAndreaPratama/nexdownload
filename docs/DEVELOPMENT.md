# Development guide

## Toolchain

- Rust **1.82+** (edition 2021); stable is recommended.
- Node.js **22.12+** and npm (Vitest 5 and the current Vite toolchain require it).
- Tauri 2 platform prerequisites:
  - Windows: Microsoft C++ Build Tools and WebView2.
  - Linux: `libwebkit2gtk-4.1-dev`, `libappindicator3-dev`, `librsvg2-dev`, `patchelf`,
    and the usual GTK/WebKit development packages.
  - macOS: Xcode command-line tools.
- Optional for live manual checks: curl or PowerShell; Python 3 for a local range
  server.

The workspace pins dependency ranges in `Cargo.lock` and
`apps/desktop/package-lock.json`. Commit lockfile changes with dependency changes.

## First setup

```powershell
npm --prefix apps/desktop install
npm --prefix apps/desktop run build
cargo check
```

`cargo check` builds the default workspace members. The Tauri desktop crate is checked
explicitly after the frontend exists:

```powershell
cargo check -p nexdownload-desktop
```

## Run the desktop app

```powershell
npm --prefix apps/desktop run tauri dev
```

This starts Vite on `127.0.0.1:1420` and the Tauri shell. The shell:

1. resolves the OS application data directory;
2. creates or reads `api-token`;
3. opens `nexdownload.db`;
4. starts the embedded API on `127.0.0.1:47821` (ephemeral port if busy);
5. exposes `api_config`, `open_path`, and `reveal_path` to the frontend.

The frontend discovers the actual base URL and token at startup. No token is hard-coded
in the bundle.

For a non-bundled local build:

```powershell
npm --prefix apps/desktop run tauri build -- --debug --no-bundle
```

The 0.1.0 debug build was launched on Windows during verification. The process opened a
window titled `NexDownload`, served `/api/v1/health` from its embedded API, and was then
stopped cleanly.

## Run the headless server

```powershell
npm --prefix apps/desktop run build
cargo run -p nexdownload-server -- --frontend apps/desktop/dist
```

Useful options:

```text
--host <addr>       default 127.0.0.1
--port <port>       default 47821
--data-dir <path>   database and token directory
--frontend <path>   built UI directory
--token <token>     token instead of api-token; minimum 16 characters
--trust-proxy       allow non-loopback bind behind a TLS proxy
```

Environment equivalents: `NEXDOWNLOAD_HOST`, `NEXDOWNLOAD_PORT`,
`NEXDOWNLOAD_DATA_DIR`, `NEXDOWNLOAD_FRONTEND`, `NEXDOWNLOAD_API_TOKEN`,
`NEXDOWNLOAD_TRUST_PROXY`.

The server prints the API URL, data directory, download directory, and token at startup.
If the frontend directory is absent it logs a warning and serves API-only.

## Test commands

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --workspace

npm --prefix apps/desktop run build
npm --prefix apps/desktop test
```

The Playwright smoke test uses a running server and is intentionally opt-in:

```powershell
$env:NEXDOWNLOAD_E2E_TOKEN = "<token>"
$env:NEXDOWNLOAD_E2E_URL = "http://127.0.0.1:47821"
$env:NEXDOWNLOAD_E2E_API = "http://127.0.0.1:47821"
npm --prefix apps/desktop run test:e2e:install
npm --prefix apps/desktop run test:e2e
```

It opens the built web client, authenticates through the token gate, checks the
dashboard, and navigates to Downloads. On Windows, `scripts/e2e.ps1` builds and starts an
isolated headless server, runs the browser test, and removes the temporary data
directory. CI starts the same kind of temporary server with a known token.

### Workspace tests and the desktop crate

`cargo test --workspace` includes the Tauri desktop crate. NexDownload ships desktop
targets only, so `apps/desktop/src-tauri/Cargo.toml` declares
`crate-type = ["rlib"]` instead of Tauri's mobile-oriented `staticlib`/`cdylib` set.
That also avoids the Windows gnullvm 65,535-export limit that could break the desktop
test DLL.

Run the full local suite after installing the Tauri platform prerequisites:

```powershell
cargo test --workspace
npm --prefix apps/desktop run tauri build -- --debug --no-bundle
```

CI keeps the jobs split for portability: the Ubuntu Rust job runs `cargo test` over the
default members without installing GTK/WebKit, while the Windows desktop job performs a
non-bundled Tauri debug build. Server, engine, and storage tests therefore never depend
on a GUI toolchain being present.

### Test coverage

- `download-core`: segment planning, state predicates, URL/filename validation,
  categories.
- `download-engine/tests/http_engine.rs`: real Axum HTTP server covering range
  downloads, redirects, no-range fallback, chunked/unknown length, resume from saved
  segments, changed ETag rejection, and plan consistency.
- `download-storage/tests/storage.rs`: migrations, task CRUD, filtering, settings,
  events, integrity rows, speed samples, queues/categories, audit, crash recovery, and
  empty/populated statistics queries.
- `download-api`: CSV escaping, status parsing, URL/path validation, constant-time token
  comparison.
- `download-scheduler`, `download-rules`, `download-integrity`: scheduling, matching,
  regex validation, hashing, size checks.
- Frontend Vitest: formatting helpers and the progress component.
- Playwright: token-gate authentication against a real running API, dashboard load, and
  route navigation in Chromium.
- Live acceptance during development: 401 without token, segmented transfer with
  matching SHA-256, pause/resume, and persistence across a server restart.

The desktop UI currently polls the REST API. There is no SSE/WebSocket stream yet, so
frontend behavior is tested against the API contract rather than a streaming transport.

## Live smoke test

1. Create a range-capable test file server. A small Python server is sufficient if it
   returns `Accept-Ranges: bytes`, `Content-Range`, and `206` for `Range` requests.
2. Start the headless server with a temporary data directory:

```powershell
$env:NEXDOWNLOAD_API_TOKEN = "development-token-0123456789"
cargo run -p nexdownload-server -- `
  --data-dir "$env:TEMP\nexdownload-dev" `
  --frontend apps/desktop/dist
```

3. Check authentication and queue a task:

```powershell
$base = "http://127.0.0.1:47821/api/v1"
$h = @{ Authorization = "Bearer $env:NEXDOWNLOAD_API_TOKEN" }

try { Invoke-WebRequest "$base/tasks" -ErrorAction Stop } catch {
  "expected 401: $([int]$_.Exception.Response.StatusCode)"
}

$body = @{
  url = "http://127.0.0.1:48100/test.bin"
  connections = 4
  confirmed = $true
} | ConvertTo-Json

$task = (Invoke-RestMethod "$base/tasks" -Method Post -Headers $h `
  -ContentType application/json -Body $body).task
Invoke-RestMethod "$base/tasks/$($task.id)" -Headers $h
```

For a pause/resume check, use a large slow test resource, pause after some progress,
wait for `paused`, then resume with `{"restart":false}`. Restart the server with the
same `--data-dir` and verify the task list still contains the completed task.

## Frontend conventions

- TypeScript strict mode and `noUnusedLocals`/`noUnusedParameters`.
- Tailwind utility classes and the local components in `src/components/ui`.
- Theme state and API bootstrap live in `src/store/app.ts`.
- API calls live in `src/lib/api.ts`; types in `src/lib/types.ts`.
- No fake progress: the UI displays byte counts and speeds reported by the API.
- `api_config` is used only when running under Tauri; the browser build uses the token
  gate and API URL.

The production build verifies both TypeScript projects and then runs Vite:

```powershell
npm --prefix apps/desktop run build
```

No `.js`/`.d.ts` files are emitted next to `vite.config.ts` or `tailwind.config.ts`.

## Browser extension

`apps/browser-extension` has no build step. Check syntax with:

```powershell
node --check apps/browser-extension/background.js
node --check apps/browser-extension/popup.js
node --check apps/browser-extension/options.js
```

Load the folder unpacked in a Chromium browser. Use the desktop app or headless server
as the API and paste the token into the extension options. See the extension README for
permissions and packaging.

## Database and data reset

The data directory contains:

- `nexdownload.db` — SQLite database (WAL files may appear beside it);
- `api-token` — generated API token.

To reset a development instance, stop the process and delete the entire data directory
or pass a different `--data-dir`. Do not delete data belonging to a real installation
without a backup.

## Logging

`download-telemetry::init` installs a `tracing` subscriber. Override filters with
`RUST_LOG`, for example:

```powershell
$env:RUST_LOG = "download_engine=debug,download_api=debug,info"
cargo run -p nexdownload-server -- --frontend apps/desktop/dist
```

Token and URL redaction helpers exist for diagnostics. Do not log raw tokens.

## Formatting and linting

`rustfmt.toml` configures Rust formatting. Clippy is run with `-D warnings` in CI.
Frontend formatting is not automated in Phase 1; keep the existing style (2-space
indentation, double quotes, semicolons).

## Release build

```powershell
npm --prefix apps/desktop run tauri build
cargo build --release -p nexdownload-server
```

Artifacts land under `target/release` and the Tauri bundle output. The release profile
uses thin LTO, one codegen unit, and symbol stripping. Verify both the desktop binary and
`nexdownloadd` against a fresh data directory before publishing.

On Windows, `src-tauri/tauri.windows.conf.json` bundles
`resources/WebView2Loader.dll` next to the executable. The gnullvm release build links
the dynamic WebView2 loader, so both the installer and the portable ZIP must contain this
DLL. After building, confirm that the installed directory contains
`nexdownload-desktop.exe` and `WebView2Loader.dll` before publishing.
