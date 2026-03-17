#!/usr/bin/env bash
# mail — NTU mail (POP3/SMTP)
# usage: mail [check|unread|read N|search Q|send TO SUBJ BODY]
# env: EMAIL_USER, EMAIL_PASS, POP3_HOST, SMTP_HOST
set -euo pipefail

CMD="${1:-check}"
shift 2>/dev/null || true

USER="${EMAIL_USER:?EMAIL_USER not set. Run: aide.sh vault set EMAIL_USER=you@ntu.edu.tw}"
PASS="${EMAIL_PASS:?EMAIL_PASS not set. Run: aide.sh vault set EMAIL_PASS=your-password}"
POP3="${POP3_HOST:-pop3.ntu.edu.tw}"
SMTP="${SMTP_HOST:-smtp.ntu.edu.tw}"

case "$CMD" in
  check|unread)
    echo "=== Inbox ($USER via $POP3) ==="
    python3 -c "
import poplib, email
from email.header import decode_header

def decode_hdr(h):
    if h is None: return '?'
    parts = decode_header(h)
    return ''.join(
        p.decode(c or 'utf-8') if isinstance(p, bytes) else p
        for p, c in parts
    )

try:
    m = poplib.POP3_SSL('$POP3')
    m.user('$USER')
    m.pass_('$PASS')
    count, size = m.stat()
    print(f'  {count} messages ({size//1024} KB)')
    print()
    # Show latest 10
    start = max(1, count - 9)
    for i in range(count, max(0, count - 10), -1):
        resp, lines, octets = m.retr(i)
        raw = b'\r\n'.join(lines)
        msg = email.message_from_bytes(raw)
        frm = decode_hdr(msg['From'])[:40]
        subj = decode_hdr(msg['Subject'])[:50]
        date = msg.get('Date','')[:20]
        print(f'  {i:4d}  {date:20s}  {frm:40s}  {subj}')
    m.quit()
except Exception as e:
    print(f'  Error: {e}')
    print(f'  Check EMAIL_USER, EMAIL_PASS, POP3_HOST in vault')
" 2>/dev/null || echo "  (POP3 connection failed — check credentials)"
    ;;

  read)
    N="${1:?usage: mail read N}"
    echo "=== Message #$N ==="
    python3 -c "
import poplib, email
from email.header import decode_header

def decode_hdr(h):
    if h is None: return '?'
    parts = decode_header(h)
    return ''.join(
        p.decode(c or 'utf-8') if isinstance(p, bytes) else p
        for p, c in parts
    )

try:
    m = poplib.POP3_SSL('$POP3')
    m.user('$USER')
    m.pass_('$PASS')
    resp, lines, octets = m.retr($N)
    raw = b'\r\n'.join(lines)
    msg = email.message_from_bytes(raw)
    print(f'From:    {decode_hdr(msg[\"From\"])}')
    print(f'To:      {decode_hdr(msg[\"To\"])}')
    print(f'Subject: {decode_hdr(msg[\"Subject\"])}')
    print(f'Date:    {msg.get(\"Date\",\"?\")}')
    print()
    # Body
    if msg.is_multipart():
        for part in msg.walk():
            ct = part.get_content_type()
            if ct == 'text/plain':
                payload = part.get_payload(decode=True)
                charset = part.get_content_charset() or 'utf-8'
                print(payload.decode(charset, errors='replace'))
                break
    else:
        payload = msg.get_payload(decode=True)
        charset = msg.get_content_charset() or 'utf-8'
        print(payload.decode(charset, errors='replace'))
    m.quit()
except Exception as e:
    print(f'Error: {e}')
" 2>/dev/null || echo "  (failed to read message)"
    ;;

  search)
    Q="${1:?usage: mail search QUERY}"
    echo "=== Search: $Q ==="
    python3 -c "
import poplib, email
from email.header import decode_header

def decode_hdr(h):
    if h is None: return '?'
    parts = decode_header(h)
    return ''.join(
        p.decode(c or 'utf-8') if isinstance(p, bytes) else p
        for p, c in parts
    )

try:
    m = poplib.POP3_SSL('$POP3')
    m.user('$USER')
    m.pass_('$PASS')
    count, _ = m.stat()
    q = '$Q'.lower()
    found = 0
    for i in range(count, max(0, count - 50), -1):
        resp, lines, octets = m.top(i, 0)
        raw = b'\r\n'.join(lines)
        msg = email.message_from_bytes(raw)
        subj = decode_hdr(msg['Subject']).lower()
        frm = decode_hdr(msg['From']).lower()
        if q in subj or q in frm:
            print(f'  {i:4d}  {decode_hdr(msg[\"Subject\"])[:60]}')
            found += 1
            if found >= 10: break
    if found == 0:
        print('  (no matches)')
    m.quit()
except Exception as e:
    print(f'Error: {e}')
" 2>/dev/null || echo "  (search failed)"
    ;;

  send)
    TO="${1:?usage: mail send TO SUBJECT BODY}"
    SUBJ="${2:?usage: mail send TO SUBJECT BODY}"
    BODY="${3:?usage: mail send TO SUBJECT BODY}"
    echo "=== Sending mail ==="
    echo "  To:      $TO"
    echo "  Subject: $SUBJ"
    python3 -c "
import smtplib
from email.mime.text import MIMEText

msg = MIMEText('''$BODY''')
msg['Subject'] = '''$SUBJ'''
msg['From'] = '$USER'
msg['To'] = '$TO'

try:
    s = smtplib.SMTP_SSL('$SMTP')
    s.login('$USER', '$PASS')
    s.send_message(msg)
    s.quit()
    print('  Sent.')
except Exception as e:
    print(f'  Error: {e}')
" 2>/dev/null || echo "  (send failed)"
    ;;

  *)
    echo "usage: mail [check|unread|read N|search Q|send TO SUBJ BODY]"
    exit 1
    ;;
esac
