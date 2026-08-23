# GOAP State

**Current Goal**: Implement issue #206 — SynthesisRequest bounds validation + audio.cpp TTS spike
**Status**: Complete

## Task Graph
- [x] Task R: Swarm recon — voice crate structure, error types, test conventions, llama feature gating
- [x] Task B: Branch `fix/synthesis-request-validation` off main + implement validation in `SynthesisRequest::validate()` (orchestrator choke point + goap direct-dispatch guard)
- [x] Task T: Boundary unit tests — absolute pins per review swarm (R2), multibyte chars test, ordering test
- [x] Task V: Local verification green under bindgen workaround (fmt, clippy -D warnings, 29 tests)
- [x] Task S: Swarm adversarial review — FIX_FIRST verdict; R1 (speed range) + R2 (self-referential tests) applied
- [x] Task P: PR #209 opened, 32/32 checks green on final SHA 04893e2
- [x] Task M: #209 squash-merged as 076601b; issue #206 auto-closed; FOLLOWUPS/GOAP_STATE updated
- [x] Task X: audio.cpp CPU spike completed — report in plans/audiocpp-tts-spike.md (RTF ≈ 1.1 CPU German; Phase 2 gated on human listening A/B)

## Evidence Log
- Issue #206 contract: sample_rate_hz 8_000..=48_000; speed finite 0.25..=4.0; text ≤ 10_000 chars.
- Speed range decision: 0.25..=4.0 confirmed by maintainer — sole consumer is openai.rs payload (`"speed": request.speed`, OpenAI documents 0.25–4.0); struct's old `0.5 - 2.0` comment was stale.
- Review findings logged to FOLLOWUPS.md: all-skipped→exit-0 semantics, zip misalignment, per-provider caps, config-side rate validation.
- Spike (audio.cpp 0.6.1 @ 62735ea): pocket_tts_german_q8_0 (122 MB), CPU RTF ≈ 1.10, 24 kHz out, ~587 MB RSS. Gotchas: german package needs german-native voice embedding sidecar (english alba.safetensors → instant-EOS 240 ms truncation); not bit-deterministic across runs (±80 ms). Artifacts /tmp/opencode/spike/.

## Evidence Log
- Issue #206 requirements: reject sample_rate_hz outside 8_000..=48_000; finite speed 0.25..=4.0; text ≤ 10_000 chars; typed errors; validation once in SynthesisOrchestrator::synthesize pre-dispatch.
- voice/mod.rs:26 defines SynthesisRequest { text, sample_rate_hz: u32, ... } with Default sample_rate_hz=16000.
- Providers consuming request.sample_rate_hz: kokoro.rs:202, qwen3.rs:142, elevenlabs.rs:70/75, orpheus.rs:201, pockettts.rs:23, modal.rs:67.
- movie-radio-voice hard-depends on llama-cpp-2 → llama-cpp-sys build needs libclang headers missing locally; CI has them. Local verification limited to syntax-level unless optional feature exists.

## History
- 2026-08-23: PR sweep complete (#204 merged d64941d, #205 closed, #207 merged e4460af, #208 merged 2d07e78). Issue #206 filed from #205 salvage.
- 2026-08-23: Started #206 implementation per maintainer go.
- 2026-08-23 (earlier sweep, archived): #204 merged after bot-push override; #205 closed as AI-slop; #207 fixed Clippy 1.98 breakage; root `src/` dead tree logged in FOLLOWUPS.md.
