# Latest Radio-Play Failure Breakdown

- Source summary: `analysis/validation/radio-play-sweep-summary.json`
- Entries analyzed: `9`
- Failing entries (<0.95 on overlap/precision/recall): `9`

## Cohort Averages
- **modern**: count=5, languages=de,en,es, overlap=0.9107, precision=0.9236, recall=0.8981, worst=elephants_dream_2006_de_radio
- **legacy**: count=2, languages=en,fr, overlap=0.8030, precision=0.8944, recall=0.7286, worst=charade-1963-fr-holdout

## Top Failures
- `synthetic_alternating` (synthetic, und) overlap=1.0000, precision=0.6333, recall=1.0000
- `synthetic_speech_music_like` (synthetic, und) overlap=1.0000, precision=0.5000, recall=1.0000
- `elephants_dream_2006_de_radio` (modern, de) overlap=0.8975, precision=0.9096, recall=0.8857
- `elephants_dream_2006_es_radio` (modern, es) overlap=0.8975, precision=0.9096, recall=0.8857
- `tos-en-holdout` (modern, en) overlap=0.9032, precision=0.9179, recall=0.8890
- `tos-de-holdout` (modern, de) overlap=0.9032, precision=0.9179, recall=0.8890
- `sintelholdoutentry` (modern, en) overlap=0.9521, precision=0.9632, recall=0.9412
- `plan9-1959-en-holdout` (legacy, en) overlap=0.8077, precision=0.9060, recall=0.7286
- `charade-1963-fr-holdout` (legacy, fr) overlap=0.7983, precision=0.8827, recall=0.7287
