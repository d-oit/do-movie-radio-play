use serde::{Deserialize, Serialize};

fn default_version() -> u32 {
    1
}

fn default_tag_delta() -> f32 {
    0.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagThresholds {
    #[serde(default = "default_tag_delta")]
    pub ambience_max_rms_delta: f32,
    #[serde(default = "default_tag_delta")]
    pub impact_min_rms_delta: f32,
    #[serde(default = "default_tag_delta")]
    pub min_centroid_hz_delta: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationProfile {
    pub name: String,
    pub energy_threshold_delta: f32,
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub tag_thresholds: Option<TagThresholds>,
}

pub fn profile(name: &str) -> CalibrationProfile {
    match name {
        "action" => CalibrationProfile {
            name: name.to_string(),
            energy_threshold_delta: 0.01,
            version: 1,
            tag_thresholds: None,
        },
        "documentary" => CalibrationProfile {
            name: name.to_string(),
            energy_threshold_delta: -0.003,
            version: 1,
            tag_thresholds: None,
        },
        "animation" => CalibrationProfile {
            name: name.to_string(),
            energy_threshold_delta: 0.0,
            version: 1,
            tag_thresholds: None,
        },
        _ => CalibrationProfile {
            name: "drama".to_string(),
            energy_threshold_delta: -0.001,
            version: 1,
            tag_thresholds: None,
        },
    }
}
