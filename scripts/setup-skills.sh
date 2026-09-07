#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SKILLS_DIR="$REPO_ROOT/.agents/skills"
CLI_DIRS=(
  ".claude/skills"
  ".qwen/skills"
)

errors=0

echo "Setting up and validating skills in $SKILLS_DIR..."

if [[ ! -d "$SKILLS_DIR" ]]; then
  echo "ERROR: $SKILLS_DIR does not exist." >&2
  exit 1
fi

# 1. Validate SKILL.md files and frontmatter
for d in "$SKILLS_DIR"/*; do
  [[ -d "$d" ]] || continue
  name="$(basename "$d")"

  # Skip hidden or backup directories
  [[ "$name" != _* ]] && [[ "$name" != .* ]] || continue

  skill_file="$d/SKILL.md"

  if [[ ! -f "$skill_file" ]]; then
    echo "MISSING: $name has no SKILL.md"
    errors=$((errors + 1))
    continue
  fi

  # Check frontmatter block exists (starts with ---)
  if ! head -n 1 "$skill_file" | grep -q '^---'; then
    echo "WARNING: $name/SKILL.md missing frontmatter header (---)"
  else
    # Check for name field in frontmatter
    if ! awk '/^---$/{n++} n==1 && /^name:/{print "yes"; exit}' "$skill_file" | grep -q "yes"; then
      echo "WARNING: $name/SKILL.md missing 'name:' field in frontmatter"
    fi
    # Check for description field in frontmatter
    if ! awk '/^---$/{n++} n==1 && /^description:/{print "yes"; exit}' "$skill_file" | grep -q "yes"; then
      echo "WARNING: $name/SKILL.md missing 'description:' field in frontmatter"
    fi
  fi

  echo "ok: $name"
done

# 2. Setup CLI directory symlinks pointing to .agents/skills
for cli_rel in "${CLI_DIRS[@]}"; do
  cli_target="$REPO_ROOT/$cli_rel"
  cli_parent="$(dirname "$cli_target")"

  if [[ ! -d "$cli_parent" ]]; then
    mkdir -p "$cli_parent"
  fi

  if [[ -L "$cli_target" ]]; then
    link_dest="$(readlink "$cli_target")"
    if [[ "$link_dest" == *"agents/skills"* ]]; then
      # Update target if relative depth was incorrect
      rm -f "$cli_target"
      ln -s "../.agents/skills" "$cli_target"
      echo "symlink ok: $cli_rel -> ../.agents/skills"
    else
      echo "updating symlink: $cli_rel"
      rm -f "$cli_target"
      ln -s "../.agents/skills" "$cli_target"
    fi
  elif [[ -d "$cli_target" ]]; then
    echo "converting dir to symlink: $cli_rel"
    rm -rf "$cli_target"
    ln -s "../.agents/skills" "$cli_target"
  else
    echo "creating symlink: $cli_rel -> ../.agents/skills"
    ln -s "../.agents/skills" "$cli_target"
  fi
done

if [[ $errors -gt 0 ]]; then
  echo "FAILED: $errors skill(s) failed setup/validation" >&2
  exit 1
fi

echo "All skills validated and CLI symlinks configured."
