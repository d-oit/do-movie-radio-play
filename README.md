# do-movie-radio-play

Extracts non-voice segments from movie audio to assist in radio play adaptation.

The tool identifies audio intervals without speech, such as music, sound effects, or ambience. It uses energy and
spectral feature analysis to classify audio frames and clusters them into segments.

## Prerequisites
- Rust 2021 toolchain (v1.75+)
- FFmpeg (optional; required only for video container inputs or non-WAV audio)

## Build
```bash
cargo build --release
```
The binary is located at `target/release/timeline`.

## Commands
- `extract <INPUT> --output <JSON>`: Run the extraction pipeline on a media file.
- `tag <MEDIA> --input <JSON> --output <JSON>`: Apply acoustic tags (music, ambience) to segments.
- `prompt <JSON> --output <JSON>`: Generate text prompts for identified segments.
- `review <MEDIA> --input <JSON> --output <HTML>`: Generate an interactive review player.
- `calibrate <DIR> --profile <NAME>`: Generate a calibration report from manual corrections.
- `apply-calibration --report <JSON>`: Update the active profile with a calibration report.
- `bench <MEDIA> --output <JSON>`: Measure pipeline performance and stage durations.
- `gen-fixtures`: Create synthetic audio test cases.
- `validate <MEDIA> --output <JSON>`: Compare extraction results against ground truth or subtitles.
- `ai-voice-extract <JSON> --output <JSON>`: Extract only speech segments for voice replacement workflows.
- `verify-timeline <MEDIA> --timeline <JSON>`: Validate segments against spectral feature bounds.
- `update-thresholds`: Adjust adaptive thresholds using the learning database.
- `learning-stats`: Display statistics from the SQL-based learning database.
- `merge-timeline <INPUT> --output <JSON>`: Combine adjacent segments based on gap thresholds.
- `export <INPUT> --output <FILE> --format <json|edl|vtt>`: Convert timeline to external formats.

## Configuration
Profiles are stored as JSON in `config/profiles/`. Options can be overridden via `TIMELINE_` environment variables.

### AnalysisConfig Fields
- `sample_rate_hz`: Processing sample rate (default: 16000).
- `frame_ms`: Analysis window size (default: 20).
- `energy_threshold`: Baseline RMS sensitivity (0.0 to 1.0).
- `vad_engine`: Classification engine ("energy", "spectral", or "hybrid").
- `min_speech_ms`: Minimum duration for a speech cluster.
- `min_non_voice_ms`: Minimum duration for a non-voice segment.
- `max_non_voice_ms`: Maximum duration for a non-voice segment.
- `speech_hangover_ms`: Duration to extend speech segments after detection.
- `merge_gap_ms`: Maximum gap to merge adjacent segments.
- `parallel_features`: Enable multi-threaded feature extraction.

## Validation Workflow
1. Run the validation suite: `python3 scripts/run_validation_manifest.py`.
2. Generate the readiness report: `python3 scripts/build_radio_play_readiness_report.py`.
3. Check codebase integrity: `bash scripts/quality_gate.sh`.

## Export Formats
- **JSON**: Internal format with timestamps, confidence, tags, and prompts.
- **EDL**: CMX 3600 Edit Decision List for NLE import.
- **VTT**: WebVTT subtitle format.

## Known Limitations
- 16-bit PCM WAV is the only natively supported format; others require FFmpeg.
- Spectral analysis is CPU-bound.
- Memory usage increases with the number of segments during HTML report generation.

## Contributing
Refer to [AGENTS.md](AGENTS.md) for development workflows and agent coordination policies.
