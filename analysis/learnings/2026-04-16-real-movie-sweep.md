# 2026-04-16 Real-Movie Sweep (Compact)

## Scope

- Engine: `spectral`
- Profile base: `config/profiles/radio-play.json`
- Flow: `extract -> verify-timeline --save-learning --learning-db`

## Movies

1. `testdata/raw/elephants_dream_2006.mp4`
2. `testdata/raw/the_hole_1962.mp4`
3. `testdata/raw/windy_day_1967.mp4`
4. `testdata/raw/elephantsdream_teaser.mp4`
5. `testdata/raw/caminandes_gran_dillama.mp4`

## Outcomes

- `elephants_dream_2006`: verified 39, suspicious 1, FP rate 2.50%
- `the_hole_1962`: verified 6, suspicious 101, FP rate 94.39%
- `windy_day_1967`: verified 33, suspicious 41, FP rate 55.41%
- `elephantsdream_teaser`: verified 2, suspicious 0, FP rate 0.00%
- `caminandes_gran_dillama`: verified 1, suspicious 6, FP rate 85.71%

## Aggregated Learning DB

- `analysis/thresholds/learning.db`
- total verifications: 230
- false positives: 149
- fp rate: 64.78%
- recommendation confidence: high (`sample_size=149`)

## Benchmarks + Evals

- Benchmarks executed for:
  - `analysis/benchmarks/elephants_dream_2006.json` (total 12186ms)
  - `analysis/benchmarks/elephantsdream_teaser.json` (total 1151ms)
  - `analysis/benchmarks/caminandes_gran_dillama.json` (total 2039ms)
- Full validation manifest executed:
  - `analysis/validation/full-sweep-summary-2026-04-16.json`
  - 4 entries across tiers A/B/C

## GitHub Impact Scan

- Open issues: none
- Open PRs: none
- Recent merged PRs reviewed: #1, #3, #4
- Impact: no active upstream blockers from repository issue/PR queue

## Practical Takeaway

- Current spectral defaults generalize well for modern clean CGI audio.
- Older film/noisy mixes need dedicated profile(s) and stricter heuristics.
- Keep modern and legacy content separated during auto-learning updates.

## FP Sweep Snapshot

- Ranked sweep report: `analysis/optimization/fp-sweep-ranked.json`
- Sweep now includes modern/legacy cohort ranking and a baseline coverage guard (`min_coverage_ratio=0.7`).
- After wiring verification threshold overrides + hysteresis, the current best weighted FP candidate in this sweep is `baseline` (`~64.78%`) against the tested matrix.

## Expanded Sweep Refresh

- Expanded run report: `analysis/optimization/fp-sweep-ranked-expanded.json`
- Latest recommended candidate: `grid_t0.0125_ms500_e3.0_em7.2_f0.38_en0.0015_c120`
- Regenerated profiles:
  - `config/profiles/modern-optimized.json`
  - `config/profiles/legacy-optimized.json`
