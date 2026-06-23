use super::{AppliedThresholds, SpectralFeatures, VerificationStatus};

const SPEECH_ZCR_MIN: f32 = 0.02;
const SPEECH_ZCR_MAX: f32 = 0.35;
const VERIFICATION_HIGH_CONFIDENCE_THRESHOLD: f32 = 0.55;
const GRAPH_STRUCTURE_WEIGHT: f32 = 0.25;

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
