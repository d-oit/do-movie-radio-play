# Latest Radio-Play Failure Breakdown

- Source summary: `analysis/validation/radio-play-sweep-summary.json`
- Entries analyzed: `5`
- Failing entries (<0.95 on overlap/precision/recall): `5`

## Cohort Averages
- **modern**: count=5, overlap=0.9301, precision=0.8667, recall=0.9203
- **legacy**: count=0, overlap=0.0000, precision=0.0000, recall=0.0000

## Top Failures
- `synthetic_alternating` (modern) overlap=1.0000, precision=0.6333, recall=1.0000
- `elephants_dream_2006_de_radio` (modern) overlap=0.8975, precision=0.9096, recall=0.8857
- `elephants_dream_2006_es_radio` (modern) overlap=0.8975, precision=0.9096, recall=0.8857
- `tos-en-holdout` (modern) overlap=0.9032, precision=0.9179, recall=0.8890
- `sintelholdoutentry` (modern) overlap=0.9521, precision=0.9632, recall=0.9412
