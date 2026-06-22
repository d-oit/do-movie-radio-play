# ADR-005: Workspace Crate Restructure

## Status

Accepted

## Context

The project was a monolithic single crate (`movie-nonvoice-timeline`) with all modules in `src/`. This caused:

- Files exceeding the 500 LOC limit (handlers.rs at 789, database.rs at 1061, pipeline/mod.rs at 527)
- No clear module boundaries or dependency management
- Difficult to test individual subsystems in isolation
- Not aligned with the [d-oit/rust-2026-template](https://github.com/d-oit/rust-2026-template) workspace pattern

## Decision

Restructure into a Cargo workspace with 8 library crates + 1 binary crate, following the template pattern.

### Workspace Structure

```
Cargo.toml              (workspace root)
crates/
  movie-radio-types/    (shared types: Frame, Segment, Metrics, Emotion, etc.)
  movie-radio-pipeline/ (core audio processing: VAD, framing, features, segmentation)
  movie-radio-voice/    (TTS providers: Kokoro, Orpheus, ElevenLabs, etc.)
  movie-radio-goap/     (GOAP planner, actions, orchestrator, gaps, narrate, assemble)
  movie-radio-learning/ (self-learning: adaptive thresholds, calibration, database)
  movie-radio-verification/ (spectral verification, fingerprinting)
  movie-radio-io/       (I/O utilities: JSON, EDL, VTT, WAV)
  movie-radio-validation/ (validation, comparison, SRT, synthetic fixtures)
  movie-radio-timeline/ (binary crate: CLI, handlers, config)
benchmarks/             (Criterion benchmarks)
```

### Dependency Graph

```
movie-radio-types (no internal deps)
  ↑
movie-radio-io (depends on: types)
  ↑
movie-radio-pipeline (depends on: types, io)
  ↑
movie-radio-voice (depends on: types)
  ↑
movie-radio-verification (depends on: types, io)
  ↑
movie-radio-learning (depends on: types, io, verification)
  ↑
movie-radio-validation (depends on: types, io, pipeline)
  ↑
movie-radio-goap (depends on: types, voice, pipeline, learning, validation)
  ↑
movie-radio-timeline (depends on: all above)
```

### Key Decisions

1. **Edition 2021** (not 2024): Maintains compatibility with current toolchain
2. **Workspace-level lints**: Clippy warnings configured at workspace level, inherited by all crates
3. **Shared types in movie-radio-types**: `FeatureSet`, `Fingerprint`, `TimelineError` moved here to break circular dependencies
4. **Binary crate has no lib.rs**: Pure binary, no library target
5. **Config types duplicated**: Voice crate has its own config types to avoid circular dependency with binary

### Compilation Fixes

- Fixed `ort` 2.0 API: `Session.inputs` is private → removed private field access, use `Arc<Session>` instead of clone
- Fixed `rand` version: Code uses 0.10 API (`RngExt`, `random()`) → `rand = "0.10"`
- Fixed circular dependency: `movie-radio-learning` ↔ `movie-radio-pipeline` broken by moving shared types to `movie-radio-types`

## Consequences

### Positive

- Each crate is independently testable
- Clear dependency boundaries enforced by Cargo
- Files within LOC limits (all under 500)
- Aligned with template pattern for future contributors
- Workspace-level lints ensure consistent code quality

### Negative

- Import paths longer (`movie_radio_types::Segment` vs `crate::types::Segment`)
- More Cargo.toml files to maintain
- Cross-crate changes require updating multiple crates
- Initial migration effort (completed in this ADR)

## References

- [d-oit/rust-2026-template](https://github.com/d-oit/rust-2026-template)
- AGENTS.md: MAX_SOURCE_FILE_LOC = 500
- Issue #90-#97: Radio play pipeline features
