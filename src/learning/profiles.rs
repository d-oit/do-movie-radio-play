use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationProfile {
    pub name: String,
    pub energy_threshold_delta: f32,
}

pub fn profile(name: &str) -> CalibrationProfile {
    match name {
        "action" => CalibrationProfile {
            name: name.to_string(),
            energy_threshold_delta: 0.01,
        },
        "documentary" => CalibrationProfile {
            name: name.to_string(),
            energy_threshold_delta: -0.003,
        },
        "animation" => CalibrationProfile {
            name: name.to_string(),
            energy_threshold_delta: 0.0,
        },
        _ => CalibrationProfile {
            name: "drama".to_string(),
            energy_threshold_delta: -0.001,
        },
    }
}
