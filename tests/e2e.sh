#!/usr/bin/env bash
# End-to-end verification against real Docker containers: builds the CLI,
# creates a throwaway project, and exercises every control the plan
# promises — hardening (isolator selftest), egress allow/deny, the audit
# chain (folding, verification, a live tamper attempt, export), and the
# git-push audit tag. Exits non-zero if anything fails. Requires Docker
# (OrbStack or otherwise) running locally; does not touch GitHub.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CLI="$ROOT/cli/target/debug/isolator"
PROJECT="e2e-$$"
FAILURES=0

pass() { echo "  [PASS] $1"; }
fail() { echo "  [FAIL] $1"; FAILURES=$((FAILURES + 1)); }
step() { echo; echo "== $1 =="; }

expect_success() {
  local desc="$1"; shift
  if "$@" >/tmp/isolator-e2e-out.$$ 2>&1; then
    pass "$desc"
  else
    fail "$desc (expected success, got exit $?)"
    sed 's/^/         /' /tmp/isolator-e2e-out.$$
  fi
  cat /tmp/isolator-e2e-out.$$
  rm -f /tmp/isolator-e2e-out.$$
}

expect_failure() {
  local desc="$1"; shift
  if "$@" >/tmp/isolator-e2e-out.$$ 2>&1; then
    fail "$desc (expected failure, but it succeeded)"
  else
    pass "$desc"
  fi
  cat /tmp/isolator-e2e-out.$$
  rm -f /tmp/isolator-e2e-out.$$
}

assert_contains() {
  local desc="$1" haystack="$2" needle="$3"
  if grep -qF -- "$needle" <<<"$haystack"; then
    pass "$desc"
  else
    fail "$desc (did not find '$needle')"
  fi
}

cleanup() {
  step "cleanup"
  "$CLI" down "$PROJECT" >/dev/null 2>&1 || true
  rm -rf "$HOME/.isolator/projects/$PROJECT"
  docker volume rm "${PROJECT}-workspace" "${PROJECT}-cache" >/dev/null 2>&1 || true
  rm -f /tmp/isolator-e2e-out.$$ /tmp/isolator-e2e-export-$$*.tar.gz
}
trap cleanup EXIT

step "build the CLI"
if (cd "$ROOT/cli" && cargo build -q); then
  pass "cargo build"
else
  fail "cargo build"
  echo "cannot continue without a working binary"
  exit 1
fi

step "build images (skipped if already present)"
for img in base node rust python egress; do
  if ! docker image inspect "isolator/${img}:latest" >/dev/null 2>&1; then
    echo "  isolator/${img}:latest missing — run ./images/build.sh first"
    exit 1
  fi
done
pass "all isolator images present"

step "create project '$PROJECT'"
expect_success "isolator new" "$CLI" new "$PROJECT" --image isolator/base:latest

step "static hardening + active breakout battery"
SELFTEST_OUT=$("$CLI" selftest "$PROJECT" 2>&1)
if [ $? -eq 0 ]; then pass "isolator selftest exits 0"; else fail "isolator selftest exits 0"; fi
echo "$SELFTEST_OUT" | sed 's/^/         /'
assert_contains "selftest reports all checks passed" "$SELFTEST_OUT" "all checks passed"
assert_contains "selftest ran the canary-domain probe" "$SELFTEST_OUT" "canary domain is unreachable"
assert_contains "selftest ran the read-only-fs probe" "$SELFTEST_OUT" "root filesystem rejects writes"
assert_contains "selftest ran the docker.sock probe" "$SELFTEST_OUT" "docker.sock is not present"

step "egress: allow-listed domain succeeds"
expect_success "curl https://github.com" "$CLI" run "$PROJECT" -- curl -sS -o /dev/null --max-time 10 https://github.com

step "egress: non-allow-listed domain is blocked"
expect_failure "curl https://example.com" "$CLI" run "$PROJECT" -- curl -sS -o /dev/null --max-time 10 https://example.com

step "git push is tagged distinctly in the audit chain"
"$CLI" run "$PROJECT" -- git -C /workspace init -q >/dev/null 2>&1 || true
"$CLI" run "$PROJECT" -- git -C /workspace push origin main >/dev/null 2>&1 || true
CHAIN_FILE="$HOME/.isolator/projects/$PROJECT/audit/chain.jsonl"
if grep -q '"kind":"git-push"' "$CHAIN_FILE" 2>/dev/null; then
  pass "a git-push attempt was tagged kind=git-push in the chain"
else
  fail "no kind=git-push entry found in $CHAIN_FILE"
fi

step "audit: egress log folds into the chain"
AUDIT_OUT=$("$CLI" audit "$PROJECT" 2>&1)
assert_contains "audit output mentions folded entries" "$AUDIT_OUT" "folded in"
assert_contains "audit chain recorded the github.com allow" "$AUDIT_OUT" "github.com"
assert_contains "audit chain recorded the example.com deny" "$AUDIT_OUT" "example.com"

step "audit: chain verifies OK before tampering"
VERIFY_OUT=$("$CLI" audit "$PROJECT" --verify 2>&1)
if [ $? -eq 0 ]; then pass "verify exits 0 on an untampered chain"; else fail "verify exits 0 on an untampered chain"; fi
assert_contains "verify reports chain OK" "$VERIFY_OUT" "chain OK"

step "audit: a live tamper attempt against the chain file is detected"
cp "$CHAIN_FILE" "${CHAIN_FILE}.bak"
python3 - "$CHAIN_FILE" <<'PY'
import json, sys
path = sys.argv[1]
with open(path) as f:
    lines = f.read().splitlines()
entry = json.loads(lines[0])
entry.setdefault("data", {})["tampered_by_e2e_test"] = True
lines[0] = json.dumps(entry)
with open(path, "w") as f:
    f.write("\n".join(lines) + "\n")
PY
TAMPER_OUT=$("$CLI" audit "$PROJECT" --verify 2>&1)
TAMPER_EXIT=$?
if [ "$TAMPER_EXIT" -ne 0 ]; then pass "verify exits non-zero after tampering"; else fail "verify exits non-zero after tampering (it exited 0!)"; fi
assert_contains "verify reports TAMPERED" "$TAMPER_OUT" "TAMPERED"
mv "${CHAIN_FILE}.bak" "$CHAIN_FILE"
RESTORED_OUT=$("$CLI" audit "$PROJECT" --verify 2>&1)
if [ $? -eq 0 ]; then pass "restoring the original file verifies OK again"; else fail "restoring the original file verifies OK again"; fi
echo "$RESTORED_OUT" | sed 's/^/         /'

step "audit: export bundle"
EXPORT_DIR="/tmp"
expect_success "isolator audit --export" "$CLI" audit "$PROJECT" --export "$EXPORT_DIR"
BUNDLE=$(ls -t "$EXPORT_DIR"/"${PROJECT}"-audit-*.tar.gz 2>/dev/null | head -1)
if [ -n "${BUNDLE:-}" ] && tar -tzf "$BUNDLE" | grep -q "chain.jsonl"; then
  pass "export bundle exists and contains chain.jsonl"
  rm -f "$BUNDLE"
else
  fail "export bundle missing or does not contain chain.jsonl"
fi

echo
if [ "$FAILURES" -eq 0 ]; then
  echo "e2e: all checks passed."
  exit 0
else
  echo "e2e: $FAILURES check(s) failed."
  exit 1
fi
