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
| `src/cli.rs` | CLI argument parsing via clap |
| `src/config.rs` | Configuration, profiles, merge options |
| `src/review.rs` | Review player HTML generation |
| `src/error.rs` | Error types and TimelineError |
| `src/io/` | JSON read/write utilities |
| `src/verification/` | Verification analysis, fingerprinting, extractor |
| `scripts/` | Quality gate, benchmarks, validation, optimization |
| `tests/` | Integration tests |
| `benches/` | Criterion benchmarks (pipeline, analysis) |
| `plans/` | ADRs, roadmaps, and status reports |
| `.agents/skills/` | Reusable skill playbooks |
| `testdata/` | All test fixtures and generated test media |
| `config/` | VAD profiles (modern-optimized, legacy-optimized) |
| `.github/` | CI workflows |
| `analysis/` | Benchmark artifacts, validation reports, thresholds |
| `schema/` | JSON schema for timeline output |

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
- `codacy`: [.agents/skills/codacy/SKILL.md](.agents/skills/codacy/SKILL.md)
- `triz-analysis`: [.agents/skills/triz-analysis/SKILL.md](.agents/skills/triz-analysis/SKILL.md)
- `triz-solver`: [.agents/skills/triz-solver/SKILL.md](.agents/skills/triz-solver/SKILL.md)

## Rules
- **Verification**: `bash scripts/quality_gate.sh` must pass with zero warnings.
- **Lint and typecheck**: Always run `cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings` before committing.
- **Test command**: Run `cargo test` alongside quality gate.
- **No unwrap() or expect()** in `src/`. Use `Result` and `?`.
- **Atomic Commits**: Use `bash scripts/ai-commit.sh`.
- **MAX_SOURCE_FILE_LOC**: Limit Rust source files to 500 lines.
- **No magic numbers**: Extract to `config.rs` or module-level constants.
- **Media Sourcing**: Use legally redistributable media only (Blender/Open Movies).
- **Secret Scanning**: Gitleaks enforcement via `.gitleaks.toml`.
- **16-bit PCM WAV only**: Direct reader supports only 16-bit PCM WAV; all other formats require ffmpeg on PATH.
- **Deterministic output**: All pipeline stages must produce deterministic output for identical inputs.
- **No test/dummy/runtime files in root**: Never commit `dummy.*`, `*.wav`, `merged.json`, `timeline.json`, `verified.json`, or any other test fixture, template, or runtime-output file to the repository root. All such files belong in `testdata/` (fixtures), `analysis/` (outputs), or are listed in `.gitignore`.

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
- [.agents/skills/agent-coordination/SKILL.md](.agents/skills/agent-coordination/SKILL.md)
- [.agents/skills/agent-coordination/PARALLEL.md](.agents/skills/agent-coordination/PARALLEL.md)
