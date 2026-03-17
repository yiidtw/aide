#!/usr/bin/env bash
# build-docs — build mdbook + cargo doc
# usage: build-docs
set -euo pipefail

cd "${AIDE_PROJECT_DIR:-$(pwd)}"

echo "=== mdbook build ==="
mdbook build docs 2>&1
echo "  output: docs/book/"
echo ""

echo "=== cargo doc ==="
cargo doc --no-deps 2>&1
echo "  output: target/doc/aide_sh/"
echo ""

echo "build-docs: done"
