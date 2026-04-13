# AGENTS.md

## Constants
- Default sample rate: 16000 Hz
- Default frame: 20 ms
- Min non-voice: 1000 ms
- Max Rust file size: 500 LOC

## Setup
- `cargo build`
- `bash scripts/fetch_test_assets.sh` (optional for smoke media)

## Quality gate
- `bash scripts/quality_gate.sh`
- `bash scripts/benchmark.sh testdata/generated/alternating.wav`

## Repository map
- `src/` production CLI and pipeline
- `.agents/skills/` reusable skill playbooks
- `plans/` ADRs, phases, validation
- `analysis/` recon, quality notes, benchmarks, learnings

## Rules
- Keep deterministic outputs and tests.
- No heavy ML unless benchmark-justified.
- Store generated analysis artifacts under `analysis/`.
- Post-task learning notes go under `analysis/learnings/` and stay concise.
