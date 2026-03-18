#!/usr/bin/env bash
# status — show all topics with generation/eval status and scores
# usage: status
set -euo pipefail

API_URL="${STORYLENS_API_URL:-https://api.storylens.ai}"
TOKEN="${STORYLENS_ADMIN_TOKEN:-}"

echo "=== teaching.monster competition status ==="
echo ""

# 1. Check API health
if ! curl -sf "${API_URL}/health" >/dev/null 2>&1; then
    echo "WARNING: API at ${API_URL} is not responding"
    echo "Check if storylens is running on formace-00"
    exit 1
fi
echo "API: ${API_URL} [healthy]"
echo ""

# 2. List all teach jobs
AUTH_HEADER=""
if [[ -n "${TOKEN}" ]]; then
    AUTH_HEADER="Authorization: Bearer ${TOKEN}"
fi

echo "--- Active Jobs ---"
if [[ -n "${AUTH_HEADER}" ]]; then
    curl -sf -H "${AUTH_HEADER}" "${API_URL}/api/jobs" | python3 -m json.tool
else
    curl -sf "${API_URL}/api/jobs" | python3 -m json.tool
fi

echo ""
echo "For detailed scores, run: aide.sh exec teaching-monster score"
echo "For browser dashboard, run: aide.sh exec teaching-monster login"
