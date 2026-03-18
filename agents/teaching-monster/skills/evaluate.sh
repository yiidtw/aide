#!/usr/bin/env bash
# evaluate — trigger AI evaluation on completed topics
# usage: evaluate [TOPIC_ID|all]
#
# This interacts with the teaching.monster dashboard to click
# "啟動 AI 評測" on topics that have status "成功" (generation success).
set -euo pipefail

if [[ $# -eq 0 ]]; then
    echo "usage: evaluate [TOPIC_ID|all]"
    echo ""
    echo "Triggers AI evaluation on the teaching.monster platform."
    echo "Requires Chrome debug port on :9222 with an active session."
    echo ""
    echo "Examples:"
    echo "  evaluate topic-001     # evaluate single topic"
    echo "  evaluate all           # evaluate all completed topics"
    exit 0
fi

TARGET="$1"

echo "=== AI Evaluation ==="
echo "Target: ${TARGET}"
echo ""

# Check Chrome debug port
if ! curl -sf http://localhost:9222/json/version >/dev/null 2>&1; then
    echo "ERROR: Chrome debug port not available on :9222"
    echo "Run 'aide.sh exec teaching-monster login' first."
    exit 1
fi

echo "AGENT_ACTION: evaluate ${TARGET}"
echo ""
echo "The agent should:"
echo "1. Navigate to the competition dashboard"
echo "2. Find topic(s) with status '成功' (success)"
echo "3. Click '啟動 AI 評測' for target topic(s)"
echo "4. Wait for evaluation to complete"
echo "5. Report scores"
