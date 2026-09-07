# Codebase Gap & New-Feature Analysis

**Status:** Draft — audit report, no code changes
**Date:** 2026-09 (analysis pass)
**Method:** Static source inspection + workspace-wide greps, cross-referenced against two parallel doc audits (all `plans/` files; ADRs/roadmaps). No build or tests were run during the analysis, so findings are code-level, not compiler/test verified.

## 1. Summary

The workspace is functionally rich and clean by marker hygiene (zero `TODO`/`FIXME`/`todo!()`/`unimplemented!()` in `crates/*`, `plans/FOLLOWUPS.md` open list empty). The risk is therefore **silent incompleteness**: no-op actions that return `Ok`, CLI flags that are accepted and ignored, and fully-built subsystems that no caller uses. This doc itemizes the confirmed gaps and the feature opportunities that build on existing assets.

**Doc-staleness caveat:** several planning docs (`plans/120-goap-radio-play-pipeline/ROADMAP.md` 2026-06-22, `plans/050-status-report/STATUS.md` 2026-06-22) lag the source (e.g. they claim "voice providers return silence", "ElevenLabs MP3 not decoded", "output not wired to CLI" — all since fixed). Only findings confirmed in current source are listed below; doc-only claims are flagged. `plans/GOAP_STATE.md` (2026-09-03) shows an in-flight unified-orchestrator goal (branch `feat/goap-unified-orchestrator`, PR #246, T7/T8 open) that may already address item A1 when merged.

## 2. Missing / incomplete implementations (ranked)

### A1. GOAP quality/learning actions are placeholders; GOAP engine unused by the CLI
- `crates/movie-radio-goap/src/actions.rs:353` `VerifyQuality` and `:383` `ApplyLearnings` only log "(placeholder)" and return `Ok(())`. They never invoke `movie-radio-verification` or `movie-radio-learning` (those crates ARE used by the timeline `validate`/`review`/`extract` handlers), so the goal flags `quality_verified`/`learnings_applied` are asserted without work: no verification report, no calibration/adaptive-threshold update, no execution trace.
- `crates/movie-radio-goap/src/orchestrator.rs:58` `should_replan()` hardcodes `false` — ADR-120 replan triggers (action failure, resource change, quality below threshold) do not exist.
- The CLI never uses the planner/orchestrator: `crates/movie-radio-timeline/src/handlers/radio_play.rs` runs a hard-coded linear pipeline and imports only goap `assemble`/`gaps`/`narrate`.
- Watch: may be superseded by the unmerged PR #246 branch per `plans/GOAP_STATE.md`.

### A2. Voice cloning (ADR-0125, accepted #239) is a dry-run facade
- `crates/movie-radio-pipeline/src/voice_clone.rs:4` `extract_candidates()` fabricates one synthetic `VoiceReference` pointing at the whole input file; no speaker/dialogue candidate extraction occurs.
- `crates/movie-radio-timeline/src/voice_clone_handler.rs` — `voice samples` prints "dry-run…", `voice list` prints "(none stored yet)"; both exit 0 with plausible output. Nothing persists references or passes `reference_audio` into the audio.cpp runtime.

### A3. `narrate` CLI never calls any LLM backend
- `crates/movie-radio-pipeline/src/narrator.rs:224` — `handle_narrate()` renders the Tera template and prints; comment: "Real generation would dispatch to backend based on cfg.backend". `OpenAiNarrator`, `OllamaLocalNarrator`, `AnthropicNarrator` (ADR-0126) are implemented but unreachable from the CLI. Uses hard-coded example scene data.

### A4. `produce` orchestrator executes no stage
- `crates/movie-radio-pipeline/src/orchestrator.rs` — 12 stages defined (`SceneDetect`, `Transcribe`, `CharacterAssign`, `VoiceSynthesize`, `SfxFetch`, `AudioMix`, …) but the non-dry-run path only writes empty checkpoint JSONs and prints "produce complete"; `--resume` is ignored. `md5_hash`/`md5_hash_str` (`:134-144`) implement a rolling hash, not MD5.

### A5. TTS provider gaps (Milestone F)
| Provider | Current state in source |
|---|---|
| PocketTts | Silence stub: returns 1 s of zeros (`crates/movie-radio-voice/src/voice/pockettts.rs:19`) while `capabilities()` advertises `supports_voice_cloning: true`, `supports_streaming: true`; `plans/130` (P1) and `GAPS.md` recommend removal. |
| Orpheus | Token inference real; SNAC→PCM decode falls back to synthetic tones (`crates/movie-radio-voice/src/voice/orpheus.rs:95-104`). |
| Kokoro | ONNX inference live; `text_to_tokens()` maps chars to raw codepoints instead of eSD phoneme vocab (`crates/movie-radio-voice/src/voice/kokoro.rs:128-131`) → acoustic output unverified. |
| GOAP `SynthesizeNarrator` | Hard-codes Modal + `language: "de"` + env key (`crates/movie-radio-goap/src/actions.rs:191-212`), ignoring `ctx.config` and the voice crate's fallback `SynthesisOrchestrator` used by the CLI handler. |
| ElevenLabs / OpenAI / Modal / audio_cpp | Real audio paths (incl. symphonia MP3 decode). |

### A6. SFX triggers are produced but never consumed; render crate is orphaned
- `autofill_silent_scene_sfx` fills `SfxTrigger` into segments (`crates/movie-radio-pipeline/src/pipeline/sfx_autofill.rs`); JSON/EDL/VTT serialize them.
- The intended consumer `movie-radio-render::sfx` (`SfxManager`, freesound/local/ai_generate) is referenced by **no** crate — only `benchmarks` depends on `movie-radio-render`, and `movie-radio-timeline` declares it without using it. Mixer/AGC/spatial/reverb all ship unused outside benchmarks.
- Assembly (`crates/movie-radio-goap/src/assemble.rs`, `crates/movie-radio-timeline/src/handlers/radio_play.rs`) inserts narration with hard-coded ducking `(50 ms, 0.3)` and hand-rolled WAV write + ffmpeg; no SFX, AGC, or master-processing layer.

### A7. Accepted-but-ignored CLI flags
- `crates/movie-radio-timeline/src/handlers/preview.rs:14-19` — `--skip` / `--duration` log "not yet implemented", play from start / full file.

### A8. Silero VAD (deliberate, documented deferral)
- `crates/movie-radio-pipeline/src/pipeline/vad/mod.rs:78` — `silero` name accepted by CLI and `create_engine`, errors "not yet implemented (blocked on ort unification, see ADR-127)". Intentional per Milestone-C decision; WebRTC behind `webrtc-vad` is implemented.

### A9. Review player (doc-reported, current paths unverified)
- From `plans/070-review-player-testing/UNRESOLVED-ISSUES.md`: merged/individual view state not persisted in saved HTML; timeline markers not draggable. Other findings later fixed per `plans/080-learning-integration/ISSUES.md`.

### A10. Infra / tooling
- `high-quality-resample` feature declared empty, `rubato` unconditional, no CI leg exercising it (`crates/movie-radio-pipeline/Cargo.toml:20,32`; `plans/130` §B3).
- Voice crate native deps (llama-cpp-2, candle-core, qwen_tts, ort) unconditional; `ort = "2.0.0-rc.9"` pre-release pin (proposal in `plans/130` §B1: per-provider or `local-tts` feature gating).
- `scripts/fetch_test_assets.sh:19` validates a file it never fetches → always fails; `scripts/pre-commit-hook.sh` `set -e` only (no `-u`/`pipefail`) and duplicates `quality_gate.sh`; `.agents/skills/docs-hook/scripts/docs-sync.sh` broken path calc + nesting + not actually hooked; stale skill references to non-existent `agents-docs/` (atomic-commit, learn).
- CI: no cargo cache, redundant quality-gate step, no coverage upload (`.codecov.yml` present), no tag/release workflow, MSRV script (`scripts/audit-msrv.sh`) orphaned, Linux-only.
- Coverage: `movie-radio-types` (~404 LOC of shared domain types) has no unit tests; EDL/SRT/VTT parsers have one test each; no per-crate integration `tests/` dirs.
- Roadmap-H gaps: cross-movie similarity (do-memory-core / CSM not integrated), genre-aware adaptation, narration-quality execution traces.

## 3. New-feature opportunities (grounded in existing assets)

1. **Wire the learning loop into GOAP** — `VerifyQuality` → verification report/fingerprint scoring; `ApplyLearnings` → `apply_calibration_report` + adaptive thresholds + libsql run trace (ADR-122); enable real replan in `should_replan()` on low quality. Crates exist; only the GOAP path fails to call them.
2. **End-to-end SFX and master render** — consume `SfxTrigger` via `SfxManager` (freesound/local/ai_generate already implemented) and mix final output through render mixer/AGC; activates an entire unused crate (with existing tests/benchmarks).
3. **Real voice-clone extraction (ADR-0125 scope)** — deterministic VAD/diarization-based candidate extraction with persisted, user-reviewable `VoiceReference` store, then pass `reference_audio` to audio.cpp `qwen3_tts`.
4. **Make `narrate`/`produce` honest** — dispatch `handle_narrate` to `cfg.backend`; execute or fail the `produce` stages instead of writing empty checkpoints; honor `--resume`.
5. **Voice consistency & acceptance (Milestone F)** — real Kokoro phonemizer, real Orpheus SNAC vocoder, PocketTts removal, provider-switch consistency checks.
6. **Memory & timing engineering (perf, PHASE-06 §6.4)** — `radio_play` decodes the full movie up to three times (`handlers/radio_play.rs` ~98/112/199); `decode_audio_chunked` accumulates anyway; chunked processing + rubato time-stretch for tight narration gaps (open Milestone-G item) targets 2 h films (~460 MB f32 copies today).
7. **Accuracy roadmap (strategic, TRIZ-001)** — multi-feature VAD tuning using already-computed ZCR/flux/centroid/band ratios: documented engine-level path toward the 0.95 modern-corpus gate (current ceiling ~0.7368); WAV 24/32-bit direct decode (§6.5); Silero behind a real feature after ort unification.
8. **DX/CI quick wins** — `models download/list/verify` CLI (ADR-121 promised); cargo feature gating for local TTS; all-features CI leg; coverage upload; benchmark-regression scheduling; fix asset script + pre-commit hook; honor `preview --skip/--duration`.

## 4. Suggested next actions (repo Standard Workflow Loop)

Per AGENTS.md, file issues before editing and keep atomic commits + zero-warning quality gate:
- 🛠️ Coding Change: A1 (GOAP verify/learn + replan), A2 (voice-clone extraction), A3 (narrate dispatch), A4 (produce honesty), A7 (preview flags).
- ⚡ Performance Change: B6 memory/chunking; local-TTS feature gating; `high-quality-resample` wiring + CI leg.
- 🛠️ Coding Change (small): A5 PocketTts removal; A10 script fixes; B8 CI/coverage.
- 🤖 Agent/Harness Change: stale skill references; `agents-docs` pointers; docs-sync hook.
- Update `plans/FOLLOWUPS.md` once items are resolved; re-verify doc-vs-code staleness of `plans/120-goap-radio-play-pipeline/ROADMAP.md` and `plans/050-status-report/STATUS.md` after PR #246 lands.

## 5. Follow-up tracking (2026-09-07)

**Implemented in the first recommendations PR** (branch `work/plans-recommendations`):
- Preview `--skip` / `--duration` now slice playback (`crates/movie-radio-io/src/preview.rs` window helpers + timeline handler) — §A7.
- `narrate` dispatches to the configured LLM backend (`openai`/`ollama`/`anthropic`) instead of silently printing — §A3.
- `produce` fails loudly (planning-only) instead of writing empty checkpoints and claiming success — §A4.
- PocketTts silence stub removed from `movie-radio-voice` + legacy `movie-radio-types` config mirrors and the timeline handler — §A5.
- `high-quality-resample` feature now really pulls in `rubato` (optional dep) — §A10.
- `plans/050-status-report/GAPS.md` and `plans/060-next-features/PHASE-06-new-capabilities.md` refreshed for the resolved items (incl. WebRTC VAD shipped, WAV 24/32-bit decode already on main).

**Remaining follow-ups filed as GitHub issues #261–#285** (GOAP verify/learn + planner wiring, voice-clone extraction, Kokoro/Orpheus provider completion, SFX end-to-end, produce executors, review-player UX, eval manifest, Silero, learning-loop H, chunked processing, time-stretch, feature gating, CI legs, agent/harness hygiene, doc staleness).

## 6. Sources

- Code evidence: paths cited per item above.
- Plans: `plans/GOAP_STATE.md`, `plans/FOLLOWUPS.md`, `plans/050-status-report/GAPS.md`, `plans/050-status-report/STATUS.md`, `plans/050-status-report/INFRA-OPTIMIZATION.md`, `plans/060-next-features/PHASE-05-hardening.md`, `plans/060-next-features/PHASE-06-new-capabilities.md`, `plans/060-next-features/FEATURE-profile-driven-tag-calibration.md`, `plans/130-improvement-analysis-2026-08-25.md`, `plans/100-radio-play-95/ROADMAP.md`, `plans/100-radio-play-95/MILESTONE-C-DECISION.md`, `plans/120-goap-radio-play-pipeline/ROADMAP.md`, ADRs 120-127, `plans/070-review-player-testing/UNRESOLVED-ISSUES.md`, `plans/040-validation/*`.
