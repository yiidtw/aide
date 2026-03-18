#!/usr/bin/env bash
# score — fetch detailed scores and analysis
# usage: score [TOPIC_ID]
#
# Without args: shows all evaluated topics' scores.
# With TOPIC_ID: shows detailed breakdown for that topic.
set -euo pipefail

API_URL="${STORYLENS_API_URL:-https://api.storylens.ai}"
TOKEN="${STORYLENS_ADMIN_TOKEN:-}"

echo "=== Competition Scores ==="
echo ""

# Check Chrome debug port for dashboard scraping
if ! curl -sf http://localhost:9222/json/version >/dev/null 2>&1; then
    echo "Chrome debug port not available — falling back to API-only data."
    echo ""
fi

if [[ $# -eq 0 ]]; then
    echo "AGENT_ACTION: scrape_all_scores"
    echo ""
    echo "The agent should:"
    echo "1. Navigate to competition dashboard"
    echo "2. Scrape all topics with their eval scores"
    echo "3. Print a summary table:"
    echo "   ID | Topic | Status | Score | Accuracy | Logic | Adapt | Engage"
    echo "4. Identify lowest-scoring dimensions for improvement priority"
else
    TOPIC_ID="$1"
    echo "AGENT_ACTION: scrape_score ${TOPIC_ID}"
    echo ""
    echo "The agent should:"
    echo "1. Navigate to topic ${TOPIC_ID} detail page"
    echo "2. Scrape full evaluation breakdown"
    echo "3. Show: dimension scores, feedback text, improvement suggestions"

    # Also check local eval data from API
    AUTH_HEADER=""
    if [[ -n "${TOKEN}" ]]; then
        AUTH_HEADER="Authorization: Bearer ${TOKEN}"
    fi

    echo ""
    echo "--- Local eval data (if available) ---"
    if [[ -n "${AUTH_HEADER}" ]]; then
        curl -sf -H "${AUTH_HEADER}" "${API_URL}/api/teach/result/${TOPIC_ID}" 2>/dev/null \
            | python3 -c "
import json, sys
data = json.load(sys.stdin)
r = data.get('result', {})
ev = r.get('eval', {})
if ev:
    print(f\"Overall:      {ev.get('overall', 'N/A')}\")
    print(f\"Accuracy:     {ev.get('accuracy', 'N/A')}\")
    print(f\"Logic & Flow: {ev.get('logic_flow', 'N/A')}\")
    print(f\"Adaptability: {ev.get('adaptability', 'N/A')}\")
    print(f\"Engagement:   {ev.get('engagement', 'N/A')}\")
    print(f\"Top Fix:      {ev.get('top_priority_fix', 'N/A')}\")
else:
    print('No eval data in API result.')
" 2>/dev/null || echo "Could not fetch from API."
fi
fi
