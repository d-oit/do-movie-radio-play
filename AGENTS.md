# AGENTS.md

## Named Constants
```bash readonly
DEFAULT_SAMPLE_RATE_HZ=16000
DEFAULT_FRAME_MS=20
MAX_SOURCE_FILE_LOC=500
MAX_LINES_AGENTS_MD=150
```

## Secret Scanning Policy
Gitleaks secret scanning is enforced via `.gitleaks.toml`. Never commit secrets or credentials.

## Version Policy
The `VERSION` file in the root is the single source of truth. Never edit version strings inline in source files.

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
| Dev Env Setup | `bash scripts/setup-dev-env.sh` |
| Build | `cargo build --workspace` |
| Test | `cargo test --workspace` |
| Quality Gate | `RUSTUP_TOOLCHAIN=stable-x86_64-unknown-linux-gnu bash scripts/quality_gate.sh` |
| Docs Update | `bash scripts/update-all-docs.sh` |
| Commit | `bash scripts/ai-commit.sh` |

## Rules
- **Verification**: `bash scripts/quality_gate.sh` must pass with zero warnings.
- **Lint**: Always run `cargo fmt --check && cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- **Atomic Commits**: Use `bash scripts/ai-commit.sh` or `bash scripts/quality_gate.sh && git add -A && git commit`.
- **No unwrap() or expect()** in `crates/*/src/`. Use `Result` and `?`.
- **MAX_SOURCE_FILE_LOC**: Limit Rust source files to 500 lines.
- **Secret Scanning**: Gitleaks enforcement via `.gitleaks.toml`.
- **Root Cleanliness**: Never commit test fixtures or runtime-output files to the repository root.
- **Deterministic output**: All pipeline stages must produce deterministic output for identical inputs.
- **Pre-existing issues**: Address pre-existing warnings or document in `plans/FOLLOWUPS.md`.
- **Narration content**: Any code generating narration/gap-filling text must describe actual detected content (grounded in segment tags/context) — never content-free filler (e.g. bare "Stille."/"Pause."). See [ADR-128](plans/adr/0128-audio-description-standards.md) and [`.agents/skills/audio-description-writer/SKILL.md`](.agents/skills/audio-description-writer/SKILL.md).

## Skill Activation Policy
- Non-voice segmentation: Load [`.agents/skills/nonvoice-segmentation/SKILL.md`](.agents/skills/nonvoice-segmentation/SKILL.md)
- CPU VAD parameters: Load [`.agents/skills/audio-vad-cpu/SKILL.md`](.agents/skills/audio-vad-cpu/SKILL.md)
- Self-learning calibration: Load [`.agents/skills/self-learning-calibration/SKILL.md`](.agents/skills/self-learning-calibration/SKILL.md)

## Agent Coordination References
- [.agents/ORCHESTRATION.md](.agents/ORCHESTRATION.md)
- [.agents/skills/agent-coordination/SKILL.md](.agents/skills/agent-coordination/SKILL.md)
- [.agents/skills/agent-coordination/PARALLEL.md](.agents/skills/agent-coordination/PARALLEL.md)

## Standard Workflow Loop
All human and agent-driven development must follow this standard "plan → execute → review" loop:
1. **Plan**: Propose/select an issue before editing code using GitHub issue templates.
2. **Execute**: Create/update the plan, then write code in minimal, atomic commits using `scripts/ai-commit.sh`.
3. **Review**: Ensure general correctness and verify code by running `scripts/quality_gate.sh` and workspace tests.

## Active Learning & Calibration Loop
For any calibration/VAD verification task:
- Always check priority review candidates first using active learning filters.
- Ensure profile changes are registered as experiments with unique `profile_id` and incremented `version` fields.

## Triage
Maintain zero open issues/PRs. A weekly reminder workflow (`.github/workflows/triage-reminder.yml`) flags stale items — triage them promptly.

## Template Sync
| Pattern | Status | Notes |
| --------- | ------ | ----- |
| Secret Scanning Enforcement | Adopted | `.gitleaks.toml` present |
| Named Constants | Adopted | `bash readonly` block above |
| `./scripts/ai-commit.sh` Helper | Adopted | Available in `scripts/ai-commit.sh` |
| `agents-docs/VERSION.md` Single-Source | Adopted | Root `VERSION` file is single source of truth |
| `MAX_LINES_AGENTS_MD=150` Guard | Adopted | Enforced at 150 lines (currently <= 110) |
| Skill Frontmatter Rule | Adopted | Verified in all `.agents/skills/*/SKILL.md` |
| Agent Config Dirs | Adopted | `.jules/`, `.opencode/`, `.qwen/` present |
| `update-all-docs.sh` | Adopted | Available in `scripts/update-all-docs.sh` |
