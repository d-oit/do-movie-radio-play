# ADR-003: VAD Engine Trait System

**Status:** Accepted

**Date:** 2026-04-13

## Context

The project needs to support multiple Voice Activity Detection (VAD) engines. Currently, only an energy-based VAD is implemented. Future support for WebRTC and Silero VAD engines is planned. The pipeline should be extensible to support new VAD engines without modifying core logic.

## Decision

Introduce a `VadEngine` trait system that allows runtime selection of VAD engines.

### Trait Definition

```rust
pub trait VadEngine: Send + Sync {
    fn classify(&self, frames: &[Frame]) -> Vec<bool>;
    fn name(&self) -> &'static str;
}
```

### Engine Implementations

| Engine | Status | Description |
|--------|--------|-------------|
| `energy` | Implemented | RMS-based threshold VAD |
| `webrtc` | Planned | Placeholder for future WebRTC VAD integration |
| `silero` | Planned | Placeholder for future Silero VAD integration |

### Factory Pattern

A `create_engine(name, threshold)` function instantiates engines by name:

```rust
pub fn create_engine(name: &str, threshold: f32) -> Box<dyn VadEngine>
```

## Consequences

### Positive
- Runtime engine selection via `--vad-engine` CLI flag for implemented engines
- Thread-safe implementations (`Send + Sync` bounds)
- Extensible: new engines only need to implement `VadEngine`
- Decoupled pipeline from specific VAD implementation

### Negative
- Trait object indirection adds runtime overhead
- Future engines need feature-gated reintroduction to avoid exposing unsupported options

### Neutral
- Existing energy VAD refactored to implement trait
- Default engine remains `energy` for backwards compatibility
