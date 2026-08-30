# do-movie-radio-play

Extracts non-voice timeline segments from movie audio to assist in radio play adaptation.

## What It Does

The tool analyzes input audio files to detect regions without speech, such as music, sound effects, and ambience.
It calculates RMS energy and spectral features per frame, applies Voice Activity Detection (VAD), and clusters audio
frames into non-voice segments suitable for production workflows.

## Prerequisites

- Rust 2021 toolchain (v1.75 or higher)
- FFmpeg (required when processing non-WAV media containers or encoded audio streams)

## Build

```bash
cargo build --workspace --release
```

The compiled executable is placed at `target/release/timeline`.

## Commands

- `extract <INPUT> --output <JSON>`: Run non-voice extraction pipeline on media file.
- `tag <INPUT_MEDIA> --input <JSON> --output <JSON>`: Assign acoustic classification tags (music, ambience) to segments.
- `prompt <INPUT_JSON> --output <JSON>`: Generate text prompts for identified non-voice segments.
- `review <INPUT_MEDIA> --input <JSON> --output <HTML>`: Generate interactive HTML review player.
- `calibrate <CORRECTIONS_DIR> --profile <NAME>`: Produce calibration report from corrected timeline files.
- `apply-calibration --report <JSON>`: Apply calibration report parameters to active profile.
- `bench <INPUT_MEDIA> --output <JSON>`: Benchmark pipeline processing speed and stage timing metrics.
- `gen-fixtures`: Generate synthetic test WAV fixtures for pipeline validation.
- `validate <INPUT_MEDIA> --output <JSON>`: Evaluate segment classification against ground truth or subtitles.
- `ai-voice-extract <INPUT_JSON> --output <JSON>`: Extract speech segments for AI voice replacement workflows.
- `verify-timeline <MEDIA> --timeline <JSON> --output <JSON>`: Validate segment spectral statistics against bounds.
- `update-thresholds`: Recalculate adaptive VAD thresholds using stored learning database runs.
- `learning-stats`: Display summary statistics from the local SQLite learning database.
- `learning-experiments`: List calibration runs, applied profile versions, and experiment records.
- `merge-timeline <INPUT> --output <JSON>`: Merge adjacent segments using gap duration thresholds.
- `export <INPUT> --output <FILE> --format <json|edl|vtt>`: Export timeline to external formats (JSON, EDL, VTT).
- `radio-play <MOVIE>`: Analyze gap context by matching VAD segments against SRT subtitle entries.
- `preview --input <WAV>`: Stream audio playback to system speakers for QA verification.

## Configuration

Configuration profiles are stored in `config/profiles/` (e.g., `modern-optimized.json`, `legacy-optimized.json`).

### AnalysisConfig Fields

- `sample_rate_hz`: Audio sample rate in Hz (default: 16000).
- `frame_ms`: Analysis window duration in milliseconds (default: 20).
- `speech_hangover_ms`: Post-speech hangover duration in milliseconds (default: 300).
- `merge_gap_ms`: Gap threshold in milliseconds for merging adjacent segments (default: 250).
- `min_speech_ms`: Minimum speech segment duration in milliseconds (default: 120).
- `min_non_voice_ms`: Minimum non-voice segment duration in milliseconds (default: 10000).
- `max_non_voice_ms`: Optional maximum non-voice segment duration in milliseconds (default: null).
- `energy_threshold`: Baseline RMS threshold for speech classification (default: 0.015).
- `vad_threshold_delta`: Delta added to baseline energy threshold (default: 0.0).
- `prompt_min_duration_ms`: Minimum segment duration for prompt generation in milliseconds (default: 2500).
- `prompt_min_confidence`: Minimum confidence threshold for prompt generation (default: 0.65).
- `vad_engine`: Classification engine ("energy", "spectral", or "hybrid", default: "energy").
- `parallel_features`: Enable multi-threaded feature extraction (default: true).
- `merge_options`: Optional merge strategy configuration object (`min_gap_to_merge`, `merge_strategy`, etc.).
- `spectral_flatness_max`: Upper bound threshold for spectral flatness (default: null).
- `spectral_entropy_min`: Lower bound threshold for spectral entropy (default: null).
- `spectral_centroid_min`: Lower bound threshold for spectral centroid in Hz (default: null).
- `spectral_centroid_max`: Upper bound threshold for spectral centroid in Hz (default: null).
- `chunk_duration_sec`: Duration in seconds for chunked parallel processing (default: null).
- `profile_id`: Profile identifier string (default: null).
- `version`: Integer profile version number (default: null).
- `experiment_tags`: Array of string tags for tracking experiment parameters.

### Segment JSON Schema

- `start_ms`: Segment start timestamp in milliseconds.
- `end_ms`: Segment end timestamp in milliseconds.
- `kind`: Segment classification type ("Speech" or "NonVoice").
- `confidence`: Classification confidence value between 0.0 and 1.0.
- `tags`: Array of acoustic tags (e.g., ["music"], ["ambience"]).
- `prompt`: String prompt text or null.

## Validation Workflow

Execute the verification steps to validate pipeline behavior:

1. Run validation suite across dataset: `python3 scripts/run_validation_manifest.py`
2. Generate readiness report: `python3 scripts/build_radio_play_readiness_report.py`
3. Perform workspace quality check: `bash scripts/quality_gate.sh`

### Spectral VAD Performance Gate

The quality gate in `scripts/quality_gate.sh` checks spectral VAD performance on `testdata/perf-manifest.json`:
- `vad_ms` & `spectral_vad_ms` < 30ms (classification duration)
- `frame_ms` < 150ms (framing and feature extraction duration)
- `total_ms` < 500ms (total execution duration)

## Export

- `json`: Internal timeline structure with timestamps, confidence scores, tags, and prompts.
- `edl`: CMX 3600 Edit Decision List for NLE audio software integration.
- `vtt`: WebVTT format for subtitle and caption displays.

## Known Limitations

- Direct audio decoding without FFmpeg supports only 16-bit PCM WAV containers.
- Spectral feature processing increases CPU usage relative to standard energy VAD.
- HTML review player generation uses streaming output (`BufWriter`) to maintain constant memory overhead.

## Development Workflow

For details on contributor workflows, agent policies, and repository guidelines, see [AGENTS.md](AGENTS.md).
