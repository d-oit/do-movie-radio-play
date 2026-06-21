#!/usr/bin/env bash
# scripts/quality_gate.sh — Full quality gate (adapted from d-oit/rust-2026-template)
# Usage: ./scripts/quality_gate.sh [--fix]
set +e
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT" || exit 1

readonly MAX_LINES=500

FIX=false
for arg in "$@"; do
  case $arg in
    --fix) FIX=true ;;
    *) echo "Unknown argument: $arg"; exit 1 ;;
  esac
done

if [[ -t 1 ]]; then
  RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; NC='\033[0m'
else
  RED=''; GREEN=''; YELLOW=''; NC=''
fi

pass() { echo -e "${GREEN}[PASS]${NC} $1"; }
fail() { echo -e "${RED}[FAIL]${NC} $1"; FAILED=1; }
warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }

FAILED=0

# --- LOC limits ---
LOC_VIOLATIONS=0
while IFS= read -r file; do
  lines=$(wc -l < "$file" 2>/dev/null || echo 0)
  if [[ "$lines" -gt "$MAX_LINES" ]]; then
    warn "  $file: $lines lines (max $MAX_LINES)"
    LOC_VIOLATIONS=$((LOC_VIOLATIONS + 1))
  fi
done < <(find src -name "*.rs" -type f 2>/dev/null)

if [[ $LOC_VIOLATIONS -gt 0 ]]; then
  fail "LOC: $LOC_VIOLATIONS files exceed $MAX_LINES lines"
else
  pass "LOC: All source files within limit"
fi

# --- Format ---
if $FIX; then
  cargo fmt --all
  pass "Format: auto-fixed"
else
  if ! cargo fmt --all -- --check 2>&1; then
    fail "Format: run './scripts/quality_gate.sh --fix'"
  else
    pass "Format: OK"
  fi
fi

# --- Clippy ---
if $FIX; then
  cargo clippy --fix --allow-dirty --allow-staged --all-targets --all-features 2>/dev/null
  pass "Clippy: auto-fixed"
else
  if ! cargo clippy --all-targets --all-features -- -D warnings 2>&1; then
    fail "Clippy: lint errors"
  else
    pass "Clippy: OK"
  fi
fi

# --- Build ---
if ! cargo build --all-targets 2>&1; then
  fail "Build: failed"
else
  pass "Build: OK"
fi

# --- Tests ---
if command -v cargo-nextest &>/dev/null; then
  if ! cargo nextest run --all-features 2>&1; then
    fail "Tests: failed"
  else
    pass "Tests (nextest): OK"
  fi
else
  if ! cargo test --all-features 2>&1; then
    fail "Tests: failed"
  else
    pass "Tests: OK"
  fi
fi

# --- Security audit (optional) ---
if command -v cargo-audit &>/dev/null; then
  if cargo audit 2>&1; then
    pass "Audit: OK"
  else
    warn "Audit: vulnerabilities found (review with 'cargo audit')"
  fi
fi

# --- Supply chain (optional) ---
if command -v cargo-deny &>/dev/null; then
  if cargo deny check 2>&1; then
    pass "Deny: OK"
  else
    warn "Deny: issues found (review with 'cargo deny check')"
  fi
fi

# --- Secret scan ---
SECRET_PATTERN="(api_key|token|secret|password|auth)[[:space:]]*[:=][[:space:]]*['\"][a-zA-Z0-9_\-]{16,}['\"]"
if grep -rE "$SECRET_PATTERN" --exclude-dir=.git --exclude-dir=target src/ config/ 2>/dev/null; then
  fail "Secret scan: potential secret detected"
else
  pass "Secret scan: OK"
fi

# --- Summary ---
echo ""
if [[ $FAILED -ne 0 ]]; then
  echo -e "${RED}✗ Quality Gate FAILED${NC}"
  exit 1
fi
echo -e "${GREEN}✓ All Quality Gates PASSED${NC}"
