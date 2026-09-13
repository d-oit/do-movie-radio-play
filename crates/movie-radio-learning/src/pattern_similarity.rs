use crate::database::types::SpectralFeatures;
use serde::{Deserialize, Serialize};

/// Normalized 8-dimensional acoustic spectral feature vector representation.
/// Features: [rms, zcr, spectral_flux, spectral_flatness, spectral_entropy, centroid_hz, low_band_ratio, high_band_ratio]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpectralVector {
    pub values: [f32; 8],
}

impl From<&SpectralFeatures> for SpectralVector {
    fn from(sf: &SpectralFeatures) -> Self {
        Self {
            values: [
                sf.rms as f32,
                sf.zcr as f32,
                sf.spectral_flux as f32,
                sf.spectral_flatness as f32,
                sf.spectral_entropy as f32,
                // Normalize centroid_hz (typical range 0 - 8000 Hz) to ~[0, 1]
                (sf.centroid_hz as f32 / 8000.0).clamp(0.0, 1.0),
                sf.low_band_ratio as f32,
                sf.high_band_ratio as f32,
            ],
        }
    }
}

impl SpectralVector {
    /// Computes cosine similarity between two spectral feature vectors.
    /// Returns a similarity score in the range [-1.0, 1.0].
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

    /// Computes Euclidean distance between two spectral feature vectors.
    pub fn euclidean_distance(&self, other: &Self) -> f32 {
        self.values
            .iter()
            .zip(other.values.iter())
            .map(|(a, b)| (a - b).powi(2))
            .sum::<f32>()
            .sqrt()
    }
}

/// Represents a cross-movie pattern match result.
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
}
