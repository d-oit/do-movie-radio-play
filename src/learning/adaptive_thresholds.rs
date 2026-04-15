use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptiveThresholds {
    pub entropy_min: f32,
    pub entropy_max: f32,
    pub flatness_max: f32,
    pub energy_min: f32,
    pub centroid_min: f32,
    pub centroid_max: f32,
    pub learning_rate: f32,
    pub history: Vec<ThresholdUpdate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThresholdUpdate {
    pub timestamp_ms: u64,
    pub segment_id: usize,
    pub was_false_positive: bool,
    pub entropy: f32,
    pub flatness: f32,
    pub energy: f32,
    pub centroid: f32,
    pub new_entropy_min: f32,
    pub new_flatness_max: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningState {
    pub current_thresholds: AdaptiveThresholds,
    pub total_verifications: usize,
    pub total_false_positives: usize,
    pub recent_fp_rate: f32,
    pub window_size: usize,
}

impl Default for LearningState {
    fn default() -> Self {
        Self {
            current_thresholds: AdaptiveThresholds::defaults(),
            total_verifications: 0,
            total_false_positives: 0,
            recent_fp_rate: 0.0,
            window_size: 20,
        }
    }
}

impl AdaptiveThresholds {
    pub fn defaults() -> Self {
        Self {
            entropy_min: 3.5,
            entropy_max: 7.0,
            flatness_max: 0.45,
            energy_min: 0.001,
            centroid_min: 100.0,
            centroid_max: 6000.0,
            learning_rate: 0.1,
            history: Vec::new(),
        }
    }
}

pub fn create_learning_state(window_size: usize) -> LearningState {
    LearningState {
        window_size,
        ..Default::default()
    }
}

pub fn record_verification_result(
    state: &mut LearningState,
    segment_id: usize,
    was_false_positive: bool,
    entropy: f32,
    flatness: f32,
    energy: f32,
    centroid: f32,
) {
    state.total_verifications += 1;
    if was_false_positive {
        state.total_false_positives += 1;
    }

    let update = ThresholdUpdate {
        timestamp_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
        segment_id,
        was_false_positive,
        entropy,
        flatness,
        energy,
        centroid,
        new_entropy_min: state.current_thresholds.entropy_min,
        new_flatness_max: state.current_thresholds.flatness_max,
    };

    state.current_thresholds.history.push(update);
    state.recent_fp_rate = calculate_recent_fp_rate(state);

    if was_false_positive {
        adjust_thresholds(state, entropy, flatness);
    }

    info!(
        verifications = state.total_verifications,
        fps = state.total_false_positives,
        fp_rate = format!("{:.2}%", state.recent_fp_rate * 100.0),
        entropy_min = state.current_thresholds.entropy_min,
        flatness_max = state.current_thresholds.flatness_max,
        "recorded verification result"
    );
}

fn adjust_thresholds(state: &mut LearningState, entropy: f32, flatness: f32) {
    let rate = state.current_thresholds.learning_rate;

    if flatness > state.current_thresholds.flatness_max * 0.8 {
        state.current_thresholds.flatness_max +=
            (flatness - state.current_thresholds.flatness_max) * rate;
        state.current_thresholds.flatness_max = state.current_thresholds.flatness_max.min(0.95);
    }

    if entropy < state.current_thresholds.entropy_min * 1.2 {
        let adjustment = (state.current_thresholds.entropy_min - entropy) * rate * 0.5;
        state.current_thresholds.entropy_min =
            (state.current_thresholds.entropy_min - adjustment).max(1.0);
    }
}

fn calculate_recent_fp_rate(state: &LearningState) -> f32 {
    let recent = state
        .current_thresholds
        .history
        .iter()
        .rev()
        .take(state.window_size);

    let recent: Vec<_> = recent.collect();
    if recent.is_empty() {
        return 0.0;
    }

    let fp_count = recent.iter().filter(|u| u.was_false_positive).count();
    fp_count as f32 / recent.len() as f32
}

pub fn adjust_thresholds_for_fp_rate(state: &mut LearningState) {
    let target_fp_rate = 0.1;
    let tolerance = 0.05;

    let diff = state.recent_fp_rate - target_fp_rate;

    if diff.abs() < tolerance {
        return;
    }

    let adjustment = diff * state.current_thresholds.learning_rate * 2.0;

    if diff > 0.0 {
        state.current_thresholds.flatness_max =
            (state.current_thresholds.flatness_max + adjustment * 0.1).min(0.95);
        state.current_thresholds.entropy_min =
            (state.current_thresholds.entropy_min - adjustment * 0.5).max(1.0);
    } else {
        state.current_thresholds.flatness_max =
            (state.current_thresholds.flatness_max + adjustment * 0.1).max(0.1);
        state.current_thresholds.entropy_min =
            (state.current_thresholds.entropy_min - adjustment * 0.5).min(6.0);
    }

    info!(
        fp_rate = format!("{:.2}%", state.recent_fp_rate * 100.0),
        adjusted_flatness_max = state.current_thresholds.flatness_max,
        adjusted_entropy_min = state.current_thresholds.entropy_min,
        "adjusted thresholds based on false positive rate"
    );
}

#[allow(dead_code)]
pub fn should_adjust_thresholds(state: &LearningState, min_samples: usize) -> bool {
    state.total_verifications >= min_samples && state.recent_fp_rate > 0.15
}

pub fn save_learning_state(state: &LearningState, path: &Path) -> Result<()> {
    let json = serde_json::to_vec_pretty(state).context("failed to serialize learning state")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, json).context("failed to write learning state")?;
    info!(path = %path.display(), "saved learning state");
    Ok(())
}

pub fn load_learning_state(path: &Path) -> Result<LearningState> {
    let data = std::fs::read_to_string(path).context("failed to read learning state")?;
    let state: LearningState =
        serde_json::from_str(&data).context("failed to parse learning state")?;
    info!(
        path = %path.display(),
        verifications = state.total_verifications,
        "loaded learning state"
    );
    Ok(state)
}

pub fn generate_threshold_recommendations(state: &LearningState) -> ThresholdRecommendations {
    let recent_false_positives: Vec<_> = state
        .current_thresholds
        .history
        .iter()
        .rev()
        .take(state.window_size)
        .filter(|u| u.was_false_positive)
        .collect();

    let avg_false_positive_flatness = if !recent_false_positives.is_empty() {
        recent_false_positives
            .iter()
            .map(|u| u.flatness)
            .sum::<f32>()
            / recent_false_positives.len() as f32
    } else {
        state.current_thresholds.flatness_max
    };

    ThresholdRecommendations {
        suggested_entropy_min: state.current_thresholds.entropy_min,
        suggested_entropy_max: state.current_thresholds.entropy_max,
        suggested_flatness_max: avg_false_positive_flatness
            .max(state.current_thresholds.flatness_max),
        confidence: if recent_false_positives.len() >= 5 {
            RecommendationConfidence::High
        } else if recent_false_positives.len() >= 2 {
            RecommendationConfidence::Medium
        } else {
            RecommendationConfidence::Low
        },
        sample_size: recent_false_positives.len(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThresholdRecommendations {
    pub suggested_entropy_min: f32,
    pub suggested_entropy_max: f32,
    pub suggested_flatness_max: f32,
    pub confidence: RecommendationConfidence,
    pub sample_size: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecommendationConfidence {
    Low,
    Medium,
    High,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn learning_state_default() {
        let state = LearningState::default();
        assert_eq!(state.total_verifications, 0);
        assert_eq!(state.total_false_positives, 0);
        assert_eq!(state.window_size, 20);
    }

    #[test]
    fn record_false_positive_adjusts_thresholds() {
        let mut state = create_learning_state(10);
        let initial_flatness = state.current_thresholds.flatness_max;

        record_verification_result(&mut state, 0, true, 3.0, 0.6, 0.01, 2000.0);

        assert!(state.current_thresholds.flatness_max >= initial_flatness);
        assert_eq!(state.total_false_positives, 1);
    }

    #[test]
    fn save_and_load_roundtrip() {
        let state = create_learning_state(10);
        let temp_path = tempfile::NamedTempFile::new().unwrap().into_temp_path();

        save_learning_state(&state, &temp_path).unwrap();
        let loaded = load_learning_state(&temp_path).unwrap();

        assert_eq!(loaded.window_size, state.window_size);
        assert_eq!(
            loaded.current_thresholds.flatness_max,
            state.current_thresholds.flatness_max
        );
    }

    #[test]
    fn recent_fp_rate_calculation() {
        let mut state = create_learning_state(5);

        for i in 0..10 {
            let is_fp = i % 3 == 0;
            record_verification_result(&mut state, i, is_fp, 5.0, 0.3, 0.01, 1500.0);
        }

        let recent = state
            .current_thresholds
            .history
            .iter()
            .rev()
            .take(5)
            .filter(|u| u.was_false_positive)
            .count();

        assert_eq!(recent, 2);
    }
}
