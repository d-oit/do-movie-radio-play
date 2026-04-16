# self-learning-calibration references
- Determinism and testability first.
- Prefer `--learning-db` for cumulative multi-movie learning.
- Compact real-movie sweep snapshot (2026-04-16):
  - elephants_dream_2006: FP 2.50% (good baseline)
  - windy_day_1967: FP 55.41% (needs profile tuning)
  - the_hole_1962: FP 94.39% (legacy/noisy content failure mode)
- Verification thresholds are now applied at runtime and use confidence hysteresis (`high=0.62`, `low=0.45`).
- Use `analysis/optimization/fp-sweep-ranked.json` + `generate_optimized_profiles.py` for deployable profile updates.
