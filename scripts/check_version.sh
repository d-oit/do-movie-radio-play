#!/usr/bin/env bash
# scripts/check_version.sh
# Validates VERSION file consistency with Cargo.toml and optional git tag.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

VERSION_FILE="$REPO_ROOT/VERSION"
CARGO_TOML="$REPO_ROOT/Cargo.toml"

if [[ ! -f "$VERSION_FILE" ]]; then
  echo "❌ Error: VERSION file not found at $VERSION_FILE"
  exit 1
fi

VERSION=$(tr -d '[:space:]' < "$VERSION_FILE")

if [[ -z "$VERSION" ]]; then
  echo "❌ Error: VERSION file is empty"
  exit 1
fi

if [[ ! -f "$CARGO_TOML" ]]; then
  echo "❌ Error: Cargo.toml not found at $CARGO_TOML"
  exit 1
fi

CARGO_VERSION=$(grep '^version' "$CARGO_TOML" | head -n 1 | sed 's/.*"\(.*\)".*/\1/' | tr -d '[:space:]')

if [[ "$VERSION" != "$CARGO_VERSION" ]]; then
  echo "❌ Error: VERSION file ($VERSION) does not match Cargo.toml ($CARGO_VERSION)"
  exit 1
fi

echo "✅ VERSION file ($VERSION) matches Cargo.toml ($CARGO_VERSION)"

TAG_INPUT="${1:-}"
if [[ -n "$TAG_INPUT" ]]; then
  # Remove refs/tags/ prefix if passed as a full ref
  TAG_NAME="${TAG_INPUT#refs/tags/}"
  TAG_VERSION="${TAG_NAME#v}"

  if [[ "$TAG_VERSION" != "$VERSION" ]]; then
    echo "❌ Error: Tag version '$TAG_VERSION' (from '$TAG_NAME') does not match VERSION file ($VERSION)"
    exit 1
  fi
  echo "✅ Tag version '$TAG_VERSION' matches VERSION file ($VERSION)"

  OBJ_TYPE=$(git cat-file -t "refs/tags/$TAG_NAME" 2>/dev/null || git cat-file -t "$TAG_NAME" 2>/dev/null || echo "")
  if [[ "$OBJ_TYPE" != "tag" ]]; then
    echo "❌ Error: Tag '$TAG_NAME' is not an annotated tag object (got '$OBJ_TYPE'). Release requires an annotated tag."
    exit 1
  fi
  echo "✅ Tag '$TAG_NAME' is a valid annotated tag object"
fi
