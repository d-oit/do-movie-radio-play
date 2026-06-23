#!/usr/bin/env bash
# scripts/validate-workflows.sh
# Validates GitHub Actions workflow YAML structure.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORKFLOWS_DIR="$REPO_ROOT/.github/workflows"

if [[ ! -d "$WORKFLOWS_DIR" ]]; then
  echo "No workflows directory found."
  exit 0
fi

FAILED=0

for workflow in "$WORKFLOWS_DIR"/*.yml "$WORKFLOWS_DIR"/*.yaml; do
  [[ -f "$workflow" ]] || continue
  name=$(basename "$workflow")

  # Check required top-level keys
  if ! grep -q "^name:" "$workflow"; then
    echo "❌ $name: missing 'name' field"
    FAILED=1
    continue
  fi

  if ! grep -q "^on:" "$workflow" && ! grep -q "^\"on\":" "$workflow"; then
    echo "❌ $name: missing 'on' trigger"
    FAILED=1
    continue
  fi

  if ! grep -q "^jobs:" "$workflow"; then
    echo "❌ $name: missing 'jobs' section"
    FAILED=1
    continue
  fi

  echo "✅ $name: valid structure"
done

if [[ $FAILED -eq 0 ]]; then
  echo "All workflows valid."
else
  echo "Workflow validation failed."
  exit 1
fi
