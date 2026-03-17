#!/usr/bin/env bash
# serve — start the GPU daemon
# usage: serve [--port PORT]
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")/.." && pwd)"

# Parse args
while [[ $# -gt 0 ]]; do
    case "$1" in
        --port)
            export GPU_SERVER_PORT="$2"
            shift 2
            ;;
        *)
            echo "unknown arg: $1"
            exit 1
            ;;
    esac
done

cd "${SCRIPT_DIR}"

# Install deps if needed
if ! python3 -c "import fastapi" 2>/dev/null; then
    echo "Installing dependencies..."
    pip install -r requirements.txt
fi

echo "Starting GPU daemon on port ${GPU_SERVER_PORT:-8844}..."
exec python3 server.py
