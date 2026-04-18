# Latest Radio-Play Readiness Report

- Source summary: `analysis/validation/full-sweep-summary.json`
- Holdout tier: `C`
- Readiness pass: `False`
- Threshold gate pass: `False`
- LB95 gate pass: `False`

## Cohort Summary
- **modern**: count=0, precision=0.0000, recall=0.0000, overlap=0.0000, precision_lb95=0.0000, recall_lb95=0.0000, overlap_lb95=0.0000
- **legacy**: count=1, precision=0.0000, recall=0.0000, overlap=0.1436, precision_lb95=0.0000, recall_lb95=0.0000, overlap_lb95=0.0050

## Threshold Failures
- the_hole_1962_subtitles: non_voice_precision=0.0000 < 0.9500
- the_hole_1962_subtitles: non_voice_recall=0.0000 < 0.9500
- the_hole_1962_subtitles: overlap_ratio=0.1436 < 0.9500

## LB95 Failures
- the_hole_1962_subtitles: precision_lb95=0.0000 < 0.9500
- the_hole_1962_subtitles: recall_lb95=0.0000 < 0.9500
- the_hole_1962_subtitles: overlap_lb95=0.0050 < 0.9500
