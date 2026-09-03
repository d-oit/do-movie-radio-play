# ADR-123: Provider & Compute-Agnostic Architecture

**Status**: Accepted
**Date**: 2026-09-01
**Issues**: #237

## Context

Existing `VoiceSynthesizer`, `SynthesisRequest`, `ProviderCapabilities` in `crates/movie-radio-types` already abstract TTS. New requirement (#235 closed) adds audio.cpp with local/remote execution. Risk: creating duplicate hierarchies `xtts_local`, `coqui_local` that couple capability to location.

## Decision

Distinguish capability/provider (TTS, narrator, SFX, transcription) from execution location (local vs remote, GPU pool routing). `audio.cpp` is a provider, not a location. Introduce minimal compute types only where needed:

```rust
pub enum ExecutionLocation { Local, Remote }
pub struct ComputeEndpoint { pub id: String, pub url: Option<Url>, pub cost_per_hour: Option<f64> }
```

Reuse `GpuPoolEndpoint` / `GpuPolicyConfig` for cost-aware routing. Provider-neutral fallback chain; marketplace SDKs out of scope.

## Consequences

- No second TTS trait; existing abstraction stays authoritative.
- Local inference optional; free/credit GPU preferred, paid requires `allow_paid=true` + budget guards.
- Core crates not dependent on RunPod/Vast/Modal SDKs.
- Tests: registration, capability checks, routing failures, cost rejection.
