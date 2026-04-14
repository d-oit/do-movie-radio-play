# Benchmarks

Smoke benchmark via `timeline bench` writing JSON to `analysis/benchmarks/`.

## Baseline Workflow

1. Generate candidate benchmark artifact:
   - `bash scripts/benchmark.sh`
2. Compare candidate against checked-in baseline:
   - `python3 scripts/check_benchmark_regression.py --baseline analysis/benchmarks/latest.json --candidate analysis/benchmarks/latest.json`
3. If intentional performance shifts exceed threshold, update `analysis/benchmarks/latest.json` and rerun the check.

Input policy for smoke/CI benchmark runs:
- Prefer post-2000 fixtures in `testdata/raw/` (Sintel 2010, Big Buck Bunny trailer 2008, Elephants Dream 2006).
- Keep older fixtures only as fallback compatibility when already downloaded locally.
- Use generated `testdata/generated/alternating.wav` only when no raw fixture exists.

## Regression Rules

- Artifact schema must include top-level `decode_ms` and all `stage_ms` fields.
- `decode_ms` must match `stage_ms.decode_ms`.
- Regressions are tolerated up to `max(2000ms, 50% of baseline)` per timing field.
- `input_file`, `frame_count`, and `segment_count` must match baseline exactly.
