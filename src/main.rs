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
use crate::io::json::{read_json, read_timeline, write_json_pretty};
use crate::learning::calibrator::{apply_calibration_report, run_calibration};
use crate::learning::profiles::CalibrationProfile;
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
            threshold,
            min_speech_ms,
            min_silence_ms,
            vad_engine,
            calibration_profile,
            save_calibration,
        } => {
            let cfg = load_analysis_config(
                config,
                threshold,
                min_speech_ms,
                min_silence_ms,
                vad_engine,
                calibration_profile,
            )?;

            if let Some(delta) = cfg.vad_threshold_delta.abs().partial_cmp(&0.0_f32) {
                if delta != std::cmp::Ordering::Equal {
                    info!(
                        base_threshold = cfg.energy_threshold,
                        delta = cfg.vad_threshold_delta,
                        effective = cfg.energy_threshold + cfg.vad_threshold_delta,
                        "applying calibration threshold delta"
                    );
                }
            }

            let timeline = extract_timeline(&input, &cfg)
                .with_context(|| format!("extract failed for {}", input.display()))?;
            write_json_pretty(&output, &timeline)
                .with_context(|| format!("failed writing {}", output.display()))?;

            if save_calibration {
                let profile_path = get_calibration_dir()?.join("latest.json");
                let profile = CalibrationProfile {
                    name: "runtime".to_string(),
                    energy_threshold_delta: cfg.vad_threshold_delta,
                    version: 1,
                };
                if let Some(parent) = profile_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&profile_path, serde_json::to_vec_pretty(&profile)?)?;
                info!(path = %profile_path.display(), "saved calibration profile");
            }
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
        Commands::Prompt {
            input_json,
            output,
            config,
        } => {
            let cfg = config::AnalysisConfig::from_args(config, None, None, None, None, None)?;
            let mut timeline = read_timeline(&input_json)?;
            add_prompts(&mut timeline, &cfg);
            write_json_pretty(&output, &timeline)?;
        }
        Commands::Calibrate {
            corrections_dir,
            profile,
        } => {
            let report_path = run_calibration(&corrections_dir, &profile)?;
            let output_profile = get_calibration_dir()?.join("latest.json");
            apply_calibration_report(&report_path, &output_profile)?;
            info!(
                report = %report_path.display(),
                output = %output_profile.display(),
                "saved calibration report and updated active profile"
            );
        }
        Commands::ApplyCalibration { report, output } => {
            let out_path = if let Some(path) = output {
                path
            } else {
                get_calibration_dir()?.join("latest.json")
            };
            apply_calibration_report(&report, &out_path)?;
            info!(
                report = %report.display(),
                output = %out_path.display(),
                "applied calibration report"
            );
        }
        Commands::Bench {
            input_media,
            config,
            threshold,
            min_speech_ms,
            min_silence_ms,
            vad_engine,
            calibration_profile,
            output,
        } => {
            let cfg = load_analysis_config(
                config,
                threshold,
                min_speech_ms,
                min_silence_ms,
                vad_engine,
                calibration_profile,
            )?;
            let benchmark = benchmark_file(&input_media, &cfg)?;
            write_json_pretty(&output, &benchmark)?;
        }
        Commands::GenFixtures { output_dir } => {
            validation::synthetic::generate_suite(&output_dir)?;
        }
        Commands::Validate {
            input_media,
            config,
            threshold,
            min_speech_ms,
            min_silence_ms,
            vad_engine,
            calibration_profile,
            truth_json,
            subtitles,
            dataset_manifest,
            total_ms,
            profile,
            output,
        } => {
            let cfg = load_analysis_config(
                config,
                threshold,
                min_speech_ms,
                min_silence_ms,
                vad_engine,
                calibration_profile,
            )?;
            let selected_inputs = [
                truth_json.is_some(),
                subtitles.is_some(),
                dataset_manifest.is_some(),
            ]
            .into_iter()
            .filter(|selected| *selected)
            .count();

            if selected_inputs != 1 {
                bail!("provide exactly one of --truth-json, --subtitles, or --dataset-manifest");
            }

            match (truth_json, subtitles, dataset_manifest) {
                (Some(truth_json), None, None) => {
                    let tolerance_ms = tolerance_for_profile(&profile);
                    validation::validate_file(
                        &input_media,
                        &truth_json,
                        &output,
                        &cfg,
                        tolerance_ms,
                        &profile,
                    )?;
                }
                (None, Some(subtitles), None) => {
                    let srt = std::fs::read_to_string(&subtitles)?;
                    let speech = validation::srt::parse_srt_segments(&srt)?;
                    let total = total_ms.context("--total-ms required for subtitle validation")?;
                    let truth = validation::timeline_from_speech_segments(
                        input_media.display().to_string(),
                        cfg.sample_rate_hz,
                        cfg.frame_ms,
                        &speech,
                        total,
                        cfg.min_non_voice_ms,
                    );
                    let predicted = extract_timeline(&input_media, &cfg)?;
                    let report = validation::validate_against_timeline(
                        &predicted,
                        &truth,
                        &profile,
                        tolerance_for_profile(&profile),
                    );
                    write_json_pretty(&output, &report)?;
                }
                (None, None, Some(dataset_manifest)) => {
                    let total = total_ms.context("--total-ms required for dataset validation")?;
                    let truth = validation::dataset::build_truth_from_manifest(
                        &dataset_manifest,
                        &input_media.display().to_string(),
                        cfg.sample_rate_hz,
                        cfg.frame_ms,
                        total,
                        cfg.min_non_voice_ms,
                    )?;
                    let predicted = extract_timeline(&input_media, &cfg)?;
                    let report = validation::validate_against_timeline(
                        &predicted,
                        &truth,
                        &profile,
                        tolerance_for_profile(&profile),
                    );
                    write_json_pretty(&output, &report)?;
                }
                _ => unreachable!("validation input source selection was checked above"),
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

fn load_analysis_config(
    config_path: Option<std::path::PathBuf>,
    threshold_override: Option<f32>,
    min_speech_override: Option<u32>,
    min_silence_override: Option<u32>,
    vad_engine: String,
    calibration_profile: Option<std::path::PathBuf>,
) -> Result<config::AnalysisConfig> {
    let threshold_delta = load_calibration_threshold_delta(calibration_profile.as_deref())?;
    config::AnalysisConfig::from_args(
        config_path,
        threshold_override,
        min_speech_override,
        min_silence_override,
        Some(vad_engine),
        threshold_delta,
    )
}

fn load_calibration_threshold_delta(profile_path: Option<&std::path::Path>) -> Result<Option<f32>> {
    let Some(profile_path) = profile_path else {
        return Ok(None);
    };
    let profile: CalibrationProfile = read_json(profile_path).with_context(|| {
        format!(
            "failed to read calibration profile: {}",
            profile_path.display()
        )
    })?;
    info!(profile = %profile.name, delta = profile.energy_threshold_delta, "loaded calibration profile");
    Ok(Some(profile.energy_threshold_delta))
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

fn get_calibration_dir() -> Result<std::path::PathBuf> {
    let base = if cfg!(target_os = "windows") {
        std::env::var("APPDATA")?
    } else if cfg!(target_os = "macos") {
        std::path::PathBuf::from(std::env::var("HOME")?)
            .join("Library/Application Support")
            .to_string_lossy()
            .to_string()
    } else {
        std::env::var("XDG_CONFIG_HOME")
            .or_else(|_| std::env::var("HOME").map(|h| format!("{h}/.config")))
            .map_err(|_| anyhow::anyhow!("Neither XDG_CONFIG_HOME nor HOME set"))?
    };
    Ok(std::path::PathBuf::from(base).join("do-movie-radio-play/profiles"))
}
