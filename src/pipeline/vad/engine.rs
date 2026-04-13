use crate::types::frame::Frame;

pub struct VadResult {
    pub decisions: Vec<bool>,
    pub likelihoods: Vec<f32>,
}

impl VadResult {
    pub fn new(decisions: Vec<bool>, likelihoods: Vec<f32>) -> Self {
        Self {
            decisions,
            likelihoods,
        }
    }
}

pub trait VadEngine: Send + Sync {
    fn classify(&self, frames: &[Frame]) -> VadResult;
    fn name(&self) -> &'static str;
}
