# Implementation Gaps

Gaps between the current specification and the implemented runtime behavior.

**Updated:** 2026-06-23

## Voice Synthesis: Most Providers Return Silence

**Affected:** Radio-play production readiness
**Location:** `crates/movie-radio-voice/src/voice/`

**Spec:** All configured TTS providers should produce actual audio output.

**Actual:** Modal (PR #110) and ElevenLabs now produce real audio. OpenAI TTS is now implemented. The remaining providers (Kokoro, Orpheus, Qwen3, PocketTts) have correct trait implementations, config structs, and capability declarations, but their `synthesize()` methods return silence or zero-filled buffers.

**Provider Status:**

| Provider | Infrastructure | Real Synthesis | Blocker |
|----------|---------------|----------------|---------|
| Modal | Complete | Yes | None |
| ElevenLabs | Complete (HTTP) | Yes (MP3 decode via symphonia) | None |
| OpenAI | Complete (HTTP) | Yes (MP3 decode via symphonia) | None |
| Kokoro | Complete (ONNX download) | No (silence) | ONNX inference not wired to output |
| Orpheus | Partial (emotion tags) | No (silence) | Needs `llama-cpp-2` integration |
| Qwen3 | Partial (emotion prompts) | No (silence) | Needs model inference |
| PocketTts | None | No (silence) | Fully stubbed |

**Fix:** Prioritize local providers (Kokoro ONNX, Orpheus GGUF, Qwen3) for offline capability.

## GOAP Orchestrator Executes Real Pipeline Stages

**Status:** Resolved — orchestrator now has `async fn execute()` on `Action` trait, wired to real pipeline functions (decode, extract, gaps, narrate, TTS, assemble).

## Radio-Play CLI Fully Wired

**Status:** Resolved — `handle_radio_play()` runs full pipeline by default (gap → narrate → TTS → assembly → output). `--analyze-only` flag preserved for gap analysis mode.

## Coverage Scope Gap: Full Raw Fixture Output Parity

**Affected:** Production eval breadth
**Location:** `testdata/raw/` vs `testdata/validation/manifest.json`

**Spec intent:** Every fixture used for production evaluation should have explicit, testable output coverage.

**Actual:** Manifest tiers A/B/C are enforced, but not every raw media file is part of the active evaluation manifest yet.

**Status:** Mostly resolved — both `manifest.json` and `radio-play-manifest.json` cover production-critical fixtures.

**Fix:** Expand the manifest intentionally (with truth source + output path per fixture) and keep scheduled sweep runtime within CI limits.

## Future Capability Gap: True Alternative VAD Engines

**Affected:** Feature completeness
**Location:** `crates/movie-radio-pipeline/src/pipeline/vad/`

**Spec:** Non-energy engines should either exist as real implementations or remain clearly unavailable.

**Actual:** The shipped CLI exposes `energy`, `spectral`, and `hybrid` engines. WebRTC and Silero implementations do not exist.

**Fix:** Implement those engines behind explicit feature flags and reintroduce them to the CLI only when the implementations exist.

## Benchmark Gap: HybridVad Not Benchmarked

**Status:** Resolved — `SpectralVad` and `HybridVad` benchmarks added in PR #62. All three engines now report Criterion results.

## OpenAI TTS Provider

**Status:** Resolved — REST client for OpenAI TTS API implemented in `crates/movie-radio-voice/src/voice/openai.rs`. Registered in `SynthesisOrchestrator` fallback chain.

## Pre-existing LOC Violations

**Status:** Resolved — All 4 files split into submodules. See `plans/FOLLOWUPS.md` for details.
