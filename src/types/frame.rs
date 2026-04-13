#[derive(Debug, Clone)]
pub struct Frame {
    pub rms: f32,
    pub zcr: f32,
    pub spectral_flux: f32,
    pub centroid_hz: f32,
    pub low_band_ratio: f32,
    pub high_band_ratio: f32,
}

impl Frame {
    pub fn speech_likelihood(&self, threshold: f32) -> f32 {
        let threshold = threshold.max(0.0001);
        let energy_term = ((self.rms - threshold) / threshold).clamp(-1.5, 1.5) * 0.3;
        let centroid_term = if (250.0..=4500.0).contains(&self.centroid_hz) {
            0.18
        } else {
            -0.18
        };
        let zcr_term = if (0.05..=0.35).contains(&self.zcr) {
            0.12
        } else {
            -0.12
        };
        let flux_term = (self.spectral_flux - 0.002).clamp(-0.01, 0.02) * 6.0;
        let music_penalty = if self.low_band_ratio > 0.45 && self.high_band_ratio < 0.12 {
            -0.22
        } else {
            0.0
        };
        let hiss_penalty = if self.high_band_ratio > 0.45 && self.zcr > 0.3 {
            -0.1
        } else {
            0.0
        };
        let raw =
            0.5 + energy_term + centroid_term + zcr_term + flux_term + music_penalty + hiss_penalty;
        raw.clamp(0.0, 1.0)
    }
}
