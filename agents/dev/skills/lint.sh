#!/usr/bin/env bash
# lint — run clippy + agent linter
# usage: lint [agent-path]
set -euo pipefail

cd "${AIDE_PROJECT_DIR:-$(pwd)}"

echo "=== cargo clippy ==="
cargo clippy --all-targets 2>&1

echo ""
echo "=== aide.sh lint (agents/jenny) ==="
cargo run --quiet -- lint agents/jenny/

if [ -n "${1:-}" ]; then
    echo ""
    echo "=== aide.sh lint ($1) ==="
    cargo run --quiet -- lint "$1"
fi

echo ""
echo "lint: all passed"
