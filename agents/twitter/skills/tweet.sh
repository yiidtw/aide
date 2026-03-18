#!/usr/bin/env bash
# tweet — post a tweet
# usage: tweet <text>
# Outputs TWEET_POST:<text> for the caller (Claude) to execute via debug Chrome.
set -euo pipefail

TEXT="${*:?Usage: tweet \"your message\"}"

# Check length
LEN=$(printf '%s' "$TEXT" | wc -c | tr -d ' ')
if [ "$LEN" -gt 280 ]; then
  echo "ERROR: ${LEN} chars — over 280 limit. Trim it."
  exit 1
fi

echo "TWEET_POST:${TEXT}"
