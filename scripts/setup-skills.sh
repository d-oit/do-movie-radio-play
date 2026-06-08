#!/usr/bin/env bash
set -euo pipefail

SKILLS_DIR=".agents/skills"
errors=0

for d in "$SKILLS_DIR"/*; do
  [[ -d "$d" ]] || continue
  name="$(basename "$d")"
  skill_file="$d/SKILL.md"

  if [[ ! -f "$skill_file" ]]; then
    echo "MISSING: $name has no SKILL.md"
    errors=$((errors + 1))
    continue
  fi

  # Check required frontmatter pattern: `## Name` or `# skill-name`
  if ! head -20 "$skill_file" | grep -qE '^## |^# '; then
    echo "WARNING: $name/SKILL.md missing heading frontmatter"
  fi

  echo "ok: $name"
done

if [[ $errors -gt 0 ]]; then
  echo "FAILED: $errors skill(s) missing SKILL.md" >&2
  exit 1
fi

echo "All skills validated."
