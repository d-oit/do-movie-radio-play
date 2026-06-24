#!/usr/bin/env bash
# scripts/audit-msrv.sh
# Checks MSRV compliance across the workspace.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT" || exit 1

MSRV="1.88"

echo "Checking MSRV compliance (target: $MSRV)..."

# Check rust-toolchain.toml
if [[ -f "rust-toolchain.toml" ]]; then
  TOOLCHAIN=$(grep 'channel' rust-toolchain.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
  if [[ "$TOOLCHAIN" == "$MSRV" || "$TOOLCHAIN" == "stable" ]]; then
    echo "✅ rust-toolchain.toml: $TOOLCHAIN"
  else
    echo "❌ rust-toolchain.toml channel '$TOOLCHAIN' does not match MSRV $MSRV"
    exit 1
  fi
else
  echo "⚠️  rust-toolchain.toml not found"
fi

# Check Cargo.toml edition
if grep -q 'edition = "2021"' Cargo.toml; then
  echo "✅ Cargo.toml: edition 2021 (compatible with MSRV $MSRV)"
elif grep -q 'edition = "2024"' Cargo.toml; then
  echo "⚠️  Cargo.toml: edition 2024 may require newer Rust than MSRV $MSRV"
fi

echo "MSRV audit passed."
