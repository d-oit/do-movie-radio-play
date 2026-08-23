# GOAP State

**Current Goal**: Config-option TTS integration — OpenAI-compatible `base_url` (default public OpenAI) enabling audio.cpp German sidecar (`OPENAI_TTS_BASE_URL` → PocketTTS alba/wav)
**Status**: In-Progress

## Task Graph
- [x] Task W1: Web research — Aug 2026 German CPU TTS SOTA (Kokoro 82M MOS 4.44; PocketTTS de MIT cloning WER 1.84; Supertonic-3 de WER 0.66% @ 8.75x RT; audio.cpp serves pocket_tts/supertonic)
- [x] Task W2: `OpenAiConfig.base_url` (serde default = public API) + optional `api_key_env` (None = no auth header)
- [x] Task W3: openai.rs endpoint()/auth_header() helpers + conditional Authorization; 5 unit tests incl. serde defaults
- [x] Task W4: radio_play.rs env wiring — OPENAI_API_KEY → cloud default; else OPENAI_TTS_BASE_URL → German sidecar defaults (pocket-tts/alba/wav)
- [ ] Task W5: Issue, PR, full green CI on final SHA, merge
- [ ] Task W6: Spike report Phase-2 addendum + CHANGELOG entry

## History
- 2026-08-23: #206 complete (PR #209 → 076601b); audio.cpp spike complete (plans/audiocpp-tts-spike.md).
- 2026-08-23: Started config-option integration per maintainer directives: config option with OpenAI default; German TTS focus; research-backed.

## Evidence Log
- Issue #206 requirements: reject sample_rate_hz outside 8_000..=48_000; finite speed 0.25..=4.0; text ≤ 10_000 chars; typed errors; validation once in SynthesisOrchestrator::synthesize pre-dispatch.
- voice/mod.rs:26 defines SynthesisRequest { text, sample_rate_hz: u32, ... } with Default sample_rate_hz=16000.
- Providers consuming request.sample_rate_hz: kokoro.rs:202, qwen3.rs:142, elevenlabs.rs:70/75, orpheus.rs:201, pockettts.rs:23, modal.rs:67.
- movie-radio-voice hard-depends on llama-cpp-2 → llama-cpp-sys build needs libclang headers missing locally; CI has them. Local verification limited to syntax-level unless optional feature exists.

## History
- 2026-08-23: PR sweep complete (#204 merged d64941d, #205 closed, #207 merged e4460af, #208 merged 2d07e78). Issue #206 filed from #205 salvage.
- 2026-08-23: Started #206 implementation per maintainer go.
- 2026-08-23 (earlier sweep, archived): #204 merged after bot-push override; #205 closed as AI-slop; #207 fixed Clippy 1.98 breakage; root `src/` dead tree logged in FOLLOWUPS.md.
