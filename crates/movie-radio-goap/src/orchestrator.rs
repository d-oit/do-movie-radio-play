use crate::planner::Planner;
use crate::{Action, PipelineContext, WorldState};
use anyhow::{anyhow, bail, Result};
use tracing::{info, warn};

/// Maximum plan-execution attempts (including the initial one) before the
/// orchestrator gives up. The bound keeps replanning from looping forever
/// on plans that can never succeed (ADR-120).
const MAX_REPLANS: usize = 3;

/// Result of one full plan execution.
enum PlanOutcome {
    GoalMet,
    Failed(anyhow::Error),
    NoProgress,
}

fn limit_error(last_error: Option<anyhow::Error>) -> anyhow::Error {
    match last_error {
        Some(err) => anyhow::anyhow!(
            "replan limit reached ({MAX_REPLANS} attempts); last action error: {err:#}"
        ),
        None => anyhow::anyhow!(
            "replan limit reached ({MAX_REPLANS} attempts) without reaching the goal"
        ),
    }
}

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
        let mut attempts = 0usize;

        loop {
            if self.current_state.meets(&self.goal_state) {
                self.check_quality_gate(ctx)?;
                info!("Goal reached!");
                return Ok(());
            }
            if attempts >= MAX_REPLANS {
                return Err(limit_error(last_error));
            }
            attempts += 1;

            match self.execute_plan(ctx).await {
                Ok(PlanOutcome::GoalMet) => {
                    self.check_quality_gate(ctx)?;
                    info!("Goal reached!");
                    return Ok(());
                }
                Ok(PlanOutcome::Failed(err)) => {
                    last_error = Some(err);
                }
                Ok(PlanOutcome::NoProgress) | Err(_) => {}
            }
            info!(attempts, limit = MAX_REPLANS, "Replanning");
        }
    }

    /// Execute one full plan against the current world state. Planner or
    /// registry errors are returned as `Err` (not retryable); action
    /// failures come back as `Failed` so the caller can retry with a bound.
    async fn execute_plan(
        &mut self,
        ctx: &mut PipelineContext,
    ) -> Result<PlanOutcome, anyhow::Error> {
        let plan = Planner::plan(&self.current_state, &self.goal_state, &self.actions)
            .ok_or_else(|| anyhow!("No valid plan found to reach goal"))?;
        info!(plan = ?plan, "Executing plan");

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
                return Ok(PlanOutcome::NoProgress);
            }

            info!(action = action_name, "Executing action");
            match action.execute(ctx).await {
                Ok(()) => {
                    self.current_state = action.apply(&self.current_state);
                    if self.current_state.meets(&self.goal_state) {
                        return Ok(PlanOutcome::GoalMet);
                    }
                }
                Err(err) => {
                    warn!(action = action_name, error = %err, "Action failed, replanning...");
                    return Ok(PlanOutcome::Failed(err));
                }
            }
        }
        Ok(PlanOutcome::NoProgress)
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
    use crate::test_support::{healthy_report, suspicious_report};
    use crate::{Action, PipelineContext, WorldState};

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
            let report = if self.suspicious {
                suspicious_report(2)
            } else {
                healthy_report()
            };
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
