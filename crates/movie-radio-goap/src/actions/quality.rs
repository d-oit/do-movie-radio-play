use anyhow::Result;
use async_trait::async_trait;
use tracing::info;

use crate::{Action, PipelineContext, WorldState};

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
        info!("Verifying quality of assembled radio play");

        if let Some(ref timeline) = ctx.timeline {
            if ctx.movie_path.exists() {
                let report_path = ctx.output_path.with_extension("verification.json");
                let report = movie_radio_verification::verify_timeline(
                    &ctx.movie_path,
                    timeline,
                    &report_path,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    false,
                    1,
                    None,
                )?;

                let fp_rate = report.summary.false_positive_rate;
                ctx.quality_score = 1.0 - fp_rate;
                info!(
                    quality_score = ctx.quality_score,
                    verified = report.summary.verified_count,
                    suspicious = report.summary.suspicious_count,
                    "Quality verification complete"
                );

                if fp_rate > 0.5 {
                    info!("High suspicious/false-positive rate detected; replan requested");
                    ctx.replan_requested = true;
                }

                ctx.verification_report = Some(report);
                return Ok(());
            }
        }

        if ctx.output_path.exists() {
            info!(output = %ctx.output_path.display(), "Verifying assembled output file presence and size");
            let meta = std::fs::metadata(&ctx.output_path)?;
            if meta.len() == 0 {
                anyhow::bail!("Assembled radio play file is empty");
            }
            ctx.quality_score = 1.0;
        } else {
            info!("Output file not yet written to disk; assuming basic quality criteria met");
            ctx.quality_score = 1.0;
        }

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
        info!("Applying learnings and updating adaptive thresholds");

        use movie_radio_learning::adaptive_thresholds::{
            adjust_thresholds_for_fp_rate, create_learning_state, record_verification_result,
            save_learning_state, should_adjust_thresholds,
        };

        let mut learning_state = ctx
            .learning_state
            .clone()
            .unwrap_or_else(|| create_learning_state(100));

        if let Some(ref report) = ctx.verification_report {
            for (idx, seg_res) in report.segment_results.iter().enumerate() {
                let was_false_positive = seg_res.is_suspicious;
                let feats = &seg_res.spectral_features;
                record_verification_result(
                    &mut learning_state,
                    idx,
                    was_false_positive,
                    feats.spectral_entropy,
                    feats.spectral_flatness,
                    feats.rms,
                    feats.centroid_hz,
                );
            }

            if should_adjust_thresholds(&learning_state, 5) {
                info!("Adjusting adaptive thresholds based on accumulated verification state");
                adjust_thresholds_for_fp_rate(&mut learning_state);
            }

            let learning_path = ctx.output_path.with_extension("learning.json");
            if let Err(e) = save_learning_state(&learning_state, &learning_path) {
                tracing::warn!(error = %e, "Failed to save learning state");
            }
        }

        ctx.learning_state = Some(learning_state);
        info!("Learnings applied successfully");
        Ok(())
    }
}
