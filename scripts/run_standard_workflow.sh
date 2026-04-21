#!/usr/bin/env bash
set -euo pipefail

dry_run=false
skip_bench_harness=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run)
      dry_run=true
      shift
      ;;
    --skip-bench-harness)
      skip_bench_harness=true
      shift
      ;;
    -h|--help)
      cat <<'EOF'
Usage: bash scripts/run_standard_workflow.sh [--dry-run] [--skip-bench-harness]

Runs the anti-regression standard workflow in fixed order:
  1) fetch test assets
  2) generate deterministic fixtures
  3) quality gate (fmt, clippy, tests)
  4) benchmark artifact generation
  5) benchmark regression check
  6) benchmark harness (optional)
EOF
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      exit 1
      ;;
  esac
done

run_step() {
  local label="$1"
  shift
  echo "==> ${label}"
  echo "    $*"
  if [[ "$dry_run" == true ]]; then
    return 0
  fi
  "$@"
}

run_step "fetch assets" bash scripts/fetch_test_assets.sh
run_step "generate fixtures" cargo run --quiet -- gen-fixtures --output-dir testdata/generated
run_step "quality gate" bash scripts/quality_gate.sh
run_step "benchmark ci artifact" bash scripts/benchmark.sh testdata/raw/elephants_dream_2006.mp4 analysis/benchmarks/ci.json
run_step "benchmark regression check" python3 scripts/check_benchmark_regression.py --baseline analysis/benchmarks/latest.json --candidate analysis/benchmarks/ci.json

if [[ "$skip_bench_harness" == false ]]; then
  run_step "benchmark harness" cargo bench --bench pipeline_bench -- --noplot
fi

echo "standard workflow completed"
