#!/usr/bin/env bash
set -euo pipefail

echo "== TrendLab verify =="
echo

echo "-> cargo fmt --check"
cargo fmt -- --check
echo

echo "-> cargo clippy (deny warnings)"
cargo clippy --all-targets --all-features -- -D warnings
echo

echo "-> cargo test"
cargo test
echo

echo "-> cargo deny check (if installed)"
if command -v cargo-deny >/dev/null 2>&1; then
  cargo deny check
else
  echo "   (skipped: cargo-deny not installed)"
fi
echo

echo "OK"


