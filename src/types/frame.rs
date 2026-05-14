#[derive(Debug, Clone)]
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
        let threshold = threshold.max(0.0001);
        let energy_term = ((self.rms - threshold) / threshold).clamp(-1.5, 1.5) * 0.3;
        let centroid_term = if (250.0..=4500.0).contains(&self.centroid_hz) {
            0.15
        } else {
            -0.15
        };
        let zcr_term = if (0.05..=0.35).contains(&self.zcr) {
            0.10
        } else {
            -0.10
        };
        let flux_term = (self.spectral_flux - 0.002).clamp(-0.01, 0.02) * 5.0;
        let flatness_term = if self.spectral_flatness > 0.35 {
            -0.15
        } else {
            0.10
        };
        let entropy_term = if self.spectral_entropy < 4.5 {
            0.12
        } else if self.spectral_entropy > 6.5 {
            -0.15
        } else {
            0.0
        };
        let music_penalty = if self.low_band_ratio > 0.45 && self.high_band_ratio < 0.12 {
            -0.20
        } else {
            0.0
        };
        let hiss_penalty = if self.high_band_ratio > 0.45 && self.zcr > 0.3 {
            -0.10
        } else {
            0.0
        };
        let raw = 0.5
            + energy_term
            + centroid_term
            + zcr_term
            + flux_term
            + flatness_term
            + entropy_term
            + music_penalty
            + hiss_penalty;
        raw.clamp(0.0, 1.0)
    }
}
