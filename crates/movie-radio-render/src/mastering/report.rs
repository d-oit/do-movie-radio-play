//! Pass/warn/fail mastering reports built from measurements.

use serde::{Deserialize, Serialize};

use super::measure::{measure, LoudnessMeasurement, MeasurementConfig, SILENCE_LUFS};
use super::profiles::MasteringProfile;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Pass,
    Warn,
    Fail,
}

impl CheckStatus {
    pub fn worst(self, other: Self) -> Self {
        match (self, other) {
            (Self::Fail, _) | (_, Self::Fail) => Self::Fail,
            (Self::Warn, _) | (_, Self::Warn) => Self::Warn,
            _ => Self::Pass,
        }
    }

    pub fn is_fail(self) -> bool {
        self == Self::Fail
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityCheck {
    pub id: String,
    pub status: CheckStatus,
    pub measured: f64,
    pub threshold: f64,
    pub unit: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MasteringReport {
    pub profile: MasteringProfile,
    pub sample_rate: u32,
    pub channels: u16,
    pub duration_s: f64,
    pub measurements: LoudnessMeasurement,
    pub checks: Vec<QualityCheck>,
    pub overall: CheckStatus,
    pub applied_gain_db: f32,
    pub limiting_reduction_db: f32,
}

impl MasteringReport {
    pub fn failures(&self) -> impl Iterator<Item = &QualityCheck> {
        self.checks.iter().filter(|c| c.status == CheckStatus::Fail)
    }

    pub fn warnings(&self) -> impl Iterator<Item = &QualityCheck> {
        self.checks.iter().filter(|c| c.status == CheckStatus::Warn)
    }

    pub fn failure_summary(&self) -> String {
        let failures: Vec<String> = self
            .failures()
            .map(|c| format!("{}: {}", c.id, c.message))
            .collect();
        if failures.is_empty() {
            "no failures".to_string()
        } else {
            failures.join("; ")
        }
    }
}

fn check(
    id: &str,
    status: CheckStatus,
    measured: f64,
    threshold: f64,
    unit: &str,
    message: impl Into<String>,
) -> QualityCheck {
    QualityCheck {
        id: id.to_string(),
        status,
        measured,
        threshold,
        unit: unit.to_string(),
        message: message.into(),
    }
}

fn build_checks(m: &LoudnessMeasurement, p: &MasteringProfile) -> Vec<QualityCheck> {
    let mut checks = Vec::new();

    let empty = m.duration_s <= 0.0;
    checks.push(check(
        "empty_output",
        if empty {
            CheckStatus::Fail
        } else {
            CheckStatus::Pass
        },
        m.duration_s,
        0.0,
        "s",
        if empty {
            "output contains no audio"
        } else {
            "output contains audio"
        },
    ));

    let integrated_status = if m.integrated_lufs <= SILENCE_LUFS + 1.0
        || m.integrated_lufs < p.min_lufs
        || m.integrated_lufs > p.max_lufs
    {
        CheckStatus::Fail
    } else if (m.integrated_lufs - p.target_lufs).abs() > p.tolerance_lu {
        CheckStatus::Warn
    } else {
        CheckStatus::Pass
    };
    checks.push(check(
        "integrated_loudness",
        integrated_status,
        f64::from(m.integrated_lufs),
        f64::from(p.target_lufs),
        "LUFS",
        format!(
            "integrated {:.1} LUFS (target {:.1} ± {:.1})",
            m.integrated_lufs, p.target_lufs, p.tolerance_lu
        ),
    ));

    let true_peak_status = if m.true_peak_dbtp > p.max_true_peak_dbtp {
        CheckStatus::Fail
    } else if m.true_peak_dbtp > p.max_true_peak_dbtp - 0.5 {
        CheckStatus::Warn
    } else {
        CheckStatus::Pass
    };
    checks.push(check(
        "true_peak",
        true_peak_status,
        f64::from(m.true_peak_dbtp),
        f64::from(p.max_true_peak_dbtp),
        "dBTP",
        format!(
            "true peak {:.2} dBTP (ceiling {:.2})",
            m.true_peak_dbtp, p.max_true_peak_dbtp
        ),
    ));

    let sample_peak_status = if m.sample_peak_dbfs > 0.0 {
        CheckStatus::Fail
    } else {
        CheckStatus::Pass
    };
    checks.push(check(
        "sample_peak",
        sample_peak_status,
        f64::from(m.sample_peak_dbfs),
        0.0,
        "dBFS",
        format!("sample peak {:.2} dBFS", m.sample_peak_dbfs),
    ));

    let clipping_status = if m.clipped_samples > 0 {
        CheckStatus::Fail
    } else {
        CheckStatus::Pass
    };
    checks.push(check(
        "clipping",
        clipping_status,
        m.clipped_samples as f64,
        0.0,
        "samples",
        format!("{} clipped sample(s)", m.clipped_samples),
    ));

    let lra_status = if m.loudness_range_lu > p.max_lra_lu {
        CheckStatus::Warn
    } else {
        CheckStatus::Pass
    };
    checks.push(check(
        "loudness_range",
        lra_status,
        f64::from(m.loudness_range_lu),
        f64::from(p.max_lra_lu),
        "LU",
        format!(
            "loudness range {:.1} LU (limit {:.1})",
            m.loudness_range_lu, p.max_lra_lu
        ),
    ));

    let dc_status = if m.dc_offset > p.max_dc_offset {
        CheckStatus::Warn
    } else {
        CheckStatus::Pass
    };
    checks.push(check(
        "dc_offset",
        dc_status,
        f64::from(m.dc_offset),
        f64::from(p.max_dc_offset),
        "linear",
        format!("DC offset {:.5}", m.dc_offset),
    ));

    let silence_status = if m.longest_silence_ms > p.max_silence_ms {
        CheckStatus::Warn
    } else {
        CheckStatus::Pass
    };
    checks.push(check(
        "silence",
        silence_status,
        m.longest_silence_ms as f64,
        p.max_silence_ms as f64,
        "ms",
        format!(
            "longest silence {} ms (limit {} ms)",
            m.longest_silence_ms, p.max_silence_ms
        ),
    ));

    let discontinuity_status = if m.max_discontinuity > p.max_discontinuity {
        CheckStatus::Warn
    } else {
        CheckStatus::Pass
    };
    checks.push(check(
        "discontinuity",
        discontinuity_status,
        f64::from(m.max_discontinuity),
        f64::from(p.max_discontinuity),
        "linear",
        format!(
            "largest sample discontinuity {:.3} (limit {:.3})",
            m.max_discontinuity, p.max_discontinuity
        ),
    ));

    let gain_change_status = if m.max_gain_change_lu > p.max_gain_change_lu {
        CheckStatus::Warn
    } else {
        CheckStatus::Pass
    };
    checks.push(check(
        "gain_change",
        gain_change_status,
        f64::from(m.max_gain_change_lu),
        f64::from(p.max_gain_change_lu),
        "LU",
        format!(
            "largest short-term gain change {:.1} LU (limit {:.1})",
            m.max_gain_change_lu, p.max_gain_change_lu
        ),
    ));

    let imbalance_status = if m.channel_imbalance_db > p.max_channel_imbalance_db {
        CheckStatus::Warn
    } else {
        CheckStatus::Pass
    };
    checks.push(check(
        "channel_imbalance",
        imbalance_status,
        f64::from(m.channel_imbalance_db),
        f64::from(p.max_channel_imbalance_db),
        "dB",
        format!(
            "channel imbalance {:.2} dB (limit {:.2})",
            m.channel_imbalance_db, p.max_channel_imbalance_db
        ),
    ));

    checks
}

pub fn validate(
    samples: &[f32],
    sample_rate: u32,
    channels: u16,
    profile: &MasteringProfile,
) -> MasteringReport {
    let config = MeasurementConfig {
        silence_threshold_dbfs: profile.silence_threshold_dbfs,
        clipping_threshold: profile.clipping_threshold,
    };
    let measurements = measure(samples, sample_rate, channels, &config);
    let checks = build_checks(&measurements, profile);
    let overall = checks
        .iter()
        .fold(CheckStatus::Pass, |acc, c| acc.worst(c.status));
    MasteringReport {
        profile: profile.clone(),
        sample_rate,
        channels,
        duration_s: measurements.duration_s,
        measurements,
        checks,
        overall,
        applied_gain_db: 0.0,
        limiting_reduction_db: 0.0,
    }
}
