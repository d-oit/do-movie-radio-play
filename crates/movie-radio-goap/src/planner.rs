use crate::{Action, WorldState};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

#[derive(Debug, Clone, Copy, PartialEq)]
struct Node {
    state: WorldState,
    f_score: f32,
}

impl Eq for Node {}

impl Ord for Node {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse for min-heap
        other
            .f_score
            .partial_cmp(&self.f_score)
            .unwrap_or(Ordering::Equal)
    }
}

impl PartialOrd for Node {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

pub struct Planner;

impl Planner {
    pub fn plan(
        start: &WorldState,
        goal: &WorldState,
        actions: &[Box<dyn Action>],
    ) -> Option<Vec<String>> {
        let mut open_set = BinaryHeap::new();
        let mut came_from: HashMap<WorldState, (WorldState, String)> = HashMap::new();
        let mut g_score: HashMap<WorldState, f32> = HashMap::new();

        g_score.insert(*start, 0.0);
        open_set.push(Node {
            state: *start,
            f_score: Self::heuristic(start, goal),
        });

        while let Some(current_node) = open_set.pop() {
            let current_state = current_node.state;

            if current_state.meets(goal) {
                return Some(Self::reconstruct_path(came_from, current_state));
            }

            for action in actions {
                if action.is_valid(&current_state) {
                    let neighbor = action.apply(&current_state);
                    let tentative_g_score = g_score.get(&current_state).unwrap_or(&f32::INFINITY)
                        + action.cost(&current_state);

                    if tentative_g_score < *g_score.get(&neighbor).unwrap_or(&f32::INFINITY) {
                        came_from.insert(neighbor, (current_state, action.name().to_string()));
                        g_score.insert(neighbor, tentative_g_score);
                        open_set.push(Node {
                            state: neighbor,
                            f_score: tentative_g_score + Self::heuristic(&neighbor, goal),
                        });
                    }
                }
            }
        }

        None
    }

    fn heuristic(state: &WorldState, goal: &WorldState) -> f32 {
        let mut count = 0;
        if goal.movie_decoded && !state.movie_decoded {
            count += 1;
        }
        if goal.audio_timeline_extracted && !state.audio_timeline_extracted {
            count += 1;
        }
        if goal.visual_gaps_identified && !state.visual_gaps_identified {
            count += 1;
        }
        if goal.narration_scripts_generated && !state.narration_scripts_generated {
            count += 1;
        }
        if goal.narrator_voice_synthesized && !state.narrator_voice_synthesized {
            count += 1;
        }
        if goal.radio_play_assembled && !state.radio_play_assembled {
            count += 1;
        }
        if goal.quality_verified && !state.quality_verified {
            count += 1;
        }
        if goal.learnings_applied && !state.learnings_applied {
            count += 1;
        }
        if goal.gpu_available && !state.gpu_available {
            count += 1;
        }
        if goal.api_keys_configured && !state.api_keys_configured {
            count += 1;
        }
        if goal.local_models_loaded && !state.local_models_loaded {
            count += 1;
        }
        // Use 0.5 as increment to ensure heuristic is admissible (cost(apply_learnings) = 0.5)
        count as f32 * 0.5
    }

    fn reconstruct_path(
        came_from: HashMap<WorldState, (WorldState, String)>,
        mut current: WorldState,
    ) -> Vec<String> {
        let mut path = Vec::new();
        while let Some((prev, action_name)) = came_from.get(&current) {
            path.push(action_name.clone());
            current = *prev;
        }
        path.reverse();
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::get_all_actions;
    use std::time::Instant;

    #[test]
    fn test_radio_play_pipeline_plan() {
        let start = WorldState::default();
        let goal = WorldState {
            radio_play_assembled: true,
            quality_verified: true,
            ..WorldState::default()
        };

        let actions = get_all_actions();
        let start_time = Instant::now();
        let plan = Planner::plan(&start, &goal, &actions);
        let duration = start_time.elapsed();

        assert!(plan.is_some());
        let plan = plan.unwrap();
        assert_eq!(plan[0], "decode_movie");
        assert!(plan.contains(&"assemble_radio_play".to_string()));
        assert!(plan.contains(&"verify_quality".to_string()));

        println!("Planning took: {:?}", duration);
        assert!(
            duration.as_millis() < 10,
            "Planning overhead should be < 10ms"
        );
    }

    #[test]
    fn test_impossible_goal() {
        let start = WorldState::default();
        let goal = WorldState {
            radio_play_assembled: true,
            ..WorldState::default()
        };

        // Remove an essential action
        let actions = get_all_actions()
            .into_iter()
            .filter(|a| a.name() != "decode_movie")
            .collect::<Vec<_>>();

        let plan = Planner::plan(&start, &goal, &actions);
        assert!(plan.is_none());
    }

    #[test]
    fn test_plan_with_existing_state() {
        let start = WorldState {
            movie_decoded: true,
            audio_timeline_extracted: true,
            ..WorldState::default()
        };

        let goal = WorldState {
            visual_gaps_identified: true,
            ..WorldState::default()
        };

        let actions = get_all_actions();
        let plan = Planner::plan(&start, &goal, &actions);

        assert!(plan.is_some());
        let plan = plan.unwrap();
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0], "identify_visual_gaps");
    }
}
