#!/usr/bin/env bash
# scripts/generate-skills-md.sh
# Auto-generates .agents/SKILLS.md from skill frontmatter.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT" || exit 1

SKILLS_DIR=".agents/skills"
OUTPUT=".agents/SKILLS.md"

if [[ ! -d "$SKILLS_DIR" ]]; then
  echo "No skills directory found."
  exit 0
fi

{
  echo "# Available Skills"
  echo ""
  echo "| Skill | Category | Description |"
  echo "|-------|----------|-------------|"

  for skill_dir in "$SKILLS_DIR"/*/; do
    [[ -d "$skill_dir" ]] || continue
    skill_name=$(basename "$skill_dir")
    skill_file="${skill_dir}SKILL.md"

    [[ "$skill_name" != _* ]] && [[ "$skill_name" != .* ]] || continue

    if [[ ! -f "$skill_file" ]]; then
      echo "| $skill_name | - | Missing SKILL.md |"
      continue
    fi

    # Extract frontmatter fields
    category=$(awk '/^---$/{n++} n==1 && /^category:/{gsub(/^category: */, ""); print; exit}' "$skill_file")
    description=$(awk '/^---$/{n++} n==1 && /^description:/{gsub(/^description: */, ""); print; exit}' "$skill_file")

    echo "| \`$skill_name\` | ${category:--} | ${description:--} |"
  done
} > "$OUTPUT"

echo "Generated $OUTPUT"
