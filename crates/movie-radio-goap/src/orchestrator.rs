use crate::planner::Planner;
use crate::{Action, PipelineContext, WorldState};
use anyhow::{anyhow, bail, Result};
use tracing::{info, warn};

/// Maximum plan-execution attempts (including the initial one) before the
/// orchestrator gives up. The bound keeps replanning from looping forever
/// on plans that can never succeed (ADR-120).
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
                self.check_quality_gate(ctx)?;
                info!("Goal reached!");
                return Ok(());
            }
            if replan_count >= MAX_REPLANS {
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
                    break;
                }

                info!(action = action_name, "Executing action");
                match action.execute(ctx).await {
                    Ok(()) => {
                        self.current_state = action.apply(&self.current_state);
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
                self.check_quality_gate(ctx)?;
                info!("Goal reached!");
                return Ok(());
            }

            if action_failed {
                replan_count += 1;
                info!(
                    replan_count,
                    limit = MAX_REPLANS,
                    "Replanning after action failure"
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

    /// Final quality gate: when the goal asks for verified quality but the
    /// verification report flags a suspicious/rejected majority, the run
    /// fails loudly instead of claiming success. `apply_learnings` (when in
    /// the plan) already records the FP evidence before this check, so the
    /// corrected thresholds apply from the next run onward.
    fn check_quality_gate(&self, ctx: &PipelineContext) -> Result<()> {
        if !self.goal_state.quality_verified {
            return Ok(());
        }
        if let Some(report) = &ctx.verification {
            if crate::verification_looks_suspicious(report) {
                let s = &report.summary;
                bail!(
                    "quality gate not met: {}/{} non-voice segments verified                      ({} suspicious, {} rejected); run apply_learnings and re-run",
                    s.verified_count,
                    s.total_segments,
                    s.suspicious_count,
                    s.rejected_count
                );
            }
        }
        Ok(())
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

    #[derive(Debug, Default)]
    struct GateAction {
        suspicious: bool,
    }

    #[async_trait::async_trait]
    impl Action for GateAction {
        fn name(&self) -> &str {
            "gate_action"
        }
        fn preconditions(&self) -> WorldState {
            WorldState::default()
        }
        fn effects(&self) -> WorldState {
            WorldState {
                movie_decoded: true,
                quality_verified: true,
                ..WorldState::default()
            }
        }
        fn cost(&self, _state: &WorldState) -> f32 {
            1.0
        }
        async fn execute(&self, ctx: &mut PipelineContext) -> Result<()> {
            let mut report = suspicious_report();
            if !self.suspicious {
                report.summary.suspicious_count = 1;
                report.summary.verified_count = 5;
                report.summary.total_segments = 6;
                report.summary.false_positive_rate = 1.0 / 6.0;
            }
            ctx.verification = Some(report);
            Ok(())
        }
    }

    #[tokio::test]
    async fn run_fails_when_quality_gate_not_met() {
        let mut orch = Orchestrator::new(
            WorldState::default(),
            WorldState {
                movie_decoded: true,
                quality_verified: true,
                ..WorldState::default()
            },
            vec![Box::new(GateAction { suspicious: true })],
        );
        let mut ctx = PipelineContext::new("movie.mkv".into(), "out.wav".into());
        let err = orch.run(&mut ctx).await.unwrap_err();
        assert!(err.to_string().contains("quality gate not met"), "{err}");
    }

    #[tokio::test]
    async fn run_succeeds_when_quality_gate_met() {
        let mut orch = Orchestrator::new(
            WorldState::default(),
            WorldState {
                movie_decoded: true,
                quality_verified: true,
                ..WorldState::default()
            },
            vec![Box::new(GateAction { suspicious: false })],
        );
        let mut ctx = PipelineContext::new("movie.mkv".into(), "out.wav".into());
        orch.run(&mut ctx)
            .await
            .expect("clean report must pass the gate");
    }
}
