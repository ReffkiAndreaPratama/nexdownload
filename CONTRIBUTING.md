# Contributing to NexDownload

Thank you for helping improve NexDownload. The project is MIT-licensed and welcomes
bug reports, documentation fixes, tests, and well-scoped features.

## Before you start

- Read the [Code of Conduct](CODE_OF_CONDUCT.md).
- For a bug, include the NexDownload version, OS, Rust/Node versions, steps to reproduce,
  expected and actual behavior, and relevant logs with tokens and URLs redacted.
- For a security issue, do **not** open a public issue. Follow [SECURITY.md](SECURITY.md).
- For a large feature, open an issue first so the design and scope can be agreed before
  implementation. Roadmap items are listed in [docs/ROADMAP.md](docs/ROADMAP.md).

## Development setup

```powershell
npm --prefix apps/desktop install
npm --prefix apps/desktop run build
cargo check
cargo test --workspace
npm --prefix apps/desktop test
```

See [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) for platform prerequisites, headless
server usage, live smoke tests, and release builds.

## Pull request expectations

1. Keep the change focused. Separate unrelated refactors from behavior changes.
2. Add or update tests for behavior changes. Engine, state machine, storage, scheduler,
   security, and frontend helper tests should remain meaningful, not coverage theater.
3. Run formatting and linting:

   ```powershell
   cargo fmt --all
   cargo clippy --all-targets -- -D warnings
   cargo test --workspace
   npm --prefix apps/desktop run build
   npm --prefix apps/desktop test
   ```

4. Update documentation and `CHANGELOG.md` when behavior, configuration, API, or
   contributed assets change.
5. Describe the user-visible result and how you verified it. Include screenshots for UI
   changes when practical.
6. Do not commit generated build output, `node_modules`, `target`, databases, tokens, or
   downloaded files.

## Code style

- Rust: stable formatting via `rustfmt.toml`; no warnings under clippy `-D warnings`.
  Prefer explicit error types and keep the service layer as the task-state authority.
- TypeScript/React: strict types, no unused locals or parameters, existing Tailwind and
  component conventions, accessible labels for controls.
- SQL: update the migration file only through a new migration once released; do not
  rewrite a migration that users may already have applied.
- Documentation: state what is actually implemented. Do not add placeholder buttons,
  fake progress, unimplemented guarantees, or unsupported compatibility claims.

## Originality and licensing

NexDownload is an independent project. Do not copy source code, UI layouts, icons, text,
or assets from another download manager. If you add a dependency or asset:

- verify its license is compatible with MIT distribution;
- update `THIRD_PARTY_NOTICES.md`;
- do not add GPL-only code to the default distribution;
- keep optional external engines separate and document the boundary.

By submitting a contribution, you agree that it may be distributed under the project’s
MIT License. No contributor license agreement is required.

## Commit and PR titles

Use short, imperative summaries. Examples:

- `Fix resume checkpoint after ETag change`
- `Add queue priority validation`
- `Document LAN TLS proxy requirement`
- `Test extension confirmation flow`

## Review and merge

Maintainers review for correctness, tests, security, originality, and documentation.
Review may request a smaller scope or additional evidence for claims. Do not force-push
over review comments unless asked; keep the history understandable.
