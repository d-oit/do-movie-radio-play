use anyhow::Result;
use async_trait::async_trait;
use movie_radio_types::{AnalysisConfig, GapAnalysisOutput, TimelineOutput};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct WorldState {
    pub movie_decoded: bool,
    pub audio_timeline_extracted: bool,
    pub visual_gaps_identified: bool,
    pub narration_scripts_generated: bool,
    pub narrator_voice_synthesized: bool,
    pub radio_play_assembled: bool,
    pub quality_verified: bool,
    pub learnings_applied: bool,
    pub gpu_available: bool,
    pub api_keys_configured: bool,
    pub local_models_loaded: bool,
}

impl WorldState {
    pub fn meets(&self, goal: &WorldState) -> bool {
        (!goal.movie_decoded || self.movie_decoded)
            && (!goal.audio_timeline_extracted || self.audio_timeline_extracted)
            && (!goal.visual_gaps_identified || self.visual_gaps_identified)
            && (!goal.narration_scripts_generated || self.narration_scripts_generated)
            && (!goal.narrator_voice_synthesized || self.narrator_voice_synthesized)
            && (!goal.radio_play_assembled || self.radio_play_assembled)
            && (!goal.quality_verified || self.quality_verified)
            && (!goal.learnings_applied || self.learnings_applied)
            && (!goal.gpu_available || self.gpu_available)
            && (!goal.api_keys_configured || self.api_keys_configured)
            && (!goal.local_models_loaded || self.local_models_loaded)
    }
}

pub struct PipelineContext {
    pub movie_path: PathBuf,
    pub output_path: PathBuf,
    pub subtitles_path: Option<PathBuf>,
    pub config: AnalysisConfig,
    pub timeline: Option<TimelineOutput>,
    pub gap_analysis: Option<GapAnalysisOutput>,
    pub scripts: Option<Vec<narrate::NarrationScript>>,
    pub narration_audio: Vec<movie_radio_voice::AudioOutput>,
    pub original_audio: Option<Vec<f32>>,
    pub sample_rate: u32,
}

impl PipelineContext {
    pub fn new(movie_path: PathBuf, output_path: PathBuf) -> Self {
        let config = AnalysisConfig::default();
        Self {
            movie_path,
            output_path,
            subtitles_path: None,
            sample_rate: config.sample_rate_hz,
            config,
            timeline: None,
            gap_analysis: None,
            scripts: None,
            narration_audio: Vec::new(),
            original_audio: None,
        }
    }
}

#[async_trait]
pub trait Action: std::fmt::Debug + Send + Sync {
    fn name(&self) -> &str;
    fn preconditions(&self) -> WorldState;
    fn effects(&self) -> WorldState;
    fn cost(&self, state: &WorldState) -> f32;

    fn is_valid(&self, state: &WorldState) -> bool {
        state.meets(&self.preconditions())
    }

    fn apply(&self, state: &WorldState) -> WorldState {
        let mut new_state = *state;
        let effects = self.effects();
        if effects.movie_decoded {
            new_state.movie_decoded = true;
        }
        if effects.audio_timeline_extracted {
            new_state.audio_timeline_extracted = true;
        }
        if effects.visual_gaps_identified {
            new_state.visual_gaps_identified = true;
        }
        if effects.narration_scripts_generated {
            new_state.narration_scripts_generated = true;
        }
        if effects.narrator_voice_synthesized {
            new_state.narrator_voice_synthesized = true;
        }
        if effects.radio_play_assembled {
            new_state.radio_play_assembled = true;
        }
        if effects.quality_verified {
            new_state.quality_verified = true;
        }
        if effects.learnings_applied {
            new_state.learnings_applied = true;
        }
        if effects.gpu_available {
            new_state.gpu_available = true;
        }
        if effects.api_keys_configured {
            new_state.api_keys_configured = true;
        }
        if effects.local_models_loaded {
            new_state.local_models_loaded = true;
        }
        new_state
    }

    async fn execute(&self, ctx: &mut PipelineContext) -> Result<()>;
}

pub mod actions;
pub mod assemble;
pub mod gaps;
pub mod narrate;
pub mod orchestrator;
pub mod planner;
