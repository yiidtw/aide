#!/usr/bin/env bash
# search — search Gmail
# usage: search <query>
set -euo pipefail
QUERY="${*:?Usage: search <query>}"
echo "GMAIL_ACTION:search ${QUERY}"
echo "Navigate to https://mail.google.com/mail/u/0/#search/${QUERY}"
echo "List matching emails: sender, subject, date."
