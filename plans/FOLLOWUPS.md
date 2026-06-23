# Followup Issues

Pre-existing issues encountered during implementation runs that could not be fixed in scope.
Each entry includes file path, description, priority, and suggested approach.

**Created:** 2026-06-23
**Updated:** 2026-06-23 — All LOC violations resolved

## Resolved

All 4 LOC violations have been resolved by splitting files into submodules:

| Original File | Lines | Resolution | New Files |
|---------------|-------|------------|-----------|
| `movie-radio-learning/src/database.rs` | 810 | Split into module directory | `mod.rs` (96), `types.rs` (74), `queries.rs` (237), `migration.rs` (92), `tests.rs` (334) |
| `movie-radio-verification/src/verification/mod.rs` | 565 | Extracted determine.rs + tests | `mod.rs` (407), `determine.rs` (89), `tests.rs` (80) |
| `movie-radio-pipeline/src/pipeline/segmenter.rs` | 541 | Split into module directory | `mod.rs` (182), `speech.rs` (171), `nonvoice.rs` (150), `merge.rs` (62) |
| `movie-radio-pipeline/src/pipeline/mod.rs` | 527 | Extracted filters.rs + benchmark.rs | `mod.rs` (475), `filters.rs` (41), `benchmark.rs` (25) |

Also fixed: `scripts/quality_gate.sh` LOC scan updated from `src/` to `crates/` (post-restructure).
