# AGENTS.md

## Named Constants

| Constant | Value | Purpose |
|----------|-------|---------|
| `MAX_SOURCE_FILE_LOC` | 500 | Maximum lines of code per Rust source file |
| `DEFAULT_SAMPLE_RATE_HZ` | 16000 | Standard audio processing sample rate |
| `DEFAULT_FRAME_MS` | 20 | Frame window duration in milliseconds |
| `DEFAULT_ENERGY_THRESHOLD` | 0.015 | RMS threshold for speech detection |
| `MIN_NON_VOICE_MS` | 1000 | Minimum non-voice segment duration |
| `MIN_SPEECH_MS` | 120 | Minimum speech segment duration |
| `SPEECH_HANGOVER_MS` | 300 | Post-speech hangover buffer |

## Environment Variables

| Variable | Overrides | Example |
|----------|-----------|---------|
| `TIMELINE_SAMPLE_RATE` | `DEFAULT_SAMPLE_RATE_HZ` | `16000` |
| `TIMELINE_FRAME_MS` | `DEFAULT_FRAME_MS` | `20` |
| `TIMELINE_MIN_SPEECH_MS` | `MIN_SPEECH_MS` | `120` |
| `TIMELINE_MIN_SILENCE_MS` | `MIN_NON_VOICE_MS` | `1000` |
| `TIMELINE_ENERGY_THRESHOLD` | `DEFAULT_ENERGY_THRESHOLD` | `0.015` |
| `RUST_LOG` | Tracing log level | `info`, `debug` |

## Setup

```bash
cargo build
bash scripts/fetch_test_assets.sh  # optional: download smoke media
```

## Quality Gate

Run before every commit. All checks must pass with zero warnings.

```bash
# Full gate (fmt + clippy + test):
bash scripts/quality_gate.sh

# Individual checks:
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test

# Benchmark smoke:
bash scripts/benchmark.sh
python3 scripts/check_benchmark_regression.py --baseline analysis/benchmarks/latest.json --candidate analysis/benchmarks/latest.json
```

## Dependency Hygiene

- Dependabot is configured for weekly Cargo and GitHub Actions updates.
- Dependabot PRs are auto-merge enabled after required checks pass.

## Standard Validation Workflow (Always Use)

Use this exact sequence for every non-trivial change:

```bash
# 1) Sync fixtures and deterministic inputs
bash scripts/fetch_test_assets.sh

# 2) Local quality gate (must be warning-free)
bash scripts/quality_gate.sh

# 3) Benchmark artifact generation + regression check
bash scripts/benchmark.sh testdata/raw/sintel_trailer_2010.mp4 analysis/benchmarks/ci.json
python3 scripts/check_benchmark_regression.py --baseline analysis/benchmarks/latest.json --candidate analysis/benchmarks/ci.json

# 4) Bench harness (performance visibility)
cargo bench --bench pipeline_bench -- --noplot

# 5) Atomic commit and push
git add -A
git commit -m "<type(scope): message>"
git push

# 6) CI monitor to completion
gh run watch --exit-status
```

Required outcome before considering work complete:
- no clippy warnings
- no failing tests
- benchmark regression check passes
- GitHub Actions `Quality Gate` passes

## Commit Policy

Use atomic commits via the `atomic-commit` skill or manual equivalent.

```bash
# Preferred: use the atomic-commit skill
/atomic-commit

# Manual: validate then commit
bash scripts/quality_gate.sh && git add -A && git commit
```

- Conventional commit format: `type(scope): description`
- Types: `feat`, `fix`, `docs`, `refactor`, `test`, `perf`, `ci`, `chore`
- Every commit must pass the full quality gate before push.

## Pre-existing Issue Policy

**Fix ALL pre-existing issues before completing any task.**

- Lint warnings: fix immediately.
- Test failures: fix immediately.
- Clippy warnings: fix immediately.
- Security vulnerabilities: fix or document with justification.
- If a fix is not possible, document the issue in `plans/050-status-report/STATUS.md`
  with file path, line number, reason it cannot be fixed, and a tracking reference.
- Never use `#[allow(...)]` to silence warnings without a comment explaining why.
- Never skip, ignore, or defer. Document and fix is the rule.

### Agent and Skill Enforcement

- Apply this policy to code, CI workflows, scripts, and skill docs.
- If an issue is fixable in the current task scope, fix it before completion.
- If not currently fixable, document it immediately in `plans/050-status-report/STATUS.md`
  with owner, impact, and next action.
- Do not ship new instructions or skills that normalize leaving warnings unresolved.

## Code Style

### Rust Rules
- Edition 2021, stable toolchain only.
- `cargo fmt` enforced (rustfmt defaults).
- `cargo clippy -D warnings` -- zero warnings policy.
- No `unwrap()` or `expect()` in production code (`src/`). Use `?` or `anyhow`.
- `unwrap()` is allowed only in `tests/` and `benches/`.
- No magic numbers. Extract to named constants in `config.rs` or module-level `const`.
- No hardcoded settings or threshold values inline. All configurable values live in
  `config.rs` and are overridable via CLI flags or environment variables.
- Maximum 500 lines of code per source file. Split into submodules when approaching limit.
- Prefer explicit error types over `anyhow` at module boundaries.
- All pipeline functions must be deterministic: same input produces same output.
- Semantic error mapping: `io::Error` maps to `IoError`, not `InvalidConfig`.

### File Organization
- One public type per file when the type has significant implementation.
- Group related functions into `impl` blocks, not free functions.
- Use `mod.rs` only for re-exports; logic goes in named files.

### Testing
- Deterministic tests only. No timing-dependent or network-dependent tests.
- Synthetic fixtures generated via `gen-fixtures` command. No committed binary test data.
- Integration tests in `tests/`. Unit tests in same file as code (`#[cfg(test)]`).
- Test names describe the behavior: `silent_input_produces_no_segments`.

## Repository Map

| Directory | Purpose |
|-----------|---------|
| `src/` | Production CLI and pipeline code |
| `src/pipeline/` | Core pipeline stages (VAD, framing, segmentation, features, tags, prompts) |
| `src/pipeline/vad/` | VAD engine trait and current energy implementation |
| `src/io/` | Audio I/O (WAV reader, ffmpeg decoder) |
| `src/learning/` | Calibration and profile system |
| `src/types/` | Shared types (Frame, Segment, Metrics, FeatureSet) |
| `src/validation/` | Validation layers (synthetic, dataset CSV, SRT subtitles) |
| `tests/` | Integration tests |
| `benches/` | Benchmark harness (stub -- not yet functional) |
| `scripts/` | Quality gate, benchmark, test asset, and skill validation scripts |
| `testdata/` | Generated fixtures and raw test assets (gitignored) |
| `plans/` | ADRs, implementation phases, validation criteria, status reports |
| `analysis/` | Recon notes, quality reports, benchmarks, learnings |
| `reports/` | Validation reports output |
| `.agents/skills/` | Reusable skill playbooks for agent workflows |
| `.github/workflows/` | CI pipeline (GitHub Actions) |

## Domain Concepts

- **Frame**: 20ms audio window with computed RMS energy (320 samples at 16kHz).
- **VAD**: Voice Activity Detection -- classifies frames as speech or non-voice.
- **Segment**: Contiguous time range with kind (Speech/NonVoice), confidence, tags, prompt.
- **Smoothing**: Hangover + flicker removal applied to raw VAD output.
- **Inversion**: Converting speech segments to their complement (non-voice segments).
- **Tag**: Acoustic category (ambience, music_bed, impact_heavy, machinery_like, crowd_like, nature_like).
- **Prompt**: Short deterministic text generated for eligible non-voice segments.
- **Calibration profile**: Genre-specific energy threshold delta (action, documentary, animation, drama).

## Architecture Decisions

Documented in `plans/010-architecture/`:
- ADR-001: Core pipeline (decode -> resample -> frame -> VAD -> smooth -> invert -> JSON).
- ADR-002: Deterministic energy-based VAD as default; trait-friendly for future engines.
- ADR-003: Stable JSON contract with snake_case enums, deterministic field ordering.
- ADR-004: Offline calibration only via explicit correction records and versioned reports.

## Rules

- Deterministic outputs: same input must produce identical JSON across runs.
- No heavy ML unless benchmark-justified against energy-based baseline.
- No runtime network access. All processing is offline and CPU-only.
- Store generated analysis artifacts under `analysis/`.
- Post-task learning notes go under `analysis/learnings/` and stay concise.
- When stuck: ask a clarifying question or propose a short plan. Do not guess.
- Before adding new code, search for existing similar code to avoid duplication.
- ffmpeg must be on PATH for non-WAV media decoding.
- WAV reader currently supports 16-bit PCM only; other formats fall through to ffmpeg.
- Prefer post-2000 downloaded movie assets in `testdata/raw/` for smoke tests, validation, and benchmarks when present; keep older fixtures only as fallback compatibility and keep deterministic generated-audio fallback when absent.

## Known Limitations

Track in `plans/050-status-report/STATUS.md`. Current items:
- Benchmark regression checks use the checked-in real-media baseline, so intentional performance shifts should update `analysis/benchmarks/latest.json`.
