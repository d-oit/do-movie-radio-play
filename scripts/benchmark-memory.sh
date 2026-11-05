#!/usr/bin/env bash
# Measures peak memory and throughput of the radio-play assembly stage across
# input durations, so a change that makes memory scale with film length is
# visible instead of silent (#418).
#
# Peak RSS is a per-process high-water mark, so each duration runs in its own
# process under GNU `time`. The script prints one JSON object per duration and
# a summary table.
#
# Usage: bash scripts/benchmark-memory.sh [duration_seconds...]
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

if ! command -v /usr/bin/time >/dev/null 2>&1; then
  echo "GNU time is required to measure peak RSS (/usr/bin/time -f %M)" >&2
  exit 1
fi

if [[ $# -gt 0 ]]; then
  durations=("$@")
else
  durations=(60 300 900)
fi

echo "Building assembly memory benchmark..." >&2
cargo build --quiet --release -p benchmarks --bin assembly_mem
binary="${repo_root}/target/release/assembly_mem"

time_report="$(mktemp)"
trap 'rm -f "${time_report}"' EXIT

printf '%-12s %-14s %-12s %-10s\n' "duration_s" "peak_rss_mb" "elapsed_ms" "x_realtime"
for duration in "${durations[@]}"; do
  # One run: GNU time reports on stderr, the benchmark writes JSON to stdout.
  json="$(/usr/bin/time -f 'peak_rss_kb=%M' "${binary}" "${duration}" 2>"${time_report}")"
  peak_kb="$(sed -n 's/.*peak_rss_kb=\([0-9]*\).*/\1/p' "${time_report}")"
  elapsed_ms="$(sed -n 's/.*"elapsed_ms":\([0-9]*\).*/\1/p' <<<"${json}")"
  throughput="$(sed -n 's/.*"throughput_x_realtime":\([0-9.]*\).*/\1/p' <<<"${json}")"
  peak_mb="$(awk -v kb="${peak_kb:-0}" 'BEGIN { printf "%.1f", kb / 1024 }')"
  printf '%-12s %-14s %-12s %-10s\n' "${duration}" "${peak_mb}" "${elapsed_ms}" "${throughput}"
  printf '{"duration_s":%s,"peak_rss_kb":%s,"elapsed_ms":%s,"throughput_x_realtime":%s}\n' \
    "${duration}" "${peak_kb:-0}" "${elapsed_ms:-0}" "${throughput:-0}"
done
