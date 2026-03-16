#!/bin/bash
# Jenny skill: Browser automation via Playwright MCP
# Delegates to wonskill chrome
# Usage: chrome.sh [navigate|snapshot|click|fill|upload|tabs|wait] [args...]
set -euo pipefail
CMD="${1:-snapshot}"
shift 2>/dev/null || true
exec wonskill chrome "$CMD" "$@"
