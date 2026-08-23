# Followup Issues

Pre-existing issues encountered during implementation runs that could not be fixed in scope.
Each entry includes file path, description, priority, and suggested approach.

**Created:** 2026-06-23
**Updated:** 2026-08-23 — Added dead root `src/` tree finding (PR review sweep)

## Open

| File/Path | Description | Priority | Suggested Approach |
|-----------|-------------|----------|-------------------|
| `src/` (repo root) | Dead legacy tree from pre-workspace layout (`main.rs`, `lib.rs`, `pipeline/`, `voice/`, `verification/`, …). Referenced by **no** package manifest — workspace members are `crates/*` + `benchmarks`; `movie-radio-timeline`'s `[[bin]] path = "src/main.rs"` resolves inside its own crate dir. Contains stale duplicates of live logic (e.g. old `compute_rms`/`compute_zcr`) that can drift silently and mislead readers/tools. | Medium | Delete the root `src/` tree in a dedicated PR after confirming no external references (scripts, CI, docs). |
| `crates/movie-radio-goap/src/actions.rs:234-238` | When every narration fails TTS/validation, `SynthesizeNarrator::execute` still returns `Ok(())` and the run completes exit-0 with a narration-less play (`narrator_voice_synthesized = true`). The #206 validate guard adds a deterministic trigger (e.g. out-of-range sample rate skips all items with WARN logs only). | Medium | Bail with context when `scripts` is non-empty but `narration_audio` ends empty; or surface a degraded-status flag into the run report. |
| `crates/movie-radio-goap/src/actions.rs:279` | `scripts.iter().zip(ctx.narration_audio.iter())` misaligns script→audio pairing when a middle item is skipped (pre-existing; failure path had it before #206). | Medium | Track surviving script indices alongside audio, or zip over `(script, Option<Audio>)`. |
| `crates/movie-radio-voice/src/config.rs:86` (timeline) | `TIMELINE_SAMPLE_RATE` env override validated only as `> 0`; currently never reaches the radio-play synthesis path (hardcoded `AnalysisConfig::default()`), but if wired later an out-of-range value would fail per-request at `SynthesisRequest::validate()` after config acceptance. | Low | Validate 8_000..=48_000 in `apply_env_overrides` for loud early failure. |
| `crates/movie-radio-voice/src/voice/mod.rs` (capabilities) | Global 10k text cap exceeds per-provider `capabilities().max_text_length` (kokoro 1000, qwen3 2000, orpheus 4000, openai 4096, modal 5000); only ElevenLabs matches 10000. Requests near the global cap will fail late inside providers. | Low | Enforce per-provider cap at dispatch using existing capabilities data. |

## Resolved

All 4 LOC violations have been resolved by splitting files into submodules:

| Original File | Lines | Resolution | New Files |
|---------------|-------|------------|-----------|
| `movie-radio-learning/src/database.rs` | 810 | Split into module directory | `mod.rs` (96), `types.rs` (74), `queries.rs` (237), `migration.rs` (92), `tests.rs` (334) |
| `movie-radio-verification/src/verification/mod.rs` | 565 | Extracted determine.rs + tests | `mod.rs` (407), `determine.rs` (89), `tests.rs` (80) |
| `movie-radio-pipeline/src/pipeline/segmenter.rs` | 541 | Split into module directory | `mod.rs` (182), `speech.rs` (171), `nonvoice.rs` (150), `merge.rs` (62) |
| `movie-radio-pipeline/src/pipeline/mod.rs` | 527 | Extracted filters.rs + benchmark.rs | `mod.rs` (475), `filters.rs` (41), `benchmark.rs` (25) |

Also fixed: `scripts/quality_gate.sh` LOC scan updated from `src/` to `crates/` (post-restructure).
