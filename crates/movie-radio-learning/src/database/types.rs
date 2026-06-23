use serde::{Deserialize, Serialize};

use movie_radio_types::FeatureSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifiedSegment {
    pub start_ms: i64,
    pub end_ms: i64,
    pub confidence: f64,
    pub spectral_features: SpectralFeatures,
    pub was_false_positive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SpectralFeatures {
    pub rms: f64,
    pub zcr: f64,
    pub spectral_flux: f64,
    pub spectral_flatness: f64,
    pub spectral_entropy: f64,
    pub centroid_hz: f64,
    pub low_band_ratio: f64,
    pub high_band_ratio: f64,
}

impl From<FeatureSet> for SpectralFeatures {
    fn from(fs: FeatureSet) -> Self {
        Self {
            rms: fs.rms as f64,
            zcr: fs.zcr as f64,
            spectral_flux: fs.spectral_flux as f64,
            spectral_flatness: fs.spectral_flatness as f64,
            spectral_entropy: fs.spectral_entropy as f64,
            centroid_hz: fs.centroid_hz as f64,
            low_band_ratio: fs.low_band_ratio as f64,
            high_band_ratio: fs.high_band_ratio as f64,
        }
    }
}

impl From<SpectralFeatures> for FeatureSet {
    fn from(sf: SpectralFeatures) -> Self {
        Self {
            rms: sf.rms as f32,
            zcr: sf.zcr as f32,
            spectral_flux: sf.spectral_flux as f32,
            spectral_flatness: sf.spectral_flatness as f32,
            spectral_entropy: sf.spectral_entropy as f32,
            centroid_hz: sf.centroid_hz as f32,
            low_band_ratio: sf.low_band_ratio as f32,
            high_band_ratio: sf.high_band_ratio as f32,
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct FalsePositive {
    pub id: i64,
    pub start_ms: i64,
    pub end_ms: i64,
    pub confidence: f64,
    pub spectral_features: SpectralFeatures,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningStatistics {
    pub total_verifications: usize,
    pub total_false_positives: usize,
    pub false_positive_rate: f64,
    pub total_fingerprints: usize,
    pub avg_fingerprints_per_segment: f64,
}
