# 2026-04-18 Holdout Readiness Gate (Compact)

- Added radio-play holdout gate script:
  - `scripts/check_radio_play_readiness.py`
- Gate checks holdout tier metrics from validation summary:
  - `non_voice_precision`
  - `non_voice_recall`
  - `overlap_ratio`
- Integrated gate into validation sweep CI:
  - `.github/workflows/validation-sweep.yml`
- Current thresholds for release readiness:
  - holdout tier `C`
  - min precision `0.95`
  - min recall `0.95`
  - min overlap `0.95`
