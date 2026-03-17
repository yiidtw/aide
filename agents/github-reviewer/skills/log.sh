#!/usr/bin/env bash
# log — recent commit history
# usage: log [count]
set -euo pipefail
if [ -n "${AIDE_PROJECT_DIR:-}" ]; then cd "$AIDE_PROJECT_DIR"; fi

COUNT="${1:-10}"
echo "=== Recent Commits ==="
git log --oneline -"$COUNT"
