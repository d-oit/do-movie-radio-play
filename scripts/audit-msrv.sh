#!/usr/bin/env bash
# scripts/audit-msrv.sh
# Checks MSRV compliance across the workspace.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT" || exit 1

MSRV="1.88"
FAILED=0

echo "Checking MSRV compliance (target: $MSRV)..."

# 1. Check rust-toolchain.toml
if [[ -f "rust-toolchain.toml" ]]; then
  TOOLCHAIN=$(grep 'channel' rust-toolchain.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
  if [[ "$TOOLCHAIN" == "$MSRV" || "$TOOLCHAIN" == "stable" ]]; then
    echo "✅ rust-toolchain.toml: $TOOLCHAIN"
  else
    echo "❌ rust-toolchain.toml channel '$TOOLCHAIN' does not match MSRV $MSRV or stable"
    FAILED=1
  fi
else
  echo "⚠️  rust-toolchain.toml not found"
fi

# 2. Check root Cargo.toml workspace.package rust-version
if grep -qE "^rust-version[[:space:]]*=[[:space:]]*\"${MSRV}\"" Cargo.toml; then
  echo "✅ Cargo.toml [workspace.package]: rust-version = \"$MSRV\""
else
  echo "❌ Cargo.toml [workspace.package]: missing or mismatched rust-version (expected \"$MSRV\")"
  FAILED=1
fi

# 3. Check member Cargo.toml files for rust-version inheritance
MEMBER_MANIFESTS=()
while IFS= read -r file; do
  MEMBER_MANIFESTS+=("$file")
done < <(find crates benchmarks -name "Cargo.toml" -type f 2>/dev/null)

for manifest in "${MEMBER_MANIFESTS[@]}"; do
  if grep -q "rust-version.workspace = true" "$manifest" || grep -qE "rust-version[[:space:]]*=[[:space:]]*\"${MSRV}\"" "$manifest"; then
    echo "✅ $manifest: valid rust-version setting"
  else
    echo "❌ $manifest: missing rust-version setting"
    FAILED=1
  fi
done

# 4. Check Cargo.toml edition
if grep -q 'edition = "2021"' Cargo.toml; then
  echo "✅ Cargo.toml: edition 2021 (compatible with MSRV $MSRV)"
elif grep -q 'edition = "2024"' Cargo.toml; then
  echo "⚠️  Cargo.toml: edition 2024 may require newer Rust than MSRV $MSRV"
fi

if [[ $FAILED -ne 0 ]]; then
  echo "❌ MSRV audit failed."
  exit 1
fi

echo "MSRV audit passed."
