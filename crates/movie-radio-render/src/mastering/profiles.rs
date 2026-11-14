//! Selectable delivery targets for mastering.

use serde::{Deserialize, Serialize};

pub const DEFAULT_PROFILE: &str = "ebu_r128";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MasteringProfile {
    pub name: String,
    pub target_lufs: f32,
    pub tolerance_lu: f32,
    pub max_true_peak_dbtp: f32,
    pub max_lra_lu: f32,
    pub min_lufs: f32,
    pub max_lufs: f32,
    pub max_dc_offset: f32,
    pub silence_threshold_dbfs: f32,
    pub max_silence_ms: u64,
    pub max_channel_imbalance_db: f32,
    pub max_discontinuity: f32,
    pub max_gain_change_lu: f32,
    pub clipping_threshold: f32,
}

impl Default for MasteringProfile {
    fn default() -> Self {
        Self::ebu_r128()
    }
}

impl MasteringProfile {
    /// EBU R128 broadcast delivery: -23 LUFS integrated, -1 dBTP ceiling.
    ///
    /// R 128 v5 sets ±0.5 LU as the normal tolerance and allows ±1.0 LU only
    /// where the target is not practically achievable, so the inner band warns
    /// and the outer band fails.
    pub fn ebu_r128() -> Self {
        Self {
            name: "ebu_r128".to_string(),
            target_lufs: -23.0,
            tolerance_lu: 0.5,
            max_true_peak_dbtp: -1.0,
            max_lra_lu: 20.0,
            min_lufs: -24.0,
            max_lufs: -22.0,
            max_dc_offset: 0.001,
            silence_threshold_dbfs: -60.0,
            max_silence_ms: 5_000,
            max_channel_imbalance_db: 3.0,
            max_discontinuity: 0.5,
            max_gain_change_lu: 15.0,
            clipping_threshold: 0.999,
        }
    }

    /// Podcast delivery: -16 LUFS integrated, -1 dBTP ceiling.
    pub fn podcast() -> Self {
        Self {
            name: "podcast".to_string(),
            target_lufs: -16.0,
            tolerance_lu: 1.0,
            max_true_peak_dbtp: -1.0,
            max_lra_lu: 20.0,
            min_lufs: -17.0,
            max_lufs: -15.0,
            ..Self::ebu_r128()
        }
    }

    /// Radio delivery: -23 LUFS integrated, -1 dBTP ceiling.
    pub fn radio() -> Self {
        Self {
            name: "radio".to_string(),
            ..Self::ebu_r128()
        }
    }

    /// ATSC A/85 delivery: -24 LUFS integrated, -2 dBTP ceiling.
    pub fn atsc_a85() -> Self {
        Self {
            name: "atsc_a85".to_string(),
            target_lufs: -24.0,
            tolerance_lu: 1.0,
            max_true_peak_dbtp: -2.0,
            max_lra_lu: 20.0,
            min_lufs: -25.0,
            max_lufs: -23.0,
            ..Self::ebu_r128()
        }
    }

    /// User-defined target with the EBU continuity limits.
    pub fn custom(name: impl Into<String>, target_lufs: f32, max_true_peak_dbtp: f32) -> Self {
        Self {
            name: name.into(),
            target_lufs,
            tolerance_lu: 1.0,
            max_true_peak_dbtp,
            min_lufs: target_lufs - 1.0,
            max_lufs: target_lufs + 1.0,
            ..Self::ebu_r128()
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "ebu_r128" | "ebu-r128" | "r128" | "broadcast" => Some(Self::ebu_r128()),
            "podcast" => Some(Self::podcast()),
            "radio" => Some(Self::radio()),
            "atsc_a85" | "atsc-a85" | "atsc" => Some(Self::atsc_a85()),
            _ => None,
        }
    }

    pub fn available() -> &'static [&'static str] {
        &["ebu_r128", "podcast", "radio", "atsc_a85"]
    }
}
