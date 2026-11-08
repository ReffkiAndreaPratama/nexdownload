# Roadmap

This roadmap separates shipped Phase 1 behavior from planned work. Planned items are not
available in the current release.

## Phase 1 — shipped (0.1.0)

- [x] Original Rust workspace with domain, engine, storage, integrity, rules, scheduler,
  API, telemetry, desktop, and server crates.
- [x] Native HTTP/HTTPS segmented downloads with range detection, redirects, fallback for
  servers without ranges, and unknown-length streaming.
- [x] Pause, resume, retry, cancel, checkpoint persistence, and 1 MiB crash-recovery
  rewind.
- [x] Resource identity checks using total size, ETag, and Last-Modified.
- [x] Adaptive connection control and global bandwidth limiting.
- [x] SQLite persistence with migrations; queues, categories, collections, settings,
  task events, integrity records, speed samples, and audit rows.
- [x] Smart rules with confirmation flow.
- [x] SHA-256 integrity verification, expected checksums, and duplicate groups.
- [x] Authenticated loopback REST API with constant-time token comparison, request rate
  limiting, CORS allow-list, and audit logging.
- [x] Tauri 2 desktop shell and React UI with dashboard, downloads, queues, categories,
  collections, rules, history, analytics, diagnostics, and settings.
- [x] Manifest V3 browser extension with download interception, context-menu action,
  popup, options, and token storage.
- [x] Rust and frontend test suites, Playwright browser smoke test, documentation,
  legal files, and CI.

## Phase 2 — next

- [ ] Server-sent events or WebSocket progress stream to replace frontend polling.
- [ ] Scheduled queue windows and per-queue bandwidth policies.
- [ ] Resumable checksum algorithms beyond SHA-256 (SHA-1, MD5, BLAKE3) with explicit
  security guidance.
- [ ] Better preallocation strategies and sparse-file handling.
- [ ] Download verification repair flow: re-fetch only corrupt segments.
- [ ] Import/export of settings, rules, categories, and queue definitions.
- [ ] Internationalization and accessibility audit.
- [ ] Signed Windows/macOS/Linux release artifacts and reproducible build documentation.
- [ ] Firefox extension packaging where Manifest V3 APIs permit equivalent behavior.
- [ ] Optional aria2 engine adapter, packaged separately and never required by default.

## Phase 3 — longer term

- [ ] HLS/DASH manifests through a pluggable media engine (FFmpeg optional and external).
- [ ] FTP/SFTP adapters.
- [ ] Site-specific extractors as an explicit, sandboxed extension boundary.
- [ ] Multi-user or remote-server mode with proper TLS, session management, and CSRF
  review.
- [ ] Plugin API for engines and post-processing.
- [ ] Mobile companion or remote-control clients.
- [ ] Advanced bandwidth shaping per host/queue and time-of-day profiles.

## Explicit non-goals for the default distribution

- Bundling GPL-only components such as aria2 or FFmpeg.
- Bypassing DRM, paywalls, or access controls.
- Browser password or cookie extraction.
- Silent background installation or auto-update without user consent.
- Claiming compatibility with another download manager’s proprietary protocol or UI.

## Contributing to the roadmap

Open an issue describing the use case before starting a large item. Changes should keep
the engine trait boundary, persist through SQLite, preserve the authenticated API
contract, and include tests. See [CONTRIBUTING](../CONTRIBUTING.md).
