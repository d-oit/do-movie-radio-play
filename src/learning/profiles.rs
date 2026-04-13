use serde::{Deserialize, Serialize};

fn default_version() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationProfile {
    pub name: String,
    pub energy_threshold_delta: f32,
    #[serde(default = "default_version")]
    pub version: u32,
}

pub fn profile(name: &str) -> CalibrationProfile {
    match name {
        "action" => CalibrationProfile {
            name: name.to_string(),
            energy_threshold_delta: 0.01,
            version: 1,
        },
        "documentary" => CalibrationProfile {
            name: name.to_string(),
            energy_threshold_delta: -0.003,
            version: 1,
        },
        "animation" => CalibrationProfile {
            name: name.to_string(),
            energy_threshold_delta: 0.0,
            version: 1,
        },
        _ => CalibrationProfile {
            name: "drama".to_string(),
            energy_threshold_delta: -0.001,
            version: 1,
        },
    }
}
