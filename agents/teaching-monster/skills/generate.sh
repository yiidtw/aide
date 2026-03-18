#!/usr/bin/env bash
# generate — trigger video generation for a topic
# usage: generate [TOPIC_ID|all]
#
# If TOPIC_ID is given, generates that single topic.
# If "all" is given, generates all pending topics sequentially.
# If no arg, prints usage.
set -euo pipefail

API_URL="${STORYLENS_API_URL:-https://api.storylens.ai}"
TOKEN="${STORYLENS_ADMIN_TOKEN:-}"

if [[ $# -eq 0 ]]; then
    echo "usage: generate [TOPIC_ID|all]"
    echo ""
    echo "Examples:"
    echo "  generate topic-001              # single topic"
    echo "  generate all                    # all pending topics"
    echo ""
    echo "This calls the competition endpoint:"
    echo "  POST ${API_URL}/api/competition/generate"
    exit 0
fi

TARGET="$1"

generate_topic() {
    local request_id="$1"
    local topic="$2"
    local persona="${3:-University student with basic math background}"

    echo ">>> Generating: ${request_id} — ${topic}"

    # Build JSON payload safely with python to avoid shell quoting issues
    PAYLOAD=$(python3 -c "
import json, sys
print(json.dumps({
    'request_id': sys.argv[1],
    'course_requirement': sys.argv[2],
    'student_persona': sys.argv[3],
}))
" "${request_id}" "${topic}" "${persona}")

    RESPONSE=$(curl -sf -X POST "${API_URL}/api/competition/generate" \
        -H "Content-Type: application/json" \
        -d "${PAYLOAD}" 2>&1) || {
        echo "ERROR: API call failed"
        echo "${RESPONSE}"
        return 1
    }

    # The streaming response sends heartbeat newlines, final line is JSON
    RESULT=$(echo "${RESPONSE}" | tail -1)
    echo "Result: ${RESULT}"
    echo ""
}

if [[ "${TARGET}" == "all" ]]; then
    echo "=== Batch generation ==="
    echo "Use the browser dashboard to identify pending topics,"
    echo "then call generate with specific topic IDs."
    echo ""
    echo "AGENT_ACTION: batch_generate"
    echo "The agent should use MCP tools to scrape the dashboard,"
    echo "find topics with status '未生成', and generate them sequentially."
else
    # Remaining args after request_id are joined as the topic text
    # (aide exec splits on whitespace, so we rejoin $2.. as topic)
    shift
    if [[ $# -eq 0 ]]; then
        # No topic text — use request_id as topic
        generate_topic "${TARGET}" "${TARGET}"
    else
        TOPIC="$*"
        generate_topic "${TARGET}" "${TOPIC}"
    fi
fi
