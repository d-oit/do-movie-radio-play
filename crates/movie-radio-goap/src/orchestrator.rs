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

                if self.should_replan() {
                    warn!("External trigger detected, replanning...");
                    break;
                }
            }
        }

        info!("Goal reached!");
        Ok(())
    }

    fn should_replan(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
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
}
