# Latest Radio-Play Readiness Report

- Source summary: `analysis/validation/radio-play-sweep-summary.json`
- Holdout tier: `C`
- Readiness pass: `False`
- Threshold gate pass: `False`
- LB95 gate pass: `False`

## Cohort Summary
- **modern**: count=2, precision=0.9405, recall=0.9151, overlap=0.9276, precision_lb95=0.9347, recall_lb95=0.9084, overlap_lb95=0.9213
- **legacy**: count=0, precision=0.0000, recall=0.0000, overlap=0.0000, precision_lb95=0.0000, recall_lb95=0.0000, overlap_lb95=0.0000

## Threshold Failures
- tos-en-holdout: non_voice_precision=0.9179 < 0.9500
- tos-en-holdout: non_voice_recall=0.8890 < 0.9500
- tos-en-holdout: overlap_ratio=0.9032 < 0.9500
- sintelholdoutentry: non_voice_recall=0.9412 < 0.9500

## LB95 Failures
- tos-en-holdout: precision_lb95=0.9105 < 0.9500
- tos-en-holdout: recall_lb95=0.8808 < 0.9500
- tos-en-holdout: overlap_lb95=0.8955 < 0.9500
- sintelholdoutentry: recall_lb95=0.9359 < 0.9500
- sintelholdoutentry: overlap_lb95=0.9472 < 0.9500
