//! Single production-stage contract shared by the `produce` and `radio-play`
//! entry points (#408).
//!
//! Both commands execute the same GOAP action registry through
//! [`run_contract`], so a stage name recorded in a checkpoint always means the
//! same work and the same artifacts regardless of which command wrote it.

use anyhow::{bail, Result};

use crate::orchestrator::Orchestrator;
use crate::{Action, PipelineContext, WorldState};

pub mod artifacts;
pub mod checkpoint;

pub use crate::orchestrator::StageObserver;
pub use artifacts::{persist_stage_artifacts, restore_stage_artifacts};
pub use checkpoint::{fingerprint, identify_input, Checkpoint, InputIdentity, StageRecord};

/// Fingerprint classes a stage can depend on. A checkpointed stage is only
/// reusable while every class it reads still hashes the same, so a changed
/// voice provider re-runs synthesis without discarding the audio analysis.
pub mod input_class {
    pub const SOURCE: &str = "source";
    pub const CUES: &str = "cues";
    pub const ANALYSIS: &str = "analysis";
    pub const GAPS: &str = "gaps";
    pub const NARRATION: &str = "narration";
    pub const VOICE: &str = "voice";
    pub const ASSEMBLY: &str = "assembly";
    pub const VERIFICATION: &str = "verification";
}

/// World-state flag set when a stage succeeds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageFlag {
    MovieDecoded,
    AudioTimelineExtracted,
    VisualGapsIdentified,
    NarrationScriptsGenerated,
    NarratorVoiceSynthesized,
    RadioPlayAssembled,
    QualityVerified,
    LearningsApplied,
}

impl StageFlag {
    pub fn is_set(self, state: &WorldState) -> bool {
        match self {
            Self::MovieDecoded => state.movie_decoded,
            Self::AudioTimelineExtracted => state.audio_timeline_extracted,
            Self::VisualGapsIdentified => state.visual_gaps_identified,
            Self::NarrationScriptsGenerated => state.narration_scripts_generated,
            Self::NarratorVoiceSynthesized => state.narrator_voice_synthesized,
            Self::RadioPlayAssembled => state.radio_play_assembled,
            Self::QualityVerified => state.quality_verified,
            Self::LearningsApplied => state.learnings_applied,
        }
    }

    pub fn set_in(self, state: &mut WorldState) {
        match self {
            Self::MovieDecoded => state.movie_decoded = true,
            Self::AudioTimelineExtracted => state.audio_timeline_extracted = true,
            Self::VisualGapsIdentified => state.visual_gaps_identified = true,
            Self::NarrationScriptsGenerated => state.narration_scripts_generated = true,
            Self::NarratorVoiceSynthesized => state.narrator_voice_synthesized = true,
            Self::RadioPlayAssembled => state.radio_play_assembled = true,
            Self::QualityVerified => state.quality_verified = true,
            Self::LearningsApplied => state.learnings_applied = true,
        }
    }
}

/// One production stage: the GOAP action that performs it, the artifacts it
/// commits, and the inputs that decide whether a checkpoint is still valid.
#[derive(Debug, Clone, Copy)]
pub struct ProductionStage {
    /// Stable stage name used in checkpoints and progress output.
    pub name: &'static str,
    /// GOAP action that executes the stage.
    pub action: &'static str,
    /// Bump when the implementation or artifact format changes; older
    /// checkpoints for this stage then stop counting as reusable.
    pub version: u32,
    /// World-state flag set on success.
    pub flag: StageFlag,
    /// Fingerprint classes the stage reads.
    pub inputs: &'static [&'static str],
    /// Artifact(s) committed for the stage (shown by `--dry-run`).
    pub produces: &'static str,
    /// Credentials, models or tools the stage needs (shown by `--dry-run`).
    pub requirements: &'static str,
    /// `None` when the stage always runs; otherwise the goal flag enabling it.
    pub optional_for: Option<StageFlag>,
}

/// Execution order of the production contract.
pub const STAGES: &[ProductionStage] = &[
    ProductionStage {
        name: "decode_audio",
        action: "decode_movie",
        version: 1,
        flag: StageFlag::MovieDecoded,
        inputs: &[input_class::SOURCE, input_class::ANALYSIS],
        produces: "decoded.wav",
        requirements: "local decode (symphonia, ffmpeg fallback)",
        optional_for: None,
    },
    ProductionStage {
        name: "analyze_speech",
        action: "extract_timeline",
        version: 1,
        flag: StageFlag::AudioTimelineExtracted,
        inputs: &[input_class::SOURCE, input_class::ANALYSIS],
        produces: "timeline.json",
        requirements: "local VAD (no network)",
        optional_for: None,
    },
    ProductionStage {
        name: "identify_gaps",
        action: "identify_visual_gaps",
        version: 1,
        flag: StageFlag::VisualGapsIdentified,
        inputs: &[
            input_class::SOURCE,
            input_class::CUES,
            input_class::ANALYSIS,
            input_class::GAPS,
        ],
        produces: "gaps.json",
        requirements: "local scoring; optional dialogue cues",
        optional_for: None,
    },
    ProductionStage {
        name: "plan_narration",
        action: "generate_narration",
        version: 1,
        flag: StageFlag::NarrationScriptsGenerated,
        inputs: &[input_class::GAPS, input_class::NARRATION],
        produces: "scripts.json",
        requirements: "local templates (ADR-128); no network",
        optional_for: None,
    },
    ProductionStage {
        name: "synthesize_narration",
        action: "synthesize_narrator",
        version: 1,
        flag: StageFlag::NarratorVoiceSynthesized,
        inputs: &[input_class::NARRATION, input_class::VOICE],
        produces: "narration.json + narration_NNNN.wav per script",
        requirements: "one TTS request per narration script to the configured voice chain",
        optional_for: None,
    },
    ProductionStage {
        name: "assemble_radio_play",
        action: "assemble_radio_play",
        version: 1,
        flag: StageFlag::RadioPlayAssembled,
        inputs: &[
            input_class::SOURCE,
            input_class::VOICE,
            input_class::ASSEMBLY,
        ],
        produces: "the deliverable at --output",
        requirements: "local mixing; ffmpeg only for MP3 output; optional SFX backend",
        optional_for: None,
    },
    ProductionStage {
        name: "verify_quality",
        action: "verify_quality",
        version: 1,
        flag: StageFlag::QualityVerified,
        inputs: &[
            input_class::CUES,
            input_class::ANALYSIS,
            input_class::VERIFICATION,
        ],
        produces: "verification.json",
        requirements: "local spectral verification",
        optional_for: Some(StageFlag::QualityVerified),
    },
    ProductionStage {
        name: "apply_learnings",
        action: "apply_learnings",
        version: 1,
        flag: StageFlag::LearningsApplied,
        inputs: &[input_class::VERIFICATION],
        produces: "learning_state.json",
        requirements: "writable learning state path",
        optional_for: Some(StageFlag::LearningsApplied),
    },
];

pub fn stage_by_action(action: &str) -> Option<&'static ProductionStage> {
    STAGES.iter().find(|s| s.action == action)
}

pub fn stage_by_name(name: &str) -> Option<&'static ProductionStage> {
    STAGES.iter().find(|s| s.name == name)
}

pub fn stage_index(name: &str) -> Option<usize> {
    STAGES.iter().position(|s| s.name == name)
}

/// Stages the given goal asks for, in execution order.
pub fn required_stages(goal: &WorldState) -> Vec<&'static ProductionStage> {
    STAGES
        .iter()
        .filter(|stage| match stage.optional_for {
            None => true,
            Some(flag) => flag.is_set(goal),
        })
        .collect()
}

/// Fails when the GOAP registry and the contract disagree, so neither entry
/// point can silently run work the contract does not describe.
pub fn validate_registry(actions: &[Box<dyn Action>]) -> Result<()> {
    for (i, stage) in STAGES.iter().enumerate() {
        if STAGES[..i].iter().any(|s| s.name == stage.name) {
            bail!("duplicate production stage name '{}'", stage.name);
        }
        if STAGES[..i].iter().any(|s| s.action == stage.action) {
            bail!("production actions must be unique: '{}'", stage.action);
        }
        if !actions.iter().any(|a| a.name() == stage.action) {
            bail!(
                "production stage '{}' maps to unknown GOAP action '{}'",
                stage.name,
                stage.action
            );
        }
    }
    for action in actions {
        if stage_by_action(action.name()).is_none() {
            bail!(
                "GOAP action '{}' has no production stage; register it in STAGES",
                action.name()
            );
        }
    }
    Ok(())
}

/// Runs the production contract: plans over the GOAP registry from `start`
/// towards `goal`, notifying `observer` after every completed or failed stage.
pub async fn run_contract(
    ctx: &mut PipelineContext,
    start: WorldState,
    goal: WorldState,
    observer: Option<Box<dyn StageObserver>>,
) -> Result<()> {
    let actions = crate::actions::get_all_actions();
    validate_registry(&actions)?;
    let mut orchestrator = Orchestrator::new(start, goal, actions);
    if let Some(observer) = observer {
        orchestrator = orchestrator.with_observer(observer);
    }
    orchestrator.run(ctx).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_covers_the_action_registry() {
        let actions = crate::actions::get_all_actions();
        validate_registry(&actions).expect("contract and registry must agree");
    }

    #[test]
    fn stages_are_ordered_and_flags_are_distinct() {
        for stage in STAGES {
            assert!(!stage.produces.is_empty(), "{}", stage.name);
            assert!(!stage.inputs.is_empty(), "{}", stage.name);
        }
        let mut flags: Vec<StageFlag> = STAGES.iter().map(|s| s.flag).collect();
        flags.sort_by_key(|f| format!("{f:?}"));
        flags.dedup();
        assert_eq!(flags.len(), STAGES.len(), "stages must set distinct flags");
    }

    #[test]
    fn required_stages_follow_the_goal() {
        let base = WorldState {
            radio_play_assembled: true,
            ..WorldState::default()
        };
        let names: Vec<&str> = required_stages(&base).iter().map(|s| s.name).collect();
        assert!(names.contains(&"assemble_radio_play"));
        assert!(!names.contains(&"verify_quality"));

        let verified = WorldState {
            quality_verified: true,
            ..base
        };
        let names: Vec<&str> = required_stages(&verified).iter().map(|s| s.name).collect();
        assert!(names.contains(&"verify_quality"));
    }

    #[test]
    fn registry_with_an_unmodelled_action_is_rejected() {
        #[derive(Debug)]
        struct Extra;
        #[async_trait::async_trait]
        impl Action for Extra {
            fn name(&self) -> &str {
                "not_in_contract"
            }
            fn preconditions(&self) -> WorldState {
                WorldState::default()
            }
            fn effects(&self) -> WorldState {
                WorldState::default()
            }
            fn cost(&self, _state: &WorldState) -> f32 {
                1.0
            }
            async fn execute(&self, _ctx: &mut PipelineContext) -> Result<()> {
                Ok(())
            }
        }
        let mut actions = crate::actions::get_all_actions();
        actions.push(Box::new(Extra));
        let err = validate_registry(&actions).expect_err("unmodelled action must fail");
        assert!(err.to_string().contains("not_in_contract"), "{err}");
    }
}
