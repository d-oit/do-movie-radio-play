#!/usr/bin/env bash
set -euo pipefail
input=${1:-testdata/generated/alternating.wav}
out=${2:-analysis/benchmarks/latest.json}
mkdir -p "$(dirname "$out")"

if [[ ! -f "$input" ]]; then
  echo "benchmark input missing ($input); generating deterministic fixtures..." >&2
  cargo run --quiet -- gen-fixtures --output-dir testdata/generated
fi

cargo run --quiet -- bench "$input" --output "$out"
echo "benchmark written: $out"
