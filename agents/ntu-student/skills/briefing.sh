#!/usr/bin/env bash
# briefing — daily briefing: COOL todos + assignments + unread mail
# Sends summary email via Resend from yiidtw.ntu@aide.sh
# usage: briefing [EMAIL]
# env: RESEND_API_KEY
set -euo pipefail

TO="${1:-${BRIEFING_TO:?BRIEFING_TO not set. Run: aide.sh vault set BRIEFING_TO=you@gmail.com}}"
RESEND="${RESEND_API_KEY:?RESEND_API_KEY not set. Run: aide.sh vault set RESEND_API_KEY=...}"
DATE=$(date '+%Y-%m-%d %A')

echo "Generating daily briefing for ${TO}..."

# ─── COOL: Todos + Assignments (via wonskill) ───
COOL_SUMMARY=$(wonskill cool todos 2>&1) || COOL_SUMMARY="(failed to fetch COOL todos)"
COOL_ASSIGNMENTS=$(wonskill cool assignments 2>&1) || COOL_ASSIGNMENTS="(failed to fetch assignments)"

# ─── NTU Mail: Latest (via wonskill) ───
MAIL_SUMMARY=$(wonskill email check 2>&1) || MAIL_SUMMARY="(failed to check mail)"

# ─── Compose ───
read -r -d '' BODY << BODY_EOF || true
Daily Briefing — ${DATE}

COOL TODO:
${COOL_SUMMARY}

Upcoming Assignments:
${COOL_ASSIGNMENTS}

NTU Mail:
${MAIL_SUMMARY}

---
Sent by ntu-student agent via aide.sh
BODY_EOF

# ─── Send via Resend ───
export BRIEFING_BODY="$BODY"
export BRIEFING_TO_ADDR="$TO"
export BRIEFING_DATE="$DATE"

PAYLOAD=$(python3 << 'PYEOF'
import json, os
body = os.environ.get("BRIEFING_BODY", "")
to = os.environ.get("BRIEFING_TO_ADDR", "")
date = os.environ.get("BRIEFING_DATE", "")
print(json.dumps({
    "from": "ntu-student <yiidtw.ntu@aide.sh>",
    "to": [to],
    "subject": f"Daily Briefing \u2014 {date}",
    "text": body
}))
PYEOF
)

RESPONSE=$(curl -s -X POST https://api.resend.com/emails \
  -H "Authorization: Bearer $RESEND" \
  -H "Content-Type: application/json" \
  -d "$PAYLOAD" 2>&1)

if echo "$RESPONSE" | grep -q '"id"'; then
  echo "Briefing sent to ${TO} from yiidtw.ntu@aide.sh"
else
  echo "Failed to send: ${RESPONSE}"
  exit 1
fi
