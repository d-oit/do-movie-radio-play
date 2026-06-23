#!/usr/bin/env bash
# scripts/quality_gate.sh
# Full quality gate with auto-detection for multiple languages.
# Usage: ./scripts/quality_gate.sh [--fix]
# Exit 0 = success, Exit 1 = errors.
set +e
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT" || exit 1

# --- Configuration ---
readonly GIT_EXCLUDE="./.git/*"
readonly MAX_LINES_PER_SOURCE_FILE=500

# --- Parse arguments ---
FIX=false
for arg in "$@"; do
  case $arg in
    --fix) FIX=true ;;
    *) echo "Unknown argument: $arg"; exit 1 ;;
  esac
done

# --- Colors ---
if [[ -t 1 ]] && [[ "${FORCE_COLOR:-}" != "0" ]]; then
  RED='\033[0;31m'
  GREEN='\033[0;32m'
  YELLOW='\033[1;33m'
  BLUE='\033[0;34m'
  NC='\033[0m'
else
  RED=''
  GREEN=''
  YELLOW=''
  BLUE=''
  NC=''
fi

pass() { echo -e "${GREEN}[PASS]${NC} $1"; }
fail() { echo -e "${RED}[FAIL]${NC} $1"; FAILED=1; }
warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
info() { echo -e "${BLUE}[INFO]${NC} $1"; }

FAILED=0

printf "Running quality gate...\n\n"

# ============================================================
# 1. LOC LIMITS
# ============================================================
info "Enforcing LOC limits (max ${MAX_LINES_PER_SOURCE_FILE} lines per file)..."
LOC_VIOLATIONS=0
while IFS= read -r file; do
  lines=$(wc -l < "$file" 2>/dev/null || echo 0)
  if [[ "$lines" -gt "$MAX_LINES_PER_SOURCE_FILE" ]]; then
    warn "  $file: $lines lines (max $MAX_LINES_PER_SOURCE_FILE)"
    LOC_VIOLATIONS=$((LOC_VIOLATIONS + 1))
  fi
done < <(find crates -path "*/src/*.rs" -type f 2>/dev/null)

if [[ $LOC_VIOLATIONS -gt 0 ]]; then
  fail "LOC: $LOC_VIOLATIONS files exceed ${MAX_LINES_PER_SOURCE_FILE} lines"
else
  pass "LOC: All source files within limit"
fi
printf "\n"

# ============================================================
# 2. SKILL VALIDATION
# ============================================================
info "Validating skills..."
if [[ -f "./scripts/validate-skills.sh" ]]; then
  if ./scripts/validate-skills.sh >/dev/null 2>&1; then
    pass "Skills: valid"
  else
    warn "Skills: validation reported issues (run ./scripts/validate-skills.sh for details)"
  fi
else
  warn "Skills: validate-skills.sh not found"
fi
printf "\n"

# ============================================================
# 3. FORMAT
# ============================================================
info "Running format check..."
if $FIX; then
  cargo fmt --all
  pass "Format: auto-fixed"
else
  if ! OUTPUT=$(cargo fmt --all -- --check 2>&1); then
    fail "Format: run 'cargo fmt --all' to fix"
    printf "%s\n" "$OUTPUT" >&2
  else
    pass "Format: OK"
  fi
fi
printf "\n"

# ============================================================
# 4. CLIPPY
# ============================================================
info "Running clippy..."
if $FIX; then
  cargo clippy --fix --allow-dirty --allow-staged --all-targets --all-features 2>/dev/null
  pass "Clippy: auto-fixed"
else
  if ! OUTPUT=$(cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1); then
    fail "Clippy: fix lint errors above"
    printf "%s\n" "$OUTPUT" >&2
  else
    pass "Clippy: OK"
  fi
fi
printf "\n"

# ============================================================
# 5. BUILD
# ============================================================
info "Running build..."
if ! OUTPUT=$(cargo build --workspace --all-targets 2>&1); then
  fail "Build: failed"
  printf "%s\n" "$OUTPUT" >&2
else
  pass "Build: OK"
fi
printf "\n"

# ============================================================
# 6. TESTS
# ============================================================
info "Running tests..."
if command -v cargo-nextest &>/dev/null; then
  if ! OUTPUT=$(cargo nextest run --workspace --all-features 2>&1); then
    fail "Tests: failed"
    printf "%s\n" "$OUTPUT" >&2
  else
    pass "Tests (nextest): OK"
  fi
else
  if ! OUTPUT=$(cargo test --workspace --all-features 2>&1); then
    fail "Tests: failed"
    printf "%s\n" "$OUTPUT" >&2
  else
    pass "Tests: OK"
  fi
fi
printf "\n"

# ============================================================
# 7. DOC TESTS
# ============================================================
info "Running doc tests..."
if ! OUTPUT=$(cargo test --doc --all-features 2>&1); then
  fail "Doc tests: failed"
  printf "%s\n" "$OUTPUT" >&2
else
  pass "Doc tests: OK"
fi
printf "\n"

# ============================================================
# 8. SECURITY AUDIT (optional)
# ============================================================
if command -v cargo-audit &>/dev/null; then
  info "Running security audit..."
  AUDIT_OUTPUT=$(cargo audit 2>&1) && AUDIT_EXIT=$? || AUDIT_EXIT=$?
  if [ $AUDIT_EXIT -ne 0 ]; then
    if echo "$AUDIT_OUTPUT" | grep -q "unsupported CVSS version"; then
      warn "cargo-audit: Skipping due to RustSec advisory format issue"
    else
      fail "Security audit: vulnerabilities found"
    fi
  else
    pass "Audit: OK"
  fi
  printf "\n"
fi

# ============================================================
# 9. SUPPLY CHAIN (optional)
# ============================================================
if command -v cargo-deny &>/dev/null; then
  info "Running supply chain check..."
  if ! OUTPUT=$(cargo deny check 2>&1); then
    fail "cargo-deny: violations found"
    printf "%s\n" "$OUTPUT" >&2
  else
    pass "Deny: OK"
  fi
  printf "\n"
fi

# ============================================================
# 10. SHELLCHECK
# ============================================================
if command -v shellcheck &>/dev/null; then
  info "Running shellcheck..."
  TMP_SH_LIST=$(mktemp)
  find . -name "*.sh" -not -path "./target/*" -not -path "./.git/*" -print0 2>/dev/null > "$TMP_SH_LIST" || true
  if [[ -s "$TMP_SH_LIST" ]]; then
    if ! xargs -0 shellcheck --severity=warning < "$TMP_SH_LIST" 2>/dev/null; then
      fail "shellcheck: issues found"
    else
      pass "shellcheck: OK"
    fi
  fi
  rm -f -- "$TMP_SH_LIST"
  printf "\n"
fi

# ============================================================
# 11. SECRET SCAN
# ============================================================
info "Scanning for potential secrets..."
SECRET_PATTERN="(api_key|token|secret|password|auth|key)[[:space:]]*[:=][[:space:]]*['\"][a-zA-Z0-9_\-]{16,}['\"]"
EXCLUDE_DIR='--exclude-dir=.git --exclude-dir=target --exclude-dir=.agents --exclude-dir=.opencode'
EXCLUDE_SECRET='example\.com|example\.org|test\.com|GITHUB_TOKEN|CARGO_REGISTRY_TOKEN|worktree'

if grep -rE "$SECRET_PATTERN" $EXCLUDE_DIR crates/ config/ 2>/dev/null | grep -vE "$EXCLUDE_SECRET"; then
  fail "Secret Scan: potential secret detected"
else
  pass "Secret Scan: OK"
fi
printf "\n"

# ============================================================
# 12. AGENT ENTRYPOINTS
# ============================================================
info "Validating agent entrypoints..."
if [[ -f "./scripts/validate-agent-entrypoints.sh" ]]; then
  if ./scripts/validate-agent-entrypoints.sh >/dev/null 2>&1; then
    pass "Agents: entrypoints valid"
  else
    warn "Agents: validation issues (run ./scripts/validate-agent-entrypoints.sh for details)"
  fi
else
  warn "validate-agent-entrypoints.sh not found"
fi
printf "\n"

# ============================================================
# SUMMARY
# ============================================================
if [[ $FAILED -ne 0 ]]; then
  printf "${RED}─────────────────────────────────────────────────────────────────${NC}\n"
  printf "${RED}│ ✗ Quality Gate FAILED                                         │${NC}\n"
  printf "${RED}─────────────────────────────────────────────────────────────────${NC}\n"
  exit 1
fi

printf "${GREEN}─────────────────────────────────────────────────────────────────${NC}\n"
printf "${GREEN}│ ✓ All Quality Gates PASSED                                    │${NC}\n"
printf "${GREEN}─────────────────────────────────────────────────────────────────${NC}\n"
