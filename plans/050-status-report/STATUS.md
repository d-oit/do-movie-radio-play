# Implementation Status Report

**Date:** 2026-04-13

## Phase Status Summary

| Phase | Description | Status | Notes |
|-------|------------|--------|-------|
| 01 | JSON-only pipeline | COMPLETE | Deterministic extract pipeline verified |
| 02 | Acoustic tags | COMPLETE | Rule-based tags with spectral features |
| 03 | Prompt generation | COMPLETE | Config passthrough and tag mappings are wired |
| 04 | Self-learning | PARTIAL | Report apply path exists, but runtime save/apply flow is incomplete |
| 05 | Hardening and quality | IN PROGRESS | Validation/config, WAV fallback, and VAD fail-fast are now complete |
| 06 | New capabilities | READY | Next feature should build on existing profile/tag infrastructure |

## Current Missing Implementations

### Calibration Save Flow Still Persists the Input Delta (Medium)

`src/main.rs:85-94` - `--save-calibration` writes `cfg.vad_threshold_delta` to a
profile named `runtime`. The learned recommendation from
`CalibrationReport.recommended_energy_threshold_delta` is only available through
the separate `apply-calibration` flow.

- The report-to-profile path exists in `src/learning/calibrator.rs`, but the most
  convenient runtime save path still stores the pre-learning value.

## Quality Issues

### unwrap() Still Present in Production Code (Medium)

| File | Line | Context |
|------|------|---------|
| `src/pipeline/features.rs` | 31 | `fft.process(...).unwrap()` in feature extraction hot path |

### Semantic Error Mapping Is Still Wrong (Low)

`src/error.rs:20-24` maps every `io::Error` to `InvalidConfig`, which is not a
good fit for file-not-found, permission, or decoder I/O failures.

### CSV Manifest Parsing Can Manufacture Fake Ground Truth (Low)

`src/validation/dataset.rs:24-25` uses `unwrap_or(0)` and `unwrap_or(start_ms)`.
Malformed rows silently become valid-looking timestamps.

### Benchmarking Exists, but CI Regression Tracking Does Not (Low)

Criterion benchmarks now exist in `benches/pipeline_bench.rs`, but CI only runs
build, fmt, clippy, test, and the shell quality gate. There is no benchmark
comparison or regression alerting yet.

## Completed Since Earlier Plan Drafts

- Prompt generation now honors `AnalysisConfig` in `src/pipeline/prompts.rs`.
- `crowd_like` and `machinery_like` have distinct prompt mappings.
- Segment confidence is derived from frame likelihoods in `src/pipeline/segmenter.rs`.
- `validate` and `bench` now accept runtime config, threshold, engine, and calibration inputs.
- Unsupported `webrtc` and `silero` engine selections now fail fast instead of silently degrading.
- Unsupported WAV direct decodes now fall back to `ffmpeg`.
- Config and env override validation now fail clearly on malformed values and invalid ranges.
- JSON schema validation exists via `schema/timeline.schema.json` and
  `tests/json_contract.rs`.
- Criterion benchmarks are configured in `Cargo.toml` and implemented in
  `benches/pipeline_bench.rs`.
- Frame construction and VAD already use spectral features through
  `src/types/frame.rs` and `src/pipeline/framing.rs`.
