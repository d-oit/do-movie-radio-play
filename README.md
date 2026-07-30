# do-movie-radio-play

The `timeline` command-line tool extracts non-voice segments from movie audio to assist in radio play adaptation.

## what it does

This tool processes audio tracks from media files, performs voice activity detection (VAD), and clusters identified
speech intervals. It then computes the complement of speech to locate non-voice segments containing music, sound
effects, or background ambience.

## prerequisites

- Rust toolchain (2021 edition, version 1.75+)
- FFmpeg (required for video container parsing and non-WAV formats)

## build

To compile the workspace and components:

```bash
cargo build --workspace --release
```

The compiled CLI binary is located at `target/release/timeline`.

## commands

- `extract <INPUT> --output <JSON>`: Run the extraction pipeline on a media file.
- `tag <INPUT_MEDIA> --input <JSON> --output <JSON>`: Map acoustic tags to segments using spectral features.
- `prompt <INPUT_JSON> --output <JSON>`: Generate AI narration prompts for tagged segments.
- `review <INPUT_MEDIA> --input <JSON> --output <HTML>`: Generate an interactive review player HTML.
- `calibrate <CORRECTIONS_DIR> --profile <NAME>`: Calibrate engine parameters from manual corrections.
- `apply-calibration --report <JSON>`: Apply a saved calibration report to the active profile.
- `bench <INPUT_MEDIA> --output <JSON>`: Measure pipeline performance and stage durations.
- `gen-fixtures`: Generate synthetic WAV test fixtures.
- `validate <INPUT_MEDIA> --output <JSON>`: Evaluate segment boundaries against subtitles or ground truth.
- `ai-voice-extract <INPUT_JSON> --output <JSON>`: Extract only speech segments for voice replacement workflows.
- `verify-timeline <MEDIA> --timeline <JSON> --output <JSON>`: Run spectral verification on non-voice segments.
- `update-thresholds`: Generate threshold recommendations from the learning state or database.
- `learning-stats`: Display statistics from the learning database.
- `learning-experiments`: List and inspect active calibration runs and applied profile versions from the database.
- `merge-timeline <INPUT> --output <JSON>`: Merge adjacent non-voice segments based on gap thresholds.
- `export <INPUT> --output <FILE> --format <json|edl|vtt>`: Convert timeline to external formats.
- `radio-play <MOVIE>`: Perform visual gap analysis by correlating VAD segments with subtitles.
- `preview --input <WAV>`: Stream a WAV file to the system audio output for preview.

## configuration

Profiles are stored as JSON in `config/profiles/`.
Core profile files include:
- `config/profiles/modern-optimized.json`: Optimized for modern audio content using a spectral VAD engine.
- `config/profiles/legacy-optimized.json`: Optimized for older, noisy content using a hybrid VAD engine.
- `config/profiles/radio-play.json`: Optimized with small minimum non-voice segment durations for quick adaptation.

### AnalysisConfig Fields

The following parameters can be configured in a profile JSON:
- `sample_rate_hz`: Audio processing sample rate (default: 16000).
- `frame_ms`: Analysis window duration in milliseconds (default: 20).
- `speech_hangover_ms`: Post-speech hangover duration in milliseconds (default: 300).
- `merge_gap_ms`: Maximum gap duration to merge adjacent segments (default: 250).
- `min_speech_ms`: Minimum speech segment duration (default: 120).
- `min_non_voice_ms`: Minimum non-voice segment duration (default: 10000).
- `max_non_voice_ms`: Optional maximum non-voice segment duration limit.
- `energy_threshold`: Baseline RMS energy threshold for speech detection (default: 0.015).
- `vad_threshold_delta`: Delta added to baseline energy threshold.
- `prompt_min_duration_ms`: Minimum segment duration to qualify for prompt generation (default: 2500).
- `prompt_min_confidence`: Minimum confidence required to generate prompts (default: 0.65).
- `vad_engine`: Core voice activity detection classification engine (`"energy"`, `"spectral"`, or `"hybrid"`).
- `parallel_features`: Boolean flag to enable multi-threaded feature extraction using Rayon.
- `merge_options`: Segment merging configuration (`min_gap_to_merge`, `merge_strategy`, `min_speech_duration`,
  `min_silence_duration`, `silence_threshold_db`).
- `spectral_flatness_max`, `spectral_entropy_min`, `spectral_centroid_min`, `spectral_centroid_max`: Fine-grained
  bounds for VAD classification and validation.
- `voice_synthesis`: Configures modular TTS voice synthesis providers and fallback chain (such as Kokoro, PocketTTS,
  Qwen3, Orpheus, ElevenLabs, or Modal).
- `chunk_duration_sec`: Duration of audio chunks for parallel processing.
- `profile_id`: Profile identifier string.
- `version`: Version number of the configuration profile.
- `experiment_tags`: List of associated experimental tags.

## validation workflow

To run the complete validation manifest and build readiness reports:
1. Run the validation test suite on the configured manifest:
```bash
python3 scripts/run_validation_manifest.py
```
2. Build the release readiness validation report:
```bash
python3 scripts/build_radio_play_readiness_report.py
```
3. Run the complete codebase quality gate:
```bash
bash scripts/quality_gate.sh
```

### Spectral VAD Performance Gate

The quality gate automatically executes a benchmark using `testdata/perf-manifest.json` on every run.
It enforces the following execution duration thresholds:
- VAD classification (`vad_ms` and `spectral_vad_ms`) < 30ms.
- Framing and feature extraction (`frame_ms`) < 150ms.
- Total processing duration (`total_ms`) < 500ms.

## export

The tool supports exporting segments using the `export` command to these formats:
- **JSON**: Internal schema containing segment timestamps, confidence, tags, and prompts.
- **EDL**: CMX 3600 Edit Decision List format for importing into Non-Linear Editors (NLEs).
- **VTT**: WebVTT subtitle formatting for media player integration.

## known limitations

- High-resolution spectral analysis is CPU-bound and memory bandwidth-intensive.
- Only 16-bit PCM WAV format is natively decoded; other file formats require FFmpeg.
- Speech verification filters apply selectively to sparse profile runs.
- <!-- TODO: verify --> Additional unsupported formats or external system dependencies.

---

For developer and AI agent workflow instructions, see [AGENTS.md](AGENTS.md).
