mod cli;
mod config;
mod error;
mod io;
mod learning;
mod pipeline;
mod types;

use anyhow::{Context, Result};
use clap::Parser;
use tracing::{info, Level};
use tracing_subscriber::EnvFilter;

use crate::cli::{Cli, Commands};
use crate::io::json::{read_timeline, write_json_pretty};
use crate::learning::calibrator::run_calibration;
use crate::pipeline::prompts::add_prompts;
use crate::pipeline::tags::add_tags;
use crate::pipeline::{benchmark_file, extract_timeline};

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    init_logging();
    let cli = Cli::parse();
    info!(command = ?cli.command, "timeline command start");

    match cli.command {
        Commands::Extract {
            input,
            output,
            config,
        } => {
            let cfg = config::load_config(config)?;
            let timeline = extract_timeline(&input, &cfg)
                .with_context(|| format!("extract failed for {}", input.display()))?;
            write_json_pretty(&output, &timeline)
                .with_context(|| format!("failed writing {}", output.display()))?;
        }
        Commands::Tag {
            input_media,
            input,
            output,
        } => {
            let mut timeline = read_timeline(&input)?;
            add_tags(&input_media, &mut timeline)?;
            write_json_pretty(&output, &timeline)?;
        }
        Commands::Prompt { input_json, output } => {
            let mut timeline = read_timeline(&input_json)?;
            add_prompts(&mut timeline);
            write_json_pretty(&output, &timeline)?;
        }
        Commands::Calibrate {
            corrections_dir,
            profile,
        } => {
            run_calibration(&corrections_dir, &profile)?;
        }
        Commands::Bench {
            input_media,
            output,
        } => {
            let benchmark = benchmark_file(&input_media)?;
            write_json_pretty(&output, &benchmark)?;
        }
    }

    info!("timeline command end");
    Ok(())
}

fn init_logging() {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::from(Level::INFO.as_str()));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .compact()
        .init();
}
