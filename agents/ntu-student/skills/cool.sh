#!/usr/bin/env bash
# cool — NTU COOL (Canvas LMS) scanner
# usage: cool [courses|assignments|grades|todos|summary|announcements|scan]
# env: NTU_COOL_TOKEN
set -euo pipefail

CMD="${1:-scan}"
shift 2>/dev/null || true

BASE_URL="${NTU_COOL_BASE_URL:-https://cool.ntu.edu.tw}"
TOKEN="${NTU_COOL_TOKEN:?NTU_COOL_TOKEN not set. Run: aide.sh vault set NTU_COOL_TOKEN=your-token}"

api() {
  curl -sf -H "Authorization: Bearer $TOKEN" "$BASE_URL/api/v1$1" 2>/dev/null
}

case "$CMD" in
  courses)
    echo "=== Enrolled Courses ==="
    api "/courses?enrollment_state=active&per_page=50" | \
      python3 -c "
import sys, json
courses = json.load(sys.stdin)
for c in courses:
    print(f\"  {c.get('course_code','?'):20s} {c.get('name','?')}\")
" 2>/dev/null || echo "  (failed to fetch — check NTU_COOL_TOKEN)"
    ;;

  assignments)
    echo "=== Upcoming Assignments ==="
    api "/courses?enrollment_state=active&per_page=50" | \
      python3 -c "
import sys, json, urllib.request, os
courses = json.load(sys.stdin)
token = os.environ['NTU_COOL_TOKEN']
base = os.environ.get('NTU_COOL_BASE_URL', 'https://cool.ntu.edu.tw')
for c in courses:
    cid = c['id']
    req = urllib.request.Request(
        f'{base}/api/v1/courses/{cid}/assignments?order_by=due_at&per_page=10',
        headers={'Authorization': f'Bearer {token}'})
    try:
        resp = urllib.request.urlopen(req)
        assignments = json.loads(resp.read())
        upcoming = [a for a in assignments if a.get('due_at') and not a.get('has_submitted_submissions')]
        if upcoming:
            print(f\"\\n  [{c.get('course_code','?')}]\")
            for a in upcoming[:5]:
                due = a['due_at'][:16].replace('T',' ')
                print(f\"    {a['name']:40s} due: {due}\")
    except: pass
" 2>/dev/null || echo "  (failed to fetch)"
    ;;

  grades)
    echo "=== Current Grades ==="
    api "/courses?enrollment_state=active&include[]=total_scores&per_page=50" | \
      python3 -c "
import sys, json
courses = json.load(sys.stdin)
for c in courses:
    enrollments = c.get('enrollments', [])
    for e in enrollments:
        if e.get('type') == 'student':
            score = e.get('computed_current_score', '?')
            grade = e.get('computed_current_grade', '')
            print(f\"  {c.get('course_code','?'):20s} {score} {grade}\")
" 2>/dev/null || echo "  (failed to fetch)"
    ;;

  todos)
    echo "=== TODO Items ==="
    api "/users/self/todo?per_page=20" | \
      python3 -c "
import sys, json
items = json.load(sys.stdin)
if not items:
    print('  (no pending items)')
else:
    for t in items:
        a = t.get('assignment', {})
        print(f\"  {a.get('name','?'):40s} due: {a.get('due_at','?')[:16] if a.get('due_at') else '?'}\")
" 2>/dev/null || echo "  (failed to fetch)"
    ;;

  announcements)
    echo "=== Recent Announcements ==="
    # Get active course IDs first
    COURSE_IDS=$(api "/courses?enrollment_state=active&per_page=50" | \
      python3 -c "import sys,json; print('&'.join([f'context_codes[]=course_{c[\"id\"]}' for c in json.load(sys.stdin)]))" 2>/dev/null)
    if [ -n "$COURSE_IDS" ]; then
      api "/announcements?$COURSE_IDS&per_page=10&latest_only=true" | \
        python3 -c "
import sys, json
items = json.load(sys.stdin)
for a in items[:10]:
    date = a.get('posted_at','')[:10]
    print(f\"  [{date}] {a.get('title','?')}\")
" 2>/dev/null || echo "  (failed to fetch)"
    fi
    ;;

  summary|scan)
    echo "=== NTU COOL Daily Summary ==="
    echo ""
    "$0" todos
    echo ""
    "$0" assignments
    echo ""
    "$0" announcements
    ;;

  *)
    echo "usage: cool [courses|assignments|grades|todos|summary|announcements|scan]"
    exit 1
    ;;
esac
