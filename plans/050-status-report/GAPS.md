# Implementation Gaps

Gaps between the current specification and the implemented runtime behavior.

**Updated:** 2026-06-22

## Voice Synthesis: Most Providers Return Silence

**Affected:** Radio-play production readiness
**Location:** `crates/movie-radio-voice/src/voice/`

**Spec:** All configured TTS providers should produce actual audio output.

**Actual:** Only Modal (PR #110) and partially ElevenLabs produce real audio. The remaining providers (Kokoro, Orpheus, Qwen3, PocketTts) have correct trait implementations, config structs, and capability declarations, but their `synthesize()` methods return silence or zero-filled buffers.

**Provider Status:**

| Provider | Infrastructure | Real Synthesis | Blocker |
|----------|---------------|----------------|---------|
| Modal | Complete | Yes | None |
| ElevenLabs | Complete (HTTP) | Partial (no MP3 decode) | Need `symphonia` or `minimp3` for MP3→PCM |
| Kokoro | Complete (ONNX download) | No (silence) | ONNX inference not wired to output |
| Orpheus | Partial (emotion tags) | No (silence) | Needs `llama-cpp-2` integration |
| Qwen3 | Partial (emotion prompts) | No (silence) | Needs model inference |
| PocketTts | None | No (silence) | Fully stubbed |

**Fix:** Prioritize Modal (already working) + ElevenLabs MP3 decode for immediate radio-play capability. Local providers need model inference wiring.

## GOAP Orchestrator Doesn't Execute Real Work

**Affected:** End-to-end radio-play pipeline
**Location:** `crates/movie-radio-goap/src/orchestrator.rs`

**Spec:** The orchestrator should execute actual pipeline stages (decode, VAD, TTS, assembly).

**Actual:** The orchestrator runs the A* planner and iterates through the plan, but each action only updates `WorldState` boolean flags. No ffmpeg calls, no VAD processing, no TTS synthesis, no audio assembly happens.

**Comment in code:** *"In a real implementation, this would call the actual pipeline stage. For now, we just update the world state based on the action's effects."*

**Fix:** Add `execute()` method to `Action` trait or wire orchestrator to call pipeline crate functions directly.

## Radio-Play CLI Not Fully Wired

**Affected:** User-facing radio-play command
**Location:** `crates/movie-radio-timeline/src/handlers/radio_play.rs`

**Spec:** `timeline radio-play <MOVIE> --output <FILE>` should produce a complete radio play.

**Actual:** The `handle_radio_play()` function only implements `--analyze-only` mode (runs `GapIdentifier`, outputs gap list). The full pipeline (narration → TTS → assembly) is marked as "not yet implemented."

**Fix:** Wire the handler through: gap identification → narration generation → TTS synthesis → audio assembly → output.

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

**Affected:** Performance visibility
**Location:** `benchmarks/benches/pipeline_bench.rs`

**Spec:** All VAD engines should have benchmark coverage.

**Actual:** Only `EnergyVad` is benchmarked. `SpectralVad` and `HybridVad` are not included in the benchmark harness.

**Status:** Resolved — `SpectralVad` and `HybridVad` benchmarks added in PR #62. All three engines now report Criterion results.

## OpenAI TTS Provider Missing

**Affected:** Provider diversity
**Location:** N/A (not implemented)

**Spec:** OpenAI TTS-1 HD as alternative paid provider (ADR-121).

**Actual:** Not implemented at all. Only ElevenLabs exists as a paid cloud provider.

**Fix:** Add `src/voice/openai.rs` with REST client for OpenAI TTS API.
