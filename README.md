# movie-nonvoice-timeline

CPU-only Rust CLI to extract non-voice timeline windows from media, tag windows, and generate short deterministic listener-safe prompts.

## Commands
- `timeline extract <input> --output out.json`
- `timeline tag <input_media> --input segments.json --output tagged.json`
- `timeline prompt <input_json> --output prompted.json`
- `timeline review <input_media> --input segments.json --output reports/nonvoice-review.html [--open]`
- `timeline calibrate <corrections_dir> --profile drama`
- `timeline apply-calibration --report analysis/learnings/latest-calibration.json --output ~/.config/do-movie-radio-play/profiles/latest.json`
- `timeline gen-fixtures --output-dir testdata/generated`
- `timeline validate <input_media> --truth-json testdata/generated/alternating.truth.json --profile synthetic`
- `timeline eval <input_media> --truth-json testdata/generated/alternating.truth.json --profile synthetic` (alias of `validate`)
- `timeline validate <input_media> --subtitles in.srt --total-ms 120000 --profile movie`
- `timeline validate <input_media> --dataset-manifest speech.csv --total-ms 120000 --profile dataset`
- `timeline bench <input_media> --output analysis/benchmarks/latest.json`

## Build
`cargo build`

## Test
`bash scripts/quality_gate.sh`

## Dependency Updates
- Dependabot checks Cargo and GitHub Actions weekly.
- Dependabot PRs use GitHub auto-merge once required checks are green.

## Bench
`bash scripts/benchmark.sh`

Benchmark input selection policy:
- Primary (post-2000): `sintel_trailer_2010.mp4`, `big_buck_bunny_trailer_2008.mov`, `elephants_dream_2006.mp4`
- Legacy fallback (optional/local compatibility): older pre-2000 fixtures if already present
- Final fallback: generated deterministic fixture `testdata/generated/alternating.wav`

CI compares a fresh benchmark artifact against the checked-in real-media baseline
in `analysis/benchmarks/latest.json` before uploading benchmark artifacts.

Manual regression check:
`python3 scripts/check_benchmark_regression.py --baseline analysis/benchmarks/latest.json --candidate analysis/benchmarks/latest.json`

## Observability
- `timeline extract` emits INFO logs per pipeline stage with elapsed milliseconds for decode, resample, framing, VAD, smoothing, segmentation, merging, and inversion.
- `timeline bench` writes a JSON report that now includes `stage_ms` timing fields for each stage so benchmarking artifacts capture bottlenecks over time.

## Fixtures
`bash scripts/fetch_test_assets.sh`

Assets are sourced from Blender Open Movies and permissively usable sources.
Post-2000 fixtures are preferred; older files are still accepted as fallback if already present locally.
The fetch script also downloads multilingual subtitle fixtures for non-English validation coverage.

## JSON schema
Top-level fields:
- `file`
- `analysis_sample_rate`
- `frame_ms`
- `segments[]` with `start_ms`, `end_ms`, `kind`, `confidence`, `tags`, `prompt`

Benchmark JSON fields:
- `input_file`
- `total_ms`
- `decode_ms`
- `frame_count`
- `segment_count`
- `stage_ms` with `decode_ms`, `resample_ms`, `frame_ms`, `vad_ms`, `smooth_ms`, `speech_ms`, `merge_ms`, `invert_ms`

## Calibration
`timeline calibrate corrections/ --profile action` reads correction JSON files and writes a versioned calibration report to `analysis/learnings/latest-calibration.json`.

## Real-Media Validation
`timeline validate testdata/raw/sintel_trailer_2010.mp4 --subtitles testdata/raw/sintel_trailer_2010.srt --total-ms 53000 --profile movie --output analysis/validation/sintel_trailer_2010.json`

Non-English subtitle validation example:
`timeline validate testdata/raw/elephants_dream_2006.mp4 --subtitles testdata/raw/elephants_dream_2006.es.srt --total-ms 653696 --profile movie --output analysis/validation/elephants_dream_2006_es.json`

Validation/eval guardrails:
- Pass exactly one truth source: `--truth-json` or `--subtitles` or `--dataset-manifest`.
- `--total-ms` is required when using `--subtitles` or `--dataset-manifest`.

## Human Review Player

Use the review player to manually confirm that extracted `non_voice` windows are actually non-voice:

`timeline review testdata/raw/the_hole_1962.mp4 --input testdata/validation/the_hole_1962.json --output reports/nonvoice-review.html --open`

Open the generated HTML file in a browser. It provides per-segment navigation with pre/post-roll playback.

## Limitations
Current VAD uses deterministic energy thresholding and conservative smoothing; it is intended for robust non-voice extraction, not transcript-grade speech detection.

Only the `energy` VAD engine is currently exposed in the CLI.
