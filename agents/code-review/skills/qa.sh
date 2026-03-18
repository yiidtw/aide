#!/usr/bin/env bash
# qa — QA testing
# usage: qa [diff|full|quick]
set -euo pipefail
if [ -n "${AIDE_PROJECT_DIR:-}" ]; then cd "$AIDE_PROJECT_DIR"; fi

MODE="${1:-diff}"
echo "=== QA: ${MODE} mode ==="
echo ""

case "$MODE" in
  diff)
    echo "Changed files (diff-aware):"
    git diff --name-only main 2>/dev/null || git diff --name-only HEAD~3
    ;;
  full)
    echo "Running full test suite..."
    if [ -f "Cargo.toml" ]; then cargo test 2>&1
    elif [ -f "package.json" ]; then npm test 2>&1
    elif [ -f "Makefile" ]; then make test 2>&1
    else echo "No test runner detected."; fi
    ;;
  quick)
    echo "Quick smoke test..."
    if [ -f "Cargo.toml" ]; then cargo check 2>&1
    elif [ -f "package.json" ]; then npx tsc --noEmit 2>&1 || true
    else echo "No quick check available."; fi
    ;;
  *)
    echo "usage: qa [diff|full|quick]"
    exit 1
    ;;
esac
