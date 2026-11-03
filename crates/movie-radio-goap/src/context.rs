//! The per-run pipeline context: decoded audio, timeline, narration, and the
//! artifacts later stages read and update.

use movie_radio_types::{AnalysisConfig, GapAnalysisOutput, TimelineOutput};
use movie_radio_verification::VerificationReport;
use std::path::PathBuf;

pub struct PipelineContext {
    pub movie_path: PathBuf,
    pub output_path: PathBuf,
    pub subtitles_path: Option<PathBuf>,
    pub config: AnalysisConfig,
    pub timeline: Option<TimelineOutput>,
    pub gap_analysis: Option<GapAnalysisOutput>,
    pub scripts: Option<Vec<crate::narrate::NarrationScript>>,
    /// Narration audio aligned 1:1 with `scripts` (same order/length);
    /// `None` marks a script whose synthesis failed.
    pub narration_audio: Vec<Option<movie_radio_voice::AudioOutput>>,
    /// Provider for each `narration_audio` entry; keeps trace attribution honest.
    pub narration_provider: Vec<Option<String>>,
    pub original_audio: Option<Vec<f32>>,
    pub sample_rate: u32,
    /// Optional voice synthesis config (providers, fallback chain, language, voice_id).
    pub voice_config: Option<movie_radio_voice::VoiceSynthesisConfig>,
    /// Adaptive thresholds after the `apply_learnings` action, when run.
    pub learning: Option<movie_radio_learning::adaptive_thresholds::AdaptiveThresholds>,
    /// Verification report produced by the `verify_quality` action.
    pub verification: Option<VerificationReport>,
    /// Adaptive-threshold state path; `None` keeps it in memory only.
    pub learning_state_path: Option<PathBuf>,
    /// Optional libsql database path for threshold history persistence.
    pub learning_db_path: Option<PathBuf>,
    /// Assembled radio play PCM samples after `assemble_radio_play` action.
    pub assembled_audio: Option<Vec<f32>>,
    /// Track and segment mapping artifact produced during assembly.
    pub track_map: Option<movie_radio_verification::TrackMap>,
    /// Provenance for every SFX asset mixed into this run.
    pub sfx_assets: Vec<movie_radio_types::SfxAssetProvenance>,
    /// Preservation and narration-placement report produced during verification.
    pub preservation_report: Option<movie_radio_verification::PreservationReport>,
    /// Mastering profile name used for normalization and quality validation.
    pub mastering_profile: String,
    /// Mastering quality report produced by assembly and re-checked by verification.
    pub mastering_report: Option<movie_radio_render::mastering::MasteringReport>,
    /// Skip trace recording and learning adaptations when true (`--no-learn`).
    pub no_learn: bool,
    /// Unique identifier for this pipeline run.
    pub run_id: Option<String>,
    /// Optional voice clone reference audio file path.
    pub voice_reference: Option<PathBuf>,
    /// Overrides the gap confidence threshold (lower = more narrated gaps).
    pub gap_confidence: Option<f32>,
    /// Cues came from speech-to-text, so gap clipping uses a wider safety pad.
    pub subtitles_derived: bool,
    /// Prebuilt synthesizer chain for tests and offline runs.
    pub voice_orchestrator: Option<std::sync::Arc<movie_radio_voice::voice::SynthesisOrchestrator>>,
    /// Narrator configuration, known characters and the visual review report.
    pub visual: crate::narrate::visual::VisualPipelineState,
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
            narration_provider: Vec::new(),
            original_audio: None,
            voice_config: None,
            verification: None,
            learning: None,
            learning_state_path: None,
            learning_db_path: None,
            assembled_audio: None,
            track_map: None,
            sfx_assets: Vec::new(),
            preservation_report: None,
            mastering_profile: movie_radio_render::mastering::DEFAULT_PROFILE.to_string(),
            mastering_report: None,
            no_learn: false,
            run_id: None,
            voice_reference: None,
            gap_confidence: None,
            subtitles_derived: false,
            voice_orchestrator: None,
            visual: crate::narrate::visual::VisualPipelineState::default(),
        }
    }
}
