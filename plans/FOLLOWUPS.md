# Followup Issues

Pre-existing issues encountered during implementation runs that could not be fixed in scope.
Each entry includes file path, description, priority, and suggested approach.

**Created:** 2026-06-23
**Updated:** 2026-08-23 — Added dead root `src/` tree finding (PR review sweep)

## Open

| File/Path | Description | Priority | Suggested Approach |
|-----------|-------------|----------|-------------------|
| `src/` (repo root) | Dead legacy tree from pre-workspace layout (`main.rs`, `lib.rs`, `pipeline/`, `voice/`, `verification/`, …). Referenced by **no** package manifest — workspace members are `crates/*` + `benchmarks`; `movie-radio-timeline`'s `[[bin]] path = "src/main.rs"` resolves inside its own crate dir. Contains stale duplicates of live logic (e.g. old `compute_rms`/`compute_zcr`) that can drift silently and mislead readers/tools. | Medium | Delete the root `src/` tree in a dedicated PR after confirming no external references (scripts, CI, docs). |
| `crates/movie-radio-timeline/src/config.rs:86` | `TIMELINE_SAMPLE_RATE` env override validated only as `> 0`; currently never reaches the radio-play synthesis path (hardcoded `AnalysisConfig::default()`), but if wired later an out-of-range value would fail per-request at `SynthesisRequest::validate()` after config acceptance. | Low | Validate 8_000..=48_000 in `apply_env_overrides` for loud early failure. |

## Resolved

| File/Path | Description | Resolution |
|-----------|-------------|------------|
| `movie-radio-goap/src/actions.rs` all-skipped semantics | All narrations failing returned `Ok(())` → exit-0 narration-less output | Bail when scripts exist and every synthesis failed |
| `movie-radio-goap/src/actions.rs:279` zip misalignment | Script→audio pairing shifted after middle-item failure | `narration_audio: Vec<Option<_>>` aligned with scripts by construction; assemble skips `None` |
| `movie-radio-voice` per-provider text caps | Global 10k cap exceeded provider capabilities, failing late inside providers | Orchestrator checks `capabilities().max_text_length` pre-dispatch and falls through the chain; goap direct-dispatch caller guards too |

## Resolved (historical)

All 4 LOC violations have been resolved by splitting files into submodules:

| Original File | Lines | Resolution | New Files |
|---------------|-------|------------|-----------|
| `movie-radio-learning/src/database.rs` | 810 | Split into module directory | `mod.rs` (96), `types.rs` (74), `queries.rs` (237), `migration.rs` (92), `tests.rs` (334) |
| `movie-radio-verification/src/verification/mod.rs` | 565 | Extracted determine.rs + tests | `mod.rs` (407), `determine.rs` (89), `tests.rs` (80) |
| `movie-radio-pipeline/src/pipeline/segmenter.rs` | 541 | Split into module directory | `mod.rs` (182), `speech.rs` (171), `nonvoice.rs` (150), `merge.rs` (62) |
| `movie-radio-pipeline/src/pipeline/mod.rs` | 527 | Extracted filters.rs + benchmark.rs | `mod.rs` (475), `filters.rs` (41), `benchmark.rs` (25) |

Also fixed: `scripts/quality_gate.sh` LOC scan updated from `src/` to `crates/` (post-restructure).
