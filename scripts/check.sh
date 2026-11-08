#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

echo "==> Rust format check"
cargo fmt --all -- --check

echo "==> Clippy"
cargo clippy --all-targets -- -D warnings

echo "==> Rust tests"
cargo test --workspace

echo "==> Frontend build"
npm --prefix apps/desktop run build

echo "==> Frontend tests"
npm --prefix apps/desktop test

echo "All checks passed."
