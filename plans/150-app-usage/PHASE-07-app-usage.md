# Phase 07: Truthful App Usage

**Status:** In progress (tracks #337; complementary to #336)
**Date:** 2026-09-24
**Scope:** Docs + CLI help text only. No pipeline tuning, no behavior change.

## Goal

Make app usage truthful: every `timeline` subcommand is documented in
`--help` and `README.md`, with one end-to-end workflow users can follow.
Profile micro-tuning stays paused per `plans/100-radio-play-95/ROADMAP.md`;
behavior fixes for `produce`/validation signals belong to #336.

## Non-goals

- No VAD/profile threshold changes.
- No `produce` stage behavior changes (see #336).
- No review-player code changes in this phase (triage only).

## 7.1 Command matrix (source: `crates/movie-radio-timeline/src/cli.rs`)

| Command | Truthful description |
|---------|----------------------|
| `extract` | Non-voice extraction pipeline on a media file. |
| `tag` | Acoustic tags (music, ambience) on an extracted timeline. |
| `prompt` | Narration prompts for tagged non-voice segments. |
| `review` | Interactive HTML review player. |
| `calibrate` | Calibration report from a corrections directory. |
| `apply-calibration` | Apply a calibration report to the active profile. |
| `bench` | Pipeline speed / stage timing JSON. |
| `gen-fixtures` | Deterministic synthetic WAV fixtures. |
| `validate` (`eval`) | Accuracy vs exactly one of `--truth-json`, `--subtitles`, `--dataset-manifest`. |
| `ai-voice-extract` | Speech segments for AI voice replacement. |
| `verify-timeline` | Spectral bounds check on a timeline. |
| `update-thresholds` | Adaptive VAD thresholds from learning state/DB. |
| `learning-stats` | Summary stats from the learning DB (`--radio-play` for radio-play scope). |
| `learning-log` | Last N learning rows (default 10). |
| `reset-learnings` | Reset adaptations; run history survives per ADR-122. Requires `--confirm`. |
| `export-learnings` | Export learning DB to JSON. |
| `learning-experiments` | Calibration runs, profile versions, experiments. |
| `merge-timeline` | Merge adjacent segments by gap/strategy. |
| `export` | Timeline to `json`, `edl`, or `vtt`. |
| `radio-play` | Full GOAP pipeline by default (gap → narrate → TTS → assemble → output); `--analyze-only` requires `--timeline` and emits gap analysis only. |
| `preview` | Stream WAV to system audio. Needs `playback` feature; `--skip`/`--duration` select a window. |
| `config validate` | Validate `config/default.toml` (or `--config` path). |
| `voice samples` | Extract per-character voice candidates to `voice_samples/{character}.json`. |
| `voice list` | Inventory stored voice references; warns on unreadable files. |
| `voice test` | Synthesize `--text` with a stored reference; fails loudly without one. |
| `narrate` | Render narrator template with example scene context; `--dry-run` prints the prompt, otherwise calls the configured LLM backend (`openai` \| `ollama_local` \| `anthropic`). |
| `produce` | 12-stage orchestrated run with checkpoint/resume. `ExtractAudio`, `VoiceActivityDetect`, `AudioMix`, `Export` do real audio work; the remaining stages currently write empty placeholder JSON and are tracked by #336. |

## 7.2 End-to-end workflow

```bash
timeline extract movie.mp4 --output analysis/timeline.json
timeline tag movie.mp4 --input analysis/timeline.json --output analysis/tagged.json
timeline prompt --input-json analysis/tagged.json --output analysis/prompts.json
timeline review movie.mp4 --input analysis/tagged.json --output reports/nonvoice-review.html
timeline radio-play movie.mp4 --timeline analysis/tagged.json --analyze-only --output analysis/gaps.json
```

Anti-regression order is fixed in `scripts/run_standard_workflow.sh`
(fetch assets → fixtures → quality gate → benchmarks → regression check).
Contributor gates live in `plans/040-validation/ACCEPTANCE.md`.

## 7.3 Radio-play modes

- Default `timeline radio-play <MOVIE>` runs the full GOAP pipeline via
  `handlers/radio_play.rs` (gap → narrate → TTS → assemble → output).
- `--analyze-only` requires `--timeline` and writes/prints gap analysis only.
- `--verify-quality` / `--apply-learnings` toggle the corresponding GOAP goals.

## 7.4 `produce` / `narrate` caveats (pending #336)

- `produce --dry-run` lists the 12 stages with provider/compute routing.
- Real runs persist `checkpoint.json`; `--resume` skips completed stages.
- Placeholder JSON stages must not be read as completed production work;
  behavior fix belongs to #336. This phase only documents current behavior.
- `narrate` uses example scene data (`narrator.rs`); pipeline-context
  narration is future work, not claimed here.

## 7.5 Review-player triage (no code in this phase)

Historical findings from `plans/070-review-player-testing/TESTING.md` + `UNRESOLVED-ISSUES.md` are all resolved in current source (`templates/review.html`, `review.rs`, `review_template.rs`) and closed in GitHub issues #269 and #270:

1. Saved HTML drops merged/individual view state (Resolved — #269).
2. No kind/confidence/duration filter or sort (Resolved — `#segment-filter`/`#segment-sort` in `review.html`).
3. Markers are click-only (no drag-to-seek) (Resolved — #270).
4. All-excluded state has no recovery path besides Undo (Resolved — #270 "Restore All (r)").
5. `m` shortcut / save-shortcut gaps noted in TESTING.md (Resolved).

## 7.6 Plans hygiene note

- `plans/050-status-report/STATUS.md` (2026-06-22) and parts of
  `plans/120-goap-radio-play-pipeline/ROADMAP.md` predate the merged
  orchestrator (#246). `plans/140-codebase-gap-analysis.md` already flags
  the staleness; do not rewrite history here — this plan is the current
  app-usage reference until #336 lands.
- Milestone C (ONNX verifier) stays deferred per
  `plans/100-radio-play-95/MILESTONE-C-DECISION.md`.

## Acceptance

- `timeline --help` shows a description for every command above.
- `README.md` command list matches the CLI surface.
- `bash scripts/quality_gate.sh` passes (or failures are filed, not hidden).
- `mise exec -- cargo test -p movie-radio-timeline` passes.
