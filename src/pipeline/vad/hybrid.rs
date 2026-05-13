use super::energy::EnergyVad;
use super::engine::{VadEngine, VadResult};
use super::spectral::SpectralVad;
use crate::types::frame::Frame;
use std::env;

const ENERGY_WEIGHT: f32 = 0.4;
const SPECTRAL_WEIGHT: f32 = 0.6;
const HYBRID_DECISION_THRESHOLD: f32 = 0.50;
const LOW_SPECTRAL_VETO_CEILING: f32 = 0.30;
const LOW_ENERGY_VETO_CEILING: f32 = 0.35;
const VETO_PENALTY_MULTIPLIER: f32 = 0.82;

#[derive(Clone, Copy)]
struct HybridParams {
    energy_weight: f32,
    spectral_weight: f32,
    decision_threshold: f32,
    low_spectral_veto_ceiling: f32,
    low_energy_veto_ceiling: f32,
    veto_penalty_multiplier: f32,
}

impl HybridParams {
    fn from_env() -> Self {
        let mut energy_weight = env_f32("TIMELINE_HYBRID_ENERGY_WEIGHT", ENERGY_WEIGHT);
        let mut spectral_weight = env_f32("TIMELINE_HYBRID_SPECTRAL_WEIGHT", SPECTRAL_WEIGHT);
        if energy_weight < 0.0 {
            energy_weight = ENERGY_WEIGHT;
        }
        if spectral_weight < 0.0 {
            spectral_weight = SPECTRAL_WEIGHT;
        }
        let sum = energy_weight + spectral_weight;
        if sum <= f32::EPSILON {
            energy_weight = ENERGY_WEIGHT;
            spectral_weight = SPECTRAL_WEIGHT;
        } else {
            energy_weight /= sum;
            spectral_weight /= sum;
        }
        Self {
            energy_weight,
            spectral_weight,
            decision_threshold: env_f32(
                "TIMELINE_HYBRID_DECISION_THRESHOLD",
                HYBRID_DECISION_THRESHOLD,
            )
            .clamp(0.0, 1.0),
            low_spectral_veto_ceiling: env_f32(
                "TIMELINE_HYBRID_LOW_SPECTRAL_VETO",
                LOW_SPECTRAL_VETO_CEILING,
            )
            .clamp(0.0, 1.0),
            low_energy_veto_ceiling: env_f32(
                "TIMELINE_HYBRID_LOW_ENERGY_VETO",
                LOW_ENERGY_VETO_CEILING,
            )
            .clamp(0.0, 1.0),
            veto_penalty_multiplier: env_f32(
                "TIMELINE_HYBRID_VETO_PENALTY",
                VETO_PENALTY_MULTIPLIER,
            )
            .clamp(0.0, 1.0),
        }
    }
}

fn env_f32(key: &str, default: f32) -> f32 {
    match env::var(key) {
        Ok(raw) => raw.parse::<f32>().unwrap_or(default),
        Err(_) => default,
    }
}

pub struct HybridVad {
    energy: EnergyVad,
    spectral: SpectralVad,
}

impl HybridVad {
    pub fn new(threshold: f32) -> Self {
        Self {
            energy: EnergyVad::new(threshold),
            spectral: SpectralVad::new(threshold),
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
            energy: EnergyVad::new(threshold),
            spectral: SpectralVad::with_thresholds(
                threshold,
                flatness_max,
                entropy_min,
                centroid_min,
                centroid_max,
            ),
        }
    }
}

impl VadEngine for HybridVad {
    fn classify(&self, frames: &[Frame]) -> VadResult {
        let params = HybridParams::from_env();
        let energy = self.energy.classify(frames);
        let spectral = self.spectral.classify(frames);

        let mut decisions = Vec::with_capacity(frames.len());
        let mut likelihoods = Vec::with_capacity(frames.len());

        for idx in 0..frames.len() {
            let e = energy.likelihoods[idx];
            let s = spectral.likelihoods[idx];
            let mut combined = params.energy_weight * e + params.spectral_weight * s;

            if s <= params.low_spectral_veto_ceiling && e <= params.low_energy_veto_ceiling {
                combined *= params.veto_penalty_multiplier;
            }

            let combined = combined.clamp(0.0, 1.0);
            likelihoods.push(combined);
            decisions.push(combined >= params.decision_threshold);
        }

        VadResult::new(decisions, likelihoods)
    }

    fn name(&self) -> &'static str {
        "hybrid"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(rms: f32, zcr: f32, flatness: f32, entropy: f32, centroid_hz: f32) -> Frame {
        Frame {
            rms,
            zcr,
            spectral_flux: 0.01,
            spectral_flatness: flatness,
            spectral_entropy: entropy,
            centroid_hz,
            low_band_ratio: 0.2,
            constellation_density: 0.0, high_band_ratio: 0.2,
        }
    }

    #[test]
    fn hybrid_classifies_typical_speech_like_frame_as_speech() {
        let engine = HybridVad::new(0.015);
        let frames = vec![frame(0.03, 0.12, 0.22, 4.5, 1600.0)];
        let result = engine.classify(&frames);
        assert_eq!(result.decisions, vec![true]);
        assert!(result.likelihoods[0] >= HYBRID_DECISION_THRESHOLD);
    }

    #[test]
    fn hybrid_penalizes_low_energy_and_low_spectral_frame() {
        let engine = HybridVad::new(0.015);
        let frames = vec![frame(0.002, 0.5, 0.7, 7.2, 7000.0)];
        let result = engine.classify(&frames);
        assert_eq!(result.decisions, vec![false]);
        assert!(result.likelihoods[0] < HYBRID_DECISION_THRESHOLD);
    }
}
