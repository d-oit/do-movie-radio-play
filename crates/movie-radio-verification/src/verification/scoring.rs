use crate::verification::analysis::{SpectralFeatures, VerificationStatus};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppliedThresholds {
    pub entropy_min: f32,
    pub entropy_max: f32,
    pub flatness_max: f32,
    pub energy_min: f32,
    pub centroid_min: f32,
    pub centroid_max: f32,
}

pub(crate) const DEFAULT_ENTROPY_MIN: f32 = 3.5;
pub(crate) const DEFAULT_ENTROPY_MAX: f32 = 7.0;
pub(crate) const DEFAULT_FLATNESS_MAX: f32 = 0.45;
pub(crate) const DEFAULT_ENERGY_MIN: f32 = 0.001;
pub(crate) const DEFAULT_CENTROID_MIN: f32 = 100.0;
pub(crate) const DEFAULT_CENTROID_MAX: f32 = 6000.0;
pub(crate) const VERIFICATION_HIGH_CONFIDENCE_THRESHOLD: f32 = 0.55;
pub(crate) const SPEECH_ZCR_MIN: f32 = 0.02;
pub(crate) const SPEECH_ZCR_MAX: f32 = 0.35;
pub(crate) const GRAPH_STRUCTURE_WEIGHT: f32 = 0.25;
const DEFAULT_FILTER_SEGMENT_CONFIDENCE_CEILING: f32 = 0.55;

pub fn default_filter_segment_confidence_ceiling() -> f32 {
    DEFAULT_FILTER_SEGMENT_CONFIDENCE_CEILING
}

pub(crate) fn build_thresholds(
    entropy_min: Option<f32>,
    entropy_max: Option<f32>,
    flatness_max: Option<f32>,
    energy_min: Option<f32>,
    centroid_min: Option<f32>,
    centroid_max: Option<f32>,
) -> AppliedThresholds {
    AppliedThresholds {
        entropy_min: entropy_min.unwrap_or(DEFAULT_ENTROPY_MIN),
        entropy_max: entropy_max.unwrap_or(DEFAULT_ENTROPY_MAX),
        flatness_max: flatness_max.unwrap_or(DEFAULT_FLATNESS_MAX),
        energy_min: energy_min.unwrap_or(DEFAULT_ENERGY_MIN),
        centroid_min: centroid_min.unwrap_or(DEFAULT_CENTROID_MIN),
        centroid_max: centroid_max.unwrap_or(DEFAULT_CENTROID_MAX),
    }
}

pub(crate) fn determine_verification_status(
    features: &SpectralFeatures,
    original_confidence: f32,
    thresholds: &AppliedThresholds,
) -> VerificationStatus {
    let entropy_voice =
        (thresholds.entropy_min..=thresholds.entropy_max).contains(&features.spectral_entropy);
    let flatness_voice = features.spectral_flatness < thresholds.flatness_max;
    let energy_voice = features.rms > thresholds.energy_min;
    let centroid_voice =
        (thresholds.centroid_min..=thresholds.centroid_max).contains(&features.centroid_hz);
    let zcr_voice = (SPEECH_ZCR_MIN..=SPEECH_ZCR_MAX).contains(&features.zcr);

    let voice_indicators = [
        entropy_voice,
        flatness_voice,
        energy_voice,
        centroid_voice,
        zcr_voice,
    ]
    .into_iter()
    .filter(|&b| b)
    .count() as f32;
    let voice_score =
        (((1.0 - original_confidence) * 0.3) + ((voice_indicators / 5.0) * 0.7)).clamp(0.0, 1.0);

    let nonvoice_indicators = [
        features.rms < thresholds.energy_min * 1.2,
        features.spectral_flatness > thresholds.flatness_max,
        !(thresholds.entropy_min..=thresholds.entropy_max).contains(&features.spectral_entropy),
        !(thresholds.centroid_min..=thresholds.centroid_max).contains(&features.centroid_hz),
        features.high_band_ratio > 0.4 && features.zcr > SPEECH_ZCR_MAX,
    ]
    .into_iter()
    .filter(|&b| b)
    .count() as f32;
    let nonvoice_score = (nonvoice_indicators / 5.0).clamp(0.0, 1.0);

    let graph_nonvoice_confidence = graph_structure_nonvoice_confidence(features, thresholds);
    let base_nonvoice_confidence =
        (((1.0 - voice_score) * 0.7) + (nonvoice_score * 0.3)).clamp(0.0, 1.0);
    let nonvoice_confidence = ((base_nonvoice_confidence * (1.0 - GRAPH_STRUCTURE_WEIGHT))
        + (graph_nonvoice_confidence * GRAPH_STRUCTURE_WEIGHT))
        .clamp(0.0, 1.0);

    if nonvoice_confidence >= VERIFICATION_HIGH_CONFIDENCE_THRESHOLD {
        VerificationStatus::Verified
    } else if voice_score >= 0.8 && nonvoice_score <= 0.2 {
        VerificationStatus::Rejected
    } else {
        VerificationStatus::Suspicious
    }
}

fn graph_structure_nonvoice_confidence(
    features: &SpectralFeatures,
    thresholds: &AppliedThresholds,
) -> f32 {
    let speech_region_score = [
        (thresholds.centroid_min..=thresholds.centroid_max).contains(&features.centroid_hz),
        (SPEECH_ZCR_MIN..=SPEECH_ZCR_MAX).contains(&features.zcr),
        features.spectral_flatness < thresholds.flatness_max,
        (thresholds.entropy_min..=thresholds.entropy_max).contains(&features.spectral_entropy),
    ]
    .into_iter()
    .filter(|&x| x)
    .count() as f32
        / 4.0;

    let nonvoice_region_score = [
        features.spectral_flatness > thresholds.flatness_max,
        features.spectral_entropy > thresholds.entropy_max,
        features.high_band_ratio > 0.4,
        features.spectral_flux > 0.02,
    ]
    .into_iter()
    .filter(|&x| x)
    .count() as f32
        / 4.0;

    ((nonvoice_region_score * 0.7) + ((1.0 - speech_region_score) * 0.3)).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_thresholds_uses_defaults() {
        let thresholds = build_thresholds(None, None, None, None, None, None);
        assert_eq!(thresholds.entropy_min, DEFAULT_ENTROPY_MIN);
        assert_eq!(thresholds.centroid_max, DEFAULT_CENTROID_MAX);
    }
}
