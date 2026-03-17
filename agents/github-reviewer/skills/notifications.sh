#!/usr/bin/env bash
# notifications — show unread GitHub notifications
# usage: notifications
set -euo pipefail
source "$(dirname "$0")/_check_gh.sh"; check_gh

echo "=== GitHub Notifications ==="
NOTIFS=$(gh api notifications --jq '.[].subject | "\(.type): \(.title)"' 2>/dev/null)
if [ -z "$NOTIFS" ]; then
  echo "No unread notifications."
else
  echo "$NOTIFS"
fi
