#!/usr/bin/env bash
# status — show last debate results
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
STATE_DIR="$(dirname "$SCRIPT_DIR")/memory"
LOG="$STATE_DIR/last-debate.md"

if [ ! -f "$LOG" ]; then
  echo "No debate history found."
  exit 0
fi

# Show summary (last 10 lines contain the summary block)
echo "=== Last Debate ==="
tail -15 "$LOG"
echo ""
echo "Full log: $LOG"
