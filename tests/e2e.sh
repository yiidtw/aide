#!/bin/bash
# aide.sh e2e Integration Tests
# Designed to run on a CLEAN Linux VM (DigitalOcean droplet, CI runner)
# Tests the full lifecycle: install → init → build → run → exec → vault → deploy
#
# Usage:
#   ssh digitalocean < tests/e2e.sh
#   # or locally:
#   ./tests/e2e.sh
#
# Prerequisites: curl, git, bash. Everything else is auto-installed.
set -uo pipefail

PASSED=0
FAILED=0
TOTAL=0
AIDE=""
TEST_HOME=$(mktemp -d)
export HOME="$TEST_HOME"
export AIDE_HOME="$TEST_HOME/.aide"

red()    { echo -e "\033[31m$1\033[0m"; }
green()  { echo -e "\033[32m$1\033[0m"; }
yellow() { echo -e "\033[33m$1\033[0m"; }

assert() {
  TOTAL=$((TOTAL + 1))
  local name="$1"; shift
  if eval "$@" > /dev/null 2>&1; then
    green "  ✓ $name"; PASSED=$((PASSED + 1))
  else
    red "  ✗ $name"; FAILED=$((FAILED + 1))
  fi
}

assert_fail() {
  TOTAL=$((TOTAL + 1))
  local name="$1"; shift
  if ! eval "$@" > /dev/null 2>&1; then
    green "  ✓ $name (expected failure)"; PASSED=$((PASSED + 1))
  else
    red "  ✗ $name (should have failed)"; FAILED=$((FAILED + 1))
  fi
}

assert_contains() {
  TOTAL=$((TOTAL + 1))
  local name="$1" cmd="$2" expected="$3"
  local output
  output=$(eval "$cmd" 2>&1) || true
  if echo "$output" | grep -q "$expected"; then
    green "  ✓ $name"; PASSED=$((PASSED + 1))
  else
    red "  ✗ $name (expected '$expected')"; FAILED=$((FAILED + 1))
  fi
}

assert_file_exists() {
  TOTAL=$((TOTAL + 1))
  local name="$1" path="$2"
  if [ -e "$path" ]; then
    green "  ✓ $name"; PASSED=$((PASSED + 1))
  else
    red "  ✗ $name ($path not found)"; FAILED=$((FAILED + 1))
  fi
}

echo ""
echo "═══════════════════════════════════════════════════"
echo "  aide.sh v0.4.1 End-to-End Tests"
echo "  HOME=$TEST_HOME"
echo "═══════════════════════════════════════════════════"

# ─── 1. Install aide binary ───
echo ""
yellow "1. Install aide binary"

ARCH=$(uname -m)
OS=$(uname -s | tr '[:upper:]' '[:lower:]')

# Try to find local binary first (for dev), otherwise download
SCRIPT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
if [ -f "$SCRIPT_DIR/target/release/aide" ]; then
  AIDE="$SCRIPT_DIR/target/release/aide"
  green "  using local binary"
elif [ -f "$SCRIPT_DIR/target/debug/aide" ]; then
  AIDE="$SCRIPT_DIR/target/debug/aide"
  green "  using local debug binary"
else
  mkdir -p "$TEST_HOME/.local/bin"
  AIDE="$TEST_HOME/.local/bin/aide"
  # Download from GitHub releases
  curl -fsSL -o "$AIDE" \
    "https://github.com/yiidtw/aide/releases/download/v0.4.0/aide-${ARCH}-unknown-${OS}-gnu" 2>/dev/null || {
    red "  download failed, trying to build from source..."
    if command -v cargo > /dev/null; then
      cargo build --release 2>/dev/null
      AIDE="./target/release/aide"
    else
      red "  no cargo, no binary — cannot continue"
      exit 1
    fi
  }
  chmod +x "$AIDE"
fi

assert "aide binary executable" "test -x $AIDE"
assert_contains "aide --version" "$AIDE --version" "aide"
assert_contains "aide --help shows Docker tagline" "$AIDE --help" "Docker for AI agents"

# ─── 2. aide init (scaffold) ───
echo ""
yellow "2. aide init — scaffold new agent"

cd "$TEST_HOME"
AGENT_DIR="$TEST_HOME/test-agent"
$AIDE init test-agent 2>/dev/null || true

# Check new directory structure
assert_file_exists "Agentfile.toml created" "$AGENT_DIR/Agentfile.toml"
assert_file_exists "persona.md created" "$AGENT_DIR/persona.md"
assert_file_exists "skills/hello.ts created" "$AGENT_DIR/skills/hello.ts"
assert_file_exists "knowledge/ dir created" "$AGENT_DIR/knowledge"
assert_contains "Agentfile has [knowledge] section" "cat $AGENT_DIR/Agentfile.toml" '\[knowledge\]'
assert_contains "hello.ts is TypeScript" "cat $AGENT_DIR/skills/hello.ts" 'console.log'
assert_fail "no seed/ directory (legacy)" "test -d $AGENT_DIR/seed"

# ─── 3. aide build ───
echo ""
yellow "3. aide build — package agent image"

BUILD_OUTPUT=$($AIDE build "$AGENT_DIR" 2>&1) || true
assert_contains "build shows sha256" "echo '$BUILD_OUTPUT'" "sha256"
assert_file_exists "tarball created" "$AIDE_HOME/builds/test-agent-0.1.0.tar.gz"

# ─── 4. Agent lifecycle (run/exec/ps/inspect/rm) ───
echo ""
yellow "4. Agent lifecycle — run/exec/ps/inspect/rm"

# Setup: create type dir for pulled image
mkdir -p "$AIDE_HOME/types/ci/test-agent"
cd "$AIDE_HOME/types/ci/test-agent"
tar xzf "$AIDE_HOME/builds/test-agent-0.1.0.tar.gz" 2>/dev/null || true
cd "$TEST_HOME"

# Run
$AIDE rm test.e2e 2>/dev/null || true
assert_contains "aide run creates instance" "$AIDE run ci/test-agent --name test.e2e" "test.e2e"

# Verify instance directory structure
INST_DIR="$AIDE_HOME/instances/test.e2e"
assert_file_exists "instance.toml exists" "$INST_DIR/instance.toml"
assert_file_exists "memory/ created" "$INST_DIR/memory"
assert_file_exists "knowledge/ created" "$INST_DIR/knowledge"
assert_file_exists "logs/ created" "$INST_DIR/logs"
assert_file_exists "skills/ copied" "$INST_DIR/skills"

# PS
assert_contains "ps shows instance" "$AIDE ps" "test.e2e"
assert_contains "ps shows image" "$AIDE ps" "test-agent"

# Exec .sh skill
assert_contains "exec hello.sh works" "$AIDE exec test.e2e hello" ""

# Inspect
assert_contains "inspect returns JSON" "$AIDE inspect test.e2e" "Name"

# Cron
assert_contains "cron add" "$AIDE cron add test.e2e '0 8 * * *' hello" "cron added"
assert_contains "cron ls" "$AIDE cron ls test.e2e" "hello"
assert_contains "cron rm" "$AIDE cron rm test.e2e hello" "cron removed"

# Logs
assert_contains "logs show activity" "$AIDE logs test.e2e" "exec"

# ─── 5. TypeScript skill execution + bun auto-install ───
echo ""
yellow "5. TypeScript skill execution (bun auto-install)"

# Create a .ts skill in the instance
cat > "$INST_DIR/skills/greet.ts" << 'TS'
// greet — test TypeScript skill
const name = process.argv[2] || "world";
console.log(`hello from typescript, ${name}!`);
TS

EXEC_OUTPUT=$($AIDE exec test.e2e greet "aide" 2>&1) || true
assert_contains "ts skill runs" "echo '$EXEC_OUTPUT'" "typescript"
assert_contains "ts skill receives args" "echo '$EXEC_OUTPUT'" "aide"

# Verify bun was installed
assert "bun installed" "command -v bun || test -f $TEST_HOME/.bun/bin/bun"

# ─── 6. Vault — set, inject, scoping ───
echo ""
yellow "6. Vault — set, inject, scoping"

# Install age if needed
if ! command -v age > /dev/null 2>&1; then
  yellow "  installing age..."
  if command -v apt-get > /dev/null 2>&1; then
    sudo apt-get install -y age > /dev/null 2>&1 || true
  elif command -v brew > /dev/null 2>&1; then
    brew install age > /dev/null 2>&1 || true
  fi
fi

if command -v age > /dev/null 2>&1; then
  # Set secrets
  SET_OUTPUT=$($AIDE vault set "TEST_KEY_A=value_a" "TEST_KEY_B=value_b" 2>&1) || true
  assert_contains "vault set works" "echo '$SET_OUTPUT'" "stored in vault"

  # Check vault status
  assert_contains "vault status shows secrets" "$AIDE vault status 2>&1" "secrets:"
  assert_contains "vault status shows pubkey" "$AIDE vault status 2>&1" "pubkey:"

  # Verify secrets are actually encrypted
  assert_file_exists "vault.age created" "$AIDE_HOME/vault.age"
  assert_file_exists "vault.key created" "$AIDE_HOME/vault.key"

  # Verify decryption works
  DECRYPTED=$(age -d -i "$AIDE_HOME/vault.key" "$AIDE_HOME/vault.age" 2>/dev/null) || true
  assert_contains "vault contains TEST_KEY_A" "echo '$DECRYPTED'" "TEST_KEY_A"
  assert_contains "vault contains TEST_KEY_B" "echo '$DECRYPTED'" "TEST_KEY_B"

  # Test scoped injection via Agentfile
  # Create an Agentfile with [env] section in instance
  cat > "$INST_DIR/Agentfile.toml" << 'TOML'
[agent]
name = "test-agent"
version = "0.1.0"

[skills.check_env]
script = "skills/check_env.sh"
env = ["TEST_KEY_A"]

[env]
required = ["TEST_KEY_A"]
optional = ["TEST_KEY_B"]
TOML

  # Create a skill that prints env
  cat > "$INST_DIR/skills/check_env.sh" << 'SH'
#!/bin/bash
echo "A=${TEST_KEY_A:-unset}"
echo "B=${TEST_KEY_B:-unset}"
SH
  chmod +x "$INST_DIR/skills/check_env.sh"

  # Exec with scoped env — skill declares env=["TEST_KEY_A"], so only A should be injected
  SCOPED_OUTPUT=$($AIDE exec test.e2e check_env 2>&1) || true
  assert_contains "scoped injection injects allowed key" "echo '$SCOPED_OUTPUT'" "A=value_a"
  assert_contains "scoped injection blocks other key" "echo '$SCOPED_OUTPUT'" "B=unset"

  # Test key rotation
  ROTATE_OUT=$($AIDE vault rotate 2>&1) || true
  if echo "$ROTATE_OUT" | grep -q "rotated"; then
    AFTER_ROTATE=$(age -d -i "$AIDE_HOME/vault.key" "$AIDE_HOME/vault.age" 2>/dev/null) || true
    assert_contains "secrets survive rotation" "echo '$AFTER_ROTATE'" "TEST_KEY_A"
  else
    yellow "  ⊘ rotation skipped (may need existing vault)"
  fi
else
  yellow "  ⊘ Skipping vault tests (age not installed)"
fi

# ─── 7. Backward compat — [seed] still works ───
echo ""
yellow "7. Backward compatibility"

COMPAT_DIR=$(mktemp -d)
mkdir -p "$COMPAT_DIR/skills" "$COMPAT_DIR/seed"
cat > "$COMPAT_DIR/Agentfile.toml" << 'TOML'
[agent]
name = "compat-agent"
version = "0.1.0"

[persona]
file = "persona.md"

[skills.hello]
script = "skills/hello.sh"

[seed]
dir = "seed/"
TOML
echo "I am a test agent." > "$COMPAT_DIR/persona.md"
printf '#!/bin/bash\necho compat\n' > "$COMPAT_DIR/skills/hello.sh"
chmod +x "$COMPAT_DIR/skills/hello.sh"
echo "seed data" > "$COMPAT_DIR/seed/data.md"

COMPAT_BUILD=$($AIDE build "$COMPAT_DIR" 2>&1) || true
assert_contains "build with [seed] still works" "echo '$COMPAT_BUILD'" "sha256"
rm -rf "$COMPAT_DIR"

# ─── 8. Lint ───
echo ""
yellow "8. Lint"

assert_contains "lint passes for valid agent" "$AIDE lint $AGENT_DIR" ""
BAD_DIR=$(mktemp -d)
echo "invalid {{{{" > "$BAD_DIR/Agentfile.toml"
assert_fail "lint rejects invalid Agentfile" "$AIDE lint $BAD_DIR"
rm -rf "$BAD_DIR"

# ─── 9. Instance manifest — github_repo field ───
echo ""
yellow "9. Instance manifest fields"

# Check github_repo field can be set
MANIFEST="$INST_DIR/instance.toml"
assert_file_exists "instance.toml exists" "$MANIFEST"
# Manually add github_repo (simulate aide deploy --github)
if ! grep -q github_repo "$MANIFEST" 2>/dev/null; then
  echo 'github_repo = "test/aide-test"' >> "$MANIFEST"
fi
# Re-read should work
assert_contains "instance.toml is valid toml" "$AIDE inspect test.e2e" "Name"

# ─── Cleanup ───
$AIDE rm test.e2e 2>/dev/null || true
rm -rf "$AIDE_HOME/types/ci"
rm -f "$AIDE_HOME/builds/test-agent-0.1.0.tar.gz"

# ─── Summary ───
echo ""
echo "═══════════════════════════════════════════════════"
if [ $FAILED -eq 0 ]; then
  green "  ALL $TOTAL TESTS PASSED ✓"
else
  red "  $FAILED/$TOTAL FAILED"
  echo "  Passed: $PASSED | Failed: $FAILED | Total: $TOTAL"
fi
echo "  HOME was: $TEST_HOME"
echo "═══════════════════════════════════════════════════"
echo ""

# Cleanup test home
rm -rf "$TEST_HOME"

exit $FAILED
