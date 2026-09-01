# ADR-125: Voice Cloning — Capability-Driven via audio.cpp

**Status**: Accepted
**Date**: 2026-09-01
**Issues**: #239

## Context

Need reproducible character voice workflow. Building separate XTTS/RVC/OpenVoice inference would duplicate native runtime and lock in Python/Conda. audio.cpp already supports voice presets, `voice_ref` (path or base64 ≤5MiB), and family-specific cloning.

## Decision

- No separate XTTS/RVC/OpenVoice Rust inference. Voice cloning is capability-driven: query selected audio.cpp model for cloning support; fail gracefully if unsupported.
- Types `VoiceReference { id, character, sample_paths, metadata }`, `VoiceReferenceParams { reference_id, reference_audio, voice_id }` integrate with `SynthesisRequest` via optional fields.
- Pipeline stage extracts dialogue candidates using transcription/VAD/diarization (configurable duration, SNR/silence filtering, deterministic selection, user-reviewable, no auto irreversible cloning).
- Persistence in project DB/config: character, sample paths, timestamps, runtime `audio_cpp`, family/model, language, creation/reproducibility metadata. No large model artifact duplication.
- Reuse #235 endpoint/GPU pool routing; paid execution requires explicit opt-in. Reference audio not uploaded unless remote explicitly selected; log when it leaves local machine.

## Consequences

- Reuses `VoiceSynthesizer`/`AudioCppProvider`; no competing TTS hierarchy.
- Works local CPU/GPU, free/paid/self-hosted remote transparently.
- Tests cover routing, capability checks, persistence, secret redaction.
