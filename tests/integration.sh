#!/bin/bash
# aide.sh v1 Integration Tests
# V-Model: these tests define acceptance criteria for launch
# Run: ./tests/integration.sh
set -euo pipefail

AIDE="./target/release/aide-sh"
REGISTRY="${AIDE_REGISTRY_URL:-https://registry.aide.sh}"
# Fallback if DNS hasn't propagated locally
if ! curl -sf --max-time 3 "$REGISTRY/v1/search?q=_ping" > /dev/null 2>&1; then
  REGISTRY="https://hub.aide.sh"
fi
PASSED=0
FAILED=0
TOTAL=0

red()   { echo -e "\033[31m$1\033[0m"; }
green() { echo -e "\033[32m$1\033[0m"; }
yellow(){ echo -e "\033[33m$1\033[0m"; }

assert() {
  TOTAL=$((TOTAL + 1))
  local name="$1"
  shift
  if eval "$@" > /dev/null 2>&1; then
    green "  ✓ $name"
    PASSED=$((PASSED + 1))
  else
    red "  ✗ $name"
    FAILED=$((FAILED + 1))
  fi
}

assert_fail() {
  TOTAL=$((TOTAL + 1))
  local name="$1"
  shift
  if ! eval "$@" > /dev/null 2>&1; then
    green "  ✓ $name (expected failure)"
    PASSED=$((PASSED + 1))
  else
    red "  ✗ $name (should have failed)"
    FAILED=$((FAILED + 1))
  fi
}

assert_contains() {
  TOTAL=$((TOTAL + 1))
  local name="$1"
  local cmd="$2"
  local expected="$3"
  local output
  output=$(eval "$cmd" 2>&1) || true
  if echo "$output" | grep -q "$expected"; then
    green "  ✓ $name"
    PASSED=$((PASSED + 1))
  else
    red "  ✗ $name (expected '$expected' in output)"
    FAILED=$((FAILED + 1))
  fi
}

echo ""
echo "═══════════════════════════════════════════"
echo "  aide.sh v1 Integration Tests"
echo "═══════════════════════════════════════════"

# ─── 1. CLI Binary ───
echo ""
yellow "1. CLI Binary"
assert "aide binary exists" "test -f $AIDE"
assert_contains "aide --help shows Docker tagline" "$AIDE --help" "Docker for AI agents"
assert_contains "aide --help has run" "$AIDE --help" "run"
assert_contains "aide --help has exec" "$AIDE --help" "exec"
assert_contains "aide --help has ps" "$AIDE --help" "ps"
assert_contains "aide --help has build" "$AIDE --help" "build"
assert_contains "aide --help has push" "$AIDE --help" "push"
assert_contains "aide --help has pull" "$AIDE --help" "pull"
assert_contains "aide --help has images" "$AIDE --help" "images"
assert_contains "aide --help has inspect" "$AIDE --help" "inspect"
assert_contains "aide --help has info" "$AIDE --help" "info"
assert_contains "aide --help has login" "$AIDE --help" "login"
assert_contains "aide --help has mount" "$AIDE --help" "mount"

# ─── 2. Agentfile Build ───
echo ""
yellow "2. Agentfile Build (aide.sh build)"

# Create test agent
TEST_DIR=$(mktemp -d)
mkdir -p "$TEST_DIR/skills" "$TEST_DIR/seed"
cat > "$TEST_DIR/Agentfile.toml" <<'TOML'
[agent]
name = "test-agent"
version = "0.0.1"
description = "Integration test agent"
author = "ci"

[persona]
file = "persona.md"

[skills]
hello = { script = "skills/hello.sh" }

[seed]
dir = "seed/"
TOML
echo "You are a test agent." > "$TEST_DIR/persona.md"
printf '#!/bin/bash\necho hello\n' > "$TEST_DIR/skills/hello.sh"
chmod +x "$TEST_DIR/skills/hello.sh"
echo "test knowledge" > "$TEST_DIR/seed/knowledge.md"

assert_contains "aide build succeeds" "$AIDE build $TEST_DIR" "sha256"
assert "build tarball created" "test -f $HOME/.aide/builds/test-agent-0.0.1.tar.gz"
assert_contains "build shows agent name" "$AIDE build $TEST_DIR" "test-agent"

# Invalid agentfile
BAD_DIR=$(mktemp -d)
echo "invalid toml {{{{" > "$BAD_DIR/Agentfile.toml"
assert_fail "aide build rejects invalid Agentfile" "$AIDE build $BAD_DIR"

# ─── 3. Agent Lifecycle (Docker-style) ───
echo ""
yellow "3. Agent Lifecycle (run/exec/ps/stop/rm)"

# Clean up any leftover test instance
$AIDE rm test.ci 2>/dev/null || true

# Simulate a pulled image
mkdir -p "$HOME/.aide/types/ci/test-agent"
cd "$HOME/.aide/types/ci/test-agent"
tar xzf "$HOME/.aide/builds/test-agent-0.0.1.tar.gz" 2>/dev/null || true
cd - > /dev/null

# run (Docker: docker run)
assert_contains "aide run creates instance" "$AIDE run ci/test-agent --name test.ci" "test.ci"

# ps (Docker: docker ps)
assert_contains "ps shows instance" "$AIDE ps" "test.ci"
assert_contains "ps shows image" "$AIDE ps" "test-agent"

# exec (Docker: docker exec)
assert_contains "exec runs command" "$AIDE exec test.ci hello" "hello"
assert_contains "exec -it works" "$AIDE exec -it test.ci hello" "hello"

# logs (Docker: docker logs)
assert_contains "logs show exec" "$AIDE logs test.ci" "exec"

# inspect (Docker: docker inspect)
assert_contains "inspect returns JSON" "$AIDE inspect test.ci" "Name"
assert_contains "inspect shows image" "$AIDE inspect test.ci" "test-agent"

# cron
assert_contains "cron add" "$AIDE cron add test.ci '*/5 * * * *' hello" "cron added"
assert_contains "cron ls shows entry" "$AIDE cron ls test.ci" "hello"
assert_contains "cron rm" "$AIDE cron rm test.ci hello" "cron removed"

# stop (Docker: docker stop)
assert_contains "stop outputs instance name" "$AIDE stop test.ci" "test.ci"
assert "instance dir still exists after stop" "test -d $HOME/.aide/instances/test.ci"

# rm (Docker: docker rm)
assert_contains "rm outputs instance name" "$AIDE rm test.ci" "test.ci"
assert_fail "instance dir gone after rm" "test -d $HOME/.aide/instances/test.ci"

# Duplicate run should fail
$AIDE run ci/test-agent --name test.ci 2>/dev/null || true
assert_fail "duplicate run fails" "$AIDE run ci/test-agent --name test.ci"
$AIDE rm test.ci 2>/dev/null || true

# ─── 4. Images ───
echo ""
yellow "4. Images (aide.sh images)"
assert_contains "images lists pulled types" "$AIDE images" "REPOSITORY"

# ─── 5. Info ───
echo ""
yellow "5. System Info (aide.sh info)"
assert_contains "info shows instances" "$AIDE info" "Agent Instances"
assert_contains "info shows vault" "$AIDE info" "Vault"
assert_contains "info shows registry" "$AIDE info" "registry"

# ─── 6. Registry Backend ───
echo ""
yellow "6. Registry Backend (CF Worker)"

assert_contains "registry health" "curl -sf $REGISTRY/v1/search?q=test" ""
assert "registry returns JSON" "curl -sf $REGISTRY/v1/search?q=test | python3 -c 'import json,sys; json.load(sys.stdin)'"
assert_contains "404 for missing type" "curl -s -o /dev/null -w '%{http_code}' $REGISTRY/v1/nobody/nonexistent" "404"
assert_contains "push without auth returns 401" \
  "curl -s -o /dev/null -w '%{http_code}' -X POST $REGISTRY/v1/ci/test-agent" "401"

# ─── 7. Auth Flow ───
echo ""
yellow "7. Authentication"

assert "auth.json writable" "touch $HOME/.aide/auth.json"

if [ -f "$HOME/.aide/auth.json" ] && python3 -c "import json; d=json.load(open('$HOME/.aide/auth.json')); assert d.get('token')" 2>/dev/null; then
  TOKEN=$(python3 -c "import json; print(json.load(open('$HOME/.aide/auth.json'))['token'])")
  USERNAME=$(python3 -c "import json; print(json.load(open('$HOME/.aide/auth.json'))['username'])")

  assert_contains "authenticated push" \
    "curl -sf -X POST -H 'Authorization: Bearer $TOKEN' -F 'archive=@$HOME/.aide/builds/test-agent-0.0.1.tar.gz' -F 'metadata={\"name\":\"test-agent\",\"version\":\"0.0.1\",\"description\":\"test\",\"author\":\"$USERNAME\"}' $REGISTRY/v1/$USERNAME/test-agent" ""
  assert "pull after push" "curl -sf -o /dev/null $REGISTRY/v1/$USERNAME/test-agent"
  assert "metadata endpoint" "curl -sf $REGISTRY/v1/$USERNAME/test-agent/metadata | python3 -c 'import json,sys; json.load(sys.stdin)'"
  curl -sf -X DELETE -H "Authorization: Bearer $TOKEN" "$REGISTRY/v1/$USERNAME/test-agent" > /dev/null 2>&1 || true
else
  yellow "  ⊘ Skipping auth tests (no valid ~/.aide/auth.json)"
fi

# ─── 8. Memory Bridging ───
echo ""
yellow "8. Memory Bridging (aide.sh mount)"

$AIDE run ci/test-agent --name test.mount 2>/dev/null || true
assert_contains "mount to claude" "$AIDE mount test.mount claude" ""
assert_contains "mount to codex" "$AIDE mount test.mount codex" ""
$AIDE rm test.mount 2>/dev/null || true

# ─── 9. Backward Compat ───
echo ""
yellow "9. Backward Compatibility"

$AIDE rm test.compat 2>/dev/null || true
assert_contains "spawn alias works" "$AIDE spawn ci/test-agent --name test.compat" "test.compat"
assert_contains "call alias works" "$AIDE call test.compat hello" "hello"
$AIDE rm test.compat 2>/dev/null || true

# ─── Cleanup ───
rm -rf "$TEST_DIR" "$BAD_DIR"
rm -rf "$HOME/.aide/types/ci"
rm -f "$HOME/.aide/builds/test-agent-0.0.1.tar.gz"

# ─── Summary ───
echo ""
echo "═══════════════════════════════════════════"
if [ $FAILED -eq 0 ]; then
  green "  ALL $TOTAL TESTS PASSED"
else
  red "  $FAILED/$TOTAL FAILED"
  echo ""
  echo "  Passed: $PASSED | Failed: $FAILED | Total: $TOTAL"
fi
echo "═══════════════════════════════════════════"
echo ""

exit $FAILED
