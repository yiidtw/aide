#!/usr/bin/env bash
# test — run test suite
# usage: test [filter]
set -euo pipefail

cd "${AIDE_PROJECT_DIR:-$(pwd)}"

if [ -n "${1:-}" ]; then
    echo "=== cargo test (filter: $1) ==="
    cargo test "$1" 2>&1
else
    echo "=== cargo test ==="
    cargo test 2>&1
fi

echo ""
echo "test: done"
