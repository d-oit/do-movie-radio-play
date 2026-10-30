# Latest Radio-Play Readiness Report

- Source summary: `analysis/validation/full-sweep-summary.json`
- Holdout tier: `C`
- Readiness pass: `False`
- Threshold gate pass: `False`
- LB95 gate pass: `False`

## Cohort Summary
- **modern**: count=1, precision=0.8115, recall=0.9533, overlap=0.8767, precision_lb95=0.7746, recall_lb95=0.9287, overlap_lb95=0.8418
- **legacy**: count=0, precision=0.0000, recall=0.0000, overlap=0.0000, precision_lb95=0.0000, recall_lb95=0.0000, overlap_lb95=0.0000

## Threshold Failures
- sintel_trailer_2010_subtitles: non_voice_precision=0.8115 < 0.9500
- sintel_trailer_2010_subtitles: overlap_ratio=0.8767 < 0.9500

## LB95 Failures
- sintel_trailer_2010_subtitles: precision_lb95=0.7746 < 0.9500
- sintel_trailer_2010_subtitles: recall_lb95=0.9287 < 0.9500
- sintel_trailer_2010_subtitles: overlap_lb95=0.8418 < 0.9500
