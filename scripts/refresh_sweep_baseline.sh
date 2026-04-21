#!/usr/bin/env bash
set -euo pipefail

current=${1:-analysis/optimization/fp-sweep-ranked.json}
baseline=${2:-analysis/optimization/fp-sweep-ranked-latest.json}

if [[ ! -s "$current" ]]; then
  echo "missing current sweep report: $current" >&2
  exit 1
fi

mkdir -p "$(dirname "$baseline")"
cp "$current" "$baseline"

echo "updated sweep baseline"
echo "  current:  $current"
echo "  baseline: $baseline"
