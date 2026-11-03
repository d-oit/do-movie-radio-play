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
- Caveat: floor and pad were chosen on these same three films at the time; a genuinely unseen film was added on 2026-11-02 and did not hold up (see the measured sweep below). Real subtitles remain the reliable path (0% leak).
- **Cue-path harness (2026-11-02):** `scripts/evaluate_cue_path.py` scores truth SRT against exact, embedded, and derived cue files and reports dialogue-time recall, false cue coverage, retained narratable time, narration-on-dialogue leakage, missed short utterances, and Wilson intervals per film, source, and language. `testdata/cues/fixtures.json` is the deterministic PR gate; `testdata/validation/cue-eval-manifest.json` is the opt-in model sweep. The sweep selects pad/threshold on `split=dev` only and reports `split=holdout` with that config.
- **Synthetic holdouts (2026-11-02):** `scripts/generate_dialogue_fixtures.py` builds two scenes with real espeak-ng speech plus music, effects and melodic vocalise, and writes truth SRT from the same timeline (`testdata/generated/dialogue/`, gitignored: regenerate with the script). They are legally clean, deterministic, and both are `split=holdout` in the cue-eval manifest. Pad and evidence floor are selected on Elephants Dream alone; no threshold was fitted on either scene.
- **Caveat on their independence:** both scenes were revised after the first sweep, which returned 0% leak and full recall because the shouts sat clear of any effect. The shouts were then moved under loud impacts and away from neighbouring lines so a miss could not hide behind a neighbour's padding. The scenes are therefore **regression fixtures, not unbiased generalization evidence**: their 0% leak figures say the cue path still catches those specific lines, not that it would on unseen material. Treat them as a guard against silent regressions, and judge generalization on real films.
- **Measured end-to-end sweep (2026-11-02, `analysis/validation/cue-path-sweep-real.json`):** pad and evidence floor were chosen on the dev film only (Elephants Dream, ES subtitles as cues against DE truth so nothing is scored against itself): `derived_whisper` pad 0.8 s / floor 0.3, `exact_subtitles` pad 0. Those two configs were then applied unchanged to every holdout. Leakage here is the share of *retained narratable* time that is still true dialogue (`narration_on_dialogue / (retained + narration_on_dialogue)`), a different denominator from the 2-4% figures quoted above, which used the detector's gap time.

| film | leak (shipped) | dialogue recall | retained | narratable |
|---|---:|---:|---:|---:|
| Elephants Dream | 2.3% | 0.93 | 315 s | 469 s |
| Tears of Steel | 2.5% | 0.90 | 440 s | 579 s |
| Sintel | 3.1% | 0.68 | 589 s | 794 s |
| **Sprite Fright (unseen)** | **8.5%** | 0.89 | 118 s | 289 s |
| synthetic kitchen / night | 0.0% | 1.00 | 0 s | 0 s |

These are the **shipped** settings: pad 0.8 s (`DERIVED_CUE_PAD_MS`) and the transcriber's 0.1 evidence floor. `cues.rs` passes neither flag, so tuning performed by the sweep is advisory unless it is wired into the product — a disconnect worth stating plainly, because the dev-selected config (pad 0.3 s / floor 0.3) reports Sprite Fright at 10.3% while the product actually runs the 8.5% row above. The sweep now reports a `shipped` section on every entry so this comparison is automatic rather than reconstructed by hand.

With the tuned config instead (pad 0.3 s / floor 0.3), the same rows are 3.8% / 3.5% / 3.5% / 10.3% — tuning makes the tuned film and the other two slightly worse than the shipped pad on this metric, i.e. the sweep's selection does not transfer. The honest reading is that 0.8 s padding, not the sweep's winner, is the better setting on the shipped path.

**Correction (2026-11-02):** earlier rows in this section scored cues alone, crediting the entire non-dialogue timeline as narratable. The shipped path only narrates *detector non-voice minus cues*, so those figures (14.0%, then 11.3%) were upper bounds that ignored dialogue the detector already refuses to narrate. Scoring now takes the Silero timeline as an input (`--detector-timeline`, `analysis/validation/detector/`), and every row above is `detector_plus_cues`. The rows without a timeline are labelled `cue_only_upper_bound` in the report rather than being mixed in silently.

Two consequences worth keeping:
- **The dev selection moved** from pad 0.8 s to pad 0.3 s once the denominator became the detector's non-voice time, because padding cues now eats into a much smaller narratable base. The pad is not robust across denominators, so any pad quoted without its scoring mode is meaningless.
- **The unseen film is still the outlier:** three of four real films land at 2.3-3.1%, under the 5% floor, while Sprite Fright is **8.5%** on the shipped config (10.3% on the tuned one) — about 1.7x the floor — and only 289 s of its 630 s is even narratable to Silero. Derived cues leak about three times as much on unseen material as on the films that informed tuning.

The synthetic scenes report 0% leak but **0 s narratable**: Silero classifies their music, effects and vocalise as speech, so they measure cue coverage but cannot measure retained narration. They are regression fixtures only.

- **The generalization gap is the finding.** The dev-chosen config leaks 3.3% on the film that chose it and **14.0% on Sprite Fright**, a real film (CC BY 4.0, 80 timed cues from Wikimedia Commons) used in no earlier detector, cue, pad or threshold work here. The shipped derived-cue path therefore does **not** meet the 5% floor on unseen real material, and the honest working figure is "roughly one in seven retained narration seconds still lands on dialogue", not the 2-4% previously quoted from three tuned films. Missed shouted lines are the mechanism: 13 of 34 shouts on Sprite Fright, 11 of 28 on Tears of Steel, all 4 on Sintel.
- **Exact subtitles remain the reliable path:** 0% leak and full recall on every film where they exist, at the cost of 17-29% of narratable time when padded.
- **The synthetic scenes were misleadingly easy:** 0% leak on both, as predicted in #426. They stay in the sweep as regression fixtures and are not evidence of generalization.
- **Dependency trap:** `faster-whisper` 1.2.1 calls `av.open(..., metadata_errors=...)`, which PyAV 15+ removed. `pip install faster-whisper` alone installs av 19 and every transcription fails at decode time, so the opt-in job pins `av==13.1.0`.
- **Why shouts were missed (2026-11-02):** the Silero evidence gate was **duration-blind** — it dropped any cue whose peak speech probability stayed under the floor, whether that cue was a 20 s hallucinated musical passage or a 1.5 s "Cut!" over an explosion. Replaying the gate over cached per-cue evidence (`scripts/research/cue_evidence.py`) isolated it: on Sprite Fright, gating every cue missed 13 of 34 shouted lines; not gating at all missed 3. Whisper *did* transcribe those shouts; the gate discarded them afterwards.
- **Fix:** the gate now applies only to cues longer than 10 s (`--gate-max-cue-seconds`, policy in `scripts/cue_gate.py`, tested by `scripts/test_cue_gate.py` in PR CI). Long passages are the hallucination case the gate was built for; a short false cue costs at most its own duration, while dropping one can hide speech. Verified on the Tears of Steel shout cluster: the old policy kept 10 of 16 cues, the new one keeps 16, and the recovered spans land on "Abort!" (371.6-372.3), "Cut!" (373.4-373.8) and "Whoaaa!" (374.2-374.4).
- **Leak and coverage are coupled:** lowering the floor keeps more cues, which vetoes more narration: the leak *ratio* falls partly because less is narrated. Reporting a leak number without retained narratable time is therefore misleading, and the earlier "2-4%" figures did exactly that.
- **"Narrate only where detector and cues agree on non-speech" is already the shipped rule (verified 2026-11-02).** `subtract_cues` in `crates/movie-radio-goap/src/gaps/split.rs` takes `gaps` (detector non-voice) and removes padded cue spans, so narration already requires *both* signals to agree. The 2026-10-06 note above said the same about the union of Silero and Whisper. It is worth re-verifying before proposing it: the idea reads as an unexplored lever but is the current design, and the residual 8.5% is dialogue that both signals missed.
- **A second, more sensitive Silero pass does not help either.** Re-running the same model at probability 0.1 (via `SILERO_VAD_THRESHOLD`; the engine ignores `--threshold`, which is an energy-RMS knob) and vetoing with its extra speech spans moves Sprite Fright 8.5% -> 8.1% and Tears of Steel 2.5% -> 2.3%, costing 6.2 s and 0.9 s of narration respectively. `scripts/research/double_veto.py` reproduces both rows. Same model, so it cannot recover speech it never heard — consistent with the earlier 36-setting sweep.
- **The pad curve reaches the floor, but only by narrating a third less (2026-11-02, `scripts/research/pad_curve.py`).** Leak / retained, shipped path, as the derived-cue pad grows:

| pad | Elephants Dream | Tears of Steel | Sintel | Sprite Fright |
|---|---|---|---|---|
| 0.8 s (shipped) | 2.3 / 67% | 2.5 / 76% | 3.1 / 74% | **8.5 / 41%** |
| 1.6 s | 1.9 / 62% | 2.6 / 70% | 2.3 / 69% | 7.0 / 36% |
| 2.5 s | 2.1 / 57% | 2.5 / 64% | 1.6 / 65% | 5.9 / 32% |
| 4.0 s | 1.7 / 48% | 2.4 / 55% | 1.4 / 57% | **4.3 / 26%** |

  A 4 s pad does bring the unseen film under 5%, and barely disturbs the other three. But it costs 15 pp of coverage on Sprite Fright (41% -> 26%) to remove 4.2 pp of leak, and **the only reason to pick 4 s is that it is the smallest pad that passes on the unseen film** — that is fitting on the holdout, which this issue forbids. Recording it as an available lever with its price, not as a recommendation.
- **Gap-filling does not beat uniform padding.** Merging cues separated by up to 3 s (to cover a missed line sitting between transcribed ones) then padding only 0.3 s outside leaks 2.9-8.7% at 42-80% coverage — worse than the uniform 2.0 s pad on every film at comparable coverage. The missed shouts are not simply sandwiched between transcribed lines, so the mechanism the idea assumed does not hold.
- **What that leaves:** every dependency-free lever is now measured and exhausted (operating point, second model pass, agreement rule, integer pad, gap-filling). Closing the gap needs a genuinely independent signal — a different ASR model, or a dedicated dialogue/music separator. Recorded as High in `plans/FOLLOWUPS.md`.

### Sintel as second sweep holdout (2026-10-08)
- Manifest tier C now has two entries (Tears of Steel, Sintel); `fetch_test_assets.sh` (with `FETCH_SECOND_FILM=1`) downloads Sintel's 681 MB zip, extracts the MKV and its English subtitle track (`-map 0:s:m:language:eng`, 26 cues, asserted). Only the weekly sweep pays the download.
- Seeded floors, Sintel: non-voice P/R 0.963/0.941, speech P/R 0.494/0.615, overlap 0.952. A hand-degraded Sintel speech recall turns the ratchet red.
- `check_radio_play_readiness.py --update-floors` now seeds floors for entries that have none (it used to refuse, which made adding a film impossible without hand-editing).
- Sweep not yet run in CI (weekly/dispatch only); the local run is `run_validation_manifest.py` over the 5 entries.

## Measured loudness trend in narration (2026-10-08)
- Each narration window now carries `GapTrend::{Rising,Falling}` measured from its own samples (`gaps/trend.rs`: RMS of the first vs last third, >= 4 dB change, span >= 3 s, audible), and `narrate` adds one clause for it ("Der Klang schwillt an." / "The sound fades away.") after the tag clause, only when the word budget allows. No model, no key, deterministic, grounded in the audio (ADR-128).
- Dry run (distinct lines / lines per film, subtitles supplied): Elephants Dream 13 / 14, Tears of Steel 20 / 37, Sintel 26 / 41; most repeated line ~4 of 37 on Tears of Steel (was 11 of 47 before variants+trend). 43-51% of windows get a trend clause.
- Still tag/level-only: it cannot say what happens, who is speaking or where. That needs a language model or visual input.

## Status 2026-10-10: issue validation, PR triage, profile sweep
**Open issues vs what is on `main`** (#374, #376, #377, #379 merged):

| Issue | Verdict | Evidence |
|---|---|---|
| #362 second detection axis | **Delivered, close** | F0 harmonicity measured and rejected (Youden J ~0.13); Silero added instead. Against timed subtitles it improves non-voice recall *and* speech recall together (Elephants Dream speech R 0.52 -> 0.75, non-voice R 0.61 -> 0.89), i.e. a dominating, independent axis; replicated on Tears of Steel and Sintel (energy finds no speech there at all). |
| #363 unreachable 0.95 gate | **Delivered, close** | Per-entry ratchet (`--floors`) is the blocking step of the weekly sweep; the 0.95 report is non-blocking. Local dry run of every sweep step passes; a hand-degraded metric turns the ratchet red. |
| #364 second validation film | **Delivered, close** (see caveat) | Tears of Steel + Sintel with pinned/asserted truth in the manifest; coverage check passes. Step 3 now done (below). Caveat: one dev film, two holdouts (the issue asked for two + two), and the weekly CI sweep has not run on the new manifest yet (dispatch is not permitted from this workspace; first scheduled run Mon 2026-10-12). Reopen if it fails. |

**#364 step 3 — 16-candidate sweep on the three-film corpus** (`scripts/research/profile_sweep.py`; score = harmonic mean of speech-time recall, non-voice precision, non-voice recall; dev = Elephants Dream, holdout = mean of Tears of Steel and Sintel): the dev winner is also the holdout winner (rank 1/16), so the old dev-to-holdout reversal does **not** reproduce, but only because the grid is degenerate. The energy threshold is inert from 0.0005 to 0.4 on real film audio and every energy candidate calls (almost) everything a gap (speech recall <= 0.02, scores 0.000-0.026). The shipped detector scores **0.844 dev / 0.802 holdout** (gap 0.04, vs ~0.15 reported for the old profile). Engine choice, not knob tuning, is what matters; fitted energy profiles are retired in `FOLLOWUPS.md`.

**Open PRs**
- #378 (`..` check on reference paths): **closed, no impact.** The producer (`voice_clone.rs`) already rejects `..`; the consumer check is bypassed by absolute paths/symlinks; it deleted ~20 lines of rationale comments. Useful form (canonical allowed root) needs a stated threat first.
- #373 (llama-cpp-2 0.1.158): **superseded** by the migration branch (Orpheus moved to the `vocab()` API; lockfile identical to Dependabot's 4/-5). `clippy` and `test` with `--features local-tts` pass locally against 0.1.158.
- Merge order: the migration PR is the only remaining change; no other PR depends on it. It must land after #376 (already merged) and needs a human merge (the workspace refuses `gh pr merge`).

### Weekly sweep replayed from a fresh clone of `main` (2026-10-10, da8d423)
- Because workflow dispatch is not permitted from the agent workspace, every `run:` step of `validation-sweep.yml` was replayed in order from a fresh `git clone` (downloads included: Elephants Dream, Tears of Steel 372 MB, Sintel 681 MB zip, the pinned Silero model): all 17 steps pass, including the Sintel extraction and the 26-cue assertion, the 5-entry manifest, the coverage check, the failure breakdown, the (non-blocking) 0.95 report and the blocking ratchet. The fresh metrics are identical to the committed ones, so the sweep is deterministic and everything it needs is in git. The Monday 2026-10-12 scheduled run remains the real confirmation.
- The committed derived reports under `analysis/` were stale (readiness report showed 0 holdout entries); refreshed from that run.
- Where the 0.95 aspiration stands (pooled over the two real holdouts, `analysis/learnings/latest-radio-play-readiness-report.md`): non-voice precision 0.941 (LB95 0.935), recall 0.915 (LB95 0.908), overlap 0.928 (LB95 0.921). Tears of Steel is the limiter (non-voice P/R 0.918/0.889); Sintel is 0.963/0.941. Still short of 0.95 on both, so the gate stays aspirational.

## Detector limits on Tears of Steel, and a config trap (2026-10-10)
- **Where the false gaps come from.** Of Tears of Steel's 10 shipped gaps, 8 hold 0.3-12.5% dialogue; the two bad ones (226-238 s: 49%, 368-384 s: 45%) are shouted lines over loud action ("Thom.", "Abort!", "Cut!", "Nooooo!"). Silero gives many of those cues a peak probability of 0.00-0.15, so the detector never sees them; others reach 0.4-0.9 but only for a few frames. 43 s of dialogue sits inside predicted gaps.
- **Pipeline settings cannot recover them.** 36-point sweep (`scripts/research/silero_pipeline_sweep.py`: Silero threshold 0.15/0.3/0.5, hangover 150/300/600 ms, merge gap 250/800 ms, min speech 60/120 ms) on all three films: holdout score spans 0.784-0.804; the shipped default is 0.802 (best 0.804, dev 0.844 vs best 0.852). Dev winner = holdout winner (rank 1/36). Detector tuning is exhausted; the gap to 0.95 on Tears of Steel is audio the model cannot hear. The route that does catch these lines is the cue path (whisper without VAD pre-filter, see above), which the weekly gate does not measure (it needs whisper in CI, ~3 min per film).
- **Config trap fixed.** `extract`, `bench` and `validate` defaulted `--vad-engine` to `energy` and always applied it, silently overriding a config file's own `vad_engine` (`modern-optimized` and `legacy-optimized` say `spectral` but ran `energy`). The flag is now optional: an explicit flag wins, otherwise the config's engine applies, otherwise `energy`. Tests cover all three cases; the 5-entry sweep is unchanged (every entry passes its engine explicitly or uses an energy config), so the committed floors still hold. This also means earlier numbers produced with `--config` profiles ran the energy engine.

## #419 holdout breadth and diagnosability (2026-11-03)
- **Corpus.** Tier C now has three modern holdouts (ToS en/de, Sintel en) and two legacy holdouts (Plan 9 1959 en, Charade 1963 fr). Legacy films are public-domain Wikimedia Commons copies; their timed text is CC BY-SA. `fetch_test_assets.sh` asserts 680 and 1363 cues so a truncated subtitle file fails loudly. Provenance and policy live in `plans/040-validation/HOLDOUT-CORPUS.md`.
- **Coverage checks.** `check_validation_coverage.py` now enforces two entries per modern/legacy cohort, at least two spoken languages, holdout/tuning separation and detector-vs-cue separation in the readiness report.
- **Measured (8-entry sweep, 2026-11-03).** Modern detector-only overlap 0.9195 (precision 0.9330, recall 0.9064); legacy 0.8030 (precision 0.8944, recall 0.7286). Sintel is the strongest film (0.9521); Charade is the limiter (0.7983). The 0.95 target still fails and stays non-blocking; the per-entry ratchet floors are seeded for the new entries and pass.
- **Diagnosis.** `export_radio_play_window_diagnosis.py` exports contiguous FP/FN windows with time ranges, predicted tags and truth labels; checked-in output for all five failing holdouts is under `analysis/validation/window-diagnosis/`. Legacy recall failures dominate (Plan 9 ~608 s FN, Charade ~808 s FN).
- **Trend.** `build_radio_play_trend.py` appends a weekly point to `analysis/validation/radio-play-trend.json` and records pending assets and unavailable optional dependencies.
