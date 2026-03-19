#!/usr/bin/env bash
# lock — lock a domain with CF Access (app + allow-email policy + one-login IdP)
# usage: lock <domain> [email]
# Creates a self-hosted Access app with Google login, allowing the specified email.
set -euo pipefail
TOKEN="${CLOUDFLARE_API_TOKEN:?not set}"
ACCOUNT="${CLOUDFLARE_ACCOUNT_ID:?not set}"
DOMAIN="${1:?Usage: lock <domain> [email]}"
EMAIL="${2:-yiidtw@gmail.com}"
API="https://api.cloudflare.com/client/v4/accounts/$ACCOUNT"

err() { echo "ERROR: $1" >&2; exit 1; }

# Check if app already exists for this domain
EXISTING=$(curl -s -H "Authorization: Bearer $TOKEN" "$API/access/apps" | \
  python3 -c "import sys,json; apps=json.load(sys.stdin).get('result') or []; matches=[a for a in apps if a.get('domain')=='$DOMAIN']; print(matches[0]['id'] if matches else '')")

if [ -n "$EXISTING" ]; then
  echo "App already exists for $DOMAIN (id: $EXISTING)"
  echo "To recreate, run: unlock $DOMAIN && lock $DOMAIN"
  exit 0
fi

echo "==> Creating Access app for $DOMAIN..."
APP_RESULT=$(curl -s -X POST -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  "$API/access/apps" \
  -d "{
    \"name\": \"$DOMAIN\",
    \"domain\": \"$DOMAIN\",
    \"type\": \"self_hosted\",
    \"session_duration\": \"24h\",
    \"auto_redirect_to_identity\": true
  }")

APP_ID=$(echo "$APP_RESULT" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d['result']['id'] if d['success'] else ''); d['success'] or print('App error:', d.get('errors'), file=sys.stderr)" 2>&1)

if [ -z "$APP_ID" ] || echo "$APP_ID" | grep -q "App error"; then
  echo "$APP_RESULT" | python3 -mjson.tool
  err "Failed to create Access app"
fi
echo "    app_id: $APP_ID"

# Create allow policy for the email
echo "==> Creating allow policy for $EMAIL..."
POLICY_RESULT=$(curl -s -X POST -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  "$API/access/apps/$APP_ID/policies" \
  -d "{
    \"name\": \"Allow $EMAIL\",
    \"decision\": \"allow\",
    \"include\": [{\"email\": {\"email\": \"$EMAIL\"}}],
    \"precedence\": 1
  }")

POLICY_OK=$(echo "$POLICY_RESULT" | python3 -c "import sys,json; d=json.load(sys.stdin); print('ok' if d['success'] else d.get('errors','fail'))")
echo "    policy: $POLICY_OK"

echo ""
echo "Locked: $DOMAIN"
echo "  Login: Google (One-time PIN fallback)"
echo "  Allowed: $EMAIL"
echo "  Session: 24h"
echo "  App ID: $APP_ID"
