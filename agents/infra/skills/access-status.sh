#!/usr/bin/env bash
# access-status — list all CF Access apps and service tokens
# usage: access-status
set -euo pipefail
TOKEN="${CLOUDFLARE_API_TOKEN:?not set}"
ACCOUNT="${CLOUDFLARE_ACCOUNT_ID:?not set}"
API="https://api.cloudflare.com/client/v4/accounts/$ACCOUNT"

echo "=== CF Access Apps ==="
curl -s -H "Authorization: Bearer $TOKEN" "$API/access/apps" | \
  python3 -c "
import sys, json
d = json.load(sys.stdin)
if not d['success']:
    print('ERROR:', d.get('errors'))
    sys.exit(1)
apps = d.get('result') or []
if not apps:
    print('  (none)')
else:
    for a in apps:
        print(f'  {a[\"domain\"]:35s} session={a.get(\"session_duration\",\"?\"):5s} id={a[\"id\"][:8]}...')
"

echo ""
echo "=== Service Tokens ==="
curl -s -H "Authorization: Bearer $TOKEN" "$API/access/service_tokens" | \
  python3 -c "
import sys, json
d = json.load(sys.stdin)
if not d['success']:
    print('ERROR:', d.get('errors'))
    sys.exit(1)
tokens = d.get('result') or []
if not tokens:
    print('  (none)')
else:
    for t in tokens:
        exp = t.get('expires_at', 'never')[:10] if t.get('expires_at') else 'never'
        print(f'  {t[\"name\"]:25s} client_id={t[\"client_id\"]:10s}  expires={exp}')
"
