#[cfg(test)]
mod tests {
    use crate::verification::scoring::{
        build_thresholds, determine_verification_status, AppliedThresholds, DEFAULT_CENTROID_MAX,
        DEFAULT_CENTROID_MIN, DEFAULT_ENERGY_MIN, DEFAULT_ENTROPY_MAX, DEFAULT_ENTROPY_MIN,
        DEFAULT_FLATNESS_MAX,
    };
    use crate::verification::{SpectralFeatures, VerificationStatus, VerificationSummary};

    #[test]
    fn verification_status_determination() {
        let speech_like_features = SpectralFeatures {
            rms: 0.02,
            zcr: 0.15,
            spectral_entropy: 5.0,
            spectral_flatness: 0.2,
            spectral_flux: 0.005,
            centroid_hz: 1500.0,
            low_band_ratio: 0.3,
            high_band_ratio: 0.4,
        };

        let thresholds = AppliedThresholds {
            entropy_min: DEFAULT_ENTROPY_MIN,
            entropy_max: DEFAULT_ENTROPY_MAX,
            flatness_max: DEFAULT_FLATNESS_MAX,
            energy_min: DEFAULT_ENERGY_MIN,
            centroid_min: DEFAULT_CENTROID_MIN,
            centroid_max: DEFAULT_CENTROID_MAX,
        };

        let status = determine_verification_status(&speech_like_features, 0.9, &thresholds);
        assert!(matches!(
            status,
            VerificationStatus::Suspicious | VerificationStatus::Rejected
        ));

        let nonvoice_like_features = SpectralFeatures {
            rms: 0.0004,
            zcr: 0.46,
            spectral_entropy: 8.0,
            spectral_flatness: 0.72,
            spectral_flux: 0.002,
            centroid_hz: 7200.0,
            low_band_ratio: 0.1,
            high_band_ratio: 0.52,
        };
        let nonvoice_status =
            determine_verification_status(&nonvoice_like_features, 0.6, &thresholds);
        assert!(matches!(nonvoice_status, VerificationStatus::Verified));
    }

    #[test]
    fn false_positive_rate_calculation() {
        let summary = VerificationSummary {
            total_segments: 10,
            verified_count: 7,
            suspicious_count: 2,
            rejected_count: 1,
            false_positive_rate: 0.2,
            average_confidence: 0.75,
            thresholds_applied: AppliedThresholds {
                entropy_min: DEFAULT_ENTROPY_MIN,
                entropy_max: DEFAULT_ENTROPY_MAX,
                flatness_max: DEFAULT_FLATNESS_MAX,
                energy_min: DEFAULT_ENERGY_MIN,
                centroid_min: DEFAULT_CENTROID_MIN,
                centroid_max: DEFAULT_CENTROID_MAX,
            },
        };

        assert_eq!(summary.false_positive_rate, 0.2);
    }

    #[test]
    fn build_thresholds_uses_defaults() {
        let thresholds = build_thresholds(None, None, None, None, None, None);
        assert_eq!(thresholds.entropy_min, DEFAULT_ENTROPY_MIN);
        assert_eq!(thresholds.centroid_max, DEFAULT_CENTROID_MAX);
    }
}
