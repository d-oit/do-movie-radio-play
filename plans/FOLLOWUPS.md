# Followup Issues

Pre-existing issues encountered during implementation runs that could not be fixed in scope.
Each entry includes file path, description, priority, and suggested approach.

**Created:** 2026-06-23
**Updated:** 2026-08-25 — Added findings from workspace-wide improvement analysis (`plans/130-improvement-analysis-2026-08-25.md`)

## Open

| File/Path | Description | Priority | Suggested Approach |
|-----------|-------------|----------|--------------------|
| `crates/movie-radio-pipeline/src/pipeline/speech_evidence.rs:43-47` | Slice-index panic when a segment timestamp exceeds the decoded frame count: `end_idx` becomes `start_idx + 1 > frames.len()` before slicing | P0 | Clamp like the twin implementation in `pipeline/segmenter/confidence.rs:53-58`: `end_idx.clamp(start_idx + 1, frames.len())`; add an out-of-range-timestamp regression test |
| `crates/movie-radio-voice/src/voice/openai.rs:91` | Sole library-code `.expect()` in the workspace (`last_err.expect("retry loop runs at least once")`); violates the AGENTS.md no-unwrap rule | P1 | Replace with typed error propagation (the invariant holds, but there must be no panic path) |
| `crates/movie-radio-voice/src/voice/modal.rs:54-63` | Response parsed as WAV via blind 44-byte header skip; assumes little-endian i16 mono PCM with no container validation | P2 | Validate RIFF/WAVE magic, format tag, and channel count; fall through the provider chain on mismatch |
| `crates/movie-radio-goap/src/gaps.rs:48,149,165` | `segments.len() - 1` would underflow on an empty slice and `seg.end_ms - seg.start_ms` on an inverted segment (unreachable via current callers; fragile private API) | P2 | Bail on empty slices; saturating subtraction for durations |

## Resolved

| File/Path | Description | Resolution |
|-----------|-------------|------------|
| `tests/` (repo root) | Orphaned integration tests from pre-workspace layout (`voice_integration_tests.rs` references the nonexistent `movie_nonvoice_timeline` package). Not compiled by any workspace member; misleading and unbuildable. | Deleted (13 files); stale CI `paths-filter` globs (`src/**`, `tests/**`) removed; broken `arch` sensor (referenced nonexistent `tests/arch_fitness.rs`) dropped from `scripts/harness-check.sh` |
| `src/` (repo root) | Dead legacy tree from pre-workspace layout (`main.rs`, `lib.rs`, `pipeline/`, `voice/`, `verification/`, …). Referenced by **no** package manifest — workspace members are `crates/*` + `benchmarks`. Contains stale duplicates of live logic (e.g. old `compute_rms`/`compute_zcr`). | Deleted (70 files, ~12k LOC); verified no references in scripts/, CI, benchmarks, docs |
| `movie-radio-goap/src/actions.rs` all-skipped semantics | All narrations failing returned `Ok(())` → exit-0 narration-less output | Bail when scripts exist and every synthesis failed |
| `movie-radio-goap/src/actions.rs:279` zip misalignment | Script→audio pairing shifted after middle-item failure | `narration_audio: Vec<Option<_>>` aligned with scripts by construction; assemble skips `None` |
| `movie-radio-voice` per-provider text caps | Global 10k cap exceeded provider capabilities, failing late inside providers | Orchestrator checks `capabilities().max_text_length` pre-dispatch and falls through the chain; goap direct-dispatch caller guards too |
| `crates/movie-radio-timeline/src/config.rs` env rate validation | `TIMELINE_SAMPLE_RATE` accepted any non-zero value; out-of-range surfaced only per-request later | Range-checked 8_000..=48_000 in `apply_env_overrides` |

## Resolved (historical)

All 4 LOC violations have been resolved by splitting files into submodules:

| Original File | Lines | Resolution | New Files |
|---------------|-------|------------|-----------|
| `movie-radio-learning/src/database.rs` | 810 | Split into module directory | `mod.rs` (96), `types.rs` (74), `queries.rs` (237), `migration.rs` (92), `tests.rs` (334) |
| `movie-radio-verification/src/verification/mod.rs` | 565 | Extracted determine.rs + tests | `mod.rs` (407), `determine.rs` (89), `tests.rs` (80) |
| `movie-radio-pipeline/src/pipeline/segmenter.rs` | 541 | Split into module directory | `mod.rs` (182), `speech.rs` (171), `nonvoice.rs` (150), `merge.rs` (62) |
| `movie-radio-pipeline/src/pipeline/mod.rs` | 527 | Extracted filters.rs + benchmark.rs | `mod.rs` (475), `filters.rs` (41), `benchmark.rs` (25) |

Also fixed: `scripts/quality_gate.sh` LOC scan updated from `src/` to `crates/` (post-restructure).
