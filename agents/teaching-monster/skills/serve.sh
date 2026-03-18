#!/usr/bin/env bash
# serve — start (or restart) the storylens API on formace-00
# usage: serve [--restart]
#
# Pulls latest code, starts uvicorn on port 8501.
# If --restart is passed, kills existing process first.
set -euo pipefail

HOST="${FORMACE_HOST:-formace-00}"
REMOTE_DIR="claude_projects/storylens-pipeline"
PORT=8501

RESTART=false
for arg in "$@"; do
    case "$arg" in
        --restart) RESTART=true ;;
    esac
done

echo "=== Starting storylens API on ${HOST} ==="

# Check if already running
RUNNING=$(ssh "${HOST}" "curl -sf http://localhost:${PORT}/health 2>/dev/null" || true)
if [[ -n "${RUNNING}" && "${RESTART}" != "true" ]]; then
    echo "API already running:"
    echo "  ${RUNNING}"
    echo ""
    echo "Use 'serve --restart' to force restart."
    exit 0
fi

# Kill existing if restarting
if [[ "${RESTART}" == "true" ]]; then
    echo "Killing existing process..."
    ssh "${HOST}" "pkill -f 'uvicorn storylens' 2>/dev/null || true"
    sleep 2
fi

# Pull latest code and start
echo "Pulling latest code and starting uvicorn..."
ssh "${HOST}" bash -s <<'REMOTE'
set -euo pipefail
cd ~/${REMOTE_DIR:-claude_projects/storylens-pipeline}
git fetch origin && git reset --hard origin/main

source .venv/bin/activate

export STORYLENS_PUBLIC_URL=https://api.storylens.ai
export ANTHROPIC_API_KEY=$(cat ~/.anthropic_key 2>/dev/null || true)

nohup .venv/bin/uvicorn storylens.web:app \
    --host 0.0.0.0 \
    --port 8501 \
    --timeout-keep-alive 1800 \
    > /tmp/storylens-api.log 2>&1 &

sleep 3
curl -sf http://localhost:8501/health || { echo "ERROR: API failed to start"; tail -20 /tmp/storylens-api.log; exit 1; }
REMOTE

echo ""
echo "API is live at https://api.storylens.ai"
echo "Logs: aide.sh exec teaching-monster logs"
