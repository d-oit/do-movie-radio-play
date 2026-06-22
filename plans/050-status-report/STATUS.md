# Implementation Status Report

**Date:** 2026-06-22

## Phase Status Summary

| Phase | Description | Status | Notes |
|-------|------------|--------|-------|
| 01 | JSON-only pipeline | COMPLETE | Deterministic extract pipeline verified |
| 02 | Acoustic tags | COMPLETE | Rule-based tags with spectral features |
| 03 | Prompt generation | COMPLETE | Config passthrough and tag mappings are wired |
| 04 | Self-learning | COMPLETE | Calibration, adaptive thresholds, learning DB, gap store, threshold store |
| 05 | Hardening and quality | COMPLETE | Validation/config, WAV fallback, VAD fail-fast, benchmark CI regression |
| 06 | New capabilities | IN PROGRESS | Workspace restructure complete; radio-play pipeline partially implemented |

## Workspace Restructure (2026-06-22)

Major workspace restructure extracted monolithic `src/` into 9 focused crates:

| Crate | LOC | Purpose | Status |
|-------|-----|---------|--------|
| `movie-radio-types` | 392 | Shared types (Frame, Segment, Metrics, Emotion, AudioOutput, config) | Complete |
| `movie-radio-pipeline` | 3,245 | VAD, framing, segmentation, features, tags, prompts, decode | Complete |
| `movie-radio-learning` | 1,605 | Calibration, adaptive thresholds, libsql database, profiles | Complete |
| `movie-radio-verification` | 1,356 | Spectral verification, fingerprinting, segment extraction | Complete |
| `movie-radio-validation` | 745 | Validation, comparison, SRT parsing, synthetic fixtures | Complete |
| `movie-radio-voice` | 624 | TTS providers (Kokoro, PocketTts, Qwen3, Orpheus, ElevenLabs) | Structurally complete; most providers return silence |
| `movie-radio-io` | 309 | JSON, EDL, VTT, WAV I/O utilities | Complete |
| `movie-radio-goap` | 1,275 | GOAP planner, orchestrator, actions, gaps, narrate, assemble | All modules implemented; orchestrator doesn't execute real work |
| `movie-radio-timeline` | 2,241 | CLI binary with 16 subcommands, handlers, config | Complete |

**Total:** ~11,800 LOC across 76 Rust source files.

## CLI Commands (16 subcommands)

| Command | Purpose | Status |
|---------|---------|--------|
| `extract` | Run VAD pipeline, produce TimelineOutput JSON | Complete |
| `tag` | Add audio-feature-based tags to timeline | Complete |
| `prompt` | Generate AI narration prompts from tagged timeline | Complete |
| `review` | Generate interactive HTML review player | Complete |
| `calibrate` | Run calibration from corrections directory | Complete |
| `apply-calibration` | Apply saved calibration report to active profile | Complete |
| `bench` | Benchmark pipeline on a file | Complete |
| `gen-fixtures` | Generate synthetic WAV test fixtures | Complete |
| `validate` (alias: `eval`) | Evaluate pipeline accuracy against ground truth | Complete |
| `ai-voice-extract` | Extract speech segments into AiVoiceOutput | Complete |
| `verify-timeline` | Spectral verification of non-voice segments | Complete |
| `update-thresholds` | Generate threshold recommendations from learning state | Complete |
| `learning-stats` | Print statistics from learning database | Complete |
| `merge-timeline` | Merge contiguous non-voice segments | Complete |
| `export` | Export timeline to JSON, EDL, or VTT format | Complete |
| `radio-play` | Gap analysis for radio play production | Analyze-only mode works; full pipeline not wired |

## Voice Synthesis Providers

| Provider | File | Real Logic | Synthesis Output | Notes |
|----------|------|------------|------------------|-------|
| **Modal** | `src/voice/modal.rs` | HTTP POST + PCM WAV decode | Real audio | PR #110; free-tier serverless GPU |
| **ElevenLabs** | `src/voice/elevenlabs.rs` | HTTP POST, API key auth | Mock audio (no MP3 decode) | Real API calls, needs MP3 decoder |
| **Kokoro** | `src/voice/kokoro.rs` | ONNX model download + session load | Silence (no inference) | Infrastructure ready |
| **Orpheus** | `src/voice/orpheus.rs` | Emotion tag wrapping | Silence | Stub |
| **Qwen3** | `src/voice/qwen3.rs` | German emotion prompts | Silence | Stub |
| **PocketTts** | `src/voice/pockettts.rs` | None | Silence | Fully stubbed |

**Fallback chain:** `SynthesisOrchestrator` in `src/voice/mod.rs` iterates configured provider list, tries each in order, falls through on failure. Monthly spend tracking via `LearningDb`.

## GOAP Pipeline

| Component | File | Status |
|-----------|------|--------|
| A* Planner | `movie-radio-goap/src/planner.rs` | Fully implemented with tests |
| World State | `movie-radio-goap/src/lib.rs` | 11-field boolean state, `meets(goal)` |
| Actions | `movie-radio-goap/src/actions.rs` | 8 actions with preconditions/effects/costs |
| Orchestrator | `movie-radio-goap/src/orchestrator.rs` | Structural loop works; doesn't execute real work |
| Gap Identifier | `movie-radio-goap/src/gaps.rs` | 5-signal scoring, fully implemented |
| Narration Generator | `movie-radio-goap/src/narrate.rs` | Template-based German text, fully implemented |
| Audio Assembler | `movie-radio-goap/src/assemble.rs` | Crossfade + ducking, fully implemented |

## Pipeline Stages (execution order)

1. **Decode** — Symphonia native + ffmpeg fallback
2. **Resample** — Linear interpolation (rubato behind feature flag)
3. **Framing** — 20ms windows, parallel feature extraction
4. **Feature Extraction** — FFT-based 8 spectral features
5. **VAD** — Energy / Spectral / Hybrid engines
6. **Tri-State Smoothing** — Speech/MusicLike/NoiseLike classification
7. **Speech Segmentation** — Hangover smoothing, merge, prune
8. **Speech Evidence Filter** — Remove implausible speech segments
9. **Invert to Non-Voice** — Complement computation
10. **Bridge Non-Voice** — Merge segments separated by short speech
11. **Non-Voice Merge Policy** — All/Longest/Sparse strategies
12. **Expand Non-Voice** — Extend into ambiguous frames
13. **Split Long Segments** — Cap non-voice duration
14. **Verification Filter** — Spectral verification (sparse profiles)
15. **Bridge Residual Gaps** — Final merge pass
16. **Tail Recovery** — Extend terminal non-voice segment

## Learning System

| Component | File | Status |
|-----------|------|--------|
| Adaptive Thresholds | `movie-radio-learning/src/adaptive_thresholds.rs` | FP rate tracking, auto-adjustment |
| Calibration | `movie-radio-learning/src/calibrator.rs` | Correction-driven threshold delta |
| Database | `movie-radio-learning/src/database.rs` | libsql SQLite, verified_segments, fingerprints |
| Profiles | `movie-radio-learning/src/profiles.rs` | Action/Documentary/Animation/Drama profiles |
| Gap Store | `movie-radio-learning/src/gap_store.rs` | Gap decision persistence |
| Threshold Store | `movie-radio-learning/src/threshold_store.rs` | Threshold recommendations + history |

## Verification System

| Component | File | Status |
|-----------|------|--------|
| Spectral Analysis | `movie-radio-verification/src/verification/analysis.rs` | FFT-based 8 features, thread-local cache |
| Fingerprinting | `movie-radio-verification/src/verification/fingerprint.rs` | Wang-style combinatorial hashing |
| Verification Engine | `movie-radio-verification/src/verification/mod.rs` | Voice/nonvoice scoring, graph structure signal |
| Segment Extractor | `movie-radio-verification/src/verification/extractor.rs` | ffmpeg-based segment extraction |

## Current Missing Implementations

| Gap | Severity | Location | Notes |
|-----|----------|----------|-------|
| Voice providers return silence | High | `movie-radio-voice/src/voice/` | Only Modal + ElevenLabs make real calls |
| GOAP orchestrator doesn't execute | Medium | `movie-radio-goap/src/orchestrator.rs` | Simulates state transitions only |
| Radio-play CLI not wired | Medium | `src/handlers/radio_play.rs` | analyze-only mode works; full pipeline stub |
| OpenAI TTS provider missing | Low | N/A | Not implemented at all |
| MP3 decode for ElevenLabs | Low | `src/voice/elevenlabs.rs` | HTTP works, response not decoded |

## Quality Issues

No active hardening gaps beyond the voice synthesis stubs above.

Dependency security (GitHub Dependabot):
- HIGH: GHSA-82j2-j2ch-gfr8 in rustls-webpki — resolved via PR #61
- Remaining moderate/low advisories accepted

## Open GitHub Issues

| # | Title | Status | Notes |
|---|-------|--------|-------|
| 97 | German narration text generator | **DONE** | Implemented in `movie-radio-goap/src/narrate.rs` |
| 96 | End-to-end radio-play CLI | **PARTIAL** | `radio-play` subcommand exists; only analyze-only mode |
| 95 | Autonomous self-learning system | **MOSTLY DONE** | Learning crate has adaptive thresholds, calibration, database |
| 94 | Radio play assembly | **DONE** | Implemented in `movie-radio-goap/src/assemble.rs` |
| 93 | Provider fallback chain | **DONE** | Implemented in `src/voice/mod.rs` SynthesisOrchestrator |
| 92 | ElevenLabs and OpenAI TTS | **PARTIAL** | ElevenLabs HTTP works; OpenAI not implemented |
| 91 | Orpheus-3B TTS provider | **PARTIAL** | Struct exists, inference stubbed |
| 110 | Modal.com TTS provider | **DONE** | PR #110 merged; real HTTP + PCM decode |

## Recent Changes

### Workspace Restructure (2026-06-22)
- 128 files changed, 14,890 insertions
- Extracted 9 crates from monolithic src/
- Added GOAP pipeline crate with planner, gaps, narrate, assemble
- Added learning crate with full calibration/threshold/database stack
- Added verification crate with spectral analysis + fingerprinting
- Added validation crate with comparison, SRT, synthetic fixtures
- Added voice crate with 5 providers + fallback orchestrator
- Added Modal.com TTS provider (PR #110)
- Added 16 CLI subcommands
- Benchmarks in dedicated crate

### PR #110 — Modal.com TTS Provider (2026-06-22)
- Added `src/voice/modal.rs` with real HTTP POST to Modal endpoint
- PCM WAV decoding (skip 44-byte header, i16→f32)
- Cost tracking via `LearningDb.provider_usage` table
- Deployment scripts: `scripts/modal_tts_deploy.py`, `scripts/modal_tts_piper.py`
- Resolved merge conflicts with main's store-module refactoring
- Fixed Codacy unused import warnings

## Open Action Items

1. **Wire GOAP orchestrator to real pipeline stages** — orchestrator currently simulates state transitions
2. **Implement TTS inference for local providers** — Kokoro/Orpheus/Qwen3 need actual neural inference
3. **Add MP3 decode for ElevenLabs** — HTTP works but response not decoded
4. **Wire radio-play CLI to full pipeline** — currently only analyze-only mode
5. **Add OpenAI TTS provider** — not implemented at all
6. **#76 AGENTS.md gaps** — Deferred by design
