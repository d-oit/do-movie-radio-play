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
| `crates/movie-radio-types/` | Shared types (Frame, Segment, Metrics, Emotion, AudioOutput) |
| `crates/movie-radio-pipeline/` | VAD, framing, segmentation, features, tags, prompts, decode |
| `crates/movie-radio-voice/` | TTS providers (Kokoro, Orpheus, ElevenLabs, Modal, etc.) |
| `crates/movie-radio-goap/` | GOAP planner, actions, orchestrator, gaps, narrate, assemble |
| `crates/movie-radio-learning/` | Calibration, adaptive thresholds, libsql database |
| `crates/movie-radio-verification/` | Spectral verification, fingerprinting, extractor |
| `crates/movie-radio-render/` | Audio mixing, AGC, spatial panning, reverb |
| `crates/movie-radio-io/` | JSON, EDL, VTT, WAV I/O utilities |
| `crates/movie-radio-validation/` | Validation, comparison, SRT parsing, synthetic fixtures |
| `crates/movie-radio-timeline/` | Binary crate (CLI, handlers, config) |
| `scripts/` | Quality gate, benchmarks, validation, optimization |
| `plans/` | ADRs, roadmaps, and status reports |
| `.agents/skills/` | Reusable skill playbooks |

## Quick Reference
| Task | Command |
|------|---------|
| Build | `cargo build --workspace` |
| Test | `cargo test --workspace` |
| Quality Gate | `bash scripts/quality_gate.sh` |
| Docs Update | `bash scripts/update-all-docs.sh` |
| Commit | `bash scripts/ai-commit.sh` |

## Rules
- **Verification**: `bash scripts/quality_gate.sh` must pass with zero warnings.
- **Lint**: Always run `cargo fmt --check && cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- **Atomic Commits**: Use `bash scripts/ai-commit.sh`.
- **No unwrap() or expect()** in `crates/*/src/`. Use `Result` and `?`.
- **MAX_SOURCE_FILE_LOC**: Limit Rust source files to 500 lines.
- **Secret Scanning**: Gitleaks enforcement via `.gitleaks.toml`.
- **Root Cleanliness**: Never commit test fixtures or runtime-output files to the repository root.
- **Deterministic output**: All pipeline stages must produce deterministic output for identical inputs.
- **Pre-existing issues**: Address pre-existing warnings or document in `plans/FOLLOWUPS.md`.

## Agent Coordination References
- [.agents/ORCHESTRATION.md](.agents/ORCHESTRATION.md)
- [.agents/skills/agent-coordination/SKILL.md](.agents/skills/agent-coordination/SKILL.md)
- [.agents/skills/agent-coordination/PARALLEL.md](.agents/skills/agent-coordination/PARALLEL.md)

## Template Sync
| Pattern | Status | Notes |
|---------|--------|-------|
| Gitleaks Scan | Adopted | `.gitleaks.toml` present |
| Named Constants | Adopted | `bash readonly` block above |
| Single Source Version | Adopted | `VERSION` file is the source of truth |
| `MAX_LINES_AGENTS_MD` | Adopted | Enforced at 150 lines |
| Skill Frontmatter | Adopted | Verified in all `.agents/skills/*.md` |
| `ai-commit.sh` | Adopted | Available in `scripts/` |
| `update-all-docs.sh` | Adopted | Available in `scripts/` |
| Agent Config Dirs | Adopted | `.jules/`, `.opencode/`, `.qwen/` present |
| `VERSION` policy | Gap | `agents-docs/VERSION.md` missing (no `agents-docs/` dir) |
