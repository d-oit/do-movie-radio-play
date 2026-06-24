#!/usr/bin/env bash
# scripts/doctor.sh
# Environment health check — verifies toolchain, optional tools, and configs.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT" || exit 1

if [[ -t 1 ]]; then
  RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; NC='\033[0m'
else
  RED=''; GREEN=''; YELLOW=''; NC=''
fi

pass() { echo -e "${GREEN}✓${NC} $1"; }
warn() { echo -e "${YELLOW}⚠${NC} $1"; }
fail() { echo -e "${RED}✗${NC} $1"; }

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  Environment Doctor"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

# Rust toolchain
if command -v rustc &>/dev/null; then
  RUSTC_VERSION=$(rustc --version | awk '{print $2}')
  pass "rustc: $RUSTC_VERSION"
else
  fail "rustc not found"
fi

if command -v cargo &>/dev/null; then
  CARGO_VERSION=$(cargo --version | awk '{print $2}')
  pass "cargo: $CARGO_VERSION"
else
  fail "cargo not found"
fi

echo ""

# Optional tools
for tool in cargo-nextest cargo-deny cargo-audit cargo-clippy shellcheck python3 ffmpeg; do
  if command -v "$tool" &>/dev/null; then
    pass "$tool: installed"
  else
    warn "$tool: not installed (optional)"
  fi
done

echo ""

# Config files
for f in .clippy.toml deny.toml rustfmt.toml rust-toolchain.toml .shellcheckrc .pre-commit-config.yaml; do
  if [[ -f "$f" ]]; then
    pass "$f: present"
  else
    warn "$f: missing"
  fi
done

echo ""

# Skills
if [[ -d ".agents/skills" ]]; then
  SKILL_COUNT=$(find .agents/skills -name "SKILL.md" -type f | wc -l)
  pass "Skills: $SKILL_COUNT found"
else
  warn "No .agents/skills directory"
fi

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
