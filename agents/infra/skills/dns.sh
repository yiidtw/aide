#!/usr/bin/env bash
# dns — Cloudflare DNS operations
# usage: dns [list ZONE|set ZONE NAME TYPE VALUE|delete ZONE ID]
set -euo pipefail
TOKEN="${CLOUDFLARE_API_TOKEN:?CLOUDFLARE_API_TOKEN not set}"
CMD="${1:?Usage: dns [list ZONE|set ZONE NAME TYPE VALUE|delete ZONE ID]}"
shift

case "$CMD" in
  list)
    ZONE="${1:?Usage: dns list ZONE}"
    ZONE_ID=$(curl -s -H "Authorization: Bearer $TOKEN" \
      "https://api.cloudflare.com/client/v4/zones?name=$ZONE" | python3 -c "import sys,json; r=json.load(sys.stdin)['result']; print(r[0]['id'] if r else '')")
    [ -z "$ZONE_ID" ] && echo "Zone not found: $ZONE" && exit 1
    curl -s -H "Authorization: Bearer $TOKEN" \
      "https://api.cloudflare.com/client/v4/zones/$ZONE_ID/dns_records" | python3 -c "
import sys,json
for r in json.load(sys.stdin)['result']:
    print(f'{r[\"type\"]:6s} {r[\"name\"]:35s} → {r[\"content\"][:45]}')"
    ;;
  set)
    ZONE="$1"; NAME="$2"; TYPE="$3"; VALUE="$4"
    ZONE_ID=$(curl -s -H "Authorization: Bearer $TOKEN" \
      "https://api.cloudflare.com/client/v4/zones?name=$ZONE" | python3 -c "import sys,json; r=json.load(sys.stdin)['result']; print(r[0]['id'] if r else '')")
    curl -s -X POST -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
      "https://api.cloudflare.com/client/v4/zones/$ZONE_ID/dns_records" \
      -d "{\"type\":\"$TYPE\",\"name\":\"$NAME\",\"content\":\"$VALUE\",\"proxied\":true}" | python3 -c "import sys,json; d=json.load(sys.stdin); print('ok' if d['success'] else d['errors'])"
    ;;
  *)
    echo "usage: dns [list ZONE|set ZONE NAME TYPE VALUE]"
    ;;
esac
