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
        #[arg(long, default_value = "energy", value_parser = ["energy"])]
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
        #[arg(long, default_value = "energy", value_parser = ["energy"])]
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
        #[arg(long, default_value = "energy", value_parser = ["energy"])]
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
}
