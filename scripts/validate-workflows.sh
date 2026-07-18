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

  echo "✅ $name: valid workflow structure"
done

# Validate GitHub issue templates
TEMPLATES_DIR="$REPO_ROOT/.github/ISSUE_TEMPLATE"
if [[ -d "$TEMPLATES_DIR" ]]; then
  for template in "$TEMPLATES_DIR"/*.yml "$TEMPLATES_DIR"/*.yaml; do
    [[ -f "$template" ]] || continue
    name=$(basename "$template")

    # Check required top-level keys
    if ! grep -q "^name:" "$template" && ! grep -q "^name[[:space:]]*:" "$template"; then
      echo "❌ $name: missing 'name' field"
      FAILED=1
      continue
    fi

    if ! grep -q "^description:" "$template" && ! grep -q "^description[[:space:]]*:" "$template"; then
      echo "❌ $name: missing 'description' field"
      FAILED=1
      continue
    fi

    if ! grep -q "^body:" "$template" && ! grep -q "^body[[:space:]]*:" "$template"; then
      echo "❌ $name: missing 'body' section"
      FAILED=1
      continue
    fi

    echo "✅ $name: valid issue template structure"
  done
fi

if [[ $FAILED -eq 0 ]]; then
  echo "All workflows and issue templates valid."
else
  echo "Validation failed."
  exit 1
fi
