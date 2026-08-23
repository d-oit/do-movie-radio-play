# GOAP State

**Current Goal**: Fix FOLLOWUPS correctness bugs — narration zip misalignment (structural Option-alignment), all-failed exit-0 degradation, per-provider text caps
**Status**: In-Progress

## Task Graph
- [x] Task F0: Issue filed; code recon (PipelineContext, assemble pairing, orchestrator loop)
- [x] Task F1: voice crate — provider cap check in fallback loop before dispatch
- [x] Task F2: goap lib.rs — `narration_audio: Vec<Option<AudioOutput>>` aligned with scripts
- [x] Task F3: actions.rs — Some/None pushes, modal cap guard, all-failed bail, assemble skips None
- [x] Task F4: Tests — fake-provider cap fallback (tokio dev-dep), offline all-failed bail, middle-skip pairing
- [ ] Task F5: Gates → PR → green CI on final SHA → merge

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
