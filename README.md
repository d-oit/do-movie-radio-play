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
- `timeline verify-timeline <media> --timeline timeline.json --output verified.json [--save-learning --learning-db analysis/thresholds/learning.db]`
- `timeline update-thresholds [--learning-state state.json | --learning-db analysis/thresholds/learning.db]`
- `timeline learning-stats [--learning-db analysis/thresholds/learning.db] [--output analysis/thresholds/learning-stats.json]`
- `python3 scripts/optimize_fp_sweep.py --output analysis/optimization/fp-sweep-ranked.json`
- `python3 scripts/generate_optimized_profiles.py --sweep-report analysis/optimization/fp-sweep-ranked.json`
- `timeline merge-timeline --input timeline.json --output merged.json`
- `timeline export --input timeline.json --output out.json --format json|edl|vtt [--verified verified.json]`

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

Modern fixture example:

`timeline review testdata/raw/elephants_dream_2006.mp4 --input testdata/validation/elephants_dream_2006_nonvoice.json --output reports/nonvoice-review-elephants-modern.html --open`

Open the generated HTML file in a browser. It provides per-segment navigation with pre/post-roll playback.

Review workflow improvements in the HTML player:
- mark current segment as voice false-positive (`x`)
- undo last review action (`u`)
- save a fully reviewed standalone HTML (`Save Reviewed HTML`) without exporting a separate JSON file
- toggle full-movie context mode (`f`) to inspect the rest of the movie while keeping non-voice markers visible

`--open` uses OS-specific openers with fallback support:
- macOS: `open`
- Linux: `xdg-open` then `gio open`
- Windows: `cmd /C start` then PowerShell `Start-Process`
- WSL2: `wslview` first, then Windows interop openers via `cmd.exe`/PowerShell

## Limitations
Current VAD is deterministic (energy + spectral) with conservative smoothing; it is intended for robust non-voice extraction, not transcript-grade speech detection.

Both `energy` and `spectral` VAD engines are exposed in the CLI.

## Spectral VAD

The spectral VAD engine uses spectral features (entropy, flatness, centroid) to better distinguish speech from music/effects:

```bash
timeline extract input.mp4 --output timeline.json --vad-engine spectral
```

Configurable thresholds in profiles:

```json
{
  "vad_engine": "spectral",
  "spectral_flatness_max": 0.5,
  "spectral_entropy_min": 3.5,
  "spectral_centroid_min": 100,
  "spectral_centroid_max": 6000
}
```

## Learning System

The system can learn from verification results to improve detection:

1. **Extract** with spectral VAD
2. **Verify** segments and save learning state:
   ```bash
   timeline verify-timeline --media input.mp4 --timeline timeline.json --output verified.json --save-learning --learning-db analysis/thresholds/learning.db
   ```
3. **Update thresholds** based on learned patterns:
   ```bash
   timeline update-thresholds --learning-db analysis/thresholds/learning.db
   ```
4. **Re-extract** with optimized thresholds
5. **Inspect learning quality**:
   ```bash
   timeline learning-stats --learning-db analysis/thresholds/learning.db
   ```

## Export

Export timelines in various formats:

```bash
# JSON with verification status
timeline export --input timeline.json --output out.json --format json --verified verified.json

# EDL for video editors
timeline export --input timeline.json --output out.edl --format edl --verified verified.json

# WebVTT for web players
timeline export --input timeline.json --output out.vtt --format vtt --verified verified.json
```

## FP Optimization Sweep

Run the built-in candidate sweep (spectral extract + verify) and rank configurations by weighted false-positive rate:

```bash
python3 scripts/optimize_fp_sweep.py --output analysis/optimization/fp-sweep-ranked.json
```

Optional controls:
- `--legacy-media <path>` (repeatable) to define legacy cohort explicitly
- `--min-coverage-ratio 0.7` to require candidate coverage vs baseline

Sweep report now includes both:
- `weighted_false_positive_rate` (legacy metric)
- `weighted_false_positive_risk_rate` (counts suspicious + rejected)

Generate profile files from sweep policy:

```bash
python3 scripts/generate_optimized_profiles.py \
  --sweep-report analysis/optimization/fp-sweep-ranked.json \
  --modern-output config/profiles/modern-optimized.json \
  --legacy-output config/profiles/legacy-optimized.json
```

Compact latest learnings:
- Verification now uses runtime threshold overrides (`verify-timeline` flags are active in status decisioning).
- Verification uses confidence hysteresis (`high=0.62`, `low=0.45`) to reduce borderline flips.
- Sweep ranking includes a coverage guard (`--min-coverage-ratio`) to avoid low-coverage false wins.
- Current sweep policy recommends baseline for both modern and legacy cohorts on available fixtures.
