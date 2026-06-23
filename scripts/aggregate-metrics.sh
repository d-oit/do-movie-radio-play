#!/usr/bin/env bash
# scripts/aggregate-metrics.sh
# Aggregate agent metrics events into a summary report.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT" || exit 1

METRICS_FILE=".agents/metrics.jsonl"

if [[ ! -f "$METRICS_FILE" ]]; then
  echo "No metrics file found at $METRICS_FILE"
  exit 0
fi

if ! command -v python3 &>/dev/null; then
  echo "python3 required for metrics aggregation"
  exit 1
fi

python3 -c "
import json, sys
from collections import defaultdict

counts = defaultdict(lambda: {'success': 0, 'failure': 0})
with open('$METRICS_FILE') as f:
    for line in f:
        line = line.strip()
        if not line:
            continue
        try:
            d = json.loads(line)
            skill = d.get('skill', 'unknown')
            if d.get('success', False):
                counts[skill]['success'] += 1
            else:
                counts[skill]['failure'] += 1
        except json.JSONDecodeError:
            continue

print('Skill Metrics Summary:')
print(f'{\"Skill\":<30} {\"Success\":>10} {\"Failure\":>10}')
print('-' * 52)
for skill, c in sorted(counts.items()):
    print(f'{skill:<30} {c[\"success\"]:>10} {c[\"failure\"]:>10}')
"
