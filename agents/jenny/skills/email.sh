#!/bin/bash
# Jenny skill: NTU Email (POP3/SMTP)
# Delegates to wonskill email
# Usage: email.sh [check|unread|read N|search Q|send TO SUBJECT BODY]
set -euo pipefail
CMD="${1:-check}"
shift 2>/dev/null || true
exec wonskill email "$CMD" "$@"
