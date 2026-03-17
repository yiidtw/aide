#!/usr/bin/env bash
# check — full CI check
# usage: check
set -euo pipefail

cd "${AIDE_PROJECT_DIR:-$(pwd)}"

echo "=== cargo build ==="
cargo build 2>&1
echo ""

echo "=== cargo clippy ==="
cargo clippy --all-targets 2>&1
echo ""

echo "=== cargo test ==="
cargo test 2>&1
echo ""

echo "=== aide.sh lint (agents/jenny) ==="
cargo run --quiet -- lint agents/jenny/
echo ""

echo "=== aide.sh lint (agents/dev) ==="
cargo run --quiet -- lint agents/dev/
echo ""

echo "=== mdbook build ==="
mdbook build docs 2>&1
echo ""

echo "check: all passed"
