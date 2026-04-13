mod cli;
mod config;
mod error;
mod io;
mod learning;
mod pipeline;
mod types;
mod validation;

use anyhow::{bail, Context, Result};
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
        Commands::GenFixtures { output_dir } => {
            validation::synthetic::generate_suite(&output_dir)?;
        }
        Commands::Validate {
            input_media,
            truth_json,
            subtitles,
            dataset_manifest,
            total_ms,
            profile,
            output,
        } => {
            let cfg = config::AnalysisConfig::default();
            if let Some(truth_json) = truth_json {
                let tolerance_ms = tolerance_for_profile(&profile);
                validation::validate_file(
                    &input_media,
                    &truth_json,
                    &output,
                    &cfg,
                    tolerance_ms,
                    &profile,
                )?;
            } else if let Some(subtitles) = subtitles {
                let srt = std::fs::read_to_string(&subtitles)?;
                let speech = validation::srt::parse_srt_segments(&srt)?;
                let total = total_ms.context("--total-ms required for subtitle validation")?;
                let truth = validation::timeline_from_speech_segments(
                    input_media.display().to_string(),
                    16000,
                    20,
                    &speech,
                    total,
                    1000,
                );
                let predicted = extract_timeline(&input_media, &cfg)?;
                let report = validation::validate_against_timeline(
                    &predicted,
                    &truth,
                    &profile,
                    tolerance_for_profile(&profile),
                );
                write_json_pretty(&output, &report)?;
            } else if let Some(dataset_manifest) = dataset_manifest {
                let total = total_ms.context("--total-ms required for dataset validation")?;
                let truth = validation::dataset::build_truth_from_manifest(
                    &dataset_manifest,
                    &input_media.display().to_string(),
                    total,
                    1000,
                )?;
                let predicted = extract_timeline(&input_media, &cfg)?;
                let report = validation::validate_against_timeline(
                    &predicted,
                    &truth,
                    &profile,
                    tolerance_for_profile(&profile),
                );
                write_json_pretty(&output, &report)?;
            } else {
                bail!("provide one of --truth-json, --subtitles, or --dataset-manifest")
            }
        }
    }

    info!("timeline command end");
    Ok(())
}

fn tolerance_for_profile(profile: &str) -> u64 {
    match profile {
        "synthetic" => 100,
        "dataset" => 200,
        _ => 400,
    }
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
