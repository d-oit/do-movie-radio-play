#!/usr/bin/env bash
set -euo pipefail

INPUT_FILE="${1:-}"
OUTPUT_FILE="${2:-}"

if [[ -z "$INPUT_FILE" ]]; then
  printf 'usage: scripts/handoff-to-tasks.sh <handoff.md> [output.json]\n' >&2
  exit 2
fi

if [[ ! -f "$INPUT_FILE" ]]; then
  printf 'handoff file not found: %s\n' "$INPUT_FILE" >&2
  exit 2
fi

if [[ -z "$OUTPUT_FILE" ]]; then
  OUTPUT_FILE="${INPUT_FILE%.md}.tasks.json"
fi

python3 - "$INPUT_FILE" "$OUTPUT_FILE" <<'PY'
import json
import re
import sys
from pathlib import Path

input_path = Path(sys.argv[1])
output_path = Path(sys.argv[2])
content = input_path.read_text(encoding="utf-8")

def extract_field(name: str) -> str:
    match = re.search(rf"^-\s+{re.escape(name)}:\s*(.+)$", content, re.MULTILINE)
    return match.group(1).strip() if match else ""

def extract_numbered_section(title: str):
    match = re.search(
        rf"^##\s+{re.escape(title)}\n\n(?P<body>.*?)(?:\n##\s+|\Z)",
        content,
        re.MULTILINE | re.DOTALL,
    )
    if not match:
        return []
    items = []
    for line in match.group("body").splitlines():
        numbered = re.match(r"^\s*\d+\.\s+(.*)$", line)
        if numbered:
            items.append(numbered.group(1).strip())
    return items

def extract_skill_routing():
    match = re.search(
        r"^##\s+Skill Routing\n\n(?P<body>.*?)(?:\n##\s+|\Z)",
        content,
        re.MULTILINE | re.DOTALL,
    )
    routing = {}
    if not match:
        return routing
    for line in match.group("body").splitlines():
        bullet = re.match(r"^\s*-\s+([^\-]+)->\s+(.+)$", line)
        if bullet:
            routing[bullet.group(1).strip()] = bullet.group(2).strip()
    return routing

failure_type = extract_field("Failure type") or "unknown"
parallel_plan = extract_numbered_section("Parallel Agent Plan")
skill_routing = extract_skill_routing()

default_skill = skill_routing.get("unknown", "web-search-researcher")
selected_skill = default_skill
if failure_type in ("rust", "tests"):
    selected_skill = skill_routing.get("rust/tests", default_skill)
elif failure_type in skill_routing:
    selected_skill = skill_routing[failure_type]

tasks = []
for item in parallel_plan:
    task_name = item.split("(", 1)[0].strip()
    tasks.append(
        {
            "task": task_name,
            "notes": item,
            "recommended_skill": selected_skill,
        }
    )

result = {
    "source_handoff": str(input_path),
    "pr": extract_field("PR"),
    "failure_type": failure_type,
    "ci_log": extract_field("CI log"),
    "recommended_skill": selected_skill,
    "tasks": tasks,
}

output_path.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
print(str(output_path))
PY
