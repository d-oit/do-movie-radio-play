use crate::{planner::Planner, Action, WorldState};
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

    pub fn run(&mut self) -> Result<()> {
        while !self.current_state.meets(&self.goal_state) {
            info!(current_state = ?self.current_state, "Planning...");
            let plan = Planner::plan(&self.current_state, &self.goal_state, &self.actions)
                .ok_or_else(|| anyhow!("No valid plan found to reach goal"))?;

            info!(plan = ?plan, "Plan found");

            for action_name in plan {
                let action = self
                    .actions
                    .iter()
                    .find(|a| a.name() == action_name)
                    .ok_or_else(|| anyhow!("Action {} not found in registry", action_name))?;

                if !action.is_valid(&self.current_state) {
                    warn!(
                        action = action_name,
                        "Action no longer valid, replanning..."
                    );
                    break;
                }

                info!(action = action_name, "Executing action");
                // In a real implementation, this would call the actual pipeline stage.
                // For now, we just update the world state based on the action's effects.
                // If an action fails, we should set some state and break to replan.

                self.current_state = action.apply(&self.current_state);

                // Simulate checking for quality or resource changes
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
        // Placeholder for external triggers (resource changes, quality threshold failures, etc.)
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::get_all_actions;
    use crate::WorldState;

    #[test]
    fn test_orchestrator_run() {
        let start = WorldState::default();
        let goal = WorldState {
            radio_play_assembled: true,
            ..WorldState::default()
        };

        let actions = get_all_actions();
        let mut orchestrator = Orchestrator::new(start, goal, actions);

        let result = orchestrator.run();
        assert!(result.is_ok());
        assert!(orchestrator.current_state.radio_play_assembled);
    }
}
