# Implementation Status Report

**Date:** 2026-06-08

## Phase Status Summary

| Phase | Description | Status | Notes |
|-------|------------|--------|-------|
| 01 | JSON-only pipeline | COMPLETE | Deterministic extract pipeline verified |
| 02 | Acoustic tags | COMPLETE | Rule-based tags with spectral features |
| 03 | Prompt generation | COMPLETE | Config passthrough and tag mappings are wired |
| 04 | Self-learning | COMPLETE | Calibration now writes a report and updates the active profile automatically |
| 05 | Hardening and quality | COMPLETE | Validation/config, WAV fallback, VAD fail-fast, and benchmark CI regression checks are complete |
| 06 | New capabilities | READY | Next feature should build on existing profile/tag infrastructure |

## Current Missing Implementations

No medium-or-higher runtime gaps are currently open in the shipped CLI flow.

## Quality Issues

No active hardening gaps are currently open beyond future feature work.

Dependency security (GitHub Dependabot):
- ✅ **HIGH**: GHSA-82j2-j2ch-gfr8 in rustls-webpki — resolved via PR #61 (2026-05-23), which disabled libsql TLS, removing 80+ transitive deps
- ✅ Remaining moderate/low advisories accepted — no active code paths exercise the vulnerable functionality (CRL checking is not enabled)
- Tracking URL: `https://github.com/d-oit/do-movie-radio-play/security/dependabot`

GitHub queue scan (2026-06-07):
- Open issues: 6 (#72-#77), all filed 2026-06-07 as refactor audit items
- Open PRs: 0 (PR #78 merged)
- Recent merged PRs: #61-#65 (GOAP closeout, 2026-05-23), #66-#71 (post-GOAP hardening), #78 (refactor extraction)

Repository integrity sweep (2026-04-18):
- All `.github/workflows/*.yml` files parse and are registered in GitHub Actions.
- Markdown local-link audit passes after fixing stale skill-reference relative paths.

## Production Evaluation Correctness

- Manifest-based eval coverage is now implemented in:
  - `testdata/validation/manifest.json` (general)
  - `testdata/validation/radio-play-manifest.json` (release-readiness)
- Tier A coverage is enforced in PR CI via `.github/workflows/ci.yml`.
- Scheduled full sweeps are implemented in `.github/workflows/validation-sweep.yml`.
- Coverage and artifact integrity checks are enforced by `scripts/check_validation_coverage.py`.
- Full manifest execution and summary emission are implemented in
  `scripts/run_validation_manifest.py` and `analysis/validation/radio-play-sweep-summary.json` for release-readiness.

## Completed Since Earlier Plan Drafts

- Prompt generation now honors `AnalysisConfig` in `src/pipeline/prompts.rs`.
- `crowd_like` and `machinery_like` have distinct prompt mappings.
- Segment confidence is derived from frame likelihoods in `src/pipeline/segmenter.rs`.
- `validate` and `bench` now accept runtime config, threshold, engine, and calibration inputs.
- `calibrate` now closes the loop by writing a report and updating the active calibration profile.
- The CLI now exposes both `energy` and `spectral` VAD engines.
- Unsupported WAV direct decodes now fall back to `ffmpeg`.
- Config and env override validation now fail clearly on malformed values and invalid ranges.
- Dataset manifest parsing now fails fast on malformed rows instead of manufacturing timestamps.
- `io::Error` is mapped semantically instead of being surfaced as config failure.
- JSON schema validation exists via `schema/timeline.schema.json` and
   `tests/json_contract.rs`.
- Criterion benchmarks are configured in `Cargo.toml` and implemented in
   `benches/pipeline_bench.rs`.
- CI now runs benchmark smoke, compares against the checked-in real-media baseline, and uploads benchmark artifacts.
- Frame construction and VAD already use spectral features through
   `src/types/frame.rs` and `src/pipeline/framing.rs`.
- Production eval governance is now codified in
  `plans/040-validation/PRODUCTION-EVALS.md` and
  `plans/040-validation/ACCEPTANCE.md`.
- Hybrid VAD engine committed and benchmarked via GOAP closeout PRs #62, #65.
- SpectralVad added to Criterion benchmarks; rubato high-quality resampling behind feature flag.

## Documentation and Tooling Gaps

The following patterns from `d-o-hub/github-template-ai-agents` are acknowledged but deferred:

| Pattern | AGENTS.md Decision | Rationale |
|---------|-------------------|-----------|
| `scripts/ai-commit.sh` | Deferred | Not needed — `quality_gate.sh` covers pre-commit checks |
| `scripts/update-all-docs.sh` | Deferred | Not needed — no generated docs to sync |
| Agent config dirs | Deferred | Not used — tools read AGENTS.md directly |

These were formally acknowledged in the AGENTS.md Template Sync table with `Deferred` status
and explicit rationale. Gitleaks scanning (`root .gitleaks.toml`) and skill frontmatter
(`.agents/skills/`) are already adopted.

## Recent Changes

### GOAP Closeout (2026-05-23)

Resolved 6 open issues via PRs #61-#65:

| # | Issue | PR | What |
|---|-------|----|------|
| 1 | #54/#58 | [#61](https://github.com/d-oit/do-movie-radio-play/pull/61) | Remove vulnerable rustls-webpki 0.102.8 by disabling libsql TLS |
| 2 | #55 | — | Hybrid VAD engine (already in main, d351f66) |
| 3 | #56 | [#62](https://github.com/d-oit/do-movie-radio-play/pull/62) | Add SpectralVad + HybridVad to Criterion benchmarks |
| 4 | #57 | [#63](https://github.com/d-oit/do-movie-radio-play/pull/63) | Profile-driven tag calibration (Phase 6.1) |
| 5 | #59 | [#64](https://github.com/d-oit/do-movie-radio-play/pull/64) | Close manifest coverage gap |
| 6 | #60 | [#65](https://github.com/d-oit/do-movie-radio-play/pull/65) | High-quality resampling via rubato (feature flag) |

### Post-GOAP Hardening (2026-05-26 to 2026-06-04)

- PR #66: Optimize spectral analysis (avoid hypot, fuse loops)
- PR #67: Add Codacy agent skill adapted for Rust project
- PR #68: DoS-resistant audio resampler error handling
- PR #69: Rewrite README.md and update AGENTS.md for agent-readiness
- PR #70: Thread-local FFT planning cache
- PR #71: XSS protection + CSP in review report

### Radio-Play 95% Milestone Progress

- **Milestone A**: ✅ Complete — entropy fix + graph-inspired verifier integration
- **Milestone B**: ✅ Complete — holdout scoring scripts, CI gate, failure breakdown, readiness reports, bounded merge behavior, tri-state smoothing
- **Milestone C**: ⏸️ Deferred — see `plans/100-radio-play-95/MILESTONE-C-DECISION.md`

Current best holdout metrics: precision=0.9988, recall=1.0000, overlap=0.9994
Modern precision ceiling: ~0.7368 (bounded ceiling check confirmed)

### PR #78 — Refactor Extraction (2026-06-08)

PR #78 extracted major modules to improve organization:

- `src/handlers.rs` — 740 lines of CLI command handlers from main.rs (main.rs: 1048→269 LOC)
- `src/review_template.rs` — 655 lines of HTML+JS template from review.rs (review.rs: 928→296 LOC)
- `src/merge.rs` — MergeOptions, MergeStrategy enum, load helpers
- `src/util.rs` — init_logging, open_in_browser, get_calibration_dir
- `MergeStrategy` enum replaces stringly-typed merge strategy literals

### Current Open Issues (#72-#77)

All opened 2026-06-07 as refactor audit items. Status as of 2026-06-08:

| # | Title | Status | Resolution |
|---|-------|--------|------------|
| 72 | Split main.rs (1,048 LOC) | ✅ **CLOSED** | main.rs is now 233 LOC (below 500 limit) after PR #78 + handler extraction |
| 73 | MergeStrategy enum + named tolerance constants | ✅ **CLOSED** | `MergeStrategy` enum in `config.rs`; named constants `TOLERANCE_SYNTHETIC_MS`, `TOLERANCE_DATASET_MS`, `TOLERANCE_DEFAULT_MS` in `util.rs`; `URL_REVOKE_TIMEOUT_MS` in `review_template.rs` |
| 74 | Extract HTML/JS template from review.rs | ✅ **CLOSED** | Template extracted to `templates/review.html`, loaded via `include_str!` in `review_template.rs`. Review.rs is 296 LOC. |
| 75 | Replace tokio::runtime with #[tokio::main] | ✅ **CLOSED** | Main dispatch is now synchronous; no `tokio::runtime::Builder` in `main.rs` |
| 76 | Close AGENTS.md template sync gaps | ⏸️ **DEFERRED** | Acknowledged in AGENTS.md with rationale — no action taken |
| 77 | Extract CLI command dispatch to dedicated files | ✅ **CLOSED** | Dispatch extracted to `handlers.rs` (740 LOC); main.rs is 233 LOC |

## Open Action Items

1. **#76 AGENTS.md gaps**: Deferred by design — gap rows acknowledged with rationale in AGENTS.md Template Sync table
2. **Milestone C (ONNX verifier)**: Deferred — engine-level speech/non-speech discrimination improvement is the next recommended step per ROADMAP.md
3. **Review player UX bugs**: 4 minor issues remain unfiled (see `plans/070-review-player-testing/UNRESOLVED-ISSUES.md`)
4. **tokio::runtime refactor (handlers.rs)**: 3 manual `tokio::runtime::Builder::new_current_thread()` constructions remain in `handlers.rs` for learning database operations — these require async and are not easily replaced with `#[tokio::main]` in the current architecture
