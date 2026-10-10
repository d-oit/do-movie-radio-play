use anyhow::{Context, Result};
use std::path::PathBuf;
use tracing::info;

use crate::config;
use crate::util;
use movie_radio_io::json::read_json;
use movie_radio_learning::calibrator::{apply_calibration_report, run_calibration};
use movie_radio_learning::profiles::CalibrationProfile;
use movie_radio_pipeline::pipeline::tags::TagRules;

mod cues;
pub mod extract;
pub mod learning;
pub mod radio_play;
pub mod validate;

pub use extract::{handle_bench, handle_extract, handle_gen_fixtures};
pub use learning::{
    handle_export_learnings, handle_learning_log, handle_learning_stats, handle_reset_learnings,
};
pub use radio_play::{handle_radio_play, RadioPlayOptions};
pub use validate::{
    handle_ai_voice_extract, handle_export, handle_learning_experiments, handle_merge_timeline,
    handle_prompt, handle_review, handle_tag, handle_update_thresholds, handle_validate,
    handle_verify_timeline,
};

pub mod preview;

#[allow(clippy::too_many_arguments)]
pub(crate) fn load_analysis_config(
    config_path: Option<PathBuf>,
    threshold_override: Option<f32>,
    min_speech_override: Option<u32>,
    min_silence_override: Option<u32>,
    max_non_voice_override: Option<u32>,
    vad_engine: Option<String>,
    calibration_profile: Option<PathBuf>,
    parallel_features: Option<bool>,
) -> Result<config::AnalysisConfig> {
    let threshold_delta = load_calibration_threshold_delta(calibration_profile.as_deref())?;
    config::build_analysis_config(
        config_path,
        threshold_override,
        min_speech_override,
        min_silence_override,
        max_non_voice_override,
        vad_engine,
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

pub(crate) fn load_tag_rules(profile_path: Option<&std::path::Path>) -> Result<Option<TagRules>> {
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

#[cfg(test)]
mod engine_selection_tests {
    use super::load_analysis_config;

    fn config_file(engine: &str) -> tempfile::NamedTempFile {
        let file = tempfile::NamedTempFile::new().unwrap();
        let json = serde_json::json!({
            "sample_rate_hz": 16000, "frame_ms": 20, "speech_hangover_ms": 300, "merge_gap_ms": 250,
            "min_speech_ms": 120, "min_non_voice_ms": 10000, "max_non_voice_ms": null,
            "energy_threshold": 0.015, "vad_threshold_delta": 0.0, "prompt_min_duration_ms": 2500,
            "prompt_min_confidence": 0.65, "vad_engine": engine
        });
        std::fs::write(file.path(), json.to_string()).unwrap();
        file
    }

    fn engine(config: Option<&tempfile::NamedTempFile>, flag: Option<&str>) -> String {
        load_analysis_config(
            config.map(|f| f.path().to_path_buf()),
            None,
            None,
            None,
            None,
            flag.map(str::to_string),
            None,
            None,
        )
        .unwrap()
        .vad_engine
    }

    #[test]
    fn config_engine_is_used_when_no_flag_is_given() {
        let cfg = config_file("spectral");
        assert_eq!(engine(Some(&cfg), None), "spectral");
    }

    #[test]
    fn explicit_flag_overrides_the_config_and_default_is_energy() {
        let cfg = config_file("spectral");
        assert_eq!(engine(Some(&cfg), Some("hybrid")), "hybrid");
        assert_eq!(engine(None, None), "energy");
    }
}
