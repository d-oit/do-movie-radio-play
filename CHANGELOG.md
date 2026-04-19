# Changelog

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
