# Holdout Corpus and Readiness Reporting

This document is the provenance and policy record for the radio-play validation
corpus. It exists so a failing film can be diagnosed without re-deriving where
its media, truth and licence came from.

## Cohorts

| Cohort | Definition | Purpose |
|---|---|---|
| `modern` | Released 2000 or later | The shipped detector's target material |
| `legacy` | Released before 2000 | Older mixes, optical sound and noisier dialogue |
| `synthetic` | Generated fixtures | Deterministic contract and regression checks |

The blocking ratchet and the aspirational 0.95 target both score tier C
holdouts. A cohort is only considered covered when it has at least two
independently annotated real-media entries; `check_validation_coverage.py
--require-cohorts` enforces that.

## Corpus

| Entry | Tier | Cohort | Language | Media | Truth | Licence |
|---|---|---|---|---|---|---|
| `synthetic_alternating` | A | synthetic | und | generated WAV | generated truth JSON | synthetic |
| `elephants_dream_2006_de_radio` | A | modern | de | *Elephants Dream* (2006) | German authored subtitles | CC BY 3.0 |
| `elephants_dream_2006_es_radio` | B | modern | es | *Elephants Dream* (2006) | Spanish authored subtitles | CC BY 3.0 |
| `tos-en-holdout` | C | modern | en | *Tears of Steel* (2012) | English authored subtitles | CC BY 3.0 |
| `tos-de-holdout` | C | modern | de | *Tears of Steel* (2012) | German authored subtitles | CC BY 3.0 |
| `sintelholdoutentry` | C | modern | en | *Sintel* (2010) | embedded English subtitles | CC BY 3.0 |
| `plan9-1959-en-holdout` | C | legacy | en | *Plan 9 from Outer Space* (1959) | Wikimedia timed text, 680 cues | PD film, CC BY-SA 3.0 text |
| `charade-1963-fr-holdout` | C | legacy | fr | *Charade* (1963) | Wikimedia timed text, 1363 cues | PD film, CC BY-SA 3.0 text |

Media is fetched by `scripts/fetch_test_assets.sh` and is never committed
(`testdata/raw/*` is ignored). The legacy films are public domain in the United
States; their timed text is community-authored on Wikimedia Commons under
CC BY-SA 3.0. Cue counts are asserted by the fetch script so a truncated or
replaced subtitle file fails loudly.

## Tuning separation

- Tier C entries are holdouts. They must not appear as a tuning target in
  `analysis/optimization/*.json`; `check_validation_coverage.py
  --require-holdout-separation` fails the sweep when one does.
- The cue-path manifest keeps `split=dev` and `split=holdout` apart. Pad and
  evidence thresholds are selected on `dev` only.
- A film that informed a threshold is still useful as a regression holdout, but
  it must say so in `prior_tuning` and must not be presented as unseen evidence.
- New candidates live in `analysis/media/*-intake.json` until they are ingested
  and measured. Promotion to tier C is a deliberate, reviewed change.

## Two scoring paths

`analysis/validation/radio-play-readiness-report.json` keeps them separate:

- `detector_only` - the VAD timeline scored against truth. This is what the
  blocking ratchet and the 0.95 target use.
- `cue_assisted` - the shipped path: subtitle/Whisper cues, cue padding, gap
  subtraction and narration placement, read from a cue-path sweep. It reports
  `dialogue_leak_ratio`, `narratable_retained_ratio`, cue precision/recall and
  missed short/shouted utterances.

Every per-film row carries cohort, language, VAD engine, cue source and truth
source. Cohort averages are reported next to the worst entry so a pooled mean
cannot hide a single failing source.

`language` is the truth/cue language, not the on-screen spoken language. The
detector-only path never reads subtitles, so `tos-en-holdout` and
`tos-de-holdout` share detector metrics by construction; the language dimension
matters for the cue-assisted path, where cues are derived from that language's
subtitle track.

## Diagnosing a failure

```bash
python3 scripts/export_radio_play_window_diagnosis.py \
  --readiness-report analysis/validation/radio-play-readiness-report.json \
  --manifest testdata/validation/radio-play-manifest.json \
  --output-dir analysis/validation/window-diagnosis
```

The export lists contiguous false-positive and false-negative windows with time
ranges, predicted label, predicted acoustic tags and the ground-truth label and
text. A single entry can be diagnosed directly with `--entry-id --media
--truth --total-ms`.

`analysis/validation/window-diagnosis/synthetic_speech_music_like.*` is the
deterministic regression fixture for the failure mode behind the Sintel miss:
music-like non-voice is called speech, so non-voice recall collapses to zero on
`testdata/generated/speech_music_like.wav`. It needs no media download or model.

## Trend artifact

```bash
python3 scripts/build_radio_play_trend.py \
  --summary analysis/validation/radio-play-sweep-summary.json \
  --history analysis/validation/radio-play-trend.json \
  --output-md analysis/learnings/latest-radio-play-trend.md
```

The weekly job appends one point per run, shows per-entry deltas, and records
pending sources and unavailable optional dependencies instead of dropping them.

## Gates

- **Blocking:** `scripts/check_radio_play_readiness.py --floors
  testdata/validation/readiness-floors.json` - per-entry regression ratchet.
- **Non-blocking:** `scripts/build_radio_play_readiness_report.py` with the 0.95
  thresholds - the aspirational target, reported separately.
- **Coverage:** `scripts/check_validation_coverage.py --require-cohorts
  --min-languages 2 --require-holdout-separation`.

The 0.95 target stays non-blocking until corpus size and confidence intervals
justify changing policy.
