#!/usr/bin/env bash
set -euo pipefail

missing=0

for f in .agents/skills/*/SKILL.md; do
  [[ -f "$f" ]] || { echo "missing skill files"; exit 1; }
  grep -q '^---' "$f" || { echo "missing frontmatter: $f"; exit 1; }
  grep -Eiq '^##[[:space:]]+When to use$' "$f" || {
    echo "missing sections: $f (expected heading: ## When to use)"
    missing=1
  }
done

if [[ "$missing" -eq 1 ]]; then
  exit 1
fi

echo "skills validated"
