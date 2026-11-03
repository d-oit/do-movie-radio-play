# Latest Radio-Play Readiness Report

- Source summary: `analysis/validation/radio-play-sweep-summary.json`
- Manifest: `testdata/validation/radio-play-manifest.json`
- Holdout tier: `C`
- Readiness pass: `False`
- Threshold gate pass: `False`
- LB95 gate pass: `False`

## Detector-Only Cohort Summary
- **modern**: count=3, languages=de,en, precision=0.9330, recall=0.9064, overlap=0.9195, precision_lb95=0.9266, recall_lb95=0.8992, overlap_lb95=0.9127, worst=tos-en-holdout (0.9032)
- **legacy**: count=2, languages=en,fr, precision=0.8944, recall=0.7286, overlap=0.8030, precision_lb95=0.8902, recall_lb95=0.7232, overlap_lb95=0.7981, worst=charade-1963-fr-holdout (0.7983)

## Detector-Only Per-Film

| Entry | Cohort | Lang | VAD | Precision | Recall | Overlap | Overlap LB95 |
|---|---|---|---|---:|---:|---:|---:|
| `tos-en-holdout` | modern | en | silero | 0.9179 | 0.8890 | 0.9032 | 0.8955 |
| `tos-de-holdout` | modern | de | silero | 0.9179 | 0.8890 | 0.9032 | 0.8955 |
| `sintelholdoutentry` | modern | en | silero | 0.9632 | 0.9412 | 0.9521 | 0.9472 |
| `plan9-1959-en-holdout` | legacy | en | silero | 0.9060 | 0.7286 | 0.8077 | 0.8024 |
| `charade-1963-fr-holdout` | legacy | fr | silero | 0.8827 | 0.7287 | 0.7983 | 0.7938 |

## Threshold Failures
- tos-en-holdout: non_voice_precision=0.9179 < 0.9500
- tos-en-holdout: non_voice_recall=0.8890 < 0.9500
- tos-en-holdout: overlap_ratio=0.9032 < 0.9500
- tos-de-holdout: non_voice_precision=0.9179 < 0.9500
- tos-de-holdout: non_voice_recall=0.8890 < 0.9500
- tos-de-holdout: overlap_ratio=0.9032 < 0.9500
- sintelholdoutentry: non_voice_recall=0.9412 < 0.9500
- plan9-1959-en-holdout: non_voice_precision=0.9060 < 0.9500
- plan9-1959-en-holdout: non_voice_recall=0.7286 < 0.9500
- plan9-1959-en-holdout: overlap_ratio=0.8077 < 0.9500
- charade-1963-fr-holdout: non_voice_precision=0.8827 < 0.9500
- charade-1963-fr-holdout: non_voice_recall=0.7287 < 0.9500
- charade-1963-fr-holdout: overlap_ratio=0.7983 < 0.9500

## LB95 Failures
- tos-en-holdout: precision_lb95=0.9105 < 0.9500
- tos-en-holdout: recall_lb95=0.8808 < 0.9500
- tos-en-holdout: overlap_lb95=0.8955 < 0.9500
- tos-de-holdout: precision_lb95=0.9105 < 0.9500
- tos-de-holdout: recall_lb95=0.8808 < 0.9500
- tos-de-holdout: overlap_lb95=0.8955 < 0.9500
- sintelholdoutentry: recall_lb95=0.9359 < 0.9500
- sintelholdoutentry: overlap_lb95=0.9472 < 0.9500
- plan9-1959-en-holdout: precision_lb95=0.9017 < 0.9500
- plan9-1959-en-holdout: recall_lb95=0.7227 < 0.9500
- plan9-1959-en-holdout: overlap_lb95=0.8024 < 0.9500
- charade-1963-fr-holdout: precision_lb95=0.8786 < 0.9500
- charade-1963-fr-holdout: recall_lb95=0.7236 < 0.9500
- charade-1963-fr-holdout: overlap_lb95=0.7938 < 0.9500

## Cue-Assisted (Shipped Path)
- Source: `analysis/validation/cue-path-sweep-real.json`
- `tears_of_steel_2012_en` (en, exact_subtitles, detector_plus_cues): dialogue_leak=0.0, narratable_retained=0.9179, missed_short=0, missed_shouted=0
- `tears_of_steel_2012_en` (en, derived_whisper, detector_plus_cues): dialogue_leak=0.0346, narratable_retained=0.8083, missed_short=5, missed_shouted=3
- `sintel_2010_en` (en, derived_whisper, detector_plus_cues): dialogue_leak=0.0351, narratable_retained=0.7953, missed_short=0, missed_shouted=3
- `synthetic_kitchen_en` (en, derived_whisper, detector_plus_cues): dialogue_leak=0.0, narratable_retained=0.0, missed_short=0, missed_shouted=0
- `sprite_fright_2021_en` (en, derived_whisper, detector_plus_cues): dialogue_leak=0.1035, narratable_retained=0.4602, missed_short=0, missed_shouted=7
- `synthetic_night_de` (de, derived_whisper, detector_plus_cues): dialogue_leak=0.0, narratable_retained=0.0, missed_short=0, missed_shouted=0
- Pending assets: cosmos_laundromat_2015_en
