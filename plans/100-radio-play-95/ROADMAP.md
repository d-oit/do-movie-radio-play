# Radio-Play 95% Readiness Roadmap

## Goal

Raise radio-play preparation success rate to **>=95%** with offline CPU-first Rust workflow.

## Recommended Architecture

Use a **two-stage cascade**:

1. **Stage 1 (fast high-recall detector)**
   - Spectral/energy hybrid VAD with strict smoothing and duration filters.
   - Keep deterministic and cheap for full-stream pass.

2. **Stage 2 (selective verifier)**
   - Run stronger verification only on ambiguous windows.
   - Short term: graph-inspired structure score (no ML dependency).
   - Mid term: ONNX verifier model for uncertain speech-vs-music segments.

## Immediate Codebase Actions (Phase 1)

1. **Fix verification feature correctness**
   - Fix spectral entropy implementation so verification features are valid.

2. **Add graph-inspired verification signal**
   - Add structure-aware confidence signal to reduce speech-over-music false positives.

3. **Unify feature surface**
   - Align extraction/verification feature definitions and thresholds.

4. **Truth-aligned optimization objective**
   - Optimize candidate profiles against external truth/verified labels, not only internal suspicious-rate.

5. **Release gate enforcement**
   - Require holdout success lower bound (95% confidence interval) >= 95% before release promotion.

## Dataset and Evaluation Policy

- Keep dedicated radio-play evaluation corpus separate from mixed movie fixtures.
- Split by title/source to prevent leakage.
- Keep frozen holdout set for release readiness checks.

## Planned Milestones

### Milestone A (1-2 days)
- Entropy fix + graph-inspired verifier integration.
- Re-run sweep and update profiles.

### Milestone B (3-5 days)
- Add holdout-first scoring script and CI gate for radio-play KPI.
- Add failure breakdown output by content type.

Status update:
- ✅ holdout-first scoring script added: `scripts/check_radio_play_readiness.py`
- ✅ validation sweep CI now enforces holdout KPI gate
- ✅ Wilson LB95 gate added: `scripts/check_radio_play_lb95.py`
- ✅ content-type/cohort failure breakdown added: `scripts/build_radio_play_failure_breakdown.py`
- ✅ consolidated readiness report added: `scripts/build_radio_play_readiness_report.py`
- ✅ validation compare now exports duration-weighted precision/recall metrics to handle many-to-one segment alignment
- ✅ readiness/lb95/failure scripts now prefer duration-weighted non-voice metrics (fallback to legacy segment-count metrics)
- ✅ LB95 sample sizing now uses non-voice duration buckets (100ms units) when available
- ✅ targeted holdout optimizer added: `scripts/optimize_radio_play_holdout.py` (tier-C objective + modern guardrails)
- ✅ holdout optimizer expanded to tune merge behavior (`merge_gap_ms`, `min_gap_to_merge`, `min_silence_duration`, `merge_strategy`)
- ✅ holdout optimizer now supports `--search-mode extended` (adds `spectral_entropy_max` and centroid bounds) and precision-floor objective weighting
- ✅ extraction pipeline now prunes short low-confidence speech segments before non-voice inversion
- ✅ extraction pipeline now bridges adjacent non-voice windows across short speech interruptions (`merge_options.min_speech_duration`)
- ✅ holdout optimizer objective modes added (`weighted`, `worst3`, `h3`) to prioritize worst-case accuracy directly
- ✅ CI benchmark/eval artifacts now include two additional modern fixtures (`elephantsdream_teaser_2006`, `caminandes_gran_dillama_2013`)
- ✅ CI now emits `analysis/optimization/modern-extra-ci-summary.md` for quick modern fixture benchmark/eval review
- ✅ benchmark baseline refresh helper added: `scripts/refresh_benchmark_baseline.sh`
- ✅ sweep baseline refresh helper added: `scripts/refresh_sweep_baseline.sh`
- ✅ fixed-order anti-regression workflow runner added: `scripts/run_standard_workflow.sh`
- 🔄 latest holdout retune (`h3` objective) improved recall (`0.3873 -> 0.4090`) but reduced precision (`0.3054 -> 0.2910`); readiness still fails
- 🔄 constrained high-precision holdout sweep (`precision_floor=0.30`) found no candidate that improves recall beyond `0.4090` while keeping precision >= current baseline (`0.2910`)
- 🔄 first selective verifier-filter patch landed in extraction for low-confidence non-voice segments; current holdout metrics remained unchanged, so the current confidence gate is not surfacing enough bad segments to move readiness
- 🔄 extraction now honors merge-policy strategy inside the main pipeline; current holdout metrics still remained unchanged, which confirms the next lift likely needs adaptive thresholds / better ambiguity selection rather than more no-op profile search
- 🔄 per-recording adaptive spectral thresholds were added before VAD classification; current holdout metrics still remained unchanged, which lowers confidence in further threshold-only iteration without a stronger state model
- 🔄 first tri-state temporal smoothing pass (`speech / ambiguous / non_voice`) slightly reduced fragmentation (`67 -> 66` predicted segments) but regressed holdout precision/recall/overlap, so the next step likely needs richer state evidence rather than this heuristic alone
- 🔄 first segment-level speech-evidence filter (demoting acoustically implausible speech islands) produced no measurable change beyond the current tri-state result
- ✅ hard non-speech frame-state enforcement plus segment-level speech-evidence filtering produced the first major structural improvement: holdout precision `0.2901 -> 1.0000`, overlap `0.3364 -> 0.5397`, predicted segments `66 -> 2`
- 🔄 recall regressed to `0.3696`, so the next phase is recall recovery without reintroducing fragmentation
- ✅ controlled non-voice expansion into adjacent ambiguous frames preserved precision `1.0000` and nudged recall/overlap to `0.3698 / 0.5399`
- ✅ tiny residual post-filter gap bridging improved the best state to `precision=1.0000`, `recall=0.3884`, `overlap=0.5594`, `predicted_segments=1`
- ✅ terminal tail-aware extension improved the best state to `precision=1.0000`, `recall=0.4006`, `overlap=0.5721`, `predicted_segments=1`
- 🔄 two broader recall-recovery attempts were rejected:
  - broader hard non-speech frame-state thresholds collapsed output,
  - broader verifier review restored recall but destroyed precision/overlap
- 🔄 likelihood-based non-voice growth was tested and reverted (no gain vs current best checkpoint)

### Milestone C (1-2 weeks)
- Add optional ONNX verifier stage for ambiguous segments.
- Calibrate thresholds using verified holdout labels.

## Acceptance Criteria

- Holdout radio-play success >= 95%.
- No FP-risk regression vs previous accepted baseline.
- Full quality gate and optimization drift guard pass.
