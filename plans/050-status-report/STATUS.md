# Implementation Status Report

**Date:** 2026-04-14

## Phase Status Summary

| Phase | Description | Status | Notes |
|-------|------------|--------|-------|
| 01 | JSON-only pipeline | COMPLETE | Deterministic extract pipeline verified |
| 02 | Acoustic tags | COMPLETE | Rule-based tags with spectral features |
| 03 | Prompt generation | COMPLETE | Config passthrough and tag mappings are wired |
| 04 | Self-learning | COMPLETE | Calibration now writes a report and updates the active profile automatically |
| 05 | Hardening and quality | COMPLETE | Validation/config, WAV fallback, VAD fail-fast, and benchmark CI regression checks are complete |
| 06 | New capabilities | READY | Next feature should build on existing profile/tag infrastructure |

## Current Missing Implementations

No medium-or-higher runtime gaps are currently open in the shipped CLI flow.

## Quality Issues

No active hardening gaps are currently open beyond future feature work.

## Completed Since Earlier Plan Drafts

- Prompt generation now honors `AnalysisConfig` in `src/pipeline/prompts.rs`.
- `crowd_like` and `machinery_like` have distinct prompt mappings.
- Segment confidence is derived from frame likelihoods in `src/pipeline/segmenter.rs`.
- `validate` and `bench` now accept runtime config, threshold, engine, and calibration inputs.
- `calibrate` now closes the loop by writing a report and updating the active calibration profile.
- The CLI now exposes only the implemented `energy` VAD engine.
- Unsupported WAV direct decodes now fall back to `ffmpeg`.
- Config and env override validation now fail clearly on malformed values and invalid ranges.
- Dataset manifest parsing now fails fast on malformed rows instead of manufacturing timestamps.
- `io::Error` is mapped semantically instead of being surfaced as config failure.
- JSON schema validation exists via `schema/timeline.schema.json` and
   `tests/json_contract.rs`.
- Criterion benchmarks are configured in `Cargo.toml` and implemented in
   `benches/pipeline_bench.rs`.
- CI now runs benchmark smoke, compares against the checked-in real-media baseline, and uploads benchmark artifacts.
- Frame construction and VAD already use spectral features through
   `src/types/frame.rs` and `src/pipeline/framing.rs`.
