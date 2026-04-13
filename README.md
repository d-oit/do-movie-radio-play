# movie-nonvoice-timeline

CPU-only Rust CLI to extract non-voice timeline windows from media, tag windows, and generate short deterministic listener-safe prompts.

## Commands
- `timeline extract <input> --output out.json`
- `timeline tag <input_media> --input segments.json --output tagged.json`
- `timeline prompt <input_json> --output prompted.json`
- `timeline calibrate <corrections_dir> --profile drama`
- `timeline gen-fixtures --output-dir testdata/generated`
- `timeline validate <input_media> --truth-json testdata/generated/alternating.truth.json --profile synthetic`
- `timeline validate <input_media> --subtitles in.srt --total-ms 120000 --profile movie`
- `timeline validate <input_media> --dataset-manifest speech.csv --total-ms 120000 --profile dataset`
- `timeline bench <input_media> --output analysis/benchmarks/latest.json`

## Build
`cargo build`

## Test
`bash scripts/quality_gate.sh`

## Bench
`bash scripts/benchmark.sh testdata/generated/alternating.wav`

## Fixtures
`bash scripts/fetch_test_assets.sh`

Assets are sourced from Wikimedia Commons public-domain or permissively usable files. The Brüder (1929) sample is used for decode smoke only.

## JSON schema
Top-level fields:
- `file`
- `analysis_sample_rate`
- `frame_ms`
- `segments[]` with `start_ms`, `end_ms`, `kind`, `confidence`, `tags`, `prompt`

## Calibration
`timeline calibrate corrections/ --profile action` reads correction JSON files and writes a versioned calibration report to `analysis/learnings/latest-calibration.json`.

## Limitations
Current VAD uses deterministic energy thresholding and conservative smoothing; it is intended for robust non-voice extraction, not transcript-grade speech detection.
