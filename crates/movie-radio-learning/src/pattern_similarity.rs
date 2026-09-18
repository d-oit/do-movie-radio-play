use crate::database::types::SpectralFeatures;
use serde::{Deserialize, Serialize};

/// Normalized 8-dimensional acoustic feature vector for cross-movie
/// pattern similarity (#274). All components are scaled to ~[0, 1] so no
/// single raw-scale dimension (e.g. centroid_hz in Hz) dominates the
/// cosine comparison.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpectralVector {
    pub values: [f32; 8],
}

impl From<&SpectralFeatures> for SpectralVector {
    fn from(sf: &SpectralFeatures) -> Self {
        Self {
            values: [
                (sf.rms as f32).clamp(0.0, 1.0),
                (sf.zcr as f32).clamp(0.0, 1.0),
                (sf.spectral_flux as f32).clamp(0.0, 1.0),
                (sf.spectral_flatness as f32).clamp(0.0, 1.0),
                // Normalize spectral entropy (Shannon bits, typical range 0-8) to ~[0, 1].
                (sf.spectral_entropy as f32 / 8.0).clamp(0.0, 1.0),
                // Normalize centroid_hz (typical range 0 - 8000 Hz) to ~[0, 1].
                (sf.centroid_hz as f32 / 8000.0).clamp(0.0, 1.0),
                (sf.low_band_ratio as f32).clamp(0.0, 1.0),
                (sf.high_band_ratio as f32).clamp(0.0, 1.0),
            ],
        }
    }
}

impl SpectralVector {
    /// Cosine similarity in [-1.0, 1.0]. Returns 0.0 for zero vectors.
    pub fn cosine_similarity(&self, other: &Self) -> f32 {
        let dot_product: f32 = self
            .values
            .iter()
            .zip(other.values.iter())
            .map(|(a, b)| a * b)
            .sum();
        let norm_a: f32 = self.values.iter().map(|a| a * a).sum::<f32>().sqrt();
        let norm_b: f32 = other.values.iter().map(|b| b * b).sum::<f32>().sqrt();
        if norm_a == 0.0 || norm_b == 0.0 {
            0.0
        } else {
            (dot_product / (norm_a * norm_b)).clamp(-1.0, 1.0)
        }
    }

    /// Euclidean distance between two spectral feature vectors.
    pub fn euclidean_distance(&self, other: &Self) -> f32 {
        self.values
            .iter()
            .zip(other.values.iter())
            .map(|(a, b)| (a - b).powi(2))
            .sum::<f32>()
            .sqrt()
    }
}

/// Cross-movie pattern match result: the matched item plus its score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternMatchResult<T> {
    pub item: T,
    pub similarity_score: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_spectral_vector_cosine_similarity() {
        let vec1 = SpectralVector {
            values: [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8],
        };
        let vec2 = SpectralVector {
            values: [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8],
        };
        let sim = vec1.cosine_similarity(&vec2);
        assert!((sim - 1.0).abs() < 1e-5);
    }
    #[test]
    fn test_spectral_vector_orthogonal() {
        let vec1 = SpectralVector {
            values: [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        };
        let vec2 = SpectralVector {
            values: [0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        };
        let sim = vec1.cosine_similarity(&vec2);
        assert_eq!(sim, 0.0);
    }
    #[test]
    fn test_raw_scale_features_do_not_dominate_similarity() {
        // Raw centroid_hz (~4000) would dwarf every other component without
        // normalization; the vector must stay in [0, 1] instead.
        let sf = SpectralFeatures {
            rms: 0.5,
            zcr: 0.1,
            spectral_flux: 0.3,
            spectral_flatness: 0.4,
            spectral_entropy: 0.6,
            centroid_hz: 4000.0,
            low_band_ratio: 0.7,
            high_band_ratio: 0.3,
        };
        let v = SpectralVector::from(&sf);
        assert!(v.values.iter().all(|x| (0.0..=1.0).contains(x)));
        assert!((v.values[5] - 0.5).abs() < 1e-6);
        // Realistic Shannon-bits entropy (~5) must normalize distinctively,
        // not collapse onto the 1.0 clamp shared by every high-entropy input.
        let hi = SpectralFeatures { spectral_entropy: 5.2, ..sf };
        let lo = SpectralFeatures { spectral_entropy: 3.1, ..sf };
        let v_hi = SpectralVector::from(&hi);
        let v_lo = SpectralVector::from(&lo);
        assert!((v_hi.values[4] - 0.65).abs() < 1e-6);
        assert!((v_lo.values[4] - 0.3875).abs() < 1e-6);
        assert!(v_hi.values[4] - v_lo.values[4] > 0.2);
    }
}
