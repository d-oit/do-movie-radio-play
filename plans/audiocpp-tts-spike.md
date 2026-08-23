# audio.cpp TTS Spike Report

**Date**: 2026-08-23 · **Spike branch**: none (all artifacts in `/tmp/opencode/`, repo untouched)
**Question**: Is audio.cpp (0xShug0/audio.cpp) a better TTS vehicle than our in-process providers?

## Verdict

**Promising for scoped adoption — proceed to Phase 2 design only after human listening test.**
CPU feasibility is proven: real German speech at RTF ≈ 1.1 on 8 cores. The blocker to an immediate
integration decision is voice-quality A/B against `kokoro-german-martin` — no local ONNX baseline
exists on this machine, so quality comparison must be done by listening to `/tmp/opencode/spike/*.wav`.

## Facts measured (audio.cpp 0.6.1 @ `62735ea`, GCC 13.3, CPU backend)

| Item | Value |
|---|---|
| Build | `--backend cpu --model-set custom --models pocket_tts`, wall **1 m 54 s** |
| Model | `pocket_tts_german_q8_0`, 122 MB GGUF + `alba.safetensors` voice sidecar (6.2 MB) |
| German line 1 (58 chars) | wall ≈ 3.95 s → audio 3.56 s, **RTF 1.09–1.13** |
| German line 2 (105 chars) | wall ≈ 5.79 s → audio 5.20 s, RTF ≈ 1.10–1.13 |
| Peak RSS | ~587 MB |
| Output | native 24 kHz mono WAV (no CLI resample flag; we resample in-crate already) |
| Sanity | RMS ~4k, no clipping, Silero-VAD sees one continuous segment @ conf 1.0 |

## Gotchas discovered (would bite integration)

1. **Voice sidecar language must match model**: English `alba.safetensors` on the German model
   truncates *every* synthesis to exactly 240 ms (instant EOS from incompatible KV-cache state).
   German-native presets live under `PocketTTS-GGUF/german/embeddings/`.
2. Overrides split across flags: language = `--load-option`, generation params = `--session-option`.
3. **Not bit-deterministic**: same seed ±80 ms duration variance (multithreaded CPU reduction order).
   Conflicts softly with the repo's determinism rule; acceptable if treated like cloud providers,
   but document it.

## Fit against our stack

- Fills the dead `PocketTtsProvider` stub (currently returns silence) with a real CPU-first German engine.
- Server exposes OpenAI-compatible `POST /v1/audio/speech` → provider ≈ clone of `openai.rs`
  (~100 lines, configurable base_url, no API key). No new Rust dependencies.
- Long-term: could evict `llama-cpp-2` (bindgen pain — see today's build incident), `ort`,
  `candle-core` from the Rust build by delegating local inference to one pinned native binary.
- Risks: project young (first release Jun 2026), fast churn → pin release tag; emotion control
  not carried by the OpenAI-compatible endpoint (matters for narration expressiveness);
  CI must stay GPU-free (CPU-mode smoke tests or mocked unit tests per crate convention).

## Recommended next steps (Phase 2 gate)

1. Maintainer listens to `/tmp/opencode/spike/de_line1.wav`, `de_line2.wav`; generate a Kokoro
   German sample on a machine with the ONNX model and compare subjectively.
2. If pass → design `AudioCppProvider`: config `{ base_url, model_id, voice_id }`, lazy health
   check, mocked tests, registered between local and cloud tiers in `fallback_chain`.
3. Pin audio.cpp release tag + model package hashes wherever deployed.
