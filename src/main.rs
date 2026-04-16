mod cli;
mod config;
mod error;
mod io;
mod learning;
mod pipeline;
mod review;
mod types;
mod validation;
mod verification;

use anyhow::{bail, Context, Result};
use clap::Parser;
use tracing::{info, Level};
use tracing_subscriber::EnvFilter;

use crate::cli::{Cli, Commands};
use crate::config::{MergeOptions, VALID_MERGE_STRATEGIES};
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
            max_non_voice_ms,
            vad_engine,
            calibration_profile,
            save_calibration,
        } => {
            let cfg = load_analysis_config(
                config,
                threshold,
                min_speech_ms,
                min_silence_ms,
                max_non_voice_ms,
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
            let cfg =
                config::AnalysisConfig::from_args(config, None, None, None, None, None, None)?;
            let mut timeline = read_timeline(&input_json)?;
            add_prompts(&mut timeline, &cfg);
            write_json_pretty(&output, &timeline)?;
        }
        Commands::Review {
            input_media,
            input,
            output,
            pre_roll_s,
            post_roll_s,
            open,
            verified,
            merged,
        } => {
            if !pre_roll_s.is_finite() || pre_roll_s < 0.0 {
                bail!("--pre-roll-s must be a finite non-negative number");
            }
            if !post_roll_s.is_finite() || post_roll_s < 0.0 {
                bail!("--post-roll-s must be a finite non-negative number");
            }

            let timeline = read_timeline(&input)?;
            let count = review::write_review_html_with_options(
                &input_media,
                &timeline,
                &output,
                pre_roll_s,
                post_roll_s,
                verified.as_deref(),
                merged,
            )?;
            info!(
                output = %output.display(),
                non_voice_segments = count,
                verified = verified.is_some(),
                "review player generated"
            );

            if open {
                open_in_browser(&output)?;
            }
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
            max_non_voice_ms,
            vad_engine,
            calibration_profile,
            output,
        } => {
            let cfg = load_analysis_config(
                config,
                threshold,
                min_speech_ms,
                min_silence_ms,
                max_non_voice_ms,
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
            max_non_voice_ms,
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
                max_non_voice_ms,
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
        Commands::AiVoiceExtract { input_json, output } => {
            let timeline = read_timeline(&input_json)?;
            let speech_segments: Vec<crate::types::Segment> = timeline
                .segments
                .into_iter()
                .filter(|s| s.kind == crate::types::SegmentKind::Speech)
                .collect();
            let ai_voice_output = crate::types::AiVoiceOutput {
                file: timeline.file,
                analysis_sample_rate: timeline.analysis_sample_rate,
                frame_ms: timeline.frame_ms,
                segments: speech_segments,
            };
            write_json_pretty(&output, &ai_voice_output)?;
        }
        Commands::VerifyTimeline {
            media,
            timeline,
            output,
            entropy_min,
            entropy_max,
            flatness_max,
            energy_min,
            centroid_min,
            centroid_max,
            learning_state,
            learning_db,
            save_learning,
        } => {
            let timeline_data = read_timeline(&timeline)?;
            let report = crate::verification::verify_timeline(
                &media,
                &timeline_data,
                &output,
                entropy_min,
                entropy_max,
                flatness_max,
                energy_min,
                centroid_min,
                centroid_max,
            )?;

            info!(
                verified = report.summary.verified_count,
                suspicious = report.summary.suspicious_count,
                fp_rate = format!("{:.2}%", report.summary.false_positive_rate * 100.0),
                output = %output.display(),
                "verification complete"
            );

            if save_learning {
                let state_path = learning_state.unwrap_or_else(|| {
                    std::path::PathBuf::from("analysis/thresholds/learning-state.json")
                });
                let mut state = learning::adaptive_thresholds::create_learning_state(20);
                for (i, result) in report.segment_results.iter().enumerate() {
                    let was_fp = result.is_suspicious;
                    learning::adaptive_thresholds::record_verification_result(
                        &mut state,
                        i,
                        was_fp,
                        result.spectral_features.spectral_entropy,
                        result.spectral_features.spectral_flatness,
                        result.spectral_features.rms,
                        result.spectral_features.centroid_hz,
                    );
                }
                learning::adaptive_thresholds::save_learning_state(&state, &state_path)?;

                let db_path = learning_db
                    .unwrap_or_else(|| std::path::PathBuf::from("analysis/thresholds/learning.db"));
                if let Some(parent) = db_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .context("failed to create async runtime for learning db")?;
                let learning_db = rt.block_on(learning::database::LearningDb::new(&db_path))?;
                for result in &report.segment_results {
                    let segment = learning::database::VerifiedSegment {
                        start_ms: result.start_ms as i64,
                        end_ms: result.end_ms as i64,
                        confidence: result.original_confidence as f64,
                        spectral_features: learning::database::SpectralFeatures {
                            rms: result.spectral_features.rms as f64,
                            zcr: result.spectral_features.zcr as f64,
                            spectral_flux: result.spectral_features.spectral_flux as f64,
                            spectral_flatness: result.spectral_features.spectral_flatness as f64,
                            spectral_entropy: result.spectral_features.spectral_entropy as f64,
                            centroid_hz: result.spectral_features.centroid_hz as f64,
                            low_band_ratio: result.spectral_features.low_band_ratio as f64,
                            high_band_ratio: result.spectral_features.high_band_ratio as f64,
                        },
                        was_false_positive: result.is_suspicious,
                    };
                    rt.block_on(learning_db.record_verification(segment))?;
                }
                info!(path = %db_path.display(), "saved learning data to database");
            }
        }
        Commands::UpdateThresholds {
            learning_state,
            learning_db,
            output,
        } => {
            let recommendations_json = if let Some(db_path) = learning_db {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .context("failed to create async runtime for learning db")?;
                let learning_db = rt.block_on(learning::database::LearningDb::new(&db_path))?;
                let recommendations = rt.block_on(learning_db.get_threshold_recommendations())?;
                rt.block_on(learning_db.record_threshold(
                    recommendations.suggested_flatness_max,
                    recommendations.suggested_entropy_min,
                    recommendations.suggested_centroid_min,
                    recommendations.suggested_centroid_max,
                ))?;
                serde_json::to_vec_pretty(&recommendations)?
            } else {
                let mut state =
                    learning::adaptive_thresholds::load_learning_state(&learning_state)?;
                learning::adaptive_thresholds::adjust_thresholds_for_fp_rate(&mut state);
                let recommendations =
                    learning::adaptive_thresholds::generate_threshold_recommendations(&state);
                learning::adaptive_thresholds::save_learning_state(&state, &learning_state)?;
                serde_json::to_vec_pretty(&recommendations)?
            };

            let output_path = output.unwrap_or_else(|| {
                std::path::PathBuf::from("analysis/thresholds/recommendations.json")
            });
            if let Some(parent) = output_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&output_path, recommendations_json)?;

            info!(
                output = %output_path.display(),
                "threshold recommendations generated"
            );
        }
        Commands::LearningStats {
            learning_db,
            output,
        } => {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .context("failed to create async runtime for learning db")?;
            let db = rt.block_on(learning::database::LearningDb::new(&learning_db))?;
            let stats = rt.block_on(db.get_statistics())?;
            let recommendations = rt.block_on(db.get_threshold_recommendations())?;
            let latest_threshold = rt.block_on(db.get_latest_threshold())?;

            let report = serde_json::json!({
                "learning_db": learning_db,
                "statistics": stats,
                "recommendations": recommendations,
                "latest_threshold": latest_threshold,
            });

            if let Some(output_path) = output {
                if let Some(parent) = output_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&output_path, serde_json::to_vec_pretty(&report)?)?;
                info!(output = %output_path.display(), "learning stats written");
            } else {
                println!("{}", serde_json::to_string_pretty(&report)?);
            }
        }
        Commands::MergeTimeline {
            input,
            output,
            config,
            min_gap_to_merge,
            merge_strategy,
            verified,
        } => {
            let merge_opts =
                load_merge_options(config.as_deref(), min_gap_to_merge, merge_strategy)?;
            let timeline = read_timeline(&input)?;

            let verified_segment_keys: Option<std::collections::HashSet<(u64, u64)>> =
                if let Some(verified_path) = &verified {
                    let report: crate::verification::VerificationReport = read_json(verified_path)
                        .with_context(|| {
                            format!("failed to read verified file: {}", verified_path.display())
                        })?;
                    let keys: std::collections::HashSet<(u64, u64)> = report
                        .segment_results
                        .into_iter()
                        .filter(|r| {
                            matches!(
                                r.verification_status,
                                crate::verification::VerificationStatus::Verified
                            )
                        })
                        .map(|r| (r.start_ms, r.end_ms))
                        .collect();
                    Some(keys)
                } else {
                    None
                };

            let merged =
                merge_nonvoice_segments(&timeline, &merge_opts, verified_segment_keys.as_ref());
            write_json_pretty(&output, &merged)?;
            info!(
                original = timeline.segments.len(),
                merged = merged.segments.len(),
                verified_only = verified.is_some(),
                verified_segments = verified_segment_keys.as_ref().map(|k| k.len()).unwrap_or(0),
                strategy = %merge_opts.merge_strategy,
                min_gap = merge_opts.min_gap_to_merge,
                output = %output.display(),
                "merged timeline"
            );
        }
        Commands::Export {
            input,
            output,
            format,
            verified,
        } => {
            let timeline = read_timeline(&input)?;
            let verified_segments: Option<std::collections::HashSet<(u64, u64)>> =
                if let Some(verified_path) = &verified {
                    let report: crate::verification::VerificationReport = read_json(verified_path)
                        .with_context(|| {
                            format!("failed to read verified file: {}", verified_path.display())
                        })?;
                    let keys: std::collections::HashSet<(u64, u64)> = report
                        .segment_results
                        .into_iter()
                        .filter(|r| {
                            matches!(
                                r.verification_status,
                                crate::verification::VerificationStatus::Verified
                            )
                        })
                        .map(|r| (r.start_ms, r.end_ms))
                        .collect();
                    Some(keys)
                } else {
                    None
                };

            match format.as_str() {
                "json" => {
                    let export_data = crate::io::json::ExportData::from_timeline(
                        &timeline,
                        verified_segments.as_ref(),
                    );
                    write_json_pretty(&output, &export_data)?;
                    info!(format = "json", output = %output.display(), "exported timeline");
                }
                "edl" => {
                    let edl_content =
                        crate::io::edl::export_edl(&timeline, verified_segments.as_ref());
                    std::fs::write(&output, edl_content)?;
                    info!(format = "edl", output = %output.display(), "exported timeline");
                }
                "vtt" => {
                    let vtt_content =
                        crate::io::vtt::export_vtt(&timeline, verified_segments.as_ref());
                    std::fs::write(&output, vtt_content)?;
                    info!(format = "vtt", output = %output.display(), "exported timeline");
                }
                _ => bail!("unknown format: {format}"),
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
    max_non_voice_override: Option<u32>,
    vad_engine: String,
    calibration_profile: Option<std::path::PathBuf>,
) -> Result<config::AnalysisConfig> {
    let threshold_delta = load_calibration_threshold_delta(calibration_profile.as_deref())?;
    config::AnalysisConfig::from_args(
        config_path,
        threshold_override,
        min_speech_override,
        min_silence_override,
        max_non_voice_override,
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

fn load_merge_options(
    config_path: Option<&std::path::Path>,
    min_gap_override: Option<u32>,
    strategy_override: Option<String>,
) -> Result<MergeOptions> {
    let cfg = if let Some(path) = config_path {
        let data = std::fs::read_to_string(path).context("failed to read config file")?;
        let analysis_cfg: crate::config::AnalysisConfig =
            serde_json::from_str(&data).context("failed to parse config file")?;
        analysis_cfg.merge_options.unwrap_or_default()
    } else {
        MergeOptions::default()
    };

    let mut opts = cfg;
    if let Some(min_gap) = min_gap_override {
        opts.min_gap_to_merge = min_gap;
    }
    if let Some(strategy) = strategy_override {
        opts.merge_strategy = strategy;
    }

    validate_merge_options(&opts)?;
    Ok(opts)
}

fn validate_merge_options(opts: &MergeOptions) -> Result<()> {
    if !VALID_MERGE_STRATEGIES.contains(&opts.merge_strategy.as_str()) {
        bail!(
            "invalid merge_strategy: must be one of {}",
            VALID_MERGE_STRATEGIES.join(", ")
        );
    }
    if opts.min_gap_to_merge == 0 {
        bail!("invalid min_gap_to_merge: must be > 0");
    }
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

fn open_in_browser(path: &std::path::Path) -> Result<()> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let path_str = absolute.to_string_lossy().to_string();

    if is_wsl() {
        if try_open("wslview", &[&path_str])? {
            info!(path = %absolute.display(), opener = "wslview", "opened review output in browser");
            return Ok(());
        }

        if let Some(win_path) = wsl_to_windows_path(&path_str)? {
            if try_open("cmd.exe", &["/C", "start", "", &win_path])? {
                info!(path = %absolute.display(), opener = "cmd.exe/start", "opened review output in browser");
                return Ok(());
            }
            if try_open(
                "powershell.exe",
                &["-NoProfile", "-Command", "Start-Process", &win_path],
            )? {
                info!(path = %absolute.display(), opener = "powershell.exe", "opened review output in browser");
                return Ok(());
            }
        }
    }

    if cfg!(target_os = "macos") {
        if try_open("open", &[&path_str])? {
            info!(path = %absolute.display(), opener = "open", "opened review output in browser");
            return Ok(());
        }
    } else if cfg!(target_os = "windows") {
        if try_open("cmd", &["/C", "start", "", &path_str])? {
            info!(path = %absolute.display(), opener = "cmd/start", "opened review output in browser");
            return Ok(());
        }
        if try_open(
            "powershell",
            &["-NoProfile", "-Command", "Start-Process", &path_str],
        )? {
            info!(path = %absolute.display(), opener = "powershell", "opened review output in browser");
            return Ok(());
        }
    } else {
        let file_url = format!("file://{}", absolute.display());

        if let Some(default_browser) = linux_default_browser_command()? {
            if try_open(&default_browser, &["--new-window", &file_url])?
                || try_open(&default_browser, &[&file_url])?
            {
                info!(
                    path = %absolute.display(),
                    opener = %default_browser,
                    "opened review output in browser"
                );
                return Ok(());
            }
        }

        if let Some(browser_env) = std::env::var_os("BROWSER") {
            let browser_env = browser_env.to_string_lossy().to_string();
            for candidate in browser_env.split(':').filter(|c| !c.is_empty()) {
                if try_open(candidate, &[&file_url])? {
                    info!(path = %absolute.display(), opener = %candidate, "opened review output in browser");
                    return Ok(());
                }
            }
        }

        for candidate in ["google-chrome", "chromium-browser", "chromium", "firefox"] {
            if try_open(candidate, &["--new-window", &file_url])?
                || try_open(candidate, &[&file_url])?
            {
                info!(path = %absolute.display(), opener = %candidate, "opened review output in browser");
                return Ok(());
            }
        }

        if try_open("xdg-open", &[&path_str])? {
            info!(path = %absolute.display(), opener = "xdg-open", "opened review output in browser");
            return Ok(());
        }
        if try_open("gio", &["open", &path_str])? {
            info!(path = %absolute.display(), opener = "gio open", "opened review output in browser");
            return Ok(());
        }
    }

    bail!(
        "could not auto-open browser for {}; open it manually",
        absolute.display()
    )
}

fn linux_default_browser_command() -> Result<Option<String>> {
    let output = match std::process::Command::new("xdg-settings")
        .args(["get", "default-web-browser"])
        .output()
    {
        Ok(output) if output.status.success() => output,
        Ok(_) => return Ok(None),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err).context("failed running xdg-settings"),
    };

    let desktop = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if desktop.is_empty() {
        return Ok(None);
    }

    let command = desktop.strip_suffix(".desktop").unwrap_or(&desktop).trim();
    if command.is_empty() {
        Ok(None)
    } else {
        Ok(Some(command.to_string()))
    }
}

fn try_open(program: &str, args: &[&str]) -> Result<bool> {
    match std::process::Command::new(program).args(args).status() {
        Ok(status) => Ok(status.success()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(err).with_context(|| format!("failed to run browser opener: {program}")),
    }
}

fn wsl_to_windows_path(path: &str) -> Result<Option<String>> {
    match std::process::Command::new("wslpath")
        .args(["-w", path])
        .output()
    {
        Ok(output) if output.status.success() => {
            let win = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if win.is_empty() {
                Ok(None)
            } else {
                Ok(Some(win))
            }
        }
        Ok(_) => Ok(None),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err).context("failed running wslpath"),
    }
}

fn is_wsl() -> bool {
    if std::env::var_os("WSL_DISTRO_NAME").is_some() || std::env::var_os("WSL_INTEROP").is_some() {
        return true;
    }
    if let Ok(version) = std::fs::read_to_string("/proc/version") {
        let lower = version.to_ascii_lowercase();
        return lower.contains("microsoft") || lower.contains("wsl");
    }
    false
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

fn merge_nonvoice_segments(
    timeline: &crate::types::TimelineOutput,
    options: &MergeOptions,
    verified_keys: Option<&std::collections::HashSet<(u64, u64)>>,
) -> crate::types::TimelineOutput {
    let non_voice_segments: Vec<_> = timeline
        .segments
        .iter()
        .filter(|s| {
            if s.kind != crate::types::SegmentKind::NonVoice {
                return false;
            }
            if let Some(keys) = verified_keys {
                keys.contains(&(s.start_ms, s.end_ms))
            } else {
                true
            }
        })
        .collect();

    if non_voice_segments.is_empty() {
        return crate::types::TimelineOutput {
            file: timeline.file.clone(),
            analysis_sample_rate: timeline.analysis_sample_rate,
            frame_ms: timeline.frame_ms,
            segments: vec![],
        };
    }

    let merged = match options.merge_strategy.as_str() {
        "all" => merge_all(non_voice_segments),
        "longest" => merge_by_gap_threshold(non_voice_segments, options.min_gap_to_merge as u64),
        "sparse" => merge_sparse_segments(non_voice_segments, options.min_gap_to_merge as u64),
        _ => merge_all(non_voice_segments),
    };

    crate::types::TimelineOutput {
        file: timeline.file.clone(),
        analysis_sample_rate: timeline.analysis_sample_rate,
        frame_ms: timeline.frame_ms,
        segments: merged,
    }
}

fn merge_all(segments: Vec<&crate::types::Segment>) -> Vec<crate::types::Segment> {
    if segments.is_empty() {
        return vec![];
    }

    let first_start = segments.first().map(|s| s.start_ms).unwrap_or(0);
    let last_end = segments.last().map(|s| s.end_ms).unwrap_or(0);

    let avg_confidence: f32 =
        segments.iter().map(|s| s.confidence).sum::<f32>() / segments.len() as f32;

    let all_tags: Vec<String> = segments.iter().flat_map(|s| s.tags.clone()).collect();

    vec![crate::types::Segment {
        start_ms: first_start,
        end_ms: last_end,
        kind: crate::types::SegmentKind::NonVoice,
        confidence: avg_confidence,
        tags: all_tags,
        prompt: None,
    }]
}

fn merge_by_gap_threshold(
    segments: Vec<&crate::types::Segment>,
    min_gap_ms: u64,
) -> Vec<crate::types::Segment> {
    if segments.is_empty() {
        return vec![];
    }

    let mut merged = Vec::new();
    let mut current_start = segments.first().unwrap().start_ms;
    let mut current_end = segments.first().unwrap().end_ms;
    let mut current_confidence = segments.first().unwrap().confidence;
    let mut current_tags: Vec<String> = segments.first().unwrap().tags.clone();

    for segment in segments.iter().skip(1) {
        let gap = segment.start_ms - current_end;
        if gap >= min_gap_ms {
            merged.push(crate::types::Segment {
                start_ms: current_start,
                end_ms: current_end,
                kind: crate::types::SegmentKind::NonVoice,
                confidence: current_confidence,
                tags: std::mem::take(&mut current_tags),
                prompt: None,
            });
            current_start = segment.start_ms;
            current_confidence = segment.confidence;
            current_tags = segment.tags.clone();
        }
        current_end = segment.end_ms;
    }

    merged.push(crate::types::Segment {
        start_ms: current_start,
        end_ms: current_end,
        kind: crate::types::SegmentKind::NonVoice,
        confidence: current_confidence,
        tags: current_tags,
        prompt: None,
    });

    merged
}

fn merge_sparse_segments(
    segments: Vec<&crate::types::Segment>,
    min_gap_ms: u64,
) -> Vec<crate::types::Segment> {
    if segments.is_empty() {
        return vec![];
    }

    let mut merged = Vec::new();
    let mut current_start = segments.first().unwrap().start_ms;
    let mut current_end = segments.first().unwrap().end_ms;
    let mut current_confidence = segments.first().unwrap().confidence;
    let mut current_tags: Vec<String> = segments.first().unwrap().tags.clone();

    for segment in segments.iter().skip(1) {
        let gap = segment.start_ms - current_end;
        if gap >= min_gap_ms {
            merged.push(crate::types::Segment {
                start_ms: current_start,
                end_ms: current_end,
                kind: crate::types::SegmentKind::NonVoice,
                confidence: current_confidence,
                tags: std::mem::take(&mut current_tags),
                prompt: None,
            });
            current_start = segment.start_ms;
            current_confidence = segment.confidence;
            current_tags = segment.tags.clone();
        } else {
            current_confidence = (current_confidence + segment.confidence) / 2.0;
            current_tags.extend(segment.tags.clone());
        }
        current_end = segment.end_ms;
    }

    merged.push(crate::types::Segment {
        start_ms: current_start,
        end_ms: current_end,
        kind: crate::types::SegmentKind::NonVoice,
        confidence: current_confidence,
        tags: current_tags,
        prompt: None,
    });

    merged
}
