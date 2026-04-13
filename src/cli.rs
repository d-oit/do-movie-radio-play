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
    },
    Calibrate {
        corrections_dir: PathBuf,
        #[arg(long, default_value = "drama")]
        profile: String,
    },
    Bench {
        input_media: PathBuf,
        #[arg(long, default_value = "analysis/benchmarks/latest.json")]
        output: PathBuf,
    },
}
