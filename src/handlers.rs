//! Command handler implementations extracted from main.rs.
//! Each function handles one CLI command.

use anyhow::{bail, Context, Result};
use std::path::PathBuf;
use tracing::info;

use crate::config;
use crate::io::json::{read_json, read_timeline, write_json_pretty};
use crate::learning;
use crate::learning::calibrator::apply_calibration_report;
use crate::learning::calibrator::run_calibration;
use crate::learning::profiles::CalibrationProfile;
use crate::merge::load_merge_options;
use crate::pipeline::prompts::add_prompts;
use crate::pipeline::tags::{add_tags, TagRules};
use crate::pipeline::{benchmark_file, extract_timeline};
use crate::review;
use crate::types;
use crate::util;
use crate::validation;

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub(crate) fn load_analysis_config(
    config_path: Option<PathBuf>,
    threshold_override: Option<f32>,
    min_speech_override: Option<u32>,
    min_silence_override: Option<u32>,
    max_non_voice_override: Option<u32>,
    vad_engine: String,
    calibration_profile: Option<PathBuf>,
    parallel_features: Option<bool>,
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
        parallel_features,
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
    info!(
        profile = %profile.name,
        delta = profile.energy_threshold_delta,
        "loaded calibration profile"
    );
    Ok(Some(profile.energy_threshold_delta))
}

fn load_tag_rules(profile_path: Option<&std::path::Path>) -> Result<Option<TagRules>> {
    let Some(profile_path) = profile_path else {
        return Ok(None);
    };
    let profile: CalibrationProfile = read_json(profile_path).with_context(|| {
        format!(
            "failed to read calibration profile: {}",
            profile_path.display()
        )
    })?;
    let rules = profile
        .tag_thresholds
        .as_ref()
        .map(TagRules::from_thresholds);
    if let Some(rules) = &rules {
        info!(
            profile = %profile.name,
            ambience_max_rms = rules.ambience_max_rms,
            impact_min_rms = rules.impact_min_rms,
            min_centroid_hz = rules.min_centroid_hz,
            "loaded tag rules from calibration profile"
        );
    } else {
        info!(
            profile = %profile.name,
            "profile has no tag thresholds, using defaults"
        );
    }
    Ok(rules)
}

// ---------------------------------------------------------------------------
// Command: Extract
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_extract(
    input: PathBuf,
    output: PathBuf,
    config_path: Option<PathBuf>,
    threshold: Option<f32>,
    min_speech_ms: Option<u32>,
    min_silence_ms: Option<u32>,
    max_non_voice_ms: Option<u32>,
    vad_engine: String,
    calibration_profile: Option<PathBuf>,
    save_calibration: bool,
    parallel_features: Option<bool>,
) -> Result<()> {
    let cfg = load_analysis_config(
        config_path,
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
        let profile_path = util::get_calibration_dir()?.join("latest.json");
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
    Ok(())
}

// ---------------------------------------------------------------------------
// Command: Validate / Eval
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_validate(
    input_media: PathBuf,
    config_path: Option<PathBuf>,
    threshold: Option<f32>,
    min_speech_ms: Option<u32>,
    min_silence_ms: Option<u32>,
    max_non_voice_ms: Option<u32>,
    vad_engine: String,
    calibration_profile: Option<PathBuf>,
    truth_json: Option<PathBuf>,
    subtitles: Option<PathBuf>,
    dataset_manifest: Option<PathBuf>,
    total_ms: Option<u64>,
    profile: String,
    parallel_features: Option<bool>,
    output: PathBuf,
) -> Result<()> {
    let cfg = load_analysis_config(
        config_path,
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
            let tolerance_ms = util::tolerance_for_profile(&profile);
            crate::validation::validate_file(
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
            let speech = crate::validation::srt::parse_srt_segments(&srt)?;
            let total = total_ms.context("--total-ms required for subtitle validation")?;
            let truth = crate::validation::timeline_from_speech_segments(
                input_media.display().to_string(),
                cfg.sample_rate_hz,
                cfg.frame_ms,
                &speech,
                total,
                cfg.frame_ms,
            );
            let predicted = extract_timeline(&input_media, &cfg)?;
            let report = crate::validation::validate_against_timeline(
                &predicted,
                &truth,
                &profile,
                util::tolerance_for_profile(&profile),
            );
            write_json_pretty(&output, &report)?;
        }
        (None, None, Some(dataset_manifest)) => {
            let total = total_ms.context("--total-ms required for dataset validation")?;
            let truth = crate::validation::dataset::build_truth_from_manifest(
                &dataset_manifest,
                &input_media.display().to_string(),
                cfg.sample_rate_hz,
                cfg.frame_ms,
                total,
                cfg.frame_ms,
            )?;
            let predicted = extract_timeline(&input_media, &cfg)?;
            let report = crate::validation::validate_against_timeline(
                &predicted,
                &truth,
                &profile,
                util::tolerance_for_profile(&profile),
            );
            write_json_pretty(&output, &report)?;
        }
        _ => unreachable!("validation input source selection was checked above"),
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Command: VerifyTimeline
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_verify_timeline(
    media: PathBuf,
    timeline: PathBuf,
    output: PathBuf,
    entropy_min: Option<f32>,
    entropy_max: Option<f32>,
    flatness_max: Option<f32>,
    energy_min: Option<f32>,
    centroid_min: Option<f32>,
    centroid_max: Option<f32>,
    learning_state: Option<PathBuf>,
    learning_db: Option<PathBuf>,
    save_learning: bool,
    use_fingerprints: bool,
    fingerprint_threshold: u32,
) -> Result<()> {
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
        let state_path = learning_state
            .unwrap_or_else(|| std::path::PathBuf::from("analysis/thresholds/learning-state.json"));
        let mut state = crate::learning::adaptive_thresholds::create_learning_state(20);
        for (i, result) in report.segment_results.iter().enumerate() {
            let was_fp = result.is_suspicious;
            crate::learning::adaptive_thresholds::record_verification_result(
                &mut state,
                i,
                was_fp,
                result.spectral_features.spectral_entropy,
                result.spectral_features.spectral_flatness,
                result.spectral_features.rms,
                result.spectral_features.centroid_hz,
            );
        }
        crate::learning::adaptive_thresholds::save_learning_state(&state, &state_path)?;

        let db_path = learning_db
            .unwrap_or_else(|| std::path::PathBuf::from("analysis/thresholds/learning.db"));
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .context("failed to create async runtime for learning db")?;
        let learning_db = rt.block_on(crate::learning::database::LearningDb::new(&db_path))?;
        for (i, result) in report.segment_results.iter().enumerate() {
            let segment = crate::learning::database::VerifiedSegment {
                start_ms: result.start_ms as i64,
                end_ms: result.end_ms as i64,
                confidence: result.original_confidence as f64,
                spectral_features: crate::learning::database::SpectralFeatures {
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
            let segment_id = rt.block_on(learning_db.record_verification(segment))?;

            // Also store fingerprints
            if let Some(fps) = report.segment_fingerprints.get(i) {
                rt.block_on(learning_db.record_fingerprints(segment_id, fps))?;
            }
        }
        info!(path = %db_path.display(), "saved learning data to database");
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Command: UpdateThresholds
// ---------------------------------------------------------------------------

pub(crate) fn handle_update_thresholds(
    learning_state: PathBuf,
    learning_db: Option<PathBuf>,
    output: Option<PathBuf>,
) -> Result<()> {
    let recommendations_json = if let Some(db_path) = learning_db {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .context("failed to create async runtime for learning db")?;
        let learning_db = rt.block_on(crate::learning::database::LearningDb::new(&db_path))?;
        let recommendations = rt.block_on(learning_db.get_threshold_recommendations())?;
        rt.block_on(learning_db.record_threshold(
            recommendations.suggested_flatness_max,
            recommendations.suggested_entropy_min,
            recommendations.suggested_centroid_min,
            recommendations.suggested_centroid_max,
        ))?;
        serde_json::to_vec_pretty(&recommendations)?
    } else {
        let mut state = crate::learning::adaptive_thresholds::load_learning_state(&learning_state)?;
        crate::learning::adaptive_thresholds::adjust_thresholds_for_fp_rate(&mut state);
        let recommendations =
            crate::learning::adaptive_thresholds::generate_threshold_recommendations(&state);
        crate::learning::adaptive_thresholds::save_learning_state(&state, &learning_state)?;
        serde_json::to_vec_pretty(&recommendations)?
    };

    let output_path = output
        .unwrap_or_else(|| std::path::PathBuf::from("analysis/thresholds/recommendations.json"));
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&output_path, recommendations_json)?;

    info!(
        output = %output_path.display(),
        "threshold recommendations generated"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Command: MergeTimeline
// ---------------------------------------------------------------------------

pub(crate) fn handle_merge_timeline(
    input: PathBuf,
    output: PathBuf,
    config_path: Option<PathBuf>,
    min_gap_to_merge: Option<u32>,
    merge_strategy: Option<String>,
    verified: Option<PathBuf>,
) -> Result<()> {
    let merge_opts = load_merge_options(config_path.as_deref(), min_gap_to_merge, merge_strategy)?;
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

    let merged = crate::merge::merge_nonvoice_segments(
        &timeline,
        &merge_opts,
        verified_segment_keys.as_ref(),
    );
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
    Ok(())
}

// ---------------------------------------------------------------------------
// Command: Export
// ---------------------------------------------------------------------------

pub(crate) fn handle_export(
    input: PathBuf,
    output: PathBuf,
    format: String,
    verified: Option<PathBuf>,
) -> Result<()> {
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
            let export_data =
                crate::io::json::ExportData::from_timeline(&timeline, verified_segments.as_ref());
            write_json_pretty(&output, &export_data)?;
            info!(format = "json", output = %output.display(), "exported timeline");
        }
        "edl" => {
            let edl_content = crate::io::edl::export_edl(&timeline, verified_segments.as_ref());
            std::fs::write(&output, edl_content)?;
            info!(format = "edl", output = %output.display(), "exported timeline");
        }
        "vtt" => {
            let vtt_content = crate::io::vtt::export_vtt(&timeline, verified_segments.as_ref());
            std::fs::write(&output, vtt_content)?;
            info!(format = "vtt", output = %output.display(), "exported timeline");
        }
        _ => bail!("unknown format: {format}"),
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Command: Tag
// ---------------------------------------------------------------------------

pub(crate) fn handle_tag(
    input_media: std::path::PathBuf,
    input: std::path::PathBuf,
    output: std::path::PathBuf,
    calibration_profile: Option<std::path::PathBuf>,
) -> Result<()> {
    let mut timeline = read_timeline(&input)?;
    let rules = load_tag_rules(calibration_profile.as_deref())?;
    add_tags(&input_media, &mut timeline, rules.as_ref())?;
    write_json_pretty(&output, &timeline)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Command: Prompt
// ---------------------------------------------------------------------------

pub(crate) fn handle_prompt(
    input_json: std::path::PathBuf,
    output: std::path::PathBuf,
    config_path: Option<std::path::PathBuf>,
) -> Result<()> {
    let cfg =
        config::AnalysisConfig::from_args(config_path, None, None, None, None, None, None, None)?;
    let mut timeline = read_timeline(&input_json)?;
    add_prompts(&mut timeline, &cfg);
    write_json_pretty(&output, &timeline)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Command: Review
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_review(
    input_media: std::path::PathBuf,
    input: std::path::PathBuf,
    output: std::path::PathBuf,
    pre_roll_s: f32,
    post_roll_s: f32,
    open: bool,
    verified: Option<std::path::PathBuf>,
    merged: bool,
) -> Result<()> {
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
        util::open_in_browser(&output)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Command: Calibrate
// ---------------------------------------------------------------------------

pub(crate) fn handle_calibrate(corrections_dir: std::path::PathBuf, profile: String) -> Result<()> {
    let report_path = run_calibration(&corrections_dir, &profile)?;
    let output_profile = util::get_calibration_dir()?.join("latest.json");
    apply_calibration_report(&report_path, &output_profile)?;
    info!(
        report = %report_path.display(),
        output = %output_profile.display(),
        "saved calibration report and updated active profile"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Command: ApplyCalibration
// ---------------------------------------------------------------------------

pub(crate) fn handle_apply_calibration(
    report: std::path::PathBuf,
    output: Option<std::path::PathBuf>,
) -> Result<()> {
    let out_path = if let Some(path) = output {
        path
    } else {
        util::get_calibration_dir()?.join("latest.json")
    };
    apply_calibration_report(&report, &out_path)?;
    info!(
        report = %report.display(),
        output = %out_path.display(),
        "applied calibration report"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Command: Bench
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_bench(
    input_media: std::path::PathBuf,
    config_path: Option<std::path::PathBuf>,
    threshold: Option<f32>,
    min_speech_ms: Option<u32>,
    min_silence_ms: Option<u32>,
    max_non_voice_ms: Option<u32>,
    vad_engine: String,
    calibration_profile: Option<std::path::PathBuf>,
    parallel_features: Option<bool>,
    output: std::path::PathBuf,
) -> Result<()> {
    let cfg = load_analysis_config(
        config_path,
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
    Ok(())
}

// ---------------------------------------------------------------------------
// Command: GenFixtures
// ---------------------------------------------------------------------------

pub(crate) fn handle_gen_fixtures(output_dir: std::path::PathBuf) -> Result<()> {
    validation::synthetic::generate_suite(&output_dir)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Command: AiVoiceExtract
// ---------------------------------------------------------------------------

pub(crate) fn handle_ai_voice_extract(
    input_json: std::path::PathBuf,
    output: std::path::PathBuf,
) -> Result<()> {
    let timeline = read_timeline(&input_json)?;
    let speech_segments: Vec<types::Segment> = timeline
        .segments
        .into_iter()
        .filter(|s| s.kind == types::SegmentKind::Speech)
        .collect();
    let ai_voice_output = types::AiVoiceOutput {
        file: timeline.file,
        analysis_sample_rate: timeline.analysis_sample_rate,
        frame_ms: timeline.frame_ms,
        segments: speech_segments,
    };
    write_json_pretty(&output, &ai_voice_output)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Command: LearningStats
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Command: RadioPlay
// ---------------------------------------------------------------------------

pub(crate) fn handle_radio_play(
    movie: PathBuf,
    timeline_path: Option<PathBuf>,
    subtitles_path: Option<PathBuf>,
    output_path: Option<PathBuf>,
    analyze_only: bool,
) -> Result<()> {
    if analyze_only {
        info!(movie = %movie.display(), "Running visual gap analysis");

        let timeline = if let Some(p) = timeline_path {
            read_timeline(&p)?
        } else {
            // In a full GOAP run, this would be generated.
            // For --analyze-only without --timeline, we might want to run extract,
            // but for now let's require it or bail if we can't find a default.
            bail!("--timeline is required for --analyze-only in this version");
        };

        let srt_content = if let Some(p) = subtitles_path {
            Some(std::fs::read_to_string(p)?)
        } else {
            None
        };

        let identifier = crate::goap::gaps::GapIdentifier::new();
        let gap_analysis = identifier.identify_gaps(&timeline, srt_content.as_deref())?;

        if let Some(out) = output_path {
            write_json_pretty(&out, &gap_analysis)?;
            info!(gaps = gap_analysis.gaps.len(), output = %out.display(), "Gap analysis complete");
        } else {
            println!("{}", serde_json::to_string_pretty(&gap_analysis)?);
        }
    } else {
        // Full GOAP orchestration would happen here
        info!("Full radio-play pipeline not yet implemented in this issue. Use --analyze-only.");
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Command: LearningStats
// ---------------------------------------------------------------------------

pub(crate) fn handle_learning_stats(
    learning_db: std::path::PathBuf,
    output: Option<std::path::PathBuf>,
) -> Result<()> {
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
    Ok(())
}
