#!/usr/bin/env bash
# draft — save a tweet draft
# usage: draft <text>
set -euo pipefail

TEXT="${*:?Usage: draft \"your message\"}"
DRAFT_DIR="${AIDE_INSTANCE_DIR:-$(dirname "$0")/..}/memory"
mkdir -p "$DRAFT_DIR"
DRAFT_FILE="$DRAFT_DIR/drafts.md"

LEN=$(printf '%s' "$TEXT" | wc -c | tr -d ' ')
if [ "$LEN" -gt 280 ]; then
  echo "WARNING: ${LEN} chars — over 280 limit"
fi

TIMESTAMP=$(date '+%Y-%m-%d %H:%M')
echo "- [${TIMESTAMP}] ${TEXT}" >> "$DRAFT_FILE"
echo "Draft saved (${LEN} chars)"
