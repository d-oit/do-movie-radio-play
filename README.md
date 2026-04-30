# do-movie-radio-play

Extracts non-voice segments from movie audio to assist in radio play adaptation.

## What it does
The tool identifies audio intervals without speech, such as music, sound effects, or ambience. It uses energy and
spectral feature analysis to classify audio frames and clusters them into segments. Output is provided as JSON
timelines, which can be exported to EDL or VTT formats.

## Prerequisites
- Rust 1.75+ (2021 edition)
- ffmpeg (must be on PATH for processing non-WAV media)

## Build
```bash
cargo build --release
```
The compiled binary is available at `target/release/timeline`.

## Commands
- `extract <INPUT> --output <JSON>`: Run the extraction pipeline on a media file.
- `tag <MEDIA> --input <JSON> --output <JSON>`: Apply acoustic tags (music, ambience) to segments.
- `prompt <JSON> --output <JSON>`: Generate text prompts for identified segments.
- `review <MEDIA> --input <JSON> --output <HTML>`: Generate an interactive review player.
- `calibrate <DIR> --profile <NAME>`: Generate a calibration report from manual corrections.
- `apply-calibration --report <JSON>`: Update the active profile with a calibration report.
- `bench <MEDIA> --output <JSON>`: Measure pipeline performance and stage durations.
- `gen-fixtures`: Create synthetic audio test cases.
- `validate <MEDIA> --output <JSON>`: Compare extraction results against ground truth or SRT.
- `ai-voice-extract <JSON> --output <JSON>`: Extract only speech segments for voice replacement workflows.
- `verify-timeline <MEDIA> --timeline <JSON>`: Validate segments against spectral feature bounds.
- `update-thresholds`: Adjust adaptive thresholds using the learning database.
- `learning-stats`: Display statistics from the SQL-based learning database.
- `merge-timeline <INPUT> --output <JSON>`: Combine adjacent segments based on gap thresholds.
- `export <INPUT> --output <FILE> --format <json|edl|vtt>`: Convert timeline to external formats.

## Configuration
Configuration is loaded from JSON profiles in `config/profiles/`. Options can be overridden via CLI flags or
environment variables (prefixed with `TIMELINE_`).

Key `AnalysisConfig` fields:
- `sample_rate_hz`: Processing sample rate (default: 16000).
- `frame_ms`: Analysis window size (default: 20).
- `energy_threshold`: Baseline RMS sensitivity (0.0 to 1.0).
- `vad_engine`: Classification engine ("energy" or "spectral").
- `min_speech_ms`: Minimum duration for a speech cluster.
- `min_non_voice_ms`: Minimum duration for a non-voice segment.

## Validation Workflow
1. Execute the validation manifest: `python3 scripts/run_validation_manifest.py`.
2. Generate the readiness report: `python3 scripts/build_radio_play_readiness_report.py`.
3. Verify codebase integrity: `bash scripts/quality_gate.sh`.

## Export
- **JSON**: Internal format with timestamps, confidence scores, and tags.
- **EDL**: CMX 3600 Edit Decision List for DAW/NLE import.
- **VTT**: WebVTT subtitle format for web players.

## Known Limitations
- Direct WAV reading is restricted to 16-bit PCM; ffmpeg is used for all other formats.
- Spectral analysis is CPU-bound and requires sequential processing for frame features.
- Memory usage scales with segment count during HTML report generation.

## Contributing
Refer to [AGENTS.md](AGENTS.md) for development workflows and agent coordination policies.
