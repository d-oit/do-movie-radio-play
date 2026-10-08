# GOAP State

**Current Goal**: Unified Orchestrator — Implement #237, #241, #239, #240, #238 (Provider & Compute Agnostic + Config + Voice Cloning + Narrator + Pipeline)
**Status**: Complete — merged via PR #246 on 2026-09-03
**Branch**: Historical: feat/goap-unified-orchestrator
**Issues**: #237 #241 #239 #240 #238 (depends on closed #235)
**PR**: #246 https://github.com/d-oit/do-movie-radio-play/pull/246 (merged)
**Strategy**: Hybrid — Sequential foundation (config+provider) → Parallel swarm (voice-clone + narrator + orchestrator scaffolding)

## Task Graph
- [x] T0: Branch & GOAP state init (this file) + workflow-state.json
- [x] T1: ADRs 123-126 (provider, unified config, voice-clone, narrator)
- [x] T2: Unified Config Schema (#241) — AppConfig, layered loading (CLI>env>local.toml>default.toml), MRPLAY_* env, validation, JSON schema, .env.example
- [x] T3: Provider Architecture (#237) — ExecutionLocation, ComputeEndpoint, registry, GPU pool routing (provider-neutral)
- [x] T4: Voice Cloning Pipeline (#239) — VoiceReference types, sample extraction, persistence, capability checks, routing
- [x] T5: Narrator AI Prompt Engine (#240) — NarratorAiBackend trait, OpenAI/Ollama/Anthropic, Tera templates, hot-reload, CLI dry-run
- [x] T6: Full Pipeline Orchestrator (#238) — 12 stages, checkpoint, retry policy, compute-aware scheduling, produce CLI
- [x] T7: Quality gates (fmt pass, clippy types/pipeline pass, deny pass, full CI on PR #246)
- [x] T8: Closeout docs & PR (address review comments, merge)

## Evidence Log
- 2026-09-01: Plan approved — audited 5 open issues, verified deps, researched audio.cpp server API, config-rs layered best practice, Tera templating
- Research: audio.cpp server endpoints GET /health /v1/models POST /v1/audio/speech, config crate layered builder, Tera 2.1 runtime templates, dotenvy fork
- 2026-09-03: T2-T6 implemented — AppConfig + validation + loader, ComputeEndpoint + ProviderRegistry, VoiceReference + extract_candidates, narrator Tera + backends, orchestrator checkpoint + produce/narrate/voice/config CLI
- 2026-09-03: fmt pass, clippy -p types/pipeline/io/verification/learning/render/validation pass, tests types 16 + pipeline 55 pass, cargo-deny pass
- 2026-09-03: PR #246 merged after CI and review closeout.

## History
- 2026-09-01: T0 start — branch feat/goap-unified-orchestrator from main @73ae0c2
- 2026-09-01: T1 complete — ADRs 123-126 pushed (e580ce4), PR #246 created, CI green on scaffold
- 2026-09-03: T2-T6 complete — full implementation, fmt/clippy/tests/deny pass locally (voice/timeline full build deferred to CI due to missing clang locally)

## F0 Harmonicity Measurement (#362, step 1) — negative result
- 2026-10-05: autocorrelation peak in the 80–300 Hz lag band, <1 kHz low-passed, on Elephants Dream audio with the German SRT as dialogue truth (non-silent frames only). Script: `scripts/research/measure_f0_harmonicity.py`.
- Best Youden J (P(speech>t) − P(non-speech>t)): 0.13 (40 ms), 0.14 (64 ms), 0.14 (100 ms); a 3-frame voicing-continuity variant was worse (0.11–0.12). Percentile medians: speech 0.42 vs non-speech 0.34.
- Conclusion: the feature does not discriminate (score and ambience are also harmonic), so per the issue's own rule it was **not** added to `Frame`/`classify_frame_states`. Caveat: SRT spans include inter-word pauses and overlapping music, which lowers the ceiling for any frame feature.
- Next candidates for #362: a learned/pretrained VAD as the independent axis, or visual input; both need new dependencies and a decision.

## Silero VAD engine (#362 second axis) — implemented, opt-in
- 2026-10-06: `--features silero-vad` (ort, load-dynamic) adds the `silero` engine; `radio-play --vad-engine silero`. Needs `scripts/fetch_silero_vad.sh` (model → `models/`, git-ignored) and `ORT_DYLIB_PATH`. Threshold via `SILERO_VAD_THRESHOLD` (default 0.5; results were identical at 0.3/0.5/0.7 on Elephants Dream).
- Elephants Dream vs German SRT (`scripts/research/score_timeline_vs_srt.py`), energy → silero: non-voice P 0.779→0.807, non-voice R 0.628→0.856, speech P 0.341→0.439, speech R 0.518→0.355. Raw Silero (python, no smoothing) reaches non-voice R 0.955.
- It improves both non-voice precision and recall, so it is an independent axis. It did **not** raise narration counts (gaps energy→silero: ED 5→4, Sintel 1→1, ToS 8→4): coverage is bounded by gap scoring in `gaps/mod.rs`, not only by detection. SRT spans include pauses, so absolute numbers are approximate. Default engine unchanged; flipping it needs the sweep re-fit (#363/#364).

## Second film + Silero tri-state fix (#364, #362) — 2026-10-06
- Second film: Tears of Steel (CC-BY). Timed `TOS-en.srt`/`TOS-de.srt` fetched with pinned SHA-256 by `scripts/fetch_test_assets.sh` (film itself behind `FETCH_SECOND_FILM=1`, 372 MB). Manifest: `testdata/validation/second-film-manifest.json` (tier C, a film no profile was fitted on); `check_validation_coverage.py --tier C` passes. en/de are one audio track, so this adds one film, not two.
- **Parser bug found by this**: `srt::parse_srt_segments` split on `"\n\n"`, so CRLF/BOM subtitle files yielded one cue (TOS-en is CRLF). Fixed + tests; the same parser feeds gap scoring.
- **Dev-fit profile does not generalise**: `modern-optimized` on ToS predicts one non-voice segment for the whole film (non-voice P 0.814 / R 1.000 = trivial "everything is a gap" baseline). This is the quantified cost of the one-film corpus. The 16-candidate sweep was **not** re-run.
- **Tri-state bug**: the music/noise spectral vetoes ran before the engine likelihood and the engine threshold was never used, so a neural speech call was overruled by hand-built rules. `tri_state::resolve_speech(.., trust_likelihood)` now lets confident likelihoods win for `silero` only; Silero likelihoods are re-centred so `SILERO_VAD_THRESHOLD` maps to the ambiguity midpoint. Other engines unchanged.
- Time-domain scores vs timed SRT (`scripts/research/score_timeline_vs_srt.py`), default config:

| film | engine | speech P/R | non-voice P/R |
|---|---|---|---|
| Elephants Dream (dev) | energy | 0.341/0.518 | 0.779/0.628 |
| Elephants Dream (dev) | silero 0.3 | 0.530/0.753 | 0.910/0.788 |
| Tears of Steel (holdout) | energy | 0.188/0.580 | 0.847/0.482 |
| Tears of Steel (holdout) | silero 0.3 | 0.412/0.651 | 0.918/0.808 |

- Default threshold 0.3 chosen on the dev film (curve flat 0.1–0.4), then confirmed on the holdout. Silero beats energy on all four metrics on both films, so the second-axis claim of #362 holds on two films. Still opt-in (needs model + ORT dylib).
- Caveat: SRT cues include pauses, so absolute values are approximate. `timeline validate` speech metrics are vacuous (0/0=1.0) because predicted timelines contain only non-voice segments; the time-domain script is the real measure until that is fixed.

## Films without subtitles — dialogue cues (2026-10-06)
- `radio-play` now resolves dialogue cues itself (`handlers/cues.rs`): explicit `--subtitles` → sidecar `film[.lang].srt` → embedded subtitle track (ffmpeg) → speech-to-text (`scripts/transcribe_cues.py`, faster-whisper `base`, CPU, ~40 s per 12 min film). Derived cues are cached as `<output>.cues.srt`; `--no-auto-cues` disables. Any failure warns and falls back to the VAD alone. Sintel's MKV had embedded tracks all along (26 cues).
- Cues are a hard veto in `subtract_cues` (+300 ms pad), so only timings matter; the transcriber auto-detects the film's language (`--language` is the *narration* language).
- Measured on the two truth films (narratable non-voice time that is truly dialogue, Silero detector): none → whisper-base cues → real subtitles = Tears of Steel 8.3% → 6.4% → 0.0%; Elephants Dream 9.1% → 4.4% → 0.0%. Whisper `small` gave no gain over `base` (6.5%). Whisper cue recall of dialogue time 0.61 (ToS) / 0.78 (ED) at +0.3 s pad.
- Conclusion: auto cues remove roughly 25–50% of the leakage; they are not a substitute for real subtitles. Residual risk for unsubtitled films is 5–6% of narratable time. Next lever: ensemble (union) of Silero and whisper speech, or narrate only windows where both agree on non-speech.

### Update: tuned speech-to-text cues (2026-10-06)
- Correction: "union of Silero + whisper" was already how it worked (gaps = Silero non-voice minus cues), so the 6% residual was the combined result. The Silero threshold (0.05-0.3) and a wider pad alone barely moved it.
- Cause of the misses: 1-3 s shouted lines over loud action ("Cut!", "Abort!", "Nooooo!") that whisper's built-in VAD pre-filter discards. `transcribe_cues.py` now runs with the filter off by default (`--vad` restores it): ~3 min per 12 min film on CPU instead of ~30 s.
- Derived cues are rougher than authored ones, so they get an 800 ms pad (`DERIVED_CUE_PAD_MS`) vs 300 ms for real subtitles; derived cues cache to `<output>.cues.stt.srt`, embedded-track cues to `<output>.cues.srt`.
- Result (narratable non-voice time that is truly dialogue / narratable seconds): Tears of Steel 8.3% -> 0.9% (563 -> 448 s); Elephants Dream 9.1% -> 2.4% (469 -> 388 s); real subtitles 0.0%. Dialogue-time recall of the cues at +0.8 s: 0.97 (ToS). Cost: 12-20% less narratable time from cues over music (precision ~0.5).

## Honest validation + gate that tracks what ships (2026-10-06)
- `timeline validate` speech metrics were vacuous (0/0 = 1.0): extraction and subtitle-truth timelines hold only non-voice segments. `score_segments_with_total` now derives speech as the complement of non-voice over `--total-ms` when neither side has speech; `overlap_ratio`/`boundary_error_ms` are unchanged. Cross-check vs the independent scorer: speech recall 0.580 / 0.651 on Tears of Steel (energy / silero), identical to 3 digits.
- Consequence: the shipped `modern-optimized` profile scores speech P/R **0/0** on both real films (it calls nearly everything a gap) where it used to print a perfect 1.0.
- `radio-play` does not use a fitted profile: it runs the default `AnalysisConfig` with Silero. The sweep manifest now scores exactly that (`vad_engine: silero`, no `config_path`, profile label `radio-play-default`); tier C is Tears of Steel (the weekly sweep had been failing since before this branch because tier C pointed at a nonexistent `sintel_trailer_2010.srt`).
- Holdout (Tears of Steel) floors, now meaningful: non-voice P/R 0.918/0.889, speech P/R 0.572/0.651, overlap 0.903 (`testdata/validation/readiness-floors.json`). The ratchet is the blocking gate; the 0.95 report is non-blocking and aspirational.
- `scripts/fetch_silero_vad.sh` now pins v6.2.3 with a SHA-256 check (the first version pinned v5.1.2, a different model than every measurement here).

## Third film (Sintel) and deterministic, gated speech-to-text cues (2026-10-08)
- **Sintel as unseen film**: embedded EN/DE subtitle tracks give 26 timed cues (`ffmpeg -map 0:s:N`); Sintel was never used to tune the detector. Default config, Silero (`timeline validate`): non-voice P/R 0.963/0.941, speech P/R 0.494/0.615, overlap 0.952. The **energy engine finds no speech at all** on Sintel (speech P/R 0/0), so Silero as default is not a tuning artefact of the two earlier films.
- **Correction**: the "leak 8.3% -> 0.9%" for speech-to-text cues (previous section) was a lucky draw. faster-whisper's temperature fallback samples randomly, so the same film gave 70, 74 and 99 cues across runs, violating the repo's deterministic-output rule. `transcribe_cues.py` now decodes greedily (`temperature=0.0`); two runs are byte-identical.
- **Honest numbers with deterministic cues** (pad 0.8 s; narratable seconds / dialogue leak): Elephants Dream 469 s/9.1% -> 276 s/1.5%; Tears of Steel 579 s/8.3% -> 431 s/2.3%; Sintel 794 s/3.7% -> 508 s/3.7% (no gain, 36% of narration lost: only 14% of whisper's cue time on Sintel is real speech, it hallucinates over score).
- **Whisper's own confidences do not help** (no_speech_prob, avg_logprob, word probability): every setting traded narratable time for leak along the same line.
- **Fix: Silero evidence gate** (`--min-vad-evidence`, default 0.1): keep a cue only if the Silero model bundled with faster-whisper reaches that peak speech probability inside it. Result (narratable s / leak): ED 384/2.1%, ToS 516/3.7%, Sintel 770/3.5%; mean narratable 405 -> 557 s (+37%) for leak 2.5% -> 3.1%; flat for 0.1-0.3. Versus no cues: leak 9.1/8.3/3.7% -> 2.1/3.7/3.5%.
- Caveat: floor and pad were chosen on these same three films; no fourth unseen film exists locally. Real subtitles remain the reliable path (0% leak).

### Sintel as second sweep holdout (2026-10-08)
- Manifest tier C now has two entries (Tears of Steel, Sintel); `fetch_test_assets.sh` (with `FETCH_SECOND_FILM=1`) downloads Sintel's 681 MB zip, extracts the MKV and its English subtitle track (`-map 0:s:m:language:eng`, 26 cues, asserted). Only the weekly sweep pays the download.
- Seeded floors, Sintel: non-voice P/R 0.963/0.941, speech P/R 0.494/0.615, overlap 0.952. A hand-degraded Sintel speech recall turns the ratchet red.
- `check_radio_play_readiness.py --update-floors` now seeds floors for entries that have none (it used to refuse, which made adding a film impossible without hand-editing).
- Sweep not yet run in CI (weekly/dispatch only); the local run is `run_validation_manifest.py` over the 5 entries.
