#!/usr/bin/env bash
set -euo pipefail
for f in .agents/skills/*/SKILL.md; do
  [[ -f "$f" ]] || { echo "missing skill files"; exit 1; }
  grep -q '^---' "$f" || { echo "missing frontmatter: $f"; exit 1; }
  grep -q '## When to use' "$f" || { echo "missing sections: $f"; exit 1; }
done
echo "skills validated"
