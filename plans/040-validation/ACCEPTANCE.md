# Acceptance

Validation acceptance is split into correctness, determinism, and production-eval readiness.

## 1) Code Quality Gate (must pass, zero warnings)

```bash
bash scripts/quality_gate.sh
```

Required outcome:
- `cargo fmt --check` clean
- `cargo clippy --all-targets --all-features -- -D warnings` clean
- `cargo test` clean

## 2) JSON Contract and Determinism (must pass)

```bash
cargo test --test json_contract
cargo test --test integration_cli repeated_extract_is_deterministic
```

Required outcome:
- extract output validates against `schema/timeline.schema.json`
- repeated runs on identical input produce byte-identical output JSON

## 3) Production Evaluation Correctness (must pass)

```bash
# Synthetic truth baseline
timeline gen-fixtures --output-dir testdata/generated
timeline validate testdata/generated/alternating.wav --truth-json testdata/generated/alternating.truth.json --profile synthetic --output testdata/validation/validate_against_synthetic.json

# Real-media subtitle baseline (preferred modern fixture)
timeline validate testdata/raw/sintel_trailer_2010.mp4 --subtitles testdata/raw/sintel_trailer_2010.srt --total-ms 53000 --profile movie --output testdata/validation/movie_real_srt_validation.json

# Coverage + artifact integrity for Tier A production evals
python3 scripts/check_validation_coverage.py --tier A --strict-files
```

Required outcome:
- validation command succeeds for each selected source
- manifest coverage check passes for required tier
- output report includes stable metrics: `overlap_ratio`, precision/recall, and boundary error
- any failure is fixed or documented in `plans/050-status-report/STATUS.md`

## 4) Benchmark Regression Visibility (must pass)

```bash
bash scripts/benchmark.sh testdata/raw/sintel_trailer_2010.mp4 analysis/benchmarks/ci.json
python3 scripts/check_benchmark_regression.py --baseline analysis/benchmarks/latest.json --candidate analysis/benchmarks/ci.json
```

Required outcome:
- candidate benchmark generated
- regression checker passes against checked-in baseline
