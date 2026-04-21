#!/usr/bin/env bash
set -euo pipefail

input=${1:-testdata/raw/elephants_dream_2006.mp4}
out=${2:-analysis/benchmarks/latest.json}
tmp="${out}.tmp"

mkdir -p "$(dirname "$out")"

echo "refreshing benchmark baseline"
echo "  input: $input"
echo "  output: $out"

bash scripts/benchmark.sh "$input" "$tmp"
mv "$tmp" "$out"

echo "updated benchmark baseline: $out"
