use super::engine::VadEngine;
use crate::types::frame::Frame;

pub struct EnergyVad {
    threshold: f32,
}

impl EnergyVad {
    pub fn new(threshold: f32) -> Self {
        Self { threshold }
    }
}

impl VadEngine for EnergyVad {
    fn classify(&self, frames: &[Frame]) -> Vec<bool> {
        frames.iter().map(|f| f.rms >= self.threshold).collect()
    }

    fn name(&self) -> &'static str {
        "energy"
    }
}
