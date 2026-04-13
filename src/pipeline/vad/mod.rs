mod energy;
mod engine;

pub use energy::EnergyVad;
pub use engine::VadEngine;

pub struct WebRtcVad;

impl WebRtcVad {
    pub fn new() -> Self {
        Self
    }
}

impl VadEngine for WebRtcVad {
    fn classify(&self, frames: &[Frame]) -> engine::VadResult {
        tracing::warn!("WebRTC VAD not yet implemented, falling back to energy VAD");
        let fallback = EnergyVad::new(0.015);
        fallback.classify(frames)
    }

    fn name(&self) -> &'static str {
        "webrtc"
    }
}

impl Default for WebRtcVad {
    fn default() -> Self {
        Self::new()
    }
}

pub struct SileroVad;

impl SileroVad {
    pub fn new() -> Self {
        Self
    }
}

impl VadEngine for SileroVad {
    fn classify(&self, frames: &[Frame]) -> engine::VadResult {
        tracing::warn!("Silero VAD not yet implemented, falling back to energy VAD");
        let fallback = EnergyVad::new(0.015);
        fallback.classify(frames)
    }

    fn name(&self) -> &'static str {
        "silero"
    }
}

impl Default for SileroVad {
    fn default() -> Self {
        Self::new()
    }
}

use crate::types::frame::Frame;

pub fn create_engine(name: &str, threshold: f32) -> Box<dyn VadEngine> {
    match name {
        "energy" => Box::new(EnergyVad::new(threshold)),
        "webrtc" => Box::new(WebRtcVad::new()),
        "silero" => Box::new(SileroVad::new()),
        _ => {
            tracing::warn!("Unknown VAD engine '{}', defaulting to energy", name);
            Box::new(EnergyVad::new(threshold))
        }
    }
}
