# Changelog

## Unreleased
- feat(voice): configurable OpenAI-compatible TTS endpoint — `base_url` + optional auth enables local German sidecars (audio.cpp/PocketTTS) with OpenAI as default (#212).
- fix(voice): harden `SynthesisRequest` validation and percent-encode the ElevenLabs voice endpoint; validate effective voice_id including config fallback (#231).
- fix(voice): validate `SynthesisRequest` parameters (sample-rate bounds, finite speed, text cap) before TTS dispatch; typed validation errors (#209).
- perf(verification): pre-allocated magnitude buffers and branchless spectral summation, ~7.5% faster spectral feature extraction (#193).
- refactor(pipeline): extracted `run_pipeline` processing stages into focused helpers with a shared `timed_stage!` macro (#189).
- refactor(goap): split `identify_gaps` signal analysis into dedicated helper methods (#188).
- build(deps): bumped `clap` to 4.6.6 (#192).

## 0.1.0
- Initial production-oriented CLI with `extract`/`tag`/`prompt`/`calibrate`/`bench`.
- Added spectral VAD path and profile-driven threshold controls.
- Added verification workflow (`verify-timeline`) with review player enhancements and learning export.
- Added adaptive learning and libsql-backed learning database integration.
- Added timeline export formats (`json`, `edl`, `vtt`).
- Added optimization toolchain:
  - `optimize_fp_sweep.py`
  - `generate_optimized_profiles.py`
  - `optimize_and_publish_profiles.sh`
  - `compare_sweeps.py` and `check_sweep_drift.py`
- Added radio-play readiness gating:
  - holdout readiness checks
  - LB95 confidence-bound checks
  - failure-breakdown and consolidated readiness report artifacts
- Added scheduled CI workflows for validation sweep and optimization sweep.
