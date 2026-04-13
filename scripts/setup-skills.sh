#!/usr/bin/env bash
set -euo pipefail
for d in .agents/skills/*; do
  [[ -d "$d" ]] || continue
  echo "skill: $(basename "$d")"
done
