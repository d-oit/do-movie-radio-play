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

# Deterministic cue-path regression gate (no media or model)
python3 scripts/evaluate_cue_path.py fixtures --manifest testdata/cues/fixtures.json --output analysis/validation/cue-path-fixtures.json
python3 scripts/test_cue_gate.py
python3 scripts/test_cue_path_metrics.py

# Opt-in end-to-end cue path (espeak-ng + faster-whisper; selects on dev, reports holdout)
python3 scripts/generate_dialogue_fixtures.py --output-dir testdata/generated/dialogue
python3 scripts/evaluate_cue_path.py sweep --manifest testdata/validation/cue-eval-manifest.json --output analysis/validation/cue-path-sweep-real.json --derive-cues --cue-cache-dir /tmp/cuecache
```

Required outcome:
- validation command succeeds for each selected source
- manifest coverage check passes for required tier
- output report includes stable metrics: `overlap_ratio`, precision/recall, and boundary error
- cue-path fixtures pass, including the deliberate dropped-cue regression that must fail its gate
- every reported leak figure names its scoring mode: `detector_plus_cues` is the shipped path, `cue_only_upper_bound` is not
- the opt-in sweep reports per-film, per-source, per-language leakage and retained narratable time, with pad/threshold chosen on `split=dev` only
- any failure is fixed or documented in `plans/050-status-report/STATUS.md`

## 4) Benchmark Regression Visibility (must pass)

```bash
bash scripts/benchmark.sh testdata/raw/sintel_trailer_2010.mp4 analysis/benchmarks/ci.json
python3 scripts/check_benchmark_regression.py --baseline analysis/benchmarks/latest.json --candidate analysis/benchmarks/ci.json
```

Required outcome:
- candidate benchmark generated
- regression checker passes against checked-in baseline

## 5) Code Coverage Policy & Floor (must pass)

Code coverage is collected and uploaded during CI via `cargo-llvm-cov` in `.github/workflows/ci.yml`.

Required policy and coverage floor:
- **CI Workflow Execution**: Automated `coverage` job executes `cargo llvm-cov --workspace --all-targets --lcov` and uploads reports to Codecov (`codecov/codecov-action@v5`).
- **Codecov Threshold**: Root `.codecov.yml` enforces `target: auto` with a `threshold: 1%` tolerance for project and patch coverage.
- **Baseline Coverage Floor**: Minimum overall workspace line coverage floor is 60% across `crates/`.
