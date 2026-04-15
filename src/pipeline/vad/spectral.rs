use super::engine::{VadEngine, VadResult};
use crate::types::frame::Frame;

pub struct SpectralVad {
    threshold: f32,
    flatness_max: f32,
    entropy_min: f32,
    centroid_min: f32,
    centroid_max: f32,
}

impl SpectralVad {
    pub fn new(threshold: f32) -> Self {
        Self {
            threshold,
            flatness_max: 0.45,
            entropy_min: 3.5,
            centroid_min: 100.0,
            centroid_max: 6000.0,
        }
    }

    pub fn with_thresholds(
        threshold: f32,
        flatness_max: f32,
        entropy_min: f32,
        centroid_min: f32,
        centroid_max: f32,
    ) -> Self {
        Self {
            threshold,
            flatness_max,
            entropy_min,
            centroid_min,
            centroid_max,
        }
    }
}

impl VadEngine for SpectralVad {
    fn classify(&self, frames: &[Frame]) -> VadResult {
        let mut decisions = Vec::with_capacity(frames.len());
        let mut likelihoods = Vec::with_capacity(frames.len());
        for frame in frames {
            let likelihood = classify_spectral(
                frame,
                self.threshold,
                self.flatness_max,
                self.entropy_min,
                self.centroid_min,
                self.centroid_max,
            );
            likelihoods.push(likelihood);
            decisions.push(likelihood >= 0.5);
        }
        VadResult::new(decisions, likelihoods)
    }

    fn name(&self) -> &'static str {
        "spectral"
    }
}

fn classify_spectral(
    frame: &Frame,
    threshold: f32,
    flatness_max: f32,
    entropy_min: f32,
    centroid_min: f32,
    centroid_max: f32,
) -> f32 {
    let threshold = threshold.max(0.0001);

    let energy_term = ((frame.rms - threshold) / threshold).clamp(-2.0, 2.0) * 0.25;

    let in_speech_freq = (250.0..=4500.0).contains(&frame.centroid_hz);
    let centroid_term = if in_speech_freq { 0.18 } else { -0.22 };

    let zcr_term = if (0.05..=0.35).contains(&frame.zcr) {
        0.12
    } else if frame.zcr > 0.45 {
        -0.15
    } else if frame.zcr < 0.02 {
        -0.08
    } else {
        0.0
    };

    let flux_term = (frame.spectral_flux - 0.003).clamp(-0.015, 0.025) * 4.0;

    let flatness_term = if frame.spectral_flatness > flatness_max {
        -0.20
    } else if frame.spectral_flatness > flatness_max * 0.67 {
        -0.10
    } else if frame.spectral_flatness < flatness_max * 0.33 {
        0.08
    } else {
        0.0
    };

    let entropy_term = if frame.spectral_entropy < entropy_min {
        0.20
    } else if frame.spectral_entropy < entropy_min + 1.0 {
        0.10
    } else if frame.spectral_entropy > entropy_min + 3.0 {
        -0.25
    } else if frame.spectral_entropy > entropy_min + 2.0 {
        -0.12
    } else {
        0.0
    };

    let music_indicator = frame.low_band_ratio > 0.5 && frame.high_band_ratio < 0.15;
    let music_penalty = if music_indicator { -0.28 } else { 0.0 };

    let hiss_indicator = frame.high_band_ratio > 0.4 && frame.zcr > 0.35;
    let hiss_penalty = if hiss_indicator { -0.18 } else { 0.0 };

    let voice_indicator = frame.zcr > 0.02
        && frame.zcr < 0.4
        && frame.centroid_hz > centroid_min
        && frame.centroid_hz < centroid_max
        && frame.spectral_flatness < flatness_max
        && frame.spectral_entropy < entropy_min + 2.5;
    let voice_bonus = if voice_indicator { 0.10 } else { 0.0 };

    let raw = 0.5
        + energy_term
        + centroid_term
        + zcr_term
        + flux_term
        + flatness_term
        + entropy_term
        + music_penalty
        + hiss_penalty
        + voice_bonus;

    raw.clamp(0.0, 1.0)
}
