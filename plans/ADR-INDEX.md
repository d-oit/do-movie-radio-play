# ADR Index

> **⚠️ Numbering Collision:** ADR-003 is used twice — once in `plans/010-architecture/ADR-003-json-contract.md` (stub) and once in `plans/020-triz/ADR-003-vad-trait-system.md` (fully documented). **Recommended fix:** renumber the architectural ADR-003 to ADR-005 (leaving the triz one as ADR-003), or keep the architectural one as ADR-003 and rename the triz one to ADR-005.

| ADR | Title | Status | Location | Description |
|-----|-------|--------|----------|-------------|
| 001 | Core pipeline | Accepted (stub) | `plans/010-architecture/ADR-001-core-pipeline.md` | Use decode → resample → frame → VAD → smooth → invert → JSON for transparent auditability. 2 lines. |
| 002 | VAD engine selection | Accepted (stub) | `plans/010-architecture/ADR-002-vad-engine-selection.md` | Select deterministic energy-based VAD as default CPU backend; keep trait-friendly boundaries for future engines. 2 lines. |
| 003 | JSON contract | Accepted (stub) | `plans/010-architecture/ADR-003-json-contract.md` | Stable serde model with snake_case enums and deterministic field ordering via fixed structs. 2 lines. |
| 003 | VAD Trait System | Accepted (54 lines) | `plans/020-triz/ADR-003-vad-trait-system.md` | Introduce a `VadEngine` trait system allowing runtime selection of VAD engines. Factory pattern with `create_engine(name, threshold)`. Covers energy (impl), webrtc (planned), silero (planned). |
| 004 | Self-learning boundaries | Accepted (stub) | `plans/010-architecture/ADR-004-self-learning-boundaries.md` | Allow only offline calibration via explicit correction records and versioned reports. 2 lines. |

| 120 | GOAP Radio Play Pipeline | Proposed | `plans/120-goap-radio-play-pipeline/ADR-120-goap-architecture.md` | GOAP-based orchestration for full movie-to-radio-play conversion with A* planning, replanning on failure, and resource-aware action selection. |
| 121 | Voice Synthesis Providers | Proposed | `plans/120-goap-radio-play-pipeline/ADR-121-voice-synthesis-providers.md` | Pluggable TTS provider architecture: Kokoro-82M (free/CPU), Qwen3-TTS (emotion/local), Orpheus-3B (GGUF/emotion tags), ElevenLabs/OpenAI (paid API). User-configurable with fallback chains. |
| 122 | Self-Improving Learning | Proposed | `plans/120-goap-radio-play-pipeline/ADR-122-self-improving-learning.md` | Three-layer learning system (trace recording, MAPE analysis, cross-run accumulation) that improves radio play quality after every run with bounded adaptation and rollback safety. |

## Notes

- ADR-001 through ADR-004 (architecture) are stubs that capture only the title and a one-line summary. They lack context, decision rationale, and consequences sections.
- ADR-003 (triz) is the only fully-documented ADR with proper Context → Decision → Consequences structure (54 lines).
- The numbering collision between `plans/010-architecture/ADR-003-json-contract.md` and `plans/020-triz/ADR-003-vad-trait-system.md` should be resolved. Recommended: renumber the architectural ADR-003 to **ADR-005** so the fully-documented triz decision retains its established reference.
- ADR-120 through ADR-122 form the radio play generation pipeline proposal (experimental, 2026-06-21). These depend on the completed extraction pipeline (phases 1-6) and extend it with GOAP orchestration, AI voice synthesis, and self-improvement.
