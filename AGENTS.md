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
| `crates/movie-radio-pipeline/` | VAD, framing, segmentation, features, tags, prompts |
| `crates/movie-radio-voice/` | TTS providers (Kokoro, Orpheus, ElevenLabs, etc.) |
| `crates/movie-radio-goap/` | GOAP planner, actions, orchestrator, gaps, narrate, assemble |
| `crates/movie-radio-learning/` | Calibration, adaptive thresholds, libsql database |
| `crates/movie-radio-verification/` | Spectral verification, fingerprinting, extractor |
| `crates/movie-radio-io/` | JSON, EDL, VTT, WAV I/O utilities |
| `crates/movie-radio-validation/` | Validation, comparison, SRT parsing, synthetic fixtures |
| `crates/movie-radio-timeline/` | Binary crate (CLI, handlers, config) |
| `benchmarks/` | Criterion benchmarks |
| `scripts/` | Quality gate, benchmarks, validation, optimization |
| `tests/` | Integration tests |
| `plans/` | ADRs, roadmaps, and status reports |
| `.agents/skills/` | Reusable skill playbooks |
| `testdata/` | All test fixtures and generated test media |
| `config/` | VAD profiles (modern-optimized, legacy-optimized) |
| `.github/` | CI workflows |
| `analysis/` | Benchmark artifacts, validation reports, thresholds |
| `schema/` | JSON schema for timeline output |

## Quick Reference
| Task | Command |
|------|---------|
| Build | `cargo build --workspace` |
| Test | `cargo test --workspace` |
| Lint | `cargo clippy --workspace --all-targets --all-features -- -D warnings` |
| Format | `cargo fmt --all -- --check` |
| Quality Gate | `bash scripts/quality_gate.sh` |

## Domain Concepts
- **Frame**: 20ms audio window (320 samples at 16kHz).
- **VAD**: Voice Activity Detection classifying frames as speech or non-voice.
- **Segment**: Contiguous time range with kind, confidence, tags, and prompt.
- **Learning Database**: Stores `verified_segments` with spectral features for efficient SQL aggregation.

## Skill Activation Policy
- `nonvoice-segmentation`: [.agents/skills/nonvoice-segmentation/SKILL.md](.agents/skills/nonvoice-segmentation/SKILL.md)
- `audio-vad-cpu`: [.agents/skills/audio-vad-cpu/SKILL.md](.agents/skills/audio-vad-cpu/SKILL.md)
- `self-learning-calibration`: [.agents/skills/self-learning-calibration/SKILL.md](.agents/skills/self-learning-calibration/SKILL.md)
- `agent-coordination`: [.agents/skills/agent-coordination/SKILL.md](.agents/skills/agent-coordination/SKILL.md)
- `goap-agent`: [.agents/skills/goap-agent/SKILL.md](.agents/skills/goap-agent/SKILL.md)
- `codacy`: [.agents/skills/codacy/SKILL.md](.agents/skills/codacy/SKILL.md)
- `triz-analysis`: [.agents/skills/triz-analysis/SKILL.md](.agents/skills/triz-analysis/SKILL.md)
- `triz-solver`: [.agents/skills/triz-solver/SKILL.md](.agents/skills/triz-solver/SKILL.md)
- `metrics-reporter`: [.agents/skills/metrics-reporter/SKILL.md](.agents/skills/metrics-reporter/SKILL.md)
- `dora-report`: [.agents/skills/dora-report/SKILL.md](.agents/skills/dora-report/SKILL.md)

## Rules
- **Verification**: `bash scripts/quality_gate.sh` must pass with zero warnings.
- **Lint and typecheck**: Always run `cargo fmt --check && cargo clippy --workspace --all-targets --all-features -- -D warnings` before committing.
- **Test command**: Run `cargo test --workspace` alongside quality gate.
- **No unwrap() or expect()** in `crates/*/src/`. Use `Result` and `?`.
- **Atomic Commits**: Use `bash scripts/ai-commit.sh`.
- **MAX_SOURCE_FILE_LOC**: Limit Rust source files to 500 lines.
- **No magic numbers**: Extract to `config.rs` or module-level constants.
- **Media Sourcing**: Use legally redistributable media only (Blender/Open Movies).
- **Secret Scanning**: Gitleaks enforcement via `.gitleaks.toml`.
- **16-bit PCM WAV only**: Direct reader supports only 16-bit PCM WAV; all other formats require ffmpeg on PATH.
- **Deterministic output**: All pipeline stages must produce deterministic output for identical inputs.
- **No test/dummy/runtime files in root**: Never commit `dummy.*`, `*.wav`, `merged.json`, `timeline.json`, `verified.json`, or any other test fixture, template, or runtime-output file to the repository root. All such files belong in `testdata/` (fixtures), `analysis/` (outputs), or are listed in `.gitignore`.
- **Pre-existing issues**: When implementing a task, always address pre-existing warnings, lint errors, and quality gate failures encountered during the run. Fix them if possible within the current scope. If a fix is out of scope (e.g., large refactor, unrelated module), document it in `plans/FOLLOWUPS.md` with file path, description, and priority. Never leave pre-existing issues silently ignored.

## Template Sync
| Pattern | Status | Notes |
|---------|--------|-------|
| Gitleaks Scan | Adopted | `.gitleaks.toml` present |
| Named Constants | Adopted | `bash readonly` block above |
| Single Source Version | Adopted | `VERSION` file is the source of truth |
| `MAX_LINES_AGENTS_MD` | Adopted | Enforced at 150 lines |
| Skill Frontmatter | Adopted | Verified in all `.agents/skills/*.md` |
| Root Cleanliness | Adopted | No dummy/test/runtime files in root |
| `ai-commit.sh` | Adopted | Available in `scripts/` |
| `update-all-docs.sh` | Adopted | Available in `scripts/` |
| Agent Config Dirs | Adopted | Directories `.jules/`, `.opencode/`, `.qwen/` present |

## Agent Coordination References
- [.agents/ORCHESTRATION.md](.agents/ORCHESTRATION.md)
- [.agents/skills/agent-coordination/SKILL.md](.agents/skills/agent-coordination/SKILL.md)
- [.agents/skills/agent-coordination/PARALLEL.md](.agents/skills/agent-coordination/PARALLEL.md)
