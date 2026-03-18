#!/usr/bin/env bash
# logs — tail the storylens API logs on formace-00
# usage: logs [--lines N]
set -euo pipefail

HOST="${FORMACE_HOST:-formace-00}"
LINES=50

while [[ $# -gt 0 ]]; do
    case "$1" in
        --lines|-n)
            LINES="$2"
            shift 2
            ;;
        *)
            echo "unknown arg: $1"
            exit 1
            ;;
    esac
done

echo "=== storylens API logs (${HOST}) ==="
echo "Tailing last ${LINES} lines..."
echo ""

ssh "${HOST}" "tail -n ${LINES} /tmp/storylens-api.log 2>/dev/null || echo 'Log file not found at /tmp/storylens-api.log'"
