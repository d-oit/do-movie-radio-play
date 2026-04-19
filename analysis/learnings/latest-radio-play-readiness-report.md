# Latest Radio-Play Readiness Report

- Source summary: `analysis/validation/radio-play-sweep-summary.json`
- Holdout tier: `C`
- Readiness pass: `False`
- Threshold gate pass: `False`
- LB95 gate pass: `False`

## Cohort Summary
- **modern**: count=0, precision=0.0000, recall=0.0000, overlap=0.0000, precision_lb95=0.0000, recall_lb95=0.0000, overlap_lb95=0.0000
- **legacy**: count=1, precision=0.0000, recall=0.0000, overlap=0.1783, precision_lb95=0.0000, recall_lb95=0.0000, overlap_lb95=0.0076

## Threshold Failures
- the_hole_1962_radio: non_voice_precision=0.0000 < 0.9500
- the_hole_1962_radio: non_voice_recall=0.0000 < 0.9500
- the_hole_1962_radio: overlap_ratio=0.1783 < 0.9500

## LB95 Failures
- the_hole_1962_radio: precision_lb95=0.0000 < 0.9500
- the_hole_1962_radio: recall_lb95=0.0000 < 0.9500
- the_hole_1962_radio: overlap_lb95=0.0076 < 0.9500
