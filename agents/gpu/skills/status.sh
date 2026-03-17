#!/usr/bin/env bash
# status — show GPU status
# usage: status
set -euo pipefail

PORT="${GPU_SERVER_PORT:-8844}"
HOST="http://localhost:${PORT}"

# Try the daemon first
if curl -sf "${HOST}/gpu/status" 2>/dev/null; then
    exit 0
fi

# Fallback: direct nvidia-smi
echo "gpu-daemon not running, falling back to nvidia-smi"
echo ""
if command -v nvidia-smi &>/dev/null; then
    nvidia-smi --query-gpu=name,memory.total,memory.used,memory.free,temperature.gpu,utilization.gpu --format=csv
else
    echo "error: nvidia-smi not found"
    exit 1
fi
