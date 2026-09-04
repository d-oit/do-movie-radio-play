use anyhow::Result;
use movie_radio_types::frame::Frame;

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

    /// Whether this engine consumes raw samples instead of pre-computed frames.
    fn uses_raw_samples(&self) -> bool {
        false
    }

    /// Classify raw mono samples at the pipeline sample rate. Only called when
    /// [`VadEngine::uses_raw_samples`] is true; the default rejects.
    fn classify_samples(
        &mut self,
        _samples: &[f32],
        _sample_rate_hz: u32,
        _frame_ms: u32,
    ) -> Result<VadResult> {
        anyhow::bail!("engine '{}' requires pre-computed frames", self.name());
    }
}
