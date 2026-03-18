#!/usr/bin/env bash
# inbox — check latest emails via debug Chrome
# usage: inbox [count]
set -euo pipefail
COUNT="${1:-5}"
echo "GMAIL_ACTION:inbox ${COUNT}"
echo "Navigate to https://mail.google.com and list the latest ${COUNT} emails."
echo "For each: sender, subject, time, one-line summary."
