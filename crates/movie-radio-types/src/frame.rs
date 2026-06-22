use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Frame {
    pub rms: f32,
    pub zcr: f32,
    pub spectral_flux: f32,
    pub spectral_flatness: f32,
    pub spectral_entropy: f32,
    pub centroid_hz: f32,
    pub low_band_ratio: f32,
    pub high_band_ratio: f32,
}

impl Frame {
    pub fn speech_likelihood(&self, threshold: f32) -> f32 {
        let energy_score = (self.rms * 10.0).min(1.0);
        let spectral_score = 1.0 - self.spectral_flatness;
        let combined = energy_score * 0.6 + spectral_score * 0.4;
        if combined > threshold {
            combined
        } else {
            0.0
        }
    }
}
