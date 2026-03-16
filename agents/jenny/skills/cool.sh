#!/bin/bash
# Jenny skill: NTU COOL (Canvas LMS)
# Delegates to wonskill cool
# Usage: cool.sh [scan|courses|assignments|grades|todos|summary|announcements]
set -euo pipefail
CMD="${1:-scan}"
shift 2>/dev/null || true
exec wonskill cool "$CMD" "$@"
