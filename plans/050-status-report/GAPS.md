# Implementation Gaps

Gaps between the current specification and the implemented runtime behavior.

**Updated:** 2026-08-25

## Voice Synthesis: Provider Status & Quality Caveats

**Affected:** Radio-play production readiness
**Location:** `crates/movie-radio-voice/src/voice/`

**Spec:** All configured TTS providers should produce actual audio output.

**Actual:** Refreshed after workspace-wide analysis (`plans/130-improvement-analysis-2026-08-25.md`). All HTTP providers (Modal, ElevenLabs, OpenAI) produce real audio. Local inference landed for Qwen3 (candle), Kokoro (ONNX Runtime), and Orpheus (llama.cpp token loop), with quality caveats below. PocketTts remains a silence stub and is recommended for removal.

**Provider Status:**

| Provider | Infrastructure | Real Synthesis | Blocker |
|----------|---------------|----------------|---------|
| Modal | Complete | Yes (no RIFF validation — see FOLLOWUPS) | Header hardening |
| ElevenLabs | Complete (HTTP) | Yes (MP3 decode via symphonia) | None |
| OpenAI | Complete (HTTP) | Yes (MP3 decode via symphonia) | One `.expect()` cleanup (see FOLLOWUPS) |
| Kokoro | Complete (ONNX download) | Partial — real ONNX inference, but tokenization maps raw codepoints instead of eSD phoneme vocabulary | Phoneme tokenizer; acoustic output unverified |
| Orpheus | Complete (llama.cpp inference) | Partial — real token generation; SNAC→PCM decoding falls back to synthetic tones | SNAC vocoder decode |
| Qwen3 | Complete (candle inference) | Yes (CUDA→CPU fallback) | None |
| PocketTts | Config-only | No (silence stub, falsely advertises cloning/streaming caps) | Recommended for removal |

**Fix:** Complete Kokoro phoneme tokenization and Orpheus SNAC decode for offline capability; remove PocketTts. Consider feature-gating local-inference dependencies (`local-tts` umbrella) so default builds skip the llama.cpp/candle/ort compile cost.

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
