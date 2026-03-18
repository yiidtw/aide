#!/usr/bin/env bash
# login — open browser and navigate to teaching.monster competition dashboard
# usage: login
#
# Requires: Chrome running with --remote-debugging-port=9222
# The script opens the competition management page. If not logged in,
# the user completes auth manually, then this agent takes over.
set -euo pipefail

COMPETITION_URL="https://teaching.monster/app/competitions/1/manage"

echo "=== teaching.monster login ==="

# Check if Chrome debug port is available
if ! curl -sf http://localhost:9222/json/version >/dev/null 2>&1; then
    echo "Chrome debug port not detected on :9222"
    echo ""
    echo "Start Chrome with remote debugging:"
    echo "  /Applications/Google\\ Chrome.app/Contents/MacOS/Google\\ Chrome \\"
    echo "    --remote-debugging-port=9222 \\"
    echo "    --user-data-dir=/tmp/chrome-debug"
    echo ""
    echo "Then re-run: aide.sh exec teaching-monster login"
    exit 1
fi

echo "Chrome debug port active on :9222"
echo "Navigate to: ${COMPETITION_URL}"
echo ""

# Use Playwright MCP or chrome-devtools MCP to navigate
# This script sets up the context; the agent uses MCP tools to interact
echo "AGENT_ACTION: navigate ${COMPETITION_URL}"
echo ""
echo "Once logged in, run 'aide.sh exec teaching-monster status' to verify."
