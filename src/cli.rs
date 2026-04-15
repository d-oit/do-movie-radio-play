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
        #[arg(long, default_value = "energy", value_parser = ["energy", "spectral"])]
        vad_engine: String,
        #[arg(long)]
        calibration_profile: Option<PathBuf>,
        #[arg(long)]
        save_calibration: bool,
    },
    Tag {
        input_media: PathBuf,
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    Prompt {
        input_json: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        config: Option<PathBuf>,
    },
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
    Calibrate {
        corrections_dir: PathBuf,
        #[arg(long, default_value = "drama")]
        profile: String,
    },
    ApplyCalibration {
        #[arg(long, default_value = "analysis/learnings/latest-calibration.json")]
        report: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
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
        #[arg(long, default_value = "energy", value_parser = ["energy", "spectral"])]
        vad_engine: String,
        #[arg(long)]
        calibration_profile: Option<PathBuf>,
        #[arg(long, default_value = "analysis/benchmarks/latest.json")]
        output: PathBuf,
    },
    GenFixtures {
        #[arg(long, default_value = "testdata/generated")]
        output_dir: PathBuf,
    },
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
        #[arg(long, default_value = "energy", value_parser = ["energy", "spectral"])]
        vad_engine: String,
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
        #[arg(long, default_value = "analysis/validation/latest.json")]
        output: PathBuf,
    },
    AiVoiceExtract {
        input_json: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
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
        save_learning: bool,
    },
    UpdateThresholds {
        #[arg(long, default_value = "analysis/thresholds/learning-state.json")]
        learning_state: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
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
}
