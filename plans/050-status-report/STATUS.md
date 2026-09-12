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

Major workspace restructure extracted monolithic `src/` into 10 focused crates:

| Crate | LOC | Purpose | Status |
|-------|-----|---------|--------|
| `movie-radio-types` | ~450 | Shared types (Frame, Segment, Metrics, Emotion, AudioOutput, config, validation) | Complete |
| `movie-radio-pipeline` | ~3,500 | VAD engines, framing, segmentation, features, tags, prompts, Symphonia decode | Complete |
| `movie-radio-learning` | ~1,650 | Calibration, adaptive thresholds, libsql database, gap store, profiles | Complete |
| `movie-radio-verification` | ~1,400 | Spectral verification, fingerprinting, segment extraction | Complete |
| `movie-radio-validation` | ~800 | Validation, comparison, SRT parsing, synthetic fixtures | Complete |
| `movie-radio-voice` | ~1,200 | TTS providers (audio_cpp, Modal, ElevenLabs, OpenAI, Kokoro, Qwen3, Orpheus) | Complete |
| `movie-radio-io` | ~350 | JSON, EDL, VTT, WAV I/O utilities, review player HTML generation | Complete |
| `movie-radio-goap` | ~1,500 | GOAP planner, orchestrator, actions, gaps, narrate, assemble | Complete |
| `movie-radio-render` | ~1,100 | Spatial audio, AGC, reverb, sound effects (`SfxManager`), mixer | Complete |
| `movie-radio-timeline` | ~2,500 | CLI binary with subcommands, handlers, config | Complete |

**Total:** ~14,500 LOC across workspace member crates.

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
| **audio_cpp** | `crates/movie-radio-voice/src/voice/audio_cpp/` | Local CLI/HTTP & remote HTTPS GPU pools | Real audio | Primary C++ runtime provider |
| **Modal** | `crates/movie-radio-voice/src/voice/modal.rs` | HTTP POST + PCM WAV decode | Real audio | PR #110; free-tier serverless GPU |
| **ElevenLabs** | `crates/movie-radio-voice/src/voice/elevenlabs.rs` | HTTP POST + Symphonia MP3 decode | Real audio | API provider with native MP3 decoding |
| **OpenAI** | `crates/movie-radio-voice/src/voice/openai.rs` | HTTP POST + Symphonia MP3 decode | Real audio | API provider with native MP3 decoding |
| **Kokoro** | `crates/movie-radio-voice/src/voice/kokoro.rs` | ONNX model download + session load | ONNX Session | Infrastructure & ONNX loading ready |
| **Orpheus** | `crates/movie-radio-voice/src/voice/orpheus.rs` | Emotion tag wrapping | Stub | Stub |
| **Qwen3** | `crates/movie-radio-voice/src/voice/qwen3.rs` | German emotion prompts | Stub | Stub |

**Fallback chain:** `SynthesisOrchestrator` in `crates/movie-radio-voice/src/voice/mod.rs` validates requests (`SynthesisRequest::validate`), iterates configured provider list, tries each in order, and falls through on failure. Monthly spend tracking via `LearningDb.provider_usage`.

## GOAP & Unified Orchestrator (#246)

| Component | File | Status |
|-----------|------|--------|
| A* Planner | `crates/movie-radio-goap/src/planner.rs` | Fully implemented with tests |
| World State | `crates/movie-radio-goap/src/lib.rs` | 11-field boolean state, `meets(goal)` |
| Actions | `crates/movie-radio-goap/src/actions/` | 8 GOAP actions with real stage execution |
| Orchestrator | `crates/movie-radio-pipeline/src/orchestrator.rs` | Unified engine executing 12 stages with checkpoint persistence |
| Gap Identifier | `crates/movie-radio-goap/src/gaps.rs` | 5-signal modular scoring, fully implemented |
| Narration Generator | `crates/movie-radio-goap/src/narrate.rs` | Context-aware German description generator |
| Audio Assembler | `crates/movie-radio-goap/src/assemble.rs` | Crossfade + ducking + SFX mixing end-to-end (#287) |

## Pipeline Stages (execution order)

1. **Decode** — Symphonia native (with 16/24-bit PCM & 32-bit float support #250) + ffmpeg fallback
2. **Resample** — Linear interpolation (rubato behind feature flag)
3. **Framing** — 20ms windows, parallel feature extraction
4. **Feature Extraction** — FFT-based 8 spectral features
5. **VAD** — Energy / Spectral / Hybrid / WebRTC (feature `webrtc-vad` #253) / Silero engines
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
| Local neural TTS stubs (Kokoro/Orpheus/Qwen3) | Low | `crates/movie-radio-voice/src/voice/` | Cloud providers (Modal, ElevenLabs, OpenAI) and audio_cpp handle real audio |

## Quality Issues

No active hardening gaps beyond the voice synthesis stubs above.

Dependency security (GitHub Dependabot):
- HIGH: GHSA-82j2-j2ch-gfr8 in rustls-webpki — resolved via PR #61
- Remaining moderate/low advisories accepted

## Open GitHub Issues

| # | Title | Status | Notes |
|---|-------|--------|-------|
| 97 | German narration text generator | **DONE** | Implemented in `movie-radio-goap/src/narrate.rs` |
| 96 | End-to-end radio-play CLI | **DONE** | Full pipeline wired; `--analyze-only` flag preserved |
| 95 | Autonomous self-learning system | **MOSTLY DONE** | Learning crate has adaptive thresholds, calibration, database |
| 94 | Radio play assembly | **DONE** | Implemented in `movie-radio-goap/src/assemble.rs` |
| 93 | Provider fallback chain | **DONE** | Implemented in `movie-radio-voice/src/voice/mod.rs` SynthesisOrchestrator |
| 92 | ElevenLabs and OpenAI TTS | **DONE** | ElevenLabs MP3 decode via symphonia; OpenAI TTS provider added |
| 91 | Orpheus-3B TTS provider | **PARTIAL** | Struct exists, inference stubbed (needs llama-cpp-2) |
| 110 | Modal.com TTS provider | **DONE** | PR #110 merged; real HTTP + PCM decode |

## Recent Changes

### SFX Mixing Engine (#287)
- Integrated `SfxManager` from `movie-radio-render` into `RadioPlayAssembler` (`assemble_with_sfx`)
- Resolved `SfxTrigger` variants (`AutoSelect`, `Specific`, `AiGenerate`, `None`) into decoded audio sample buffers and mixed end-to-end with ducking.

### WebRTC VAD Integration (#253)
- Added WebRTC VAD engine support under `--features webrtc-vad` in `crates/movie-radio-pipeline/src/pipeline/vad/webrtc.rs`.
- Validated support for 8k/16k/32k/48kHz sample rates and 10/20/30ms frames.

### Native 24-bit PCM & 32-bit Float WAV Decoding (#250)
- Expanded Symphonia WAV decoder in `crates/movie-radio-pipeline/src/pipeline/decode.rs` to decode 24-bit signed PCM and 32-bit float WAV files natively without requiring `ffmpeg` on PATH.

### Unified Orchestrator (#246)
- Unified GOAP planning and pipeline stage execution in `crates/movie-radio-pipeline/src/orchestrator.rs`.
- Implemented `ProduceCheckpoint` JSON persistence and progress resumption via `--resume`.
- Standardized provider configuration and fallback handling.

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

1. **Implement TTS inference for local providers** — Kokoro/Orpheus/Qwen3 need actual neural inference (Orpheus issue #91)
2. **#76 AGENTS.md gaps** — Deferred by design
