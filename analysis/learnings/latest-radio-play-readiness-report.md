# Latest Radio-Play Readiness Report

- Source summary: `analysis/validation/radio-play-sweep-summary.json`
- Holdout tier: `C`
- Readiness pass: `False`
- Threshold gate pass: `False`
- LB95 gate pass: `False`

## Cohort Summary
- **modern**: count=0, precision=0.0000, recall=0.0000, overlap=0.0000, precision_lb95=0.0000, recall_lb95=0.0000, overlap_lb95=0.0000
- **legacy**: count=1, precision=1.0000, recall=0.4006, overlap=0.5721, precision_lb95=0.9903, recall_lb95=0.3704, overlap_lb95=0.5408

## Threshold Failures
- the_hole_1962_radio: non_voice_recall=0.4006 < 0.9500
- the_hole_1962_radio: overlap_ratio=0.5721 < 0.9500

## LB95 Failures
- the_hole_1962_radio: recall_lb95=0.3704 < 0.9500
- the_hole_1962_radio: overlap_lb95=0.5408 < 0.9500

## Compact Learnings
- Current best structural state is `precision=1.0000`, `recall=0.4006`, `overlap=0.5721`, `predicted_segments=1`.
- Two broader recall-recovery attempts were worse and were not kept:
  - broad hard `music_like` / `noise_like` frame-state detection collapsed non-voice output,
  - widening verifier filtering beyond short uncertain spans restored recall but destroyed precision/overlap.
- Tail-aware extension of the final accepted non-voice segment improved recall/overlap again without sacrificing precision, confirming that the safest gains are narrow and shape-aware.
