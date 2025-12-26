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

echo "OK"


