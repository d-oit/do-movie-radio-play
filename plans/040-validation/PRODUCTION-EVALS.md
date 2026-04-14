# Production Evals Correctness Plan

Goal: make non-voice extraction correctness testable and repeatable on real media, not only synthetic fixtures.

## Scope

- Evaluate extraction accuracy with real subtitle-aligned inputs.
- Ensure every production fixture has an owned evaluation output path.
- Keep reports reproducible and easy to diff in CI.

## Current Coverage Snapshot

Raw media currently present under `testdata/raw/` includes modern and legacy files.
Validation artifacts currently present under `testdata/validation/` are partial.

Implication: not every raw video currently has a corresponding validation report artifact.

## Required Output Coverage

For each fixture selected for production evaluation, maintain:

- one command definition (exact CLI invocation)
- one expected output path under `testdata/validation/`
- one truth source (`--truth-json`, `--subtitles`, or `--dataset-manifest`)

## Tiered Evaluation Matrix

### Tier A (required on every change)

- synthetic validation
- one post-2000 real-media subtitle validation (`sintel_trailer_2010.mp4` when available)

### Tier B (required before release tags)

- multilingual subtitle validation (`elephants_dream_2006.es.srt` or `.de.srt`)
- one fallback legacy fixture validation (for compatibility confidence)

### Tier C (optional nightly or scheduled)

- full fixture sweep across all `testdata/raw/*.{mp4,mov,webm}` with available truth inputs
- aggregate metric trend report under `analysis/validation/`

## Correctness Gates

Every production-eval run must satisfy:

- command exits successfully
- report JSON is parseable and schema-compatible
- metrics are present: `overlap_ratio`, `speech_precision`, `speech_recall`, `non_voice_precision`, `non_voice_recall`, `boundary_error_ms`
- no warning/failure is silently ignored (fix or document in `plans/050-status-report/STATUS.md`)

## Implementation Tasks

1. Add a fixture-to-output manifest file (source -> truth -> output path). ✅ `testdata/validation/manifest.json`
2. Add a script to run the manifest and fail on missing outputs. ✅ `scripts/check_validation_coverage.py`
3. Add CI job for Tier A manifest checks. ✅ `.github/workflows/ci.yml` (`test` job)
4. Add scheduled CI for Tier C full sweep. ✅ `.github/workflows/validation-sweep.yml`
5. Add metric drift summary artifact upload for release visibility. ✅ validation sweep summary + reports uploaded as workflow artifacts

## Operational Command

Run Tier A enforcement locally:

```bash
python3 scripts/check_validation_coverage.py --tier A --strict-files
```

## Exit Criteria

- Tier A is green on PR CI.
- Tier B is green before release.
- Missing-output gap is zero for fixtures in the active manifest.
