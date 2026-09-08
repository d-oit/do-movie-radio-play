use anyhow::{Context, Result};
use async_trait::async_trait;
use tracing::info;

use crate::{Action, PipelineContext, WorldState};
use movie_radio_learning::adaptive_thresholds::{
    adjust_thresholds_for_fp_rate, create_learning_state, load_learning_state,
    record_verification_result, save_learning_state,
};
use movie_radio_learning::database::LearningDb;
use movie_radio_pipeline::pipeline::decode::decode_audio;
use movie_radio_verification::verify_timeline;

/// Six optional spectral thresholds passed to `verify_timeline`.
type ThresholdOptions = (
    Option<f32>,
    Option<f32>,
    Option<f32>,
    Option<f32>,
    Option<f32>,
    Option<f32>,
);

fn threshold_tuple(
    t: &movie_radio_learning::adaptive_thresholds::AdaptiveThresholds,
) -> ThresholdOptions {
    (
        Some(t.entropy_min),
        Some(t.entropy_max),
        Some(t.flatness_max),
        Some(t.energy_min),
        Some(t.centroid_min),
        Some(t.centroid_max),
    )
}

#[derive(Debug, Default)]
pub struct VerifyQuality;

#[async_trait]
impl Action for VerifyQuality {
    fn name(&self) -> &str {
        "verify_quality"
    }
    fn preconditions(&self) -> WorldState {
        WorldState {
            radio_play_assembled: true,
            ..WorldState::default()
        }
    }
    fn effects(&self) -> WorldState {
        WorldState {
            quality_verified: true,
            ..WorldState::default()
        }
    }
    fn cost(&self, _state: &WorldState) -> f32 {
        2.0
    }

    async fn execute(&self, ctx: &mut PipelineContext) -> Result<()> {
        let timeline = ctx.timeline.as_ref().context("Timeline not extracted")?;
        if !ctx.movie_path.exists() {
            anyhow::bail!(
                "cannot verify quality: media file not found: {}",
                ctx.movie_path.display()
            );
        }

        // Reuse persisted adaptive thresholds so the learning loop closes:
        // a previous apply_learnings run adjusts what this run verifies with.
        let (entropy_min, entropy_max, flatness_max, energy_min, centroid_min, centroid_max) =
            match &ctx.learning_state_path {
                Some(path) if path.exists() => {
                    let state = load_learning_state(path)?;
                    let t = &state.current_thresholds;
                    threshold_tuple(t)
                }
                _ => match &ctx.learning {
                    Some(t) => threshold_tuple(t),
                    None => (None, None, None, None, None, None),
                },
            };

        // The report JSON is an intermediate artifact: verify into a temp
        // file and keep the typed report in the context for downstream
        // actions (apply_learnings) and replanning decisions.
        let report_path = tempfile::NamedTempFile::new()
            .context("failed to create temporary verification report path")?;
        let report = verify_timeline(
            &ctx.movie_path,
            timeline,
            report_path.path(),
            entropy_min,
            entropy_max,
            flatness_max,
            energy_min,
            centroid_min,
            centroid_max,
            false,
            10,
            None,
        )
        .context("quality verification failed")?;

        let summary = &report.summary;
        info!(
            total = summary.total_segments,
            verified = summary.verified_count,
            suspicious = summary.suspicious_count,
            rejected = summary.rejected_count,
            avg_confidence = format!("{:.3}", summary.average_confidence),
            "Quality verification complete"
        );
        if crate::verification_looks_suspicious(&report) {
            tracing::warn!(
                suspicious = summary.suspicious_count,
                rejected = summary.rejected_count,
                verified = summary.verified_count,
                "Most non-voice segments were not verified; thresholds likely need                  recalibration (apply_learnings)"
            );
        }
        self.check_assembled_output(ctx)?;
        ctx.verification = Some(report);
        Ok(())
    }
}

impl VerifyQuality {
    /// Verify the assembled radio play file itself: it must exist, decode
    /// cleanly, and (when the original audio is in context) stay within the
    /// roadmap's 10% duration tolerance of the original.
    fn check_assembled_output(&self, ctx: &PipelineContext) -> Result<()> {
        if !ctx.output_path.exists() {
            anyhow::bail!("assembled output missing: {}", ctx.output_path.display());
        }
        let (output, output_rate) =
            decode_audio(&ctx.output_path, ctx.sample_rate).with_context(|| {
                format!(
                    "assembled output failed to decode: {}",
                    ctx.output_path.display()
                )
            })?;
        if output.is_empty() {
            anyhow::bail!("assembled output decodes to zero samples");
        }
        if let Some(original) = &ctx.original_audio {
            let original_duration = original.len() as f64 / f64::from(ctx.sample_rate);
            let output_duration = output.len() as f64 / f64::from(output_rate);
            if !durations_within_tolerance(original_duration, output_duration, 0.10) {
                anyhow::bail!(
                    "assembled output duration {output_duration:.2}s deviates from original {original_duration:.2}s by more than 10%"
                );
            }
        }
        tracing::info!("Assembled output verified (decode + duration)");
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct ApplyLearnings;

#[async_trait]
impl Action for ApplyLearnings {
    fn name(&self) -> &str {
        "apply_learnings"
    }
    fn preconditions(&self) -> WorldState {
        WorldState {
            quality_verified: true,
            ..WorldState::default()
        }
    }
    fn effects(&self) -> WorldState {
        WorldState {
            learnings_applied: true,
            ..WorldState::default()
        }
    }
    fn cost(&self, _state: &WorldState) -> f32 {
        0.5
    }

    async fn execute(&self, ctx: &mut PipelineContext) -> Result<()> {
        let report = ctx
            .verification
            .as_ref()
            .context("no verification report available: run verify_quality first")?;

        // Feed every non-voice segment verdict into the adaptive-threshold
        // learning state. A segment verification flagged as suspicious or
        // rejected means the extractor cut a likely speech segment as
        // non-voice - a false positive from the learning loop's viewpoint.
        let mut state = match &ctx.learning_state_path {
            Some(path) if path.exists() => load_learning_state(path)?,
            _ => create_learning_state(20),
        };
        for (i, result) in report.segment_results.iter().enumerate() {
            let feats = &result.spectral_features;
            record_verification_result(
                &mut state,
                i,
                !result.is_verified,
                feats.spectral_entropy,
                feats.spectral_flatness,
                feats.rms,
                feats.centroid_hz,
            );
        }

        // Bounded per-run adjustment: the learning-rate-limited updates in
        // movie-radio-learning keep each parameter move small and clamped.
        if state.total_verifications >= 5 {
            adjust_thresholds_for_fp_rate(&mut state);
        }

        // Persist the state file first: it is the durable record. The
        // threshold-history database write is best-effort telemetry, so a
        // failure there cannot fail the action (which would otherwise retry
        // and re-record the same rows or duplicate state history).
        if let Some(path) = &ctx.learning_state_path {
            save_learning_state(&state, path)?;
        } else {
            tracing::info!(
                "no learning_state_path configured: adjusted thresholds kept in memory only"
            );
        }

        if let Some(db_path) = &ctx.learning_db_path {
            let t = &state.current_thresholds;
            match LearningDb::new(db_path).await {
                Ok(db) => {
                    if let Err(err) = db
                        .record_threshold(
                            f64::from(t.flatness_max),
                            f64::from(t.entropy_min),
                            f64::from(t.centroid_min),
                            f64::from(t.centroid_max),
                        )
                        .await
                    {
                        tracing::warn!(
                            error = %err,
                            "failed to record thresholds in learning database"
                        );
                    }
                }
                Err(err) => {
                    tracing::warn!(error = %err, "failed to open learning database");
                }
            }
        }

        let state_path = ctx
            .learning_state_path
            .as_ref()
            .map_or_else(|| "<memory>".to_string(), |p| p.display().to_string());
        info!(
            verifications = state.total_verifications,
            fp_rate = format!("{:.2}%", state.recent_fp_rate * 100.0),
            flatness_max = state.current_thresholds.flatness_max,
            entropy_min = state.current_thresholds.entropy_min,
            state_path = state_path,
            "Applying learnings complete"
        );
        ctx.learning = Some(state.current_thresholds.clone());
        Ok(())
    }
}

/// True when `output` stays within `tolerance` (fraction) of `original`.
fn durations_within_tolerance(original: f64, output: f64, tolerance: f64) -> bool {
    if original <= 0.0 {
        return output > 0.0;
    }
    let ratio = output / original;
    (1.0 - tolerance..=1.0 + tolerance).contains(&ratio)
}
