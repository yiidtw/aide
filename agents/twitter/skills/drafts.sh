#!/usr/bin/env bash
# drafts — list saved drafts
# usage: drafts
set -euo pipefail

DRAFT_DIR="${AIDE_INSTANCE_DIR:-$(dirname "$0")/..}/memory"
DRAFT_FILE="$DRAFT_DIR/drafts.md"

if [ -f "$DRAFT_FILE" ]; then
  echo "=== Drafts ==="
  cat "$DRAFT_FILE"
else
  echo "No drafts."
fi
