use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "timeline")]
#[command(about = "Extract non-voice timeline segments from movie audio")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Run non-voice extraction pipeline on a media file.
    Extract {
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        threshold: Option<f32>,
        #[arg(long)]
        min_speech_ms: Option<u32>,
        #[arg(long)]
        min_silence_ms: Option<u32>,
        #[arg(long)]
        max_non_voice_ms: Option<u32>,
        /// VAD engine; when omitted, the config file's `vad_engine` applies (default `energy`).
        #[arg(long, value_parser = ["energy", "spectral", "hybrid", "webrtc", "silero"])]
        vad_engine: Option<String>,
        #[arg(long)]
        calibration_profile: Option<PathBuf>,
        #[arg(long)]
        save_calibration: bool,
        #[arg(long)]
        parallel_features: Option<bool>,
        #[arg(long)]
        chunk_duration: Option<u64>,
    },
    /// Assign acoustic tags (music, ambience) to an extracted timeline.
    Tag {
        input_media: PathBuf,
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        calibration_profile: Option<PathBuf>,
    },
    /// Generate narration prompts for tagged non-voice segments.
    Prompt {
        input_json: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Generate an interactive HTML review player.
    Review {
        input_media: PathBuf,
        #[arg(long)]
        input: PathBuf,
        #[arg(long, default_value = "reports/nonvoice-review.html")]
        output: PathBuf,
        #[arg(long, default_value_t = 1.0)]
        pre_roll_s: f32,
        #[arg(long, default_value_t = 1.0)]
        post_roll_s: f32,
        #[arg(long)]
        open: bool,
        #[arg(long)]
        verified: Option<PathBuf>,
        #[arg(long)]
        merged: bool,
    },
    /// Produce a calibration report from a corrections directory.
    Calibrate {
        corrections_dir: PathBuf,
        #[arg(long, default_value = "drama")]
        profile: String,
    },
    /// Apply a calibration report to the active profile.
    ApplyCalibration {
        #[arg(long, default_value = "analysis/learnings/latest-calibration.json")]
        report: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Benchmark pipeline speed and stage timings on a media file.
    Bench {
        input_media: PathBuf,
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        threshold: Option<f32>,
        #[arg(long)]
        min_speech_ms: Option<u32>,
        #[arg(long)]
        min_silence_ms: Option<u32>,
        #[arg(long)]
        max_non_voice_ms: Option<u32>,
        /// VAD engine; when omitted, the config file's `vad_engine` applies (default `energy`).
        #[arg(long, value_parser = ["energy", "spectral", "hybrid", "webrtc", "silero"])]
        vad_engine: Option<String>,
        #[arg(long)]
        calibration_profile: Option<PathBuf>,
        #[arg(long)]
        parallel_features: Option<bool>,
        #[arg(long, default_value = "analysis/benchmarks/latest.json")]
        output: PathBuf,
    },
    /// Generate deterministic synthetic WAV fixtures for validation.
    GenFixtures {
        #[arg(long, default_value = "testdata/generated")]
        output_dir: PathBuf,
    },
    /// Evaluate segment accuracy against one of --truth-json, --subtitles, or --dataset-manifest.
    #[command(alias = "eval")]
    Validate {
        input_media: PathBuf,
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        threshold: Option<f32>,
        #[arg(long)]
        min_speech_ms: Option<u32>,
        #[arg(long)]
        min_silence_ms: Option<u32>,
        #[arg(long)]
        max_non_voice_ms: Option<u32>,
        /// VAD engine; when omitted, the config file's `vad_engine` applies (default `energy`).
        #[arg(long, value_parser = ["energy", "spectral", "hybrid", "webrtc", "silero"])]
        vad_engine: Option<String>,
        #[arg(long)]
        calibration_profile: Option<PathBuf>,
        #[arg(long)]
        truth_json: Option<PathBuf>,
        #[arg(long)]
        subtitles: Option<PathBuf>,
        #[arg(long)]
        dataset_manifest: Option<PathBuf>,
        #[arg(long)]
        total_ms: Option<u64>,
        #[arg(long, default_value = "movie")]
        profile: String,
        #[arg(long)]
        parallel_features: Option<bool>,
        #[arg(long, default_value = "analysis/validation/latest.json")]
        output: PathBuf,
    },
    /// Extract speech segments for AI voice replacement workflows.
    AiVoiceExtract {
        input_json: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Check timeline segments against spectral bounds.
    VerifyTimeline {
        media: PathBuf,
        #[arg(long)]
        timeline: PathBuf,
        #[arg(long, default_value = "verified.json")]
        output: PathBuf,
        #[arg(long)]
        entropy_min: Option<f32>,
        #[arg(long)]
        entropy_max: Option<f32>,
        #[arg(long)]
        flatness_max: Option<f32>,
        #[arg(long)]
        energy_min: Option<f32>,
        #[arg(long)]
        centroid_min: Option<f32>,
        #[arg(long)]
        centroid_max: Option<f32>,
        #[arg(long)]
        learning_state: Option<PathBuf>,
        #[arg(long)]
        learning_db: Option<PathBuf>,
        #[arg(long)]
        save_learning: bool,
        #[arg(long)]
        use_fingerprints: bool,
        #[arg(long, default_value = "10")]
        fingerprint_threshold: u32,
    },
    /// Recalculate adaptive VAD thresholds from learning state.
    UpdateThresholds {
        #[arg(long, default_value = "analysis/thresholds/learning-state.json")]
        learning_state: PathBuf,
        #[arg(long)]
        learning_db: Option<PathBuf>,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Show summary statistics from the learning database.
    LearningStats {
        #[arg(long)]
        radio_play: bool,
        #[arg(long, default_value = "analysis/thresholds/learning.db")]
        learning_db: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Show the last N learning rows (default 10).
    LearningLog {
        // 100k rows is the display ceiling; the store clamps to i64 range.
        #[arg(long, default_value_t = 10)]
        last: usize,
        #[arg(long, default_value = "analysis/thresholds/learning.db")]
        learning_db: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Reset learned adaptations (run history survives per ADR-122).
    ResetLearnings {
        /// Reset adaptations; run history survives per ADR-122.
        #[arg(long)]
        confirm: bool,
        #[arg(long, default_value = "analysis/thresholds/learning.db")]
        learning_db: PathBuf,
    },
    /// Export the learning database to JSON.
    ExportLearnings {
        #[arg(long, default_value = "analysis/learnings/export.json")]
        output: PathBuf,
        #[arg(long, default_value = "analysis/thresholds/learning.db")]
        learning_db: PathBuf,
    },
    /// List calibration runs, profile versions, and experiments.
    LearningExperiments {
        #[arg(long, default_value = "analysis/thresholds/learning.db")]
        learning_db: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Merge adjacent segments by gap duration and strategy.
    MergeTimeline {
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        min_gap_to_merge: Option<u32>,
        #[arg(long, value_parser = ["all", "longest", "sparse"])]
        merge_strategy: Option<String>,
        #[arg(long)]
        verified: Option<PathBuf>,
    },
    /// Export a timeline to json, edl, or vtt.
    Export {
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, value_parser = ["json", "edl", "vtt"])]
        format: String,
        #[arg(long)]
        verified: Option<PathBuf>,
    },
    /// Run the radio-play pipeline (full GOAP run by default; --analyze-only needs --timeline).
    RadioPlay {
        movie: PathBuf,
        #[arg(long)]
        timeline: Option<PathBuf>,
        #[arg(long)]
        subtitles: Option<PathBuf>,
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long)]
        analyze_only: bool,
        #[arg(long)]
        verify_quality: bool,
        #[arg(long)]
        apply_learnings: bool,
        #[arg(long)]
        learning_state: Option<PathBuf>,
        #[arg(long)]
        learning_db: Option<PathBuf>,
        #[arg(long)]
        no_learn: bool,
        #[arg(long)]
        voice_reference: Option<PathBuf>,
        #[arg(long)]
        character: Option<String>,
        /// Narration language (text and TTS): `de` (default) or `en`.
        #[arg(long)]
        language: Option<String>,
        /// Gap confidence threshold (default 0.8; lower narrates more gaps).
        #[arg(long)]
        gap_confidence: Option<f32>,
        /// VAD engine for extraction (energy, spectral, hybrid, webrtc, silero).
        #[arg(long)]
        vad_engine: Option<String>,
        /// Do not derive dialogue cues (embedded track / speech-to-text) when no subtitles are given.
        #[arg(long)]
        no_auto_cues: bool,
        /// Mastering profile for the deliverable (ebu_r128, podcast, radio, atsc_a85).
        #[arg(long)]
        mastering_profile: Option<String>,
    },
    /// Preview a WAV file by streaming to system audio output.
    /// Useful for quick QA without writing intermediate files.
    /// Requires the `playback` feature; --skip/--duration select a window.
    Preview {
        /// Path to the WAV file to preview.
        #[arg(short, long)]
        input: PathBuf,
        /// Skip first N seconds of the file.
        #[arg(long, default_value = "0")]
        skip: f32,
        /// Play only the first N seconds.
        #[arg(long)]
        duration: Option<f32>,
    },
    /// Validate configuration files.
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },
    /// Manage per-character voice references and synthesis tests.
    Voice {
        #[command(subcommand)]
        command: VoiceCommands,
    },
    /// Render a narrator prompt (dry-run prints it, otherwise calls the configured LLM backend).
    Narrate {
        #[arg(long)]
        scene: Option<u32>,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        template: Option<PathBuf>,
    },
    /// Measure an existing output against a mastering profile, or master it.
    Mastering {
        input: PathBuf,
        /// Selectable profile: ebu_r128, podcast, radio, atsc_a85.
        #[arg(long, default_value = "ebu_r128")]
        profile: String,
        /// Override the profile's integrated loudness target (LUFS).
        #[arg(long)]
        target_lufs: Option<f32>,
        /// Override the profile's true-peak ceiling (dBTP).
        #[arg(long)]
        max_true_peak: Option<f32>,
        /// Write the machine-readable report here.
        #[arg(long)]
        json: Option<PathBuf>,
        /// Write the human-readable HTML summary here.
        #[arg(long)]
        html: Option<PathBuf>,
        /// Normalize and limit to the profile, writing the mastered file here.
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Produce the complete radio-play deliverable: decode, speech analysis,
    /// gap identification, narration planning, voice synthesis and assembly,
    /// plus optional verification. Every stage is checkpointed and resumable.
    ///
    /// `produce` and `radio-play` execute the same stage contract; use
    /// `radio-play --analyze-only` when you only want gap analysis.
    Produce {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        resume: Option<PathBuf>,
        #[arg(long)]
        dry_run: bool,
        /// Deliverable path (default: `<run dir>/<input stem>.radio-play.wav`).
        #[arg(long)]
        output: Option<PathBuf>,
        /// Add quality verification (and learning adaptation) stages.
        #[arg(long)]
        verify_quality: bool,
        /// Reuse an existing timeline instead of extracting one.
        #[arg(long)]
        timeline: Option<PathBuf>,
        /// Dialogue cues used to clip narration against speech.
        #[arg(long)]
        subtitles: Option<PathBuf>,
        /// Narration language (text and TTS): `de` (default) or `en`.
        #[arg(long)]
        language: Option<String>,
        /// Gap confidence threshold (default 0.8; lower narrates more gaps).
        #[arg(long)]
        gap_confidence: Option<f32>,
        /// Do not derive dialogue cues when no subtitles are given.
        #[arg(long)]
        no_auto_cues: bool,
        /// Freeze learning adaptations.
        #[arg(long)]
        no_learn: bool,
        /// Mastering profile for the deliverable (ebu_r128, podcast, radio, atsc_a85).
        #[arg(long)]
        mastering_profile: Option<String>,
    },
    /// Inspect, validate and re-generate the manifest for a production run directory.
    Report {
        /// Run directory containing `checkpoint.json` and its artifacts.
        run_dir: PathBuf,
        /// Print the manifest JSON to stdout instead of the stage summary.
        #[arg(long)]
        json: bool,
        /// Validate the run without rewriting `manifest.json` or `report.html`.
        #[arg(long)]
        validate: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommands {
    /// Validate a config file (defaults to config/default.toml).
    Validate {
        #[arg(long)]
        config: Option<PathBuf>,
    },
}

#[derive(Debug, Subcommand)]
pub enum VoiceCommands {
    /// Extract voice candidates for a character into voice_samples/{character}.json.
    Samples {
        #[arg(long)]
        character: String,
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// List stored voice references.
    List,
    /// Synthesize text with a stored voice reference.
    Test {
        #[arg(long)]
        character: String,
        #[arg(long)]
        text: String,
        /// Sample file written by `voice samples --output`; defaults to
        /// `voice_samples/{character}.json`.
        #[arg(long)]
        samples_from: Option<PathBuf>,
    },
}
