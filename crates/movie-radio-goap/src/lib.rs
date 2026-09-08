use anyhow::Result;
use async_trait::async_trait;
use movie_radio_types::{AnalysisConfig, GapAnalysisOutput, TimelineOutput};
use movie_radio_verification::VerificationReport;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct WorldState {
    pub movie_decoded: bool,
    pub audio_timeline_extracted: bool,
    pub visual_gaps_identified: bool,
    pub narration_scripts_generated: bool,
    pub narrator_voice_synthesized: bool,
    pub radio_play_assembled: bool,
    pub quality_verified: bool,
    pub learnings_applied: bool,
    pub gpu_available: bool,
    pub api_keys_configured: bool,
    pub local_models_loaded: bool,
}

impl WorldState {
    pub fn meets(&self, goal: &WorldState) -> bool {
        (!goal.movie_decoded || self.movie_decoded)
            && (!goal.audio_timeline_extracted || self.audio_timeline_extracted)
            && (!goal.visual_gaps_identified || self.visual_gaps_identified)
            && (!goal.narration_scripts_generated || self.narration_scripts_generated)
            && (!goal.narrator_voice_synthesized || self.narrator_voice_synthesized)
            && (!goal.radio_play_assembled || self.radio_play_assembled)
            && (!goal.quality_verified || self.quality_verified)
            && (!goal.learnings_applied || self.learnings_applied)
            && (!goal.gpu_available || self.gpu_available)
            && (!goal.api_keys_configured || self.api_keys_configured)
            && (!goal.local_models_loaded || self.local_models_loaded)
    }
}

pub struct PipelineContext {
    pub movie_path: PathBuf,
    pub output_path: PathBuf,
    pub subtitles_path: Option<PathBuf>,
    pub config: AnalysisConfig,
    pub timeline: Option<TimelineOutput>,
    pub gap_analysis: Option<GapAnalysisOutput>,
    pub scripts: Option<Vec<narrate::NarrationScript>>,
    /// Narration audio aligned 1:1 with `scripts` (same order/length);
    /// `None` marks a script whose synthesis failed.
    pub narration_audio: Vec<Option<movie_radio_voice::AudioOutput>>,
    pub original_audio: Option<Vec<f32>>,
    pub sample_rate: u32,
    /// Adaptive thresholds after the `apply_learnings` action, when run.
    pub learning: Option<movie_radio_learning::adaptive_thresholds::AdaptiveThresholds>,
    /// Verification report produced by the `verify_quality` action.
    pub verification: Option<VerificationReport>,
    /// Optional path for the adaptive-threshold learning state
    /// (`learning_state_path`); when `None`, `apply_learnings` keeps the
    /// state in memory only.
    pub learning_state_path: Option<PathBuf>,
    /// Optional libsql database path for threshold history persistence.
    pub learning_db_path: Option<PathBuf>,
}

impl PipelineContext {
    pub fn new(movie_path: PathBuf, output_path: PathBuf) -> Self {
        let config = AnalysisConfig::default();
        Self {
            movie_path,
            output_path,
            subtitles_path: None,
            sample_rate: config.sample_rate_hz,
            config,
            timeline: None,
            gap_analysis: None,
            scripts: None,
            narration_audio: Vec::new(),
            original_audio: None,
            verification: None,
            learning: None,
            learning_state_path: None,
            learning_db_path: None,
        }
    }
}

/// Replanning/learning signal: verification flagged most non-voice segments
/// as suspicious or rejected (i.e., the extraction thresholds produced
/// likely false positives).
pub fn verification_looks_suspicious(report: &VerificationReport) -> bool {
    let s = &report.summary;
    s.total_segments > 0 && s.suspicious_count + s.rejected_count > s.verified_count
}

#[async_trait]
pub trait Action: std::fmt::Debug + Send + Sync {
    fn name(&self) -> &str;
    fn preconditions(&self) -> WorldState;
    fn effects(&self) -> WorldState;
    fn cost(&self, state: &WorldState) -> f32;

    fn is_valid(&self, state: &WorldState) -> bool {
        state.meets(&self.preconditions())
    }

    fn apply(&self, state: &WorldState) -> WorldState {
        let mut new_state = *state;
        let effects = self.effects();
        if effects.movie_decoded {
            new_state.movie_decoded = true;
        }
        if effects.audio_timeline_extracted {
            new_state.audio_timeline_extracted = true;
        }
        if effects.visual_gaps_identified {
            new_state.visual_gaps_identified = true;
        }
        if effects.narration_scripts_generated {
            new_state.narration_scripts_generated = true;
        }
        if effects.narrator_voice_synthesized {
            new_state.narrator_voice_synthesized = true;
        }
        if effects.radio_play_assembled {
            new_state.radio_play_assembled = true;
        }
        if effects.quality_verified {
            new_state.quality_verified = true;
        }
        if effects.learnings_applied {
            new_state.learnings_applied = true;
        }
        if effects.gpu_available {
            new_state.gpu_available = true;
        }
        if effects.api_keys_configured {
            new_state.api_keys_configured = true;
        }
        if effects.local_models_loaded {
            new_state.local_models_loaded = true;
        }
        new_state
    }

    async fn execute(&self, ctx: &mut PipelineContext) -> Result<()>;
}

pub mod actions;
pub mod assemble;
pub mod gaps;
pub mod narrate;
pub mod orchestrator;
pub mod planner;

#[cfg(test)]
pub(crate) mod test_support {
    use movie_radio_types::TimelineOutput;
    use movie_radio_verification::verification::{SegmentVerification, VerificationSummary};
    use movie_radio_verification::{AppliedThresholds, VerificationReport, VerificationStatus};

    pub(crate) fn empty_timeline() -> TimelineOutput {
        TimelineOutput {
            file: "movie.mkv".to_string(),
            analysis_sample_rate: 16_000,
            frame_ms: 20,
            segments: Vec::new(),
        }
    }

    pub(crate) fn segment_result(i: usize) -> SegmentVerification {
        SegmentVerification {
            start_ms: i as u64 * 1000,
            end_ms: (i as u64 + 1) * 1000,
            original_confidence: 0.9,
            verification_status: VerificationStatus::Suspicious,
            spectral_features: Default::default(),
            is_verified: false,
            is_suspicious: true,
            reason: Some("synthetic".to_string()),
        }
    }

    pub(crate) fn suspicious_report(segments: usize) -> VerificationReport {
        let results = (0..segments).map(segment_result).collect::<Vec<_>>();
        VerificationReport {
            verified_timeline: empty_timeline(),
            segment_results: results.clone(),
            segment_fingerprints: results.iter().map(|_| Vec::new()).collect(),
            summary: VerificationSummary {
                total_segments: segments,
                verified_count: 0,
                suspicious_count: segments,
                rejected_count: 0,
                false_positive_rate: 1.0,
                average_confidence: 0.9,
                thresholds_applied: default_thresholds(),
            },
        }
    }

    pub(crate) fn healthy_report() -> VerificationReport {
        let results = (0..6)
            .map(|i| {
                let mut result = segment_result(i);
                if i < 5 {
                    result.is_verified = true;
                    result.is_suspicious = false;
                    result.verification_status = VerificationStatus::Verified;
                }
                result
            })
            .collect::<Vec<_>>();
        VerificationReport {
            verified_timeline: empty_timeline(),
            segment_results: results.clone(),
            segment_fingerprints: vec![Vec::new(); 6],
            summary: VerificationSummary {
                total_segments: 6,
                verified_count: 5,
                suspicious_count: 1,
                rejected_count: 0,
                false_positive_rate: 1.0 / 6.0,
                average_confidence: 0.9,
                thresholds_applied: default_thresholds(),
            },
        }
    }

    fn default_thresholds() -> AppliedThresholds {
        AppliedThresholds {
            entropy_min: 3.5,
            entropy_max: 7.0,
            flatness_max: 0.45,
            energy_min: 0.001,
            centroid_min: 100.0,
            centroid_max: 6000.0,
        }
    }
}
