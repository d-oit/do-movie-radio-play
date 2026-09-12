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
    /// Optional voice synthesis config (providers, fallback chain, language, voice_id).
    pub voice_config: Option<movie_radio_voice::VoiceSynthesisConfig>,
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
    /// Assembled radio play PCM samples after `assemble_radio_play` action.
    pub assembled_audio: Option<Vec<f32>>,
    /// Skip trace recording and learning adaptations when true (`--no-learn`).
    pub no_learn: bool,
    /// Unique identifier for this pipeline run.
    pub run_id: Option<String>,
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
            voice_config: None,
            verification: None,
            learning: None,
            learning_state_path: None,
            learning_db_path: None,
            assembled_audio: None,
            no_learn: false,
            run_id: None,
        }
    }
}

/// Record execution traces (`RunTrace`, `EmotionOutcome`, `ProviderPerformance`)
/// to the learning database if learning is enabled (`!ctx.no_learn`).
pub async fn record_execution_trace(ctx: &PipelineContext) -> Result<()> {
    if ctx.no_learn {
        tracing::info!("--no-learn enabled: skipping execution trace recording");
        return Ok(());
    }

    let Some(ref db_path) = ctx.learning_db_path else {
        tracing::info!("no learning_db_path configured: skipping trace recording");
        return Ok(());
    };

    let run_id = ctx.run_id.clone().unwrap_or_else(|| {
        let movie_name = ctx
            .movie_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("movie");
        let truncated_name = &movie_name[..std::cmp::min(8, movie_name.len())];
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        format!("run-{timestamp}-{truncated_name}")
    });

    let movie_hash = ctx
        .movie_path
        .file_name()
        .map_or_else(|| "unknown".to_string(), |s| s.to_string_lossy().to_string());

    let quality_score = if let Some(ref rep) = ctx.verification {
        let total = rep.summary.total_segments;
        if total > 0 {
            Some(rep.summary.verified_count as f64 / total as f64)
        } else {
            Some(1.0)
        }
    } else if let Some(ref scripts) = ctx.scripts {
        let total = scripts.len();
        if total > 0 {
            let succ = ctx.narration_audio.iter().filter(|a| a.is_some()).count();
            Some(succ as f64 / total as f64)
        } else {
            Some(1.0)
        }
    } else {
        Some(1.0)
    };

    let duration_ms = ctx.original_audio.as_ref().map(|s| {
        if ctx.sample_rate > 0 {
            ((s.len() as f64 / f64::from(ctx.sample_rate)) * 1000.0) as i64
        } else {
            0
        }
    });

    let db = movie_radio_learning::database::LearningDb::new(db_path).await?;

    let trace = movie_radio_learning::trace_store::RunTrace {
        id: run_id.clone(),
        movie_hash,
        created_at: None,
        quality_score,
        total_cost_usd: Some(0.0),
        duration_ms,
    };

    db.record_run_trace(&trace).await?;

    if let Some(ref scripts) = ctx.scripts {
        let provider_name = ctx
            .voice_config
            .as_ref()
            .and_then(|c| c.fallback_chain.first())
            .cloned()
            .unwrap_or_else(|| "auto".to_string());

        for (i, script) in scripts.iter().enumerate() {
            let is_success = ctx.narration_audio.get(i).and_then(|a| a.as_ref()).is_some();
            let outcome = movie_radio_learning::trace_store::EmotionOutcome {
                id: None,
                segment_tag: "narration_gap".to_string(),
                emotion_used: format!("{:?}", script.emotion).to_lowercase(),
                provider: provider_name.clone(),
                quality_score: Some(if is_success { 1.0 } else { 0.0 }),
                user_approved: None,
                run_id: Some(run_id.clone()),
            };
            let _ = db.record_emotion_outcome(&outcome).await;
        }

        let perf = movie_radio_learning::trace_store::ProviderPerformance {
            id: None,
            provider: provider_name,
            scene_type: Some("radio-play".to_string()),
            avg_quality: quality_score,
            avg_latency_ms: None,
            failure_rate: Some(1.0 - quality_score.unwrap_or(1.0)),
            cost_per_char: Some(0.0),
            last_updated: None,
        };
        let _ = db.record_provider_performance(&perf).await;
    }

    tracing::info!(run_id = %run_id, "Execution trace recorded to learning database");
    Ok(())
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
    use movie_radio_verification::verification::{
        SegmentVerification, SpectralFeatures, VerificationSummary,
    };
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
            spectral_features: SpectralFeatures::default(),
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
