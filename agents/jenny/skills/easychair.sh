#!/bin/bash
# Jenny skill: EasyChair conference reviews
# Delegates to wonskill easychair
# Usage: easychair.sh [reviews|view|download|submit|summary]
set -euo pipefail
CMD="${1:-summary}"
shift 2>/dev/null || true
exec wonskill easychair "$CMD" "$@"
