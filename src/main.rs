mod cli;
mod config;
mod error;
mod io;
mod learning;
mod merge;
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
use crate::config::{
    get_calibration_dir, load_analysis_config, load_merge_options, load_tag_rules,
    tolerance_for_profile,
};
use crate::io::browser::open_in_browser;
use crate::io::json::{read_json, read_timeline, write_json_pretty};
use crate::learning::calibrator::{apply_calibration_report, run_calibration};
use crate::learning::profiles::CalibrationProfile;
use crate::merge::merge_nonvoice_segments;
use crate::pipeline::prompts::add_prompts;
use crate::pipeline::tags::add_tags;
use crate::pipeline::{benchmark_file, extract_timeline};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
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
            parallel_features,
        } => {
            let cfg = load_analysis_config(
                config,
                threshold,
                min_speech_ms,
                min_silence_ms,
                max_non_voice_ms,
                vad_engine,
                calibration_profile,
                parallel_features,
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
                    tag_thresholds: None,
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
            calibration_profile,
        } => {
            let mut timeline = read_timeline(&input)?;
            let rules = load_tag_rules(calibration_profile.as_deref())?;
            add_tags(&input_media, &mut timeline, rules.as_ref())?;
            write_json_pretty(&output, &timeline)?;
        }
        Commands::Prompt {
            input_json,
            output,
            config,
        } => {
            let cfg = config::AnalysisConfig::from_args(
                config, None, None, None, None, None, None, None,
            )?;
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
            parallel_features,
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
                parallel_features,
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
            parallel_features,
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
                parallel_features,
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
                        cfg.frame_ms,
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
                        cfg.frame_ms,
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
            use_fingerprints,
            fingerprint_threshold,
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
                use_fingerprints,
                fingerprint_threshold,
                learning_db.clone(),
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
                let local = tokio::task::LocalSet::new();
                local
                    .run_until(async {
                        let learning_db = learning::database::LearningDb::new(&db_path).await?;
                        for (i, result) in report.segment_results.iter().enumerate() {
                            let segment = learning::database::VerifiedSegment {
                                start_ms: result.start_ms as i64,
                                end_ms: result.end_ms as i64,
                                confidence: result.original_confidence as f64,
                                spectral_features: learning::database::SpectralFeatures {
                                    rms: result.spectral_features.rms as f64,
                                    zcr: result.spectral_features.zcr as f64,
                                    spectral_flux: result.spectral_features.spectral_flux as f64,
                                    spectral_flatness: result.spectral_features.spectral_flatness
                                        as f64,
                                    spectral_entropy: result.spectral_features.spectral_entropy
                                        as f64,
                                    centroid_hz: result.spectral_features.centroid_hz as f64,
                                    low_band_ratio: result.spectral_features.low_band_ratio as f64,
                                    high_band_ratio: result.spectral_features.high_band_ratio
                                        as f64,
                                },
                                was_false_positive: result.is_suspicious,
                            };
                            let segment_id = learning_db.record_verification(segment).await?;

                            if let Some(fps) = report.segment_fingerprints.get(i) {
                                learning_db.record_fingerprints(segment_id, fps).await?;
                            }
                        }
                        Ok::<(), anyhow::Error>(())
                    })
                    .await?;
                info!(path = %db_path.display(), "saved learning data to database");
            }
        }
        Commands::UpdateThresholds {
            learning_state,
            learning_db,
            output,
        } => {
            let recommendations_json = if let Some(db_path) = learning_db {
                let local = tokio::task::LocalSet::new();
                local
                    .run_until(async {
                        let learning_db = learning::database::LearningDb::new(&db_path).await?;
                        let recommendations = learning_db.get_threshold_recommendations().await?;
                        learning_db
                            .record_threshold(
                                recommendations.suggested_flatness_max,
                                recommendations.suggested_entropy_min,
                                recommendations.suggested_centroid_min,
                                recommendations.suggested_centroid_max,
                            )
                            .await?;
                        Ok::<_, anyhow::Error>(serde_json::to_vec_pretty(&recommendations)?)
                    })
                    .await?
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
            let local = tokio::task::LocalSet::new();
            let report = local
                .run_until(async {
                    let db = learning::database::LearningDb::new(&learning_db).await?;
                    let stats = db.get_statistics().await?;
                    let recommendations = db.get_threshold_recommendations().await?;
                    let latest_threshold = db.get_latest_threshold().await?;

                    Ok::<_, anyhow::Error>(serde_json::json!({
                        "learning_db": learning_db,
                        "statistics": stats,
                        "recommendations": recommendations,
                        "latest_threshold": latest_threshold,
                    }))
                })
                .await?;

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

fn init_logging() {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::from(Level::INFO.as_str()));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .compact()
        .init();
}
