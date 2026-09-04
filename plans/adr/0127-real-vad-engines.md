# ADR-127: Real VAD Engines Behind Feature Flags

**Status**: Accepted
**Date**: 2026-09-04
**Issues**: #253 (Phase 6.2)

## Context

`VadEngine::classify(&[Frame])` consumes pre-computed features (`Frame` carries
rms/zcr/spectral stats, no raw samples). Sample-level engines cannot plug in:
WebRTC VAD needs mono i16 frames; Silero VAD needs f32 windows with recurrent
state. Researched 2026-09-04 against official docs:

- `webrtc-vad` 0.4.0 ([docs.rs](https://docs.rs/webrtc-vad/latest/webrtc_vad/)):
  rates 8/16/32/48 kHz, frames 10/20/30 ms, `is_voice_segment(&[i16])`,
  modes Quality/LowBitrate/Aggressive/VeryAggressive. No model files, CPU trivial.
  Our 16 kHz / 20 ms default = 320-sample frames, a native fit (f32→i16 convert).
- `silero-vad-rust` 6.2.2 ([GitHub](https://github.com/sheldonix/silero-vad-rust)):
  ONNX models **bundled** in the published package (`src/silero_vad/data/*.onnx`),
  no build-time or runtime downloads (fits the no-auto-download policy).
  Requires `ort 2.0.0-rc.10`; workspace pins rc.9 — both caret, so Cargo unifies
  to one version. `forward_chunk` is stateful over exact 512-sample windows @16kHz;
  CPU execution provider default. Runtime needs the ONNX dylib (`load-dynamic`,
  same as the existing voice crate usage).
- Alternative `wavekat-vad` rejected: its Silero backend downloads the model at
  build time, breaking offline builds.

## Decision

1. Extend `VadEngine` with defaulted methods (backward compatible):
   `uses_raw_samples() -> bool` (default `false`) and
   `classify_samples(&[f32], sample_rate_hz, frame_ms) -> Result<VadResult>`
   (default: bail "engine requires pre-computed frames"). Pipeline routes
   sample-based engines on the mono signal before framing.
2. `WebRtcVad` behind `webrtc-vad` feature; aggressiveness mapped from the
   configured threshold; unsupported rate/frame combos rejected with a typed error.
3. `SileroVad` **DEFERRED**: `silero-vad-rust` 6.2.2 does not compile against
   `ort` rc.13 as resolved in `Cargo.lock` (the manifest declares
   `ort ^2.0.0-rc.9`, which admits rc.13; the crate targets the rc.10-era API:
   `execution_providers::CPUExecutionProvider`, non-generic `ort::Error`,
   old `Tensor::from_array` bounds — all changed/removed in rc.13; verified
   2026-09-04 via `cargo check -p movie-radio-pipeline --features silero-vad`).
   The workspace lock already unified on rc.13 (voice crate is compatible), so
   this is purely a `silero-vad-rust` release gap. Additionally there is no
   weight-vendoring convention (`models/` does not exist; root-cleanliness
   rules). Unblock path: upstream release supporting ort rc.13+, or an explicit
   decision to vendor `silero_vad.onnx` (~2 MB, MIT) plus hand-rolled session
   code over the workspace `ort`. The `silero` engine name stays reserved and
   fails with a pointer to this ADR.
4. `create_engine` accepts `webrtc`/`silero` unconditionally; when the feature
   is compiled out it bails with "rebuild with `--features webrtc-vad`".
   CLI `value_parser` and config `VALID_VAD_ENGINES` extended the same way
   (fail at creation, not at arg parse, so `--help` stays stable).
5. Default build unchanged: energy/spectral/hybrid, CPU-only, no new Deps.

## Consequences

- New optional dep: `webrtc-vad` (C code via `cc`, needs C compiler at build —
  present in CI; verified compiling + tested locally with gcc).
- `Cargo.lock` untouched (`ort` stays rc.13, no new transitive deps).
- Silero deferred per §3 above; shipping WebRTC alone still satisfies the
  "implement WebRTC or remove the option" half of Phase 6.2 and unblocks real
  engine comparison against energy/spectral baselines.
- Determinism preserved: fresh WebRTC instance per call, zero-padded tails,
  no RNG.
