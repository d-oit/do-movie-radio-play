# 2026-04-16 Real-Movie Sweep (Compact)

## Scope

- Engine: `spectral`
- Profile base: `config/profiles/radio-play.json`
- Flow: `extract -> verify-timeline --save-learning --learning-db`

## Movies

1. `testdata/raw/elephants_dream_2006.mp4`
2. `testdata/raw/the_hole_1962.mp4`
3. `testdata/raw/windy_day_1967.mp4`

## Outcomes

- `elephants_dream_2006`: verified 39, suspicious 1, FP rate 2.50%
- `the_hole_1962`: verified 6, suspicious 101, FP rate 94.39%
- `windy_day_1967`: verified 33, suspicious 41, FP rate 55.41%

## Aggregated Learning DB

- `analysis/thresholds/learning.db`
- total verifications: 221
- false positives: 143
- fp rate: 64.71%
- recommendation confidence: high (`sample_size=143`)

## Practical Takeaway

- Current spectral defaults generalize well for modern clean CGI audio.
- Older film/noisy mixes need dedicated profile(s) and stricter heuristics.
- Keep modern and legacy content separated during auto-learning updates.
