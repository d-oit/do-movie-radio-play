# Implementation Status Report

**Date:** 2026-04-14

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

Dependency security note (GitHub Dependabot, 2026-04-16):
- Open advisories on default branch: 5 (1 moderate, 4 low)
- Tracking URL: `https://github.com/d-oit/do-movie-radio-play/security/dependabot`
- Impact: dependency hygiene risk; no runtime exploit confirmed in current offline CLI usage
- Next action: triage and patch dependency versions in next dependency-maintenance pass

GitHub queue scan (2026-04-16):
- Open issues: 0
- Open PRs: 0
- Recent merged PRs: #1, #3, #4 (baseline hardening and validation infrastructure)

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

## Documentation and Tooling Gaps (2026-04-18)
The following patterns from `d-o-hub/github-template-ai-agents` are missing:
- `./scripts/ai-commit.sh`: Helper for atomic commits is missing.
- `./scripts/update-all-docs.sh`: Tool for synchronizing all documentation is missing.
- `.jules/`, `.opencode/`, `.qwen/`: Agent-specific configuration directories are missing.
- `.gitleaks.toml`: Gitleaks configuration for secret scanning is missing.
