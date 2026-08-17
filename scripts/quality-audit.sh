#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

echo "[x11-quality] scanning source for unfinished implementation markers"
if grep -RInE --exclude-dir=.git --exclude='Cargo.lock' 'TODO|FIXME|todo!\(|unimplemented!\(|IMPLEMENT_ME|PLACEHOLDER|TRUNCATED' crates; then
  echo "[x11-quality] unfinished implementation marker found"
  exit 1
fi

echo "[x11-quality] cargo fmt"
cargo fmt --all -- --check

echo "[x11-quality] cargo check"
cargo check --workspace

echo "[x11-quality] cargo test"
cargo test --workspace --all-targets

echo "[x11-quality] cargo clippy"
cargo clippy --workspace --all-targets --all-features -- -D warnings

echo "[x11-quality] all gates passed"
