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
