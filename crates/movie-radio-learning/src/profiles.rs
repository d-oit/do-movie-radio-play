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

fn default_density_multiplier() -> f32 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationProfile {
    pub name: String,
    pub energy_threshold_delta: f32,
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub tag_thresholds: Option<TagThresholds>,
    #[serde(default)]
    pub profile_id: Option<String>,
    #[serde(default)]
    pub experiment_tags: Vec<String>,
    #[serde(default)]
    pub min_non_voice_ms_delta: i64,
    #[serde(default)]
    pub confidence_threshold_delta: f64,
    #[serde(default = "default_density_multiplier")]
    pub narration_density_multiplier: f32,
}

pub fn profile(name: &str) -> CalibrationProfile {
    match name {
        "action" => CalibrationProfile {
            name: name.to_string(),
            energy_threshold_delta: 0.01,
            version: 1,
            tag_thresholds: None,
            profile_id: Some("action".to_string()),
            experiment_tags: vec![],
            min_non_voice_ms_delta: 500,
            confidence_threshold_delta: 0.05,
            narration_density_multiplier: 0.8,
        },
        "documentary" => CalibrationProfile {
            name: name.to_string(),
            energy_threshold_delta: -0.003,
            version: 1,
            tag_thresholds: None,
            profile_id: Some("documentary".to_string()),
            experiment_tags: vec![],
            min_non_voice_ms_delta: -300,
            confidence_threshold_delta: -0.05,
            narration_density_multiplier: 1.25,
        },
        "animation" => CalibrationProfile {
            name: name.to_string(),
            energy_threshold_delta: 0.0,
            version: 1,
            tag_thresholds: None,
            profile_id: Some("animation".to_string()),
            experiment_tags: vec![],
            min_non_voice_ms_delta: 0,
            confidence_threshold_delta: 0.0,
            narration_density_multiplier: 1.0,
        },
        _ => CalibrationProfile {
            name: "drama".to_string(),
            energy_threshold_delta: -0.001,
            version: 1,
            tag_thresholds: None,
            profile_id: Some("drama".to_string()),
            experiment_tags: vec![],
            min_non_voice_ms_delta: -100,
            confidence_threshold_delta: -0.02,
            narration_density_multiplier: 1.1,
        },
    }
}
