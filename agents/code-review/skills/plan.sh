#!/usr/bin/env bash
# plan — CEO-level product review
# usage: plan [path]
set -euo pipefail
if [ -n "${AIDE_PROJECT_DIR:-}" ]; then cd "$AIDE_PROJECT_DIR"; fi

PATH_ARG="${1:-.}"
echo "=== Product Review: ${PATH_ARG} ==="
echo ""
echo "Recent changes:"
git log --oneline -10 -- "$PATH_ARG" 2>/dev/null || echo "(not a git repo)"
echo ""
echo "Files changed recently:"
git diff --stat HEAD~5 -- "$PATH_ARG" 2>/dev/null || ls -la "$PATH_ARG" 2>/dev/null
echo ""
echo "Use with -p for AI-powered review:"
echo "  aide exec -p <instance> \"review the product direction of ${PATH_ARG}\""
