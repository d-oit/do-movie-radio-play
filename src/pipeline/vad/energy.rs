use super::engine::{VadEngine, VadResult};
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
    fn classify(&self, frames: &[Frame]) -> VadResult {
        let mut decisions = Vec::with_capacity(frames.len());
        let mut likelihoods = Vec::with_capacity(frames.len());
        for frame in frames {
            let likelihood = frame.speech_likelihood(self.threshold);
            likelihoods.push(likelihood);
            decisions.push(likelihood >= 0.5);
        }
        VadResult::new(decisions, likelihoods)
    }

    fn name(&self) -> &'static str {
        "energy"
    }
}
