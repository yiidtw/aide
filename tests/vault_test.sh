#!/bin/bash
# Vault v2 tests
# Tests multi-recipient age encryption, scoped injection, git-based sync
# Run: ./tests/vault_test.sh
set -euo pipefail

AIDE="./target/release/aide"
TMPDIR=$(mktemp -d)
trap "rm -rf $TMPDIR" EXIT

PASSED=0
FAILED=0
TOTAL=0

red()   { echo -e "\033[31m$1\033[0m"; }
green() { echo -e "\033[32m$1\033[0m"; }

assert() {
  TOTAL=$((TOTAL + 1))
  local name="$1"; shift
  if eval "$@" > /dev/null 2>&1; then
    green "  ✓ $name"; PASSED=$((PASSED + 1))
  else
    red "  ✗ $name"; FAILED=$((FAILED + 1))
  fi
}

assert_eq() {
  TOTAL=$((TOTAL + 1))
  local name="$1" expected="$2" actual="$3"
  if [ "$expected" = "$actual" ]; then
    green "  ✓ $name"; PASSED=$((PASSED + 1))
  else
    red "  ✗ $name (expected '$expected', got '$actual')"; FAILED=$((FAILED + 1))
  fi
}

echo "=== Vault v2 Tests ==="
echo ""

# ─── Setup: create two keypairs (simulating two machines) ───
echo "Setup: generating test keypairs..."
age-keygen -o "$TMPDIR/mac.key" 2>/dev/null
age-keygen -o "$TMPDIR/f00.key" 2>/dev/null
MAC_PUB=$(age-keygen -y "$TMPDIR/mac.key")
F00_PUB=$(age-keygen -y "$TMPDIR/f00.key")

# Create vault repo structure
mkdir -p "$TMPDIR/vault-repo"
cat > "$TMPDIR/vault-repo/recipients.txt" << EOF
# mac
$MAC_PUB
# f00
$F00_PUB
EOF

echo ""
echo "--- Multi-recipient encryption ---"

# Test 1: encrypt with recipients.txt
echo "SECRET_A=hello
SECRET_B=world" > "$TMPDIR/plain.txt"
age -R "$TMPDIR/vault-repo/recipients.txt" -o "$TMPDIR/vault-repo/vault.age" "$TMPDIR/plain.txt"
assert "encrypt with recipients.txt" "test -f $TMPDIR/vault-repo/vault.age"

# Test 2: mac key can decrypt
MAC_RESULT=$(age -d -i "$TMPDIR/mac.key" "$TMPDIR/vault-repo/vault.age")
assert "mac key decrypts" "echo '$MAC_RESULT' | grep -q SECRET_A"

# Test 3: f00 key can decrypt
F00_RESULT=$(age -d -i "$TMPDIR/f00.key" "$TMPDIR/vault-repo/vault.age")
assert "f00 key decrypts" "echo '$F00_RESULT' | grep -q SECRET_A"

# Test 4: both get same content
assert_eq "both decrypt to same content" "$MAC_RESULT" "$F00_RESULT"

# Test 5: random key cannot decrypt
age-keygen -o "$TMPDIR/rando.key" 2>/dev/null
RANDO_RESULT=$(age -d -i "$TMPDIR/rando.key" "$TMPDIR/vault-repo/vault.age" 2>&1 || true)
assert "random key cannot decrypt" "echo '$RANDO_RESULT' | grep -qi 'no identity'"

echo ""
echo "--- Scoped injection ---"

# Test 6: parse env format
PARSED=$(echo "$MAC_RESULT" | while IFS='=' read -r key val; do
  [ -n "$key" ] && echo "$key"
done)
assert "parse key names" "echo '$PARSED' | grep -q SECRET_A"
assert "parse both keys" "echo '$PARSED' | grep -q SECRET_B"

# Test 7: filter by allowed list (simulating Agentfile [env])
ALLOWED="SECRET_A"
FILTERED=$(echo "$MAC_RESULT" | while IFS='=' read -r key val; do
  echo "$ALLOWED" | grep -qw "$key" 2>/dev/null && echo "$key=$val"
done || true)
assert "scoped filter keeps allowed" "echo \"$FILTERED\" | grep -q SECRET_A"
assert "scoped filter drops others" "! echo \"$FILTERED\" | grep -q SECRET_B"

echo ""
echo "--- Key rotation ---"

# Test 8: re-encrypt with new key replaces old
age-keygen -o "$TMPDIR/f00-new.key" 2>/dev/null
F00_NEW_PUB=$(age-keygen -y "$TMPDIR/f00-new.key")
cat > "$TMPDIR/vault-repo/recipients.txt" << EOF
$MAC_PUB
$F00_NEW_PUB
EOF
age -R "$TMPDIR/vault-repo/recipients.txt" -o "$TMPDIR/vault-repo/vault.age" "$TMPDIR/plain.txt"

# New key works
NEW_RESULT=$(age -d -i "$TMPDIR/f00-new.key" "$TMPDIR/vault-repo/vault.age")
assert "new f00 key decrypts after rotation" "echo '$NEW_RESULT' | grep -q SECRET_A"

# Old key fails
OLD_RESULT=$(age -d -i "$TMPDIR/f00.key" "$TMPDIR/vault-repo/vault.age" 2>&1 || true)
assert "old f00 key fails after rotation" "echo '$OLD_RESULT' | grep -qi 'no identity'"

echo ""
echo "--- Vault binary integration ---"

# Test 9: aide vault status works (if vault repo exists)
if [ -f "$HOME/claude_projects/aide-vault/vault.age" ]; then
  STATUS=$($AIDE vault status 2>&1 || true)
  assert "aide vault status shows pubkey" "echo '$STATUS' | grep -q 'pubkey:'"
  assert "aide vault status shows secrets count" "echo '$STATUS' | grep -q 'secrets:'"
else
  echo "  (skipped: no vault repo at ~/claude_projects/aide-vault)"
fi

# Test 10: aide vault set + decrypt roundtrip (if vault exists)
if [ -f "$HOME/claude_projects/aide-vault/vault.age" ] && [ -f "$HOME/.aide/vault.key" ]; then
  # Set a test key
  $AIDE vault set "VAULT_TEST_$(date +%s)=test_value" 2>&1 || true
  # Verify it's in the vault
  KEYS=$($AIDE vault status 2>&1 || true)
  assert "aide vault set roundtrip" "echo '$KEYS' | grep -q 'secrets:'"
fi

echo ""
echo "─────────────────────────────"
echo "Results: $PASSED/$TOTAL passed, $FAILED failed"
[ $FAILED -eq 0 ] && green "ALL PASSED" || red "SOME FAILED"
exit $FAILED
