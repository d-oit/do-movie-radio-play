# do-movie-radio-play

`do-movie-radio-play` extracts non-voice timeline segments from movie audio and produces radio play audio adaptations.

## What It Does

The tool processes movie audio to detect non-voice gaps (music, sound effects, ambience), generates scene descriptions
for audio description, synthesizes narrator voice tracks, and mixes final radio play master audio files.

## Prerequisites

- Rust 1.88 or higher (`rustc`, `cargo`, `rustfmt`, `clippy`)
- FFmpeg (required when processing non-WAV media containers)
- ALSA development libraries (`libasound2-dev` on Linux) when built with system playback support

Development setup script:

```bash
bash scripts/setup-dev-env.sh
```

## Build

```bash
cargo build --workspace --release
```

The compiled binary is placed at `target/release/timeline`.

## Commands

- `extract <INPUT> --output <JSON>`: Extract non-voice timeline segments from media file.
- `tag <INPUT_MEDIA> --input <JSON> --output <JSON>`: Assign acoustic classification tags (music, ambience) to segments.
- `prompt <INPUT_JSON> --output <JSON>`: Generate AI narration prompts for tagged non-voice segments.
- `review <INPUT_MEDIA> --input <JSON> [--output <HTML>]`: Generate interactive HTML review player.
- `calibrate <CORRECTIONS_DIR> [--profile <NAME>]`: Generate calibration report from manual correction files.
- `apply-calibration [--report <JSON>]`: Apply calibration parameters to active profile.
- `bench <INPUT_MEDIA> [--output <JSON>]`: Benchmark processing speed and stage timing metrics.
- `gen-fixtures [--output-dir <DIR>]`: Generate synthetic test WAV files for validation.
- `validate` (alias `eval`) `<INPUT_MEDIA> [--output <JSON>]`: Evaluate accuracy against ground truth annotations.
- `ai-voice-extract <INPUT_JSON> --output <JSON>`: Extract speech segments for voice replacement workflows.
- `verify-timeline <MEDIA> --timeline <JSON> [--output <JSON>]`: Validate segment spectral statistics against bounds.
- `update-thresholds`: Recalculate adaptive VAD thresholds using stored learning database runs.
- `learning-stats [--radio-play]`: Display summary statistics from local SQLite learning database.
- `learning-log [--last N]`: Show recent learning database records.
- `reset-learnings --confirm`: Reset learned threshold adaptations (run history is preserved).
- `export-learnings [--output <JSON>]`: Export learning database to JSON file.
- `learning-experiments`: List calibration runs, applied profile versions, and experiment records.
- `merge-timeline <INPUT> --output <JSON>`: Merge adjacent non-voice segments using gap duration thresholds.
- `export <INPUT> --output <FILE> --format <json|edl|vtt>`: Export timeline to JSON, EDL, or VTT format.
- `radio-play <MOVIE>`: Execute GOAP-driven radio play production (gap identification, narration, TTS, assembly).
- `preview --input <WAV>`: Stream audio file playback to system speakers (requires `playback` feature).
- `config validate [--config <TOML>]`: Validate application configuration format and values.
- `voice samples --character <NAME> --input <MOVIE>`: Extract per-character voice sample candidates.
- `voice list`: Display inventory of stored voice references.
- `voice test --character <NAME> --text <TEXT>`: Synthesize speech using a stored character voice reference.
- `narrate [--scene <N>] [--dry-run]`: Render narrator prompt template or request narration text from LLM backend.
- `produce --input <MEDIA>`: Run 12-stage production pipeline with checkpoint persistence and resumption.

## Configuration

Configuration profiles are stored in `config/profiles/` (e.g., `modern-optimized.json`, `legacy-optimized.json`).

### AnalysisConfig Fields

- `sample_rate_hz`: Audio sample rate in Hz (default: 16000).
- `frame_ms`: Analysis window duration in milliseconds (default: 20).
- `speech_hangover_ms`: Post-speech hangover duration in milliseconds (default: 300).
- `merge_gap_ms`: Gap threshold for merging adjacent segments in milliseconds (default: 250).
- `min_speech_ms`: Minimum speech segment duration in milliseconds (default: 120).
- `min_non_voice_ms`: Minimum non-voice segment duration in milliseconds (default: 10000).
- `max_non_voice_ms`: Optional maximum non-voice segment duration in milliseconds (default: null).
- `energy_threshold`: Baseline RMS threshold for speech classification (default: 0.015).
- `vad_threshold_delta`: Delta added to baseline energy threshold (default: 0.0).
- `prompt_min_duration_ms`: Minimum segment duration for prompt generation in milliseconds (default: 2500).
- `prompt_min_confidence`: Minimum confidence threshold for prompt generation (default: 0.65).
- `vad_engine`: Classification engine ("energy", "spectral", "hybrid", "webrtc", "silero", default: "energy").
- `parallel_features`: Enable multi-threaded feature extraction (default: true).
- `merge_options`: Optional merge strategy configuration object (`min_gap_to_merge`, `merge_strategy`, etc.).
- `spectral_flatness_max`: Upper bound threshold for spectral flatness (default: null).
- `spectral_entropy_min`: Lower bound threshold for spectral entropy (default: null).
- `spectral_centroid_min`: Lower bound threshold for spectral centroid in Hz (default: null).
- `spectral_centroid_max`: Upper bound threshold for spectral centroid in Hz (default: null).
- `voice_synthesis`: Configuration object for TTS providers and fallback chains (default: null).
- `chunk_duration_sec`: Duration in seconds for chunked parallel processing (default: null).
- `profile_id`: Profile identifier string (default: null).
- `version`: Integer profile version number (default: null).
- `experiment_tags`: Array of string tags for tracking experiment parameters.
- `sound_effects`: Configuration object for SFX indexing, local paths, and API backends (default: null).

### Segment Output Schema

- `start_ms`: Segment start timestamp in milliseconds.
- `end_ms`: Segment end timestamp in milliseconds.
- `kind`: Segment classification ("speech" or "non_voice").
- `confidence`: Classification confidence score between 0.0 and 1.0.
- `tags`: Array of acoustic tags (e.g., ["music"], ["ambience"]).
- `prompt`: Optional generated narration prompt string.
- `sfx_trigger`: Optional SFX trigger (`none`, `auto_select`, `specific`, `ai_generate`).

## Validation Workflow

Run validation scripts and quality checks:

```bash
python3 scripts/run_validation_manifest.py
python3 scripts/build_radio_play_readiness_report.py
RUSTUP_TOOLCHAIN=stable-x86_64-unknown-linux-gnu bash scripts/quality_gate.sh
```

## Export

Supported export formats:

- `json`: Native timeline structure containing timestamps, confidence scores, tags, and prompts.
- `edl`: CMX 3600 Edit Decision List for video and audio editing applications.
- `vtt`: WebVTT format for subtitle timing.

## Known Limitations

- Direct audio decoding without FFmpeg is limited to 16-bit, 24-bit PCM, and 32-bit float WAV containers.
- Local neural TTS providers (Kokoro, Orpheus, Qwen3) require optional feature flags and model weights.

## Development Workflow

For contributor guidelines and agent instructions, see [AGENTS.md](AGENTS.md).
