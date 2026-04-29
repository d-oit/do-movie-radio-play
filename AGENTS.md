# AGENTS.md

## Named Constants
```bash readonly
DEFAULT_SAMPLE_RATE_HZ=16000
DEFAULT_FRAME_MS=20
MAX_SOURCE_FILE_LOC=500
MAX_LINES_AGENTS_MD=150
```

## Versioning
The `VERSION` file in the root is the single source of truth. Never edit version strings inline.

## Repository Map
| Directory | Purpose |
|-----------|---------|
| `src/pipeline/` | VAD, framing, segmentation, features, tags, prompts |
| `src/learning/` | Calibration, adaptive thresholds, and libsql database |
| `src/types/` | Shared types (Frame, Segment, Metrics) |
| `scripts/` | Quality gate, benchmarks, and validation |
| `plans/` | ADRs, roadmaps, and status reports |
| `.agents/skills/` | Reusable skill playbooks |

## Domain Concepts
- **Frame**: 20ms audio window (320 samples at 16kHz).
- **VAD**: Voice Activity Detection classifying frames as speech or non-voice.
- **Segment**: Contiguous time range with kind, confidence, tags, and prompt.
- **Calibration profile**: Genre-specific energy threshold deltas.

## Skill Activation Policy
Load skills from `.agents/skills/` before starting tasks:
- `nonvoice-segmentation`: Segmentation behavior descriptions.
- `audio-vad-cpu`: VAD parameter documentation.
- `self-learning-calibration`: Learning system documentation.
- `agent-coordination`: Coordination strategy and handoffs.

## Rules
- **Verification**: `bash scripts/quality_gate.sh` must pass with zero warnings.
- **Fix ALL pre-existing issues** (lint, tests, clippy) before completing any task.
- **No unwrap() or expect()** in `src/`. Use `Result` and `?`.
- **Deterministic outputs**: Same input must produce identical JSON.
- **Atomic Commits**: `bash scripts/quality_gate.sh && git add -A && git commit`.
- **MAX_SOURCE_FILE_LOC**: Limit Rust source files to 500 lines.
- **No magic numbers**: Extract to `config.rs` or module-level constants.
- **Media Sourcing**: Use legally redistributable media only (Blender/Open Movies).

## Template Sync
| Pattern | Status | Notes |
|---------|--------|-------|
| Gitleaks Scan | Gap | `.gitleaks.toml` missing |
| Named Constants | Adopted | `bash readonly` block above |
| `ai-commit.sh` | Gap | Script missing; see `plans/050-status-report/STATUS.md` |
| Single Source Version | Adopted | `VERSION` file |
| `MAX_LINES_AGENTS_MD` | Adopted | Enforced at 150 lines |
| Skill Frontmatter | Adopted | Verified in `.agents/skills/` |
| Agent Config Dirs | Gap | `.jules/`, `.opencode/`, `.qwen/` missing |
| `update-all-docs.sh`| Gap | Script missing; see `plans/050-status-report/STATUS.md` |

## Agent Coordination References
Reference [.agents/skills/agent-coordination/SKILL.md](.agents/skills/agent-coordination/SKILL.md) and
[.agents/skills/agent-coordination/PARALLEL.md](.agents/skills/agent-coordination/PARALLEL.md).
