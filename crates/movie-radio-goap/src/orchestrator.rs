use crate::{planner::Planner, Action, PipelineContext, WorldState};
use anyhow::{anyhow, Result};
use tracing::{info, warn};

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
        while !self.current_state.meets(&self.goal_state) {
            info!(current_state = ?self.current_state, "Planning...");
            let plan = Planner::plan(&self.current_state, &self.goal_state, &self.actions)
                .ok_or_else(|| anyhow!("No valid plan found to reach goal"))?;

            info!(plan = ?plan, "Plan found");

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
                action.execute(ctx).await?;
                self.current_state = action.apply(&self.current_state);

                if self.should_replan(ctx) {
                    warn!("External trigger or quality threshold breach detected, replanning...");
                    break;
                }
            }
        }

        info!("Goal reached!");
        Ok(())
    }

    fn should_replan(&self, ctx: &mut PipelineContext) -> bool {
        if ctx.replan_requested {
            ctx.replan_requested = false;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::get_all_actions;
    use crate::planner::Planner;
    use crate::WorldState;

    #[test]
    fn test_orchestrator_plan() {
        let start = WorldState::default();
        let goal = WorldState {
            radio_play_assembled: true,
            ..WorldState::default()
        };

        let actions = get_all_actions();
        let plan = Planner::plan(&start, &goal, &actions);
        assert!(plan.is_some());
        let plan = plan.unwrap();
        assert!(plan.contains(&"decode_movie".to_string()));
        assert!(plan.contains(&"assemble_radio_play".to_string()));
    }

    #[test]
    fn test_full_goap_actions_preconditions_and_effects() {
        let actions = get_all_actions();
        let mut state = WorldState::default();

        for action in &actions {
            assert!(
                action.name() == "decode_movie" || action.is_valid(&state),
                "Action {} should be valid in sequence",
                action.name()
            );
            state = action.apply(&state);
        }

        assert!(state.movie_decoded);
        assert!(state.audio_timeline_extracted);
        assert!(state.visual_gaps_identified);
        assert!(state.narration_scripts_generated);
        assert!(state.narrator_voice_synthesized);
        assert!(state.radio_play_assembled);
        assert!(state.quality_verified);
        assert!(state.learnings_applied);
    }

    #[tokio::test]
    async fn test_orchestrator_replan_trigger() {
        use crate::Action;
        use anyhow::Result;
        use async_trait::async_trait;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        #[derive(Debug)]
        struct StepA {
            count: Arc<AtomicUsize>,
        }

        #[async_trait]
        impl Action for StepA {
            fn name(&self) -> &str {
                "step_a"
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
            fn cost(&self, _s: &WorldState) -> f32 {
                1.0
            }
            async fn execute(&self, ctx: &mut crate::PipelineContext) -> Result<()> {
                let c = self.count.fetch_add(1, Ordering::SeqCst);
                if c == 0 {
                    ctx.replan_requested = true;
                }
                Ok(())
            }
        }

        #[derive(Debug)]
        struct StepB;

        #[async_trait]
        impl Action for StepB {
            fn name(&self) -> &str {
                "step_b"
            }
            fn preconditions(&self) -> WorldState {
                WorldState {
                    movie_decoded: true,
                    ..WorldState::default()
                }
            }
            fn effects(&self) -> WorldState {
                WorldState {
                    radio_play_assembled: true,
                    ..WorldState::default()
                }
            }
            fn cost(&self, _s: &WorldState) -> f32 {
                1.0
            }
            async fn execute(&self, _ctx: &mut crate::PipelineContext) -> Result<()> {
                Ok(())
            }
        }

        let step_a_count = Arc::new(AtomicUsize::new(0));
        let actions: Vec<Box<dyn Action>> = vec![
            Box::new(StepA {
                count: step_a_count.clone(),
            }),
            Box::new(StepB),
        ];

        let start = WorldState::default();
        let goal = WorldState {
            radio_play_assembled: true,
            ..WorldState::default()
        };

        let mut orchestrator = Orchestrator::new(start, goal, actions);
        let mut ctx = crate::PipelineContext::new(
            std::path::PathBuf::from("in.mp4"),
            std::path::PathBuf::from("out.wav"),
        );

        orchestrator.run(&mut ctx).await.unwrap();

        assert_eq!(step_a_count.load(Ordering::SeqCst), 1);
        assert!(orchestrator.current_state.radio_play_assembled);
    }
}
