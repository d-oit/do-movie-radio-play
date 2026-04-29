# do-movie-radio-play

Extracts non-voice audio segments from movie files for radio play adaptation.

## What it does
This tool identifies segments in movie audio that do not contain speech (music, ambience, sound effects). It uses
energy-based and spectral-based Voice Activity Detection (VAD) to generate a timeline of non-voice events. Results
can be exported for use in audio editing software or used to generate descriptive prompts.

## Prerequisites
- Rust toolchain (Edition 2021)
- ffmpeg (required on PATH for decoding non-WAV media)

## Build
```bash
cargo build --release
```
The binary is located at `target/release/timeline`.

## Commands
- `extract <INPUT> --output <JSON>`: Extract non-voice segments from media.
- `tag <MEDIA> --input <JSON> --output <JSON>`: Categorize segments (ambience, music, etc.).
- `prompt <JSON> --output <JSON>`: Generate deterministic text prompts for segments.
- `review <MEDIA> --input <JSON> --output <HTML>`: Create an interactive HTML player for verification.
- `calibrate <DIR> --profile <NAME>`: Adjust threshold deltas based on manual corrections.
- `apply-calibration --report <JSON>`: Apply a calibration report to current profiles.
- `bench <MEDIA> --output <JSON>`: Benchmark the pipeline performance.
- `gen-fixtures`: Generate synthetic audio fixtures for testing.
- `validate <MEDIA> --truth-json <JSON>`: Compare extraction against ground truth or SRT.
- `ai-voice-extract <JSON> --output <JSON>`: Filter segments specifically for AI voice replacement.
- `verify-timeline <MEDIA> --timeline <JSON>`: Validate segments against spectral feature bounds.
- `update-thresholds`: Update adaptive thresholds using the learning database.
- `learning-stats`: Display statistics from the learning database.
- `merge-timeline <INPUT> --output <JSON>`: Combine adjacent segments based on gap thresholds.
- `export <INPUT> --output <FILE> --format <json|edl|vtt>`: Convert timeline to specific formats.

## Configuration
Configuration is defined in JSON profiles (see `config/profiles/`). Overrides are available via CLI flags or
environment variables (prefixed with `TIMELINE_`).

Key fields in `AnalysisConfig`:
- `sample_rate_hz`: Processing sample rate (default 16000).
- `frame_ms`: Analysis window size (default 20ms).
- `energy_threshold`: Baseline VAD sensitivity (0.0 to 1.0).
- `vad_engine`: "energy" or "spectral".
- `min_speech_ms`: Minimum duration to count as speech.
- `min_non_voice_ms`: Minimum duration for non-voice segments.
- `spectral_entropy_min`: Minimum spectral entropy for voice classification.
- `spectral_flatness_max`: Maximum flatness for non-voice classification.

## Validation Workflow
1. Run `python3 scripts/run_validation_manifest.py` to evaluate against the full dataset.
2. Generate a readiness report: `python3 scripts/build_radio_play_readiness_report.py`.
3. Check status: `bash scripts/quality_gate.sh`.

## Export
- **JSON**: Native format containing timestamps, confidence, tags, and prompts.
- **EDL**: CMX 3600 Edit Decision List for import into DAWs/NLEs.
- **VTT**: WebVTT subtitle format for web players.

## Known Limitations
- Native WAV reader supports 16-bit PCM only; other formats require ffmpeg.
- Processing is offline and CPU-only.
- Large files may require significant memory for spectral analysis buffers.

## Contributing
See [AGENTS.md](AGENTS.md) for the development workflow and agent-specific instructions.
