use anyhow::{Context, Result};
use std::path::PathBuf;
use tracing::info;

use movie_radio_io::json::write_json_pretty;
use movie_radio_learning::profiles::CalibrationProfile;
use movie_radio_pipeline::pipeline::{benchmark::benchmark_file, extract_timeline};
use movie_radio_validation::synthetic;

use crate::util;

use super::load_analysis_config;

#[allow(clippy::too_many_arguments)]
pub fn handle_extract(
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
    chunk_duration: Option<u64>,
) -> Result<()> {
    let mut cfg = load_analysis_config(
        config_path,
        threshold,
        min_speech_ms,
        min_silence_ms,
        max_non_voice_ms,
        vad_engine,
        calibration_profile,
        parallel_features,
    )?;

    cfg.chunk_duration_sec = chunk_duration;

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

pub fn handle_bench(
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

pub fn handle_gen_fixtures(output_dir: std::path::PathBuf) -> Result<()> {
    synthetic::generate_suite(&output_dir)?;
    Ok(())
}
