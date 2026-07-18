use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveLearningConfig {
    pub confidence_min: f64,
    pub confidence_max: f64,
    pub proximity_threshold: f64, // e.g. 0.15 for 15%
    pub random_sample_rate: f64,  // e.g. 0.10 for 10%
}

impl Default for ActiveLearningConfig {
    fn default() -> Self {
        Self {
            confidence_min: 0.45,
            confidence_max: 0.75,
            proximity_threshold: 0.15,
            random_sample_rate: 0.10,
        }
    }
}

#[derive(Default)]
pub struct ActiveLearningSelector {
    pub config: ActiveLearningConfig,
}

impl ActiveLearningSelector {
    pub fn new(config: ActiveLearningConfig) -> Self {
        Self { config }
    }

    /// Selects segments for priority review.
    /// Returns true if the segment is a priority review candidate.
    pub fn select_segment(
        &self,
        index: usize,
        confidence: f64,
        rms: f64,
        spectral_flatness: f64,
        spectral_entropy: f64,
        centroid_hz: f64,
        // The thresholds we are validating against
        flatness_max: f64,
        entropy_min: f64,
        centroid_min: f64,
        centroid_max: f64,
        energy_min: f64,
    ) -> bool {
        // 1. Confidence interval check (low-confidence)
        if confidence >= self.config.confidence_min && confidence <= self.config.confidence_max {
            return true;
        }

        // 2. Proximity to thresholds (boundary check)
        // Check flatness
        if flatness_max > 0.0 {
            let flatness_dist = (spectral_flatness - flatness_max).abs();
            if flatness_dist / flatness_max <= self.config.proximity_threshold {
                return true;
            }
        }

        // Check entropy_min
        if entropy_min > 0.0 {
            let entropy_dist = (spectral_entropy - entropy_min).abs();
            if entropy_dist / entropy_min <= self.config.proximity_threshold {
                return true;
            }
        }

        // Check centroid_min
        if centroid_min > 0.0 {
            let centroid_min_dist = (centroid_hz - centroid_min).abs();
            if centroid_min_dist / centroid_min <= self.config.proximity_threshold {
                return true;
            }
        }

        // Check centroid_max
        if centroid_max > 0.0 {
            let centroid_max_dist = (centroid_hz - centroid_max).abs();
            if centroid_max_dist / centroid_max <= self.config.proximity_threshold {
                return true;
            }
        }

        // Check energy_min
        if energy_min > 0.0 {
            let energy_dist = (rms - energy_min).abs();
            if energy_dist / energy_min <= self.config.proximity_threshold {
                return true;
            }
        }

        // 3. Periodic/random sampling to prevent bias (deterministic based on segment index)
        let hash = (index * 2654435761) % 1000;
        let limit = (self.config.random_sample_rate * 1000.0) as usize;
        if hash < limit {
            return true;
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_active_learning_selector_confidence_interval() {
        let selector = ActiveLearningSelector::default();
        // Confidence is in range 0.45..0.75 -> priority review
        assert!(selector.select_segment(0, 0.50, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0));
        // Confidence is high -> not priority review (unless other checks trigger)
        assert!(!selector.select_segment(9, 0.90, 0.01, 0.1, 4.0, 1500.0, 0.0, 0.0, 0.0, 0.0, 0.0));
    }

    #[test]
    fn test_active_learning_selector_threshold_proximity() {
        let config = ActiveLearningConfig {
            confidence_min: 0.80,
            confidence_max: 0.90,
            proximity_threshold: 0.10, // 10% proximity
            random_sample_rate: 0.0,
        };
        let selector = ActiveLearningSelector::new(config);

        // FLATNESS threshold flatness_max = 0.45
        // Within 10% is 0.405 to 0.495.
        // Let flatness be 0.43 (within 10% of 0.45)
        assert!(selector
            .select_segment(1, 0.95, 0.02, 0.43, 4.5, 1500.0, 0.45, 3.5, 100.0, 6000.0, 0.001));

        // Outside 10% is < 0.405 or > 0.495
        // Let flatness be 0.10 (far from 0.45)
        assert!(!selector
            .select_segment(1, 0.95, 0.02, 0.10, 4.5, 1500.0, 0.45, 3.5, 100.0, 6000.0, 0.001));
    }
}
