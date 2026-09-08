use crate::planner::Planner;
use crate::{Action, PipelineContext, WorldState};
use anyhow::{anyhow, bail, Result};
use tracing::{info, warn};

/// Maximum replan attempts (action failure or low-quality signal) before the
/// orchestrator gives up. Bound keeps replanning from looping forever on
/// plans that can never succeed (ADR-120).
const MAX_REPLANS: usize = 3;

pub struct Orchestrator {
    current_state: WorldState,
    goal_state: WorldState,
    actions: Vec<Box<dyn Action>>,
}

impl Orchestrator {
    pub fn new(start: WorldState, goal: WorldState, actions: Vec<Box<dyn Action>>) -> Self {
        Self {
            current_state: start,
            goal_state: goal,
            actions,
        }
    }

    pub async fn run(&mut self, ctx: &mut PipelineContext) -> Result<()> {
        let mut last_error: Option<anyhow::Error> = None;
        let mut replan_count = 0usize;

        loop {
            if self.current_state.meets(&self.goal_state) {
                info!("Goal reached!");
                return Ok(());
            }
            if replan_count > MAX_REPLANS {
                match last_error {
                    Some(err) => bail!(
                        "replan limit reached ({MAX_REPLANS} attempts); last action error: {err:#}"
                    ),
                    None => bail!(
                        "replan limit reached ({MAX_REPLANS} attempts) without reaching the goal"
                    ),
                }
            }

            let plan = Planner::plan(&self.current_state, &self.goal_state, &self.actions)
                .ok_or_else(|| anyhow!("No valid plan found to reach goal"))?;
            info!(replan_count, plan = ?plan, "Executing plan");

            let mut action_failed = false;
            let mut replan_requested = false;

            for action_name in &plan {
                let action = self
                    .actions
                    .iter()
                    .find(|a| a.name() == *action_name)
                    .ok_or_else(|| anyhow!("Action {} not found in registry", action_name))?;

                if !action.is_valid(&self.current_state) {
                    warn!(
                        action = action_name,
                        "Action no longer valid, replanning..."
                    );
                    replan_requested = true;
                    break;
                }

                info!(action = action_name, "Executing action");
                match action.execute(ctx).await {
                    Ok(()) => {
                        self.current_state = action.apply(&self.current_state);
                        if self.should_replan(ctx) {
                            warn!("Low quality signal after action, replanning...");
                            replan_requested = true;
                            break;
                        }
                    }
                    Err(err) => {
                        warn!(action = action_name, error = %err, "Action failed, replanning...");
                        last_error = Some(err);
                        action_failed = true;
                        break;
                    }
                }
            }

            if self.current_state.meets(&self.goal_state) {
                info!("Goal reached!");
                return Ok(());
            }

            if action_failed || replan_requested || self.should_replan(ctx) {
                replan_count += 1;
                info!(
                    replan_count,
                    limit = MAX_REPLANS,
                    "Replanning with remaining actions"
                );
            } else {
                // A plan executed without failure but did not reach the goal;
                // retry is bounded by the replan limit.
                replan_count += 1;
                info!(
                    replan_count,
                    limit = MAX_REPLANS,
                    "No forward progress, replanning"
                );
            }
        }
    }

    /// Replan when verification of the current run flags most non-voice
    /// segments as suspicious/rejected (likely extraction false positives).
    fn should_replan(&self, ctx: &PipelineContext) -> bool {
        ctx.verification
            .as_ref()
            .map(crate::verification_looks_suspicious)
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Action, PipelineContext, WorldState};
    use movie_radio_types::TimelineOutput;
    use movie_radio_verification::{AppliedThresholds, VerificationReport};

    #[derive(Debug, Default)]
    struct FailingAction;

    #[async_trait::async_trait]
    impl Action for FailingAction {
        fn name(&self) -> &str {
            "failing_action"
        }
        fn preconditions(&self) -> WorldState {
            WorldState::default()
        }
        fn effects(&self) -> WorldState {
            WorldState {
                movie_decoded: true,
                ..WorldState::default()
            }
        }
        fn cost(&self, _state: &WorldState) -> f32 {
            1.0
        }
        async fn execute(&self, _ctx: &mut PipelineContext) -> Result<()> {
            anyhow::bail!("injected failure")
        }
    }

    #[derive(Debug, Default)]
    struct DecodeOk;

    #[async_trait::async_trait]
    impl Action for DecodeOk {
        fn name(&self) -> &str {
            "decode_ok"
        }
        fn preconditions(&self) -> WorldState {
            WorldState::default()
        }
        fn effects(&self) -> WorldState {
            WorldState {
                movie_decoded: true,
                ..WorldState::default()
            }
        }
        fn cost(&self, _state: &WorldState) -> f32 {
            1.0
        }
        async fn execute(&self, _ctx: &mut PipelineContext) -> Result<()> {
            Ok(())
        }
    }

    fn suspicious_report() -> VerificationReport {
        VerificationReport {
            verified_timeline: TimelineOutput {
                file: "movie.mkv".to_string(),
                analysis_sample_rate: 16_000,
                frame_ms: 20,
                segments: Vec::new(),
            },
            segment_results: Vec::new(),
            segment_fingerprints: Vec::new(),
            summary: movie_radio_verification::verification::VerificationSummary {
                total_segments: 2,
                verified_count: 0,
                suspicious_count: 2,
                rejected_count: 0,
                false_positive_rate: 1.0,
                average_confidence: 0.9,
                thresholds_applied: AppliedThresholds {
                    entropy_min: 3.5,
                    entropy_max: 7.0,
                    flatness_max: 0.45,
                    energy_min: 0.001,
                    centroid_min: 100.0,
                    centroid_max: 6000.0,
                },
            },
        }
    }

    #[test]
    fn test_orchestrator_plan() {
        let start = WorldState::default();
        let goal = WorldState {
            radio_play_assembled: true,
            ..WorldState::default()
        };
        let actions = crate::actions::get_all_actions();
        let plan = Planner::plan(&start, &goal, &actions);
        assert!(plan.is_some());
        let plan = plan.unwrap();
        assert!(plan.contains(&"decode_movie".to_string()));
        assert!(plan.contains(&"assemble_radio_play".to_string()));
    }

    #[tokio::test]
    async fn run_reaches_goal_with_healthy_actions() {
        let mut orch = Orchestrator::new(
            WorldState::default(),
            WorldState {
                movie_decoded: true,
                ..WorldState::default()
            },
            vec![Box::new(DecodeOk)],
        );
        let mut ctx = PipelineContext::new("movie.mkv".into(), "out.wav".into());
        orch.run(&mut ctx).await.expect("goal reachable");
    }

    #[tokio::test]
    async fn run_bails_after_replan_limit_on_action_failure() {
        let mut orch = Orchestrator::new(
            WorldState::default(),
            WorldState {
                movie_decoded: true,
                ..WorldState::default()
            },
            vec![Box::new(FailingAction)],
        );
        let mut ctx = PipelineContext::new("movie.mkv".into(), "out.wav".into());
        let err = orch.run(&mut ctx).await.unwrap_err();
        assert!(err.to_string().contains("replan limit"), "{err}");
    }

    #[test]
    fn should_replan_reflects_verification_quality_signal() {
        let orch = Orchestrator::new(
            WorldState::default(),
            WorldState {
                movie_decoded: true,
                ..WorldState::default()
            },
            vec![Box::new(DecodeOk)],
        );

        let mut ctx = PipelineContext::new("movie.mkv".into(), "out.wav".into());
        assert!(
            !orch.should_replan(&ctx),
            "no report must not trigger replan"
        );

        ctx.verification = Some(suspicious_report());
        assert!(
            orch.should_replan(&ctx),
            "suspicious majority must trigger replan"
        );

        let mut healthy = suspicious_report();
        healthy.summary.suspicious_count = 1;
        healthy.summary.rejected_count = 0;
        healthy.summary.verified_count = 5;
        healthy.summary.total_segments = 6;
        ctx.verification = Some(healthy);
        assert!(
            !orch.should_replan(&ctx),
            "healthy majority must not trigger replan"
        );
    }
}
