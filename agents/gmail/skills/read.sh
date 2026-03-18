#!/usr/bin/env bash
# read — read a specific email
# usage: read <query>
set -euo pipefail
QUERY="${*:?Usage: read <query>}"
echo "GMAIL_ACTION:read ${QUERY}"
echo "Search for '${QUERY}' in Gmail, open the first match, read the content."
