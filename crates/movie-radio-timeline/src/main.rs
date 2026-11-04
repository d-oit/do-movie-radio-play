mod app_config_loader;
mod cli;
mod cli_review_feedback;
mod config;
mod handlers;
mod merge;
mod review;
mod review_template;
mod util;
mod voice_clone_handler;

use anyhow::Result;
use clap::Parser;
use tracing::info;

use crate::cli::{Cli, Commands};
use crate::cli_review_feedback::ReviewFeedbackCommands;

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    util::init_logging();
    let cli = Cli::parse();
    info!(command = ?cli.command, "timeline command start");
    dispatch_command(cli.command)?;
    info!("timeline command end");
    Ok(())
}

/// Dispatch processing/analysis commands (high-frequency commands).
fn dispatch_command(cmd: Commands) -> Result<()> {
    match cmd {
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
            chunk_duration,
        } => handlers::handle_extract(
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
            chunk_duration,
        ),
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
        } => handlers::handle_validate(
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
        ),
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
        } => handlers::handle_bench(
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
        ),
        Commands::GenFixtures { output_dir } => handlers::handle_gen_fixtures(output_dir),
        Commands::Tag {
            input_media,
            input,
            output,
            calibration_profile,
        } => handlers::handle_tag(input_media, input, output, calibration_profile),
        Commands::Prompt {
            input_json,
            output,
            config,
        } => handlers::handle_prompt(input_json, output, config),
        Commands::Review {
            input_media,
            input,
            output,
            pre_roll_s,
            post_roll_s,
            open,
            verified,
            merged,
        } => handlers::handle_review(
            input_media,
            input,
            output,
            pre_roll_s,
            post_roll_s,
            open,
            verified,
            merged,
        ),
        Commands::AiVoiceExtract { input_json, output } => {
            handlers::handle_ai_voice_extract(input_json, output)
        }
        Commands::RadioPlay {
            movie,
            timeline,
            subtitles,
            output,
            analyze_only,
            verify_quality,
            apply_learnings,
            learning_state,
            learning_db,
            no_learn,
            voice_reference,
            character,
            language,
            gap_confidence,
            vad_engine,
            no_auto_cues,
            mastering_profile,
        } => handlers::handle_radio_play(
            movie,
            handlers::RadioPlayOptions {
                timeline,
                subtitles,
                output,
                analyze_only,
                verify_quality,
                apply_learnings,
                learning_state,
                learning_db,
                no_learn,
                voice_reference,
                character,
                language,
                gap_confidence,
                vad_engine,
                no_auto_cues,
                mastering_profile,
            },
        ),
        Commands::Calibrate {
            corrections_dir,
            profile,
        } => handlers::handle_calibrate(corrections_dir, profile),
        Commands::Preview {
            input,
            skip,
            duration,
        } => handlers::preview::handle_preview(input, skip, duration),
        Commands::Config { command } => match command {
            crate::cli::ConfigCommands::Validate { config } => {
                let path =
                    config.unwrap_or_else(|| std::path::PathBuf::from("config/default.toml"));
                crate::app_config_loader::validate_config_file(&path)?;
                println!("config valid: {}", path.display());
                Ok(())
            }
        },
        Commands::Voice { command } => match command {
            crate::cli::VoiceCommands::Samples {
                character,
                input,
                output,
            } => crate::voice_clone_handler::handle_voice_samples(character, input, output),
            crate::cli::VoiceCommands::List => crate::voice_clone_handler::handle_voice_list(),
            crate::cli::VoiceCommands::Test {
                character,
                text,
                samples_from,
            } => crate::voice_clone_handler::handle_voice_test(character, text, samples_from),
        },
        Commands::Narrate {
            scene,
            dry_run,
            config,
            template,
        } => {
            let cfg = crate::app_config_loader::load_app_config(config)?;
            // Delegate to pipeline narrator handler
            movie_radio_pipeline::narrator::handle_narrate(scene, dry_run, template, &cfg.narrator)
        }
        Commands::Mastering {
            input,
            profile,
            target_lufs,
            max_true_peak,
            json,
            html,
            output,
        } => handlers::handle_mastering(
            input,
            profile,
            target_lufs,
            max_true_peak,
            json,
            html,
            output,
        ),
        Commands::Produce {
            input,
            config,
            resume,
            dry_run,
            output,
            verify_quality,
            timeline,
            subtitles,
            language,
            gap_confidence,
            no_auto_cues,
            no_learn,
            mastering_profile,
        } => {
            let cfg = crate::app_config_loader::load_app_config(config)?;
            handlers::handle_produce(
                handlers::ProduceOptions {
                    input,
                    output,
                    resume,
                    dry_run,
                    verify_quality,
                    timeline,
                    subtitles,
                    language,
                    gap_confidence,
                    no_auto_cues,
                    no_learn,
                    mastering_profile,
                },
                &cfg,
            )
        }
        Commands::Report {
            run_dir,
            json,
            validate,
        } => handlers::handle_report(handlers::ReportOptions {
            run_dir,
            json,
            validate,
        }),
        Commands::Sfx { command } => match command {
            crate::cli::SfxCommands::CacheInspect { config, json } => {
                let cfg = crate::app_config_loader::load_app_config(config)?;
                handlers::handle_sfx_cache_inspect(&cfg, handlers::SfxCacheInspectOptions { json })
            }
            crate::cli::SfxCommands::CacheClean {
                config,
                run_dirs,
                all,
                dry_run,
            } => {
                let cfg = crate::app_config_loader::load_app_config(config)?;
                handlers::handle_sfx_cache_clean(
                    &cfg,
                    handlers::SfxCacheCleanOptions {
                        run_dirs,
                        all,
                        dry_run,
                    },
                )
            }
        },
        rest => dispatch_verification_and_output(rest),
    }
}

/// Dispatch verification/calibration/output commands.
fn dispatch_verification_and_output(cmd: Commands) -> Result<()> {
    match cmd {
        Commands::ApplyCalibration { report, output } => {
            handlers::handle_apply_calibration(report, output)
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
        } => handlers::handle_verify_timeline(
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
        ),
        Commands::UpdateThresholds {
            learning_state,
            learning_db,
            output,
        } => handlers::handle_update_thresholds(learning_state, learning_db, output),
        Commands::LearningStats {
            radio_play,
            learning_db,
            output,
        } => handlers::handle_learning_stats(learning_db, radio_play, output),
        Commands::LearningLog {
            last,
            learning_db,
            output,
        } => handlers::handle_learning_log(last, learning_db, output),
        Commands::ResetLearnings {
            confirm,
            learning_db,
        } => handlers::handle_reset_learnings(confirm, learning_db),
        Commands::ExportLearnings {
            output,
            learning_db,
        } => handlers::handle_export_learnings(output, learning_db),
        Commands::LearningExperiments {
            learning_db,
            output,
        } => handlers::handle_learning_experiments(learning_db, output),
        Commands::ReviewFeedback { command } => dispatch_review_feedback(command),
        Commands::MergeTimeline {
            input,
            output,
            config,
            min_gap_to_merge,
            merge_strategy,
            verified,
        } => handlers::handle_merge_timeline(
            input,
            output,
            config,
            min_gap_to_merge,
            merge_strategy,
            verified,
        ),
        Commands::Export {
            input,
            output,
            format,
            verified,
        } => handlers::handle_export(input, output, format, verified),
        _ => unreachable!(),
    }
}

fn dispatch_review_feedback(cmd: ReviewFeedbackCommands) -> Result<()> {
    match cmd {
        ReviewFeedbackCommands::Import {
            input,
            learning_db,
            reviewer,
            dry_run,
            output,
        } => handlers::handle_import_review_feedback(input, learning_db, reviewer, dry_run, output),
        ReviewFeedbackCommands::Proposals {
            learning_db,
            output,
        } => handlers::handle_review_proposals(learning_db, output),
        ReviewFeedbackCommands::Apply {
            proposal_id,
            confirm,
            learning_db,
        } => handlers::handle_apply_review_proposal(proposal_id, confirm, learning_db),
        ReviewFeedbackCommands::Rollback {
            proposal_id,
            confirm,
            learning_db,
        } => handlers::handle_rollback_review_proposal(proposal_id, confirm, learning_db),
        ReviewFeedbackCommands::Metrics {
            learning_db,
            output,
        } => handlers::handle_review_metrics(learning_db, output),
        ReviewFeedbackCommands::RegisterHoldout {
            fixture_id,
            path,
            learning_db,
        } => handlers::handle_register_holdout(fixture_id, path, learning_db),
        ReviewFeedbackCommands::Export {
            output,
            learning_db,
            include_artifact_paths,
        } => handlers::handle_export_review_feedback(output, learning_db, include_artifact_paths),
    }
}
