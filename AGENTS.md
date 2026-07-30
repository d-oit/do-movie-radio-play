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
| ----------- | --------- |
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
| ------ | --------- |
| Build | `cargo build --workspace` |
| Test | `cargo test --workspace` |
| Quality Gate | `bash scripts/quality_gate.sh` |
| Docs Update | `bash scripts/update-all-docs.sh` |
| Commit | `bash scripts/ai-commit.sh` |

## Skill Activation Policy
Agents must load skill playbooks on demand when touching their domain. Current active skills map:
- Non-Voice Segmentation: [.agents/skills/nonvoice-segmentation/SKILL.md](.agents/skills/nonvoice-segmentation/SKILL.md)
- Audio VAD on CPU: [.agents/skills/audio-vad-cpu/SKILL.md](.agents/skills/audio-vad-cpu/SKILL.md)
- Self Learning & Calibration: [.agents/skills/self-learning-calibration/SKILL.md](.agents/skills/self-learning-calibration/SKILL.md)

## Rules
- **Verification**: `bash scripts/quality_gate.sh` must pass cleanly.
- **Lint**: Run `cargo fmt --check && cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- **Atomic Commits**: Use `bash scripts/ai-commit.sh` to compile, check quality gates, and commit atomically.
- **No unwrap() or expect()** in `crates/*/src/`. Use `Result` and the `?` operator.
- **MAX_SOURCE_FILE_LOC**: Limit Rust source files to 500 lines.
- **Secret Scanning**: Secret scanning is enforced via Gitleaks with `.gitleaks.toml`.
- **Root Cleanliness**: Never commit test fixtures or runtime-output files to the repository root.
- **Deterministic output**: All pipeline stages must produce deterministic output for identical inputs.

## Agent Coordination References
- Coordinator: [.agents/skills/agent-coordination/SKILL.md](.agents/skills/agent-coordination/SKILL.md)
- Parallelism: [.agents/skills/agent-coordination/PARALLEL.md](.agents/skills/agent-coordination/PARALLEL.md)

## Standard Workflow Loop
All development must follow this standard "plan → execute → review" loop:
1. **Plan**: Select/propose issues using GitHub templates (`coding`, `perf`, `agent`).
2. **Execute**: Create/update a plan file, write code, and make minimal, atomic commits with `ai-commit.sh`.
3. **Review**: Verify with `scripts/quality_gate.sh` and workspace tests before submission.

## Template Sync
| Pattern | Status | Notes |
| --------- | ------ | ----- |
| Gitleaks Scan | Adopted | `.gitleaks.toml` is used for pre-commit checks |
| Named Constants | Adopted | Specified in the fenced `bash readonly` block above |
| Single Source Version | Gap | `agents-docs/VERSION.md` is missing (no `agents-docs/` dir in repo) |
| `MAX_LINES_AGENTS_MD` | Adopted | Enforced at 150 lines |
| Skill Frontmatter | Adopted | Verified in all `.agents/skills/*.md` files |
| `ai-commit.sh` | Adopted | Script exists in `scripts/ai-commit.sh` |
| `update-all-docs.sh` | Adopted | Script exists in `scripts/update-all-docs.sh` |
| Agent Config Dirs | Adopted | `.jules/`, `.opencode/`, `.qwen/` are present |
