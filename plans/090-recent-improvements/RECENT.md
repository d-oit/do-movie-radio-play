# Recent Improvements

Features implemented in recent releases.

## Compact Operator Summary

1. Run sweep: `python3 scripts/optimize_fp_sweep.py --output analysis/optimization/fp-sweep-ranked.json`
2. Generate profiles: `python3 scripts/generate_optimized_profiles.py --sweep-report analysis/optimization/fp-sweep-ranked.json`
3. Use output profiles:
   - `config/profiles/modern-optimized.json`
   - `config/profiles/legacy-optimized.json`
4. Verify learning health: `timeline learning-stats --learning-db analysis/thresholds/learning.db`

## 1. Spectral VAD Engine

Added alternative spectral-based VAD engine with configurable thresholds.

### Description
Uses spectral features (flatness, entropy, centroid) instead of RMS energy for voice detection. Useful for content with varying background noise levels.

### How to Use

```bash
# Via config file
timeline extract input.mp4 --config vad-spectral.json --output out.json

# Via CLI
timeline extract input.mp4 --vad-engine spectral --output out.json
```

### Configuration Options

| Option | Type | Description | Default |
|--------|------|-------------|---------|
| `vad_engine` | string | VAD engine: "energy" or "spectral" | "energy" |
| `spectral_flatness_max` | f32 | Max spectral flatness (0-1). Higher = more noise-like | none |
| `spectral_entropy_min` | f32 | Min spectral entropy (log2 scale). Higher = more tonal | none |
| `spectral_centroid_min` | f32 | Min spectral centroid (Hz) | none |
| `spectral_centroid_max` | f32 | Max spectral centroid (Hz) | none |

Example config:
```json
{
  "vad_engine": "spectral",
  "spectral_flatness_max": 0.5,
  "spectral_entropy_min": 3.5,
  "spectral_centroid_min": 200,
  "spectral_centroid_max": 4000
}
```

---

## 2. Verification System

Added timeline verification with spectral analysis to validate segment boundaries.

### Description
Analyzes media at verified segment boundaries to confirm speech/non-voice classification. Supports saving learning data for threshold adaptation.

### How to Use

```bash
timeline verify-timeline input.mp4 --timeline timeline.json --output verified.json
timeline verify-timeline input.mp4 --timeline timeline.json --output verified.json --save-learning
```

### Options

| Option | Description |
|--------|-------------|
| `--timeline` | Input timeline JSON |
| `--output` | Output path for verified timeline |
| `--save-learning` | Save learning data to `analysis/thresholds/learning-state.json` |
| `--learning-db` | Also persist verification results to libsql database |

---

## 3. Learning System

Adaptive threshold system that adjusts VAD parameters based on verification feedback.

### Description
Maintains learning state from verification results and updates configuration thresholds automatically. Reduces false positives/negatives over time.

### How to Use

```bash
# Update thresholds from learning state
timeline update-thresholds --learning-state analysis/thresholds/learning-state.json
```

### Options

| Option | Description |
|--------|-------------|
| `--learning-state` | Path to learning state JSON from verify-timeline |
| `--learning-db` | Path to libsql database for recommendation generation |

### Database-backed Learning

Learning data is now persisted in `analysis/thresholds/learning.db` when `--learning-db` is provided.

- `verified_segments` table: per-segment verification outcomes + spectral features
- `threshold_history` table: generated threshold recommendations over time

This enables queryable, cumulative learning across runs instead of single JSON snapshots.

Inspect database learning health:

```bash
timeline learning-stats --learning-db analysis/thresholds/learning.db
timeline learning-stats --learning-db analysis/thresholds/learning.db --output analysis/thresholds/learning-stats.json
```

### Output
Generates updated configuration in `analysis/thresholds/updated-config.json`.

---

## 4. Export System

Export timelines to multiple formats (JSON, EDL, WebVTT).

### Description
Converts internal timeline format to industry-standard formats for downstream use in video editors, subtitle tools, or web players.

### How to Use

```bash
# JSON export
timeline export --input timeline.json --output out.json --format json

# CMX 3600 EDL export
timeline export --input timeline.json --output out.edl --format edl

# WebVTT export
timeline export --input timeline.json --output out.vtt --format vtt

# Include verified segments
timeline export --input timeline.json --output out.json --format json --verified verified.json
```

### Options

| Option | Description |
|--------|-------------|
| `--input` | Input timeline JSON |
| `--output` | Output file path |
| `--format` | Format: json, edl, vtt |
| `--verified` | Optional verified timeline for flagged segments |

---

## 5. Review Player Improvements

Enhanced UI for segment review with filtering, sorting, and keyboard shortcuts.

### Description
Improved review player with filter/sort controls, keyboard shortcuts, and learning data export.

### Features

- **Filter**: Show All / Verified / Unverified / Suspicious / Excluded
- **Sort**: By Time / By Confidence / By Duration
- **Ctrl+S**: Save current review state
- **Export Learning Data**: Button to export learning data (keyboard shortcut: E)

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| Space | Play/Pause |
| Arrow Left/Right | Seek ±5s |
| Ctrl+S | Save |
| E | Export Learning Data |

### How to Use

```bash
timeline review input.mp4 --input segments.json --output report.html --open
```

---

## 6. Real-Movie Learning Sweep (2026-04-16)

Executed spectral extract + verify + DB learning on real fixtures:

```bash
timeline extract testdata/raw/elephants_dream_2006.mp4 --output analysis/validation/elephants_dream_2006_spectral.json --config config/profiles/modern-optimized.json --vad-engine spectral
timeline verify-timeline testdata/raw/elephants_dream_2006.mp4 --timeline analysis/validation/elephants_dream_2006_spectral.json --output analysis/validation/elephants_dream_2006_verified.json --save-learning --learning-db analysis/thresholds/learning.db

timeline extract testdata/raw/the_hole_1962.mp4 --output analysis/validation/the_hole_1962_spectral.json --config config/profiles/legacy-optimized.json --vad-engine spectral
timeline verify-timeline testdata/raw/the_hole_1962.mp4 --timeline analysis/validation/the_hole_1962_spectral.json --output analysis/validation/the_hole_1962_verified.json --save-learning --learning-db analysis/thresholds/learning.db

timeline extract testdata/raw/windy_day_1967.mp4 --output analysis/validation/windy_day_1967_spectral.json --config config/profiles/legacy-optimized.json --vad-engine spectral
timeline verify-timeline testdata/raw/windy_day_1967.mp4 --timeline analysis/validation/windy_day_1967_spectral.json --output analysis/validation/windy_day_1967_verified.json --save-learning --learning-db analysis/thresholds/learning.db
```

---

## 10. Modern Fixture CI Expansion + Compact Digest (2026-04-19)

Added two post-2000 modern fixtures to routine benchmark/eval coverage:

- `testdata/raw/elephantsdream_teaser.mp4` (2006)
- `testdata/raw/caminandes_gran_dillama.mp4` (2013)

CI benchmark job now emits:

- per-movie benchmark artifacts,
- focused FP eval sweep for those two fixtures,
- compact markdown digest `analysis/optimization/modern-extra-ci-summary.md`.

Manual benchmark baseline refresh script added:

```bash
bash scripts/refresh_benchmark_baseline.sh testdata/raw/elephants_dream_2006.mp4 analysis/benchmarks/latest.json
```

Observed verification outcomes:

| Movie | Verified | Suspicious | FP Rate |
|------|----------|------------|---------|
| elephants_dream_2006 | 39 | 1 | 2.50% |
| the_hole_1962 | 6 | 101 | 94.39% |
| windy_day_1967 | 33 | 41 | 55.41% |

Aggregated DB learning stats (`analysis/thresholds/learning-stats.json`):
- total verifications: 221
- false positives: 143
- false positive rate: 64.71%
- recommendation confidence: high (sample_size=143)

Takeaway: spectral defaults perform well on modern CGI fixture (`elephants_dream_2006`) but need profile specialization for older/noisier films.

Additional modern fixtures (Blender, legally redistributable) tested:

| Movie | Verified | Suspicious | FP Rate |
|------|----------|------------|---------|
| elephantsdream_teaser | 2 | 0 | 0.00% |
| caminandes_gran_dillama | 1 | 6 | 85.71% |

Benchmark and eval artifacts from this optimization pass:
- `analysis/benchmarks/elephants_dream_2006.json`
- `analysis/benchmarks/elephantsdream_teaser.json`
- `analysis/benchmarks/caminandes_gran_dillama.json`
- `analysis/validation/full-sweep-summary-2026-04-16.json`

## 11. External Workflow / DSP Reference Review (2026-04-19)

Reviewed possible reuse from:

- `d-o-hub/github-template-ai-agents`
- `d-o-hub/chaotic_semantic_memory`
- `ruvnet/musica`

Adopted/reinforced patterns with concrete fit:

- single-source agent contract in `AGENTS.md`
- fixed-order anti-regression workflow runner (`scripts/run_standard_workflow.sh`)
- benchmark/sweep baseline refresh helpers as explicit operator actions
- benchmark-first / interpretable DSP development discipline

Potential Rust-portable logic retained as future references:

- structure-first graph/audio grouping ideas from `musica`
- stricter contract-regeneration and validation discipline from Rust-first repos

Rejected for direct reuse:

- non-Rust runtime stacks,
- heavyweight neural/audio frameworks as default execution path,
- any workflow pattern that weakens deterministic offline execution.

## 7. Verification and Sweep Optimization

- Verification now applies runtime threshold overrides for entropy/flatness/energy/centroid during status decisioning.
- Added non-voice confidence hysteresis in verification (`high=0.55`) with conservative suspicious fallback for borderline cases.
- Added sweep script: `scripts/optimize_fp_sweep.py`
  - Runs candidate matrix across modern + legacy fixtures
  - Produces ranked output: `analysis/optimization/fp-sweep-ranked.json`
  - Adds cohort split (`modern` vs `legacy`) and coverage guard to avoid low-coverage "wins"

## 8. Profile Generation from Sweep Policy

- Added profile generator: `scripts/generate_optimized_profiles.py`
- Reads `analysis/optimization/fp-sweep-ranked.json` and emits:
  - `config/profiles/modern-optimized.json`
  - `config/profiles/legacy-optimized.json`
- Ensures generated profiles set `vad_engine: spectral` and carry selected threshold fields.

## 9. Verification + Sweep Robustness Updates

- Verification scoring now explicitly models non-voice confidence (instead of voice-oriented indicator counting).
- Runtime threshold overrides remain active in verification status decisioning.
- Sweep reports now include:
  - `false_positive_risk_rate` (counts suspicious + rejected)
  - `assessed_non_voice_segments`
  - coverage guard against assessed baseline to prevent metric gaming via rejection-only behavior.

## 10. Wrapper Automation for Optimization Publishing

- Added `scripts/optimize_and_publish_profiles.sh`:
  - runs expanded sweep
  - compares with previous report (`scripts/compare_sweeps.py`)
  - regenerates optimized profiles
  - writes compact note: `analysis/learnings/latest-optimization-note.md`
- Added `scripts/compare_sweeps.py` for report-to-report winner/metric deltas.

Example:

```bash
bash scripts/optimize_and_publish_profiles.sh analysis/optimization/fp-sweep-ranked-latest.json 20 0.7
```

## 11. Optimization Sweep CI Workflow

- Added `.github/workflows/optimization-sweep.yml`
- Runs weekly + manual dispatch:
  - executes `optimize_and_publish_profiles.sh` with bounded candidate count
  - uploads optimization reports and generated optimized profiles as artifacts
  - enforces drift guard with `check_sweep_drift.py` (fails on FP/risk regression over configured thresholds)

## 12. Radio-Play 95% Recovery Track

- Added dedicated roadmap: `plans/100-radio-play-95/ROADMAP.md`
- Verification feature correctness fixed (spectral entropy computation)
- Added graph-inspired structure confidence signal into verifier scoring for ambiguous segments

## 13. Latest Optimization Refresh

- Ran wrapper automation after verifier update:
  - `scripts/optimize_and_publish_profiles.sh analysis/optimization/fp-sweep-ranked-latest.json 8 0.7`
- New winner:
  - `grid_t0.0125_ms500_e3.0_em7.2_f0.38_en0.0010_c120`
- Weighted FP improved from `0.7391` to `0.1814` (`analysis/optimization/fp-sweep-comparison.json`)
- Profiles regenerated and compact note refreshed.

## 14. Radio-Play Holdout Readiness Gate

- Added `scripts/check_radio_play_readiness.py` to enforce holdout KPI gates from validation summary.
- Wired into `.github/workflows/validation-sweep.yml` after manifest execution.
- Current CI thresholds:
  - `--holdout-tier C`
  - `--min-non-voice-precision 0.95`
  - `--min-non-voice-recall 0.95`
  - `--min-overlap 0.95`

## 15. Radio-Play LB95 Confidence Gate

- Added `scripts/check_radio_play_lb95.py` to enforce Wilson lower bound readiness.
- Integrated into `.github/workflows/validation-sweep.yml`.
- Current threshold: `--min-lb95 0.95` on holdout tier `C`.

## 16. Radio-Play Failure Breakdown Artifact

- Added `scripts/build_radio_play_failure_breakdown.py`.
- Produces:
  - `analysis/validation/radio-play-failure-breakdown.json`
  - `analysis/learnings/latest-radio-play-failure-breakdown.md`
- Integrated into validation sweep CI and uploaded as workflow artifacts.

## 17. Consolidated Radio-Play Readiness Report

- Added `scripts/build_radio_play_readiness_report.py`.
- Produces unified artifacts combining threshold gate + LB95 gate + cohort summary:
  - `analysis/validation/radio-play-readiness-report.json`
  - `analysis/learnings/latest-radio-play-readiness-report.md`
- Integrated into validation sweep CI with `--require-pass` and uploaded as workflow artifacts.
- Validation workflow now uses this consolidated report as the single gate source; standalone readiness/LB95 checks are retained for local diagnostics.

## 18. Radio-Play Manifest Separation

- Added dedicated readiness manifest: `testdata/validation/radio-play-manifest.json`.
- Validation sweep workflow now runs against radio-play manifest and emits:
  - `analysis/validation/radio-play-sweep-summary.json`
- Coverage check now runs after manifest execution to validate generated report artifacts.

## 19. Compact Update: Swarm Triage + Filter Scope Fix (2026-04-23)

- Applied a targeted pipeline guardrail in `src/pipeline/mod.rs`:
  - low-confidence verification filtering now runs only for sparse-merge, low `min_non_voice_ms` profiles.
- Re-ran radio-play manifest sweep (tiers A/B/C) and rebuilt readiness/failure artifacts.
- Resulting shift:
  - modern/synthetic no longer collapse to zero predicted non-voice segments,
  - modern recall/overlap recovered strongly,
  - holdout tier C readiness remains green.
- Remaining gap:
  - modern precision is still below release threshold.
- Swarm-coordinated next best action:
  - change non-voice merge behavior for `merge_strategy=all` from global collapse to bounded gap-aware merge,
  - keep holdout protections (sparse-profile filter + tail-recovery floor) unchanged.
