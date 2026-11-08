# Changelog

All notable changes to NexDownload are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project aims to follow
[Semantic Versioning](https://semver.org/).

## [0.1.0] - 2026-11-07

### Added

- Native Rust HTTP/HTTPS engine with range detection, segmented transfers, redirects,
  single-stream fallback, unknown-length streaming, adaptive connections, and bandwidth
  limiting.
- Task lifecycle with pause, resume, retry, cancel, cooperative cancellation, and
  per-segment checkpoints.
- SQLite persistence with WAL mode, migrations, task events, queues, categories,
  collections, rules, settings, integrity history, speed samples, and audit records.
- Crash recovery that rewinds recovered segment checkpoints by up to 1 MiB to avoid
  claiming non-durable bytes.
- Resource identity validation using size, ETag, and Last-Modified on resume.
- SHA-256 verification, expected checksums, size checks, and duplicate detection.
- Scheduler with global concurrency, queue priority, queue concurrency, and task
  priority.
- Deterministic rules for extensions, domains, filename/URL regex, size, and content
  type, with category, destination, queue, priority, connection, and confirmation
  actions.
- Authenticated Axum REST API with constant-time token comparison, rate limiting, CORS
  allow-list, audit logging, health endpoint, diagnostics, benchmark, history export,
  and settings validation.
- Tauri 2 desktop shell with embedded API, token discovery, file open/reveal commands,
  restrictive CSP, and React UI for downloads, queues, categories, collections, rules,
  history, analytics, diagnostics, and settings.
- `nexdownloadd` headless server with CLI/environment configuration, loopback default,
  TLS-proxy opt-in for non-loopback binds, and graceful shutdown.
- Manifest V3 browser extension with browser-download interception, context-menu action,
  popup dashboard, options page, token storage, and confirmation-aware task creation.
- Rust tests for domain, engine, storage, scheduler, rules, integrity, API helpers, and
  telemetry; Vitest tests for frontend helpers and progress UI.
- Playwright browser smoke test that authenticates through the token gate, loads the
  dashboard, and navigates the web client against a real headless server.
- Documentation for architecture, API, development, security, contributing, roadmap,
  third-party notices, and licensing.
- GitHub Actions CI for Rust formatting, clippy, portable tests, frontend build/tests,
  Playwright browser tests, and a non-bundled Tauri desktop debug build.

### Fixed

- Resume casts and segment sizing in the HTTP engine integration tests.
- Tauri build artifact and frontend-dist ordering.
- Generated TypeScript configuration files and Tauri ACL schemas leaking into the source
  tree.
- Custom API base URLs were persisted but not restored after a page reload.
- Fresh databases returned HTTP 500 from stats/analytics because empty SQLite
  aggregates were typed as integers instead of floats. Added a regression test for empty
  and populated statistics.
- Clippy findings in the engine, storage, and service layers.
- Desktop crate now builds as `rlib` for the desktop-only distribution. This removes the
  mobile cdylib/staticlib outputs, allows `cargo test --workspace` and non-bundled Tauri
  debug builds on Windows gnullvm, and keeps the desktop test binary under the portable
  linker export limit.

### Security

- API tokens are generated from UUID entropy, written to a separate file, and compared
  in constant time.
- Non-loopback binding requires `NEXDOWNLOAD_TRUST_PROXY=1` and an external TLS
  terminator.
- The Tauri Content Security Policy no longer uses `csp: null`.
- Browser extension host permissions are limited by default and requested explicitly for
  custom API hosts.
- Updated React Router and Vitest to patched releases; `npm audit` reports zero known
  vulnerabilities for the frontend dependency graph.
