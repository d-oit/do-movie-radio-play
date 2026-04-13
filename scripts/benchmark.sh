#!/usr/bin/env bash
set -euo pipefail
input=${1:-testdata/generated/alternating.wav}
out=${2:-analysis/benchmarks/latest.json}
mkdir -p "$(dirname "$out")"
cargo run --quiet -- bench "$input" --output "$out"
echo "benchmark written: $out"
