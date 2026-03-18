#!/usr/bin/env bash
# stop — stop the storylens API on formace-00
# usage: stop
set -euo pipefail

HOST="${FORMACE_HOST:-formace-00}"

echo "=== Stopping storylens API on ${HOST} ==="
ssh "${HOST}" "pkill -f 'uvicorn storylens' 2>/dev/null && echo 'Stopped.' || echo 'Not running.'" || true
