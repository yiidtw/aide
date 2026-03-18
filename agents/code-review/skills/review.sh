#!/usr/bin/env bash
# review — staff engineer code review
# usage: review [branch|path]
set -euo pipefail
if [ -n "${AIDE_PROJECT_DIR:-}" ]; then cd "$AIDE_PROJECT_DIR"; fi

TARGET="${1:-HEAD}"
echo "=== Code Review: ${TARGET} ==="
echo ""

# Show diff
if git rev-parse "$TARGET" &>/dev/null; then
  echo "Diff against main:"
  git diff --stat main..."$TARGET" 2>/dev/null || git diff --stat "$TARGET"
  echo ""
  echo "Changed files:"
  git diff --name-only main..."$TARGET" 2>/dev/null || git diff --name-only "$TARGET"
else
  echo "Reviewing path: $TARGET"
  find "$TARGET" -type f -name "*.rs" -o -name "*.ts" -o -name "*.py" -o -name "*.go" 2>/dev/null | head -20
fi
echo ""
echo "Use with -p for AI-powered review:"
echo "  aide exec -p <instance> \"review ${TARGET} for bugs and security issues\""
