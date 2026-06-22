use crate::{Action, WorldState};

#[derive(Debug, Default)]
pub struct DecodeMovie;

impl Action for DecodeMovie {
    fn name(&self) -> &str {
        "decode_movie"
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
}

#[derive(Debug, Default)]
pub struct ExtractTimeline;

impl Action for ExtractTimeline {
    fn name(&self) -> &str {
        "extract_timeline"
    }
    fn preconditions(&self) -> WorldState {
        WorldState {
            movie_decoded: true,
            ..WorldState::default()
        }
    }
    fn effects(&self) -> WorldState {
        WorldState {
            audio_timeline_extracted: true,
            ..WorldState::default()
        }
    }
    fn cost(&self, _state: &WorldState) -> f32 {
        2.0
    }
}

#[derive(Debug, Default)]
pub struct IdentifyVisualGaps;

impl Action for IdentifyVisualGaps {
    fn name(&self) -> &str {
        "identify_visual_gaps"
    }
    fn preconditions(&self) -> WorldState {
        WorldState {
            audio_timeline_extracted: true,
            ..WorldState::default()
        }
    }
    fn effects(&self) -> WorldState {
        WorldState {
            visual_gaps_identified: true,
            ..WorldState::default()
        }
    }
    fn cost(&self, _state: &WorldState) -> f32 {
        3.0
    }
}

#[derive(Debug, Default)]
pub struct GenerateNarration;

impl Action for GenerateNarration {
    fn name(&self) -> &str {
        "generate_narration"
    }
    fn preconditions(&self) -> WorldState {
        WorldState {
            visual_gaps_identified: true,
            ..WorldState::default()
        }
    }
    fn effects(&self) -> WorldState {
        WorldState {
            narration_scripts_generated: true,
            ..WorldState::default()
        }
    }
    fn cost(&self, _state: &WorldState) -> f32 {
        2.0
    }
}

#[derive(Debug, Default)]
pub struct SynthesizeNarrator;

impl Action for SynthesizeNarrator {
    fn name(&self) -> &str {
        "synthesize_narrator"
    }
    fn preconditions(&self) -> WorldState {
        WorldState {
            narration_scripts_generated: true,
            ..WorldState::default()
        }
    }
    fn effects(&self) -> WorldState {
        WorldState {
            narrator_voice_synthesized: true,
            ..WorldState::default()
        }
    }
    fn cost(&self, _state: &WorldState) -> f32 {
        5.0
    }
}

#[derive(Debug, Default)]
pub struct AssembleRadioPlay;

impl Action for AssembleRadioPlay {
    fn name(&self) -> &str {
        "assemble_radio_play"
    }
    fn preconditions(&self) -> WorldState {
        WorldState {
            narrator_voice_synthesized: true,
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
    fn cost(&self, _state: &WorldState) -> f32 {
        1.5
    }
}

#[derive(Debug, Default)]
pub struct VerifyQuality;

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
}

#[derive(Debug, Default)]
pub struct ApplyLearnings;

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
}

pub fn get_all_actions() -> Vec<Box<dyn Action>> {
    vec![
        Box::new(DecodeMovie),
        Box::new(ExtractTimeline),
        Box::new(IdentifyVisualGaps),
        Box::new(GenerateNarration),
        Box::new(SynthesizeNarrator),
        Box::new(AssembleRadioPlay),
        Box::new(VerifyQuality),
        Box::new(ApplyLearnings),
    ]
}
