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

### Milestone C (1-2 weeks)
- Add optional ONNX verifier stage for ambiguous segments.
- Calibrate thresholds using verified holdout labels.

## Acceptance Criteria

- Holdout radio-play success >= 95%.
- No FP-risk regression vs previous accepted baseline.
- Full quality gate and optimization drift guard pass.
