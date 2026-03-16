#!/bin/bash
# Jenny skill: JudgeBoi ML homework submission
# Delegates to wonskill judgeboi
# Usage: judgeboi.sh [submit|status|leaderboard|submissions]
set -euo pipefail
CMD="${1:-status}"
shift 2>/dev/null || true
exec wonskill judgeboi "$CMD" "$@"
