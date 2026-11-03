//! Standalone mastering validation for an existing output file.

use anyhow::{bail, Context, Result};
use std::path::PathBuf;
use tracing::info;

use movie_radio_render::mastering::{self, CheckStatus, MasteringProfile, DEFAULT_PROFILE};

#[allow(clippy::too_many_arguments)]
pub fn handle_mastering(
    input: PathBuf,
    profile_name: String,
    target_lufs: Option<f32>,
    max_true_peak: Option<f32>,
    json: Option<PathBuf>,
    html: Option<PathBuf>,
    output: Option<PathBuf>,
) -> Result<()> {
    let mut profile = MasteringProfile::from_name(&profile_name).with_context(|| {
        format!(
            "unknown mastering profile '{profile_name}'; available: {}",
            MasteringProfile::available().join(", ")
        )
    })?;
    if let Some(target) = target_lufs {
        profile.target_lufs = target;
        profile.min_lufs = target - profile.tolerance_lu;
        profile.max_lufs = target + profile.tolerance_lu;
    }
    if let Some(ceiling) = max_true_peak {
        profile.max_true_peak_dbtp = ceiling;
    }
    if profile_name.eq_ignore_ascii_case(DEFAULT_PROFILE)
        && (target_lufs.is_some() || max_true_peak.is_some())
    {
        profile.name = format!("{DEFAULT_PROFILE}-custom");
    }

    let (samples, sample_rate, channels) = mastering::read_audio(&input)
        .with_context(|| format!("cannot read audio from {}", input.display()))?;
    let report = if let Some(output_path) = output {
        if output_path
            .extension()
            .is_none_or(|ext| !ext.eq_ignore_ascii_case("wav"))
        {
            bail!("mastered output must be a WAV file");
        }
        let outcome = mastering::master(&samples, sample_rate, channels, &profile);
        if let Some(parent) = output_path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        mastering::write_wav(&output_path, &outcome.samples, sample_rate, channels)?;
        info!(output = %output_path.display(), "mastered output written");
        outcome.report
    } else {
        mastering::validate(&samples, sample_rate, channels, &profile)
    };

    let json_path = json.unwrap_or_else(|| input.with_extension("mastering.json"));
    mastering::write_json(&json_path, &report)?;
    if let Some(html_path) = html {
        mastering::write_html(&html_path, &report)?;
    }

    info!(
        profile = %report.profile.name,
        integrated_lufs = report.measurements.integrated_lufs,
        true_peak_dbtp = report.measurements.true_peak_dbtp,
        overall = ?report.overall,
        json = %json_path.display(),
        "mastering validation complete"
    );
    if report.overall == CheckStatus::Fail {
        bail!(
            "mastering validation failed for profile '{}': {}",
            report.profile.name,
            report.failure_summary()
        );
    }
    Ok(())
}
