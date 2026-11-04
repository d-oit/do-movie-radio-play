//! Types for human-in-the-loop review feedback on narration and gap decisions.

use std::collections::BTreeMap;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

pub const REVIEW_FEEDBACK_SCHEMA_VERSION: u32 = 1;

/// Reviewer verdict for a single narration/gap decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewLabel {
    Accept,
    Reject,
    EditNarration,
    DialogueOverlap,
    MissingDescription,
    IncorrectSoundInference,
    WrongTiming,
    UnacceptableVoice,
}

impl ReviewLabel {
    pub const ALL: [Self; 8] = [
        Self::Accept,
        Self::Reject,
        Self::EditNarration,
        Self::DialogueOverlap,
        Self::MissingDescription,
        Self::IncorrectSoundInference,
        Self::WrongTiming,
        Self::UnacceptableVoice,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accept => "accept",
            Self::Reject => "reject",
            Self::EditNarration => "edit_narration",
            Self::DialogueOverlap => "dialogue_overlap",
            Self::MissingDescription => "missing_description",
            Self::IncorrectSoundInference => "incorrect_sound_inference",
            Self::WrongTiming => "wrong_timing",
            Self::UnacceptableVoice => "unacceptable_voice",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|label| label.as_str() == value)
            .ok_or_else(|| anyhow::anyhow!("unknown review label: {value}"))
    }

    pub fn is_negative(self) -> bool {
        !matches!(self, Self::Accept)
    }
}

/// Model/provider identities that produced the reviewed decision.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelVersions {
    #[serde(default)]
    pub vad_engine: String,
    #[serde(default)]
    pub vad_model: String,
    #[serde(default)]
    pub narrator_model: String,
    #[serde(default)]
    pub tts_provider: String,
    #[serde(default)]
    pub tts_model: String,
}

/// Acoustic and visual evidence the reviewer saw for a gap.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ReviewEvidence {
    #[serde(default)]
    pub acoustic_tags: Vec<String>,
    #[serde(default)]
    pub visual_evidence: Option<String>,
    #[serde(default)]
    pub confidence: f64,
}

/// One reviewed narration/gap decision, linked to its source run and artifacts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewDecision {
    /// Stable ID; derived from source fingerprint, run and gap interval when absent.
    #[serde(default)]
    pub decision_id: String,
    pub run_id: String,
    pub source_fingerprint: String,
    pub gap_start_ms: u64,
    pub gap_end_ms: u64,
    #[serde(default)]
    pub film: String,
    #[serde(default)]
    pub language: String,
    #[serde(default)]
    pub genre: Option<String>,
    #[serde(default)]
    pub cue_source: String,
    #[serde(default)]
    pub evidence: ReviewEvidence,
    #[serde(default)]
    pub narration: Option<String>,
    #[serde(default)]
    pub tts_output: Option<String>,
    #[serde(default)]
    pub assembly_position_ms: Option<u64>,
    #[serde(default)]
    pub model_versions: ModelVersions,
    pub label: ReviewLabel,
    #[serde(default)]
    pub edited_narration: Option<String>,
    #[serde(default)]
    pub reviewer: String,
    #[serde(default)]
    pub source: String,
}

impl ReviewDecision {
    /// Returns the caller-supplied ID or the deterministic derived one.
    pub fn stable_id(&self) -> String {
        if self.decision_id.is_empty() {
            decision_id(
                &self.source_fingerprint,
                &self.run_id,
                self.gap_start_ms,
                self.gap_end_ms,
            )
        } else {
            self.decision_id.clone()
        }
    }
}

/// Deterministic decision ID over the source media, run and gap interval.
pub fn decision_id(source_fingerprint: &str, run_id: &str, start_ms: u64, end_ms: u64) -> String {
    super::hash::fingerprint(&[
        ("source", source_fingerprint.to_string()),
        ("run", run_id.to_string()),
        ("start", start_ms.to_string()),
        ("end", end_ms.to_string()),
    ])
}

/// Import payload produced by a review UI or an export.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewFeedbackBatch {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub decisions: Vec<ReviewDecision>,
}

fn default_schema_version() -> u32 {
    REVIEW_FEEDBACK_SCHEMA_VERSION
}

/// Immutable, append-only record of a reviewer change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CorrectionEvent {
    pub event_id: i64,
    pub decision_id: String,
    pub label: ReviewLabel,
    pub original_narration: Option<String>,
    pub corrected_narration: Option<String>,
    pub reviewer: String,
    pub source: String,
    pub model_versions: ModelVersions,
    pub payload_hash: String,
    pub recorded_at: String,
}

#[derive(Debug, Clone, Default)]
pub struct ImportOptions {
    pub reviewer_override: Option<String>,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportSummary {
    pub decisions_seen: usize,
    pub decisions_imported: usize,
    pub decisions_duplicate: usize,
    pub events_recorded: usize,
    pub events_duplicate: usize,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalKind {
    GapThreshold,
    NarrationCoverage,
    TagRule,
    TimingOffset,
    VoiceRanking,
    NarrationGuidance,
}

impl ProposalKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::GapThreshold => "gap_threshold",
            Self::NarrationCoverage => "narration_coverage",
            Self::TagRule => "tag_rule",
            Self::TimingOffset => "timing_offset",
            Self::VoiceRanking => "voice_ranking",
            Self::NarrationGuidance => "narration_guidance",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "gap_threshold" => Ok(Self::GapThreshold),
            "narration_coverage" => Ok(Self::NarrationCoverage),
            "tag_rule" => Ok(Self::TagRule),
            "timing_offset" => Ok(Self::TimingOffset),
            "voice_ranking" => Ok(Self::VoiceRanking),
            "narration_guidance" => Ok(Self::NarrationGuidance),
            other => bail!("unknown proposal kind: {other}"),
        }
    }
}

/// Whether a parameter came from configuration or from learned feedback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalOrigin {
    Manual,
    Learned,
}

impl ProposalOrigin {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Learned => "learned",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "manual" => Ok(Self::Manual),
            "learned" => Ok(Self::Learned),
            other => bail!("unknown proposal origin: {other}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalStatus {
    Proposed,
    Applied,
    RolledBack,
}

impl ProposalStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Proposed => "proposed",
            Self::Applied => "applied",
            Self::RolledBack => "rolled_back",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "proposed" => Ok(Self::Proposed),
            "applied" => Ok(Self::Applied),
            "rolled_back" => Ok(Self::RolledBack),
            other => bail!("unknown proposal status: {other}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VersionAction {
    Apply,
    Rollback,
}

impl VersionAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Apply => "apply",
            Self::Rollback => "rollback",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "apply" => Ok(Self::Apply),
            "rollback" => Ok(Self::Rollback),
            other => bail!("unknown version action: {other}"),
        }
    }
}

/// Evidence behind a proposed change, so reviewers can audit it before applying.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProposalEvidence {
    pub decision_ids: Vec<String>,
    pub label_counts: BTreeMap<String, usize>,
    pub sample_size: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LearningProposal {
    pub proposal_id: String,
    pub kind: ProposalKind,
    pub parameter: String,
    pub current_value: String,
    pub proposed_value: String,
    pub evidence: ProposalEvidence,
    pub expected_effect: String,
    pub origin: ProposalOrigin,
    pub status: ProposalStatus,
    pub created_at: String,
    pub applied_at: Option<String>,
    pub rolled_back_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LearningVersion {
    pub version_id: i64,
    pub proposal_id: String,
    pub parameter: String,
    pub prior_value: Option<String>,
    pub new_value: Option<String>,
    pub prior_origin: Option<ProposalOrigin>,
    pub new_origin: Option<ProposalOrigin>,
    pub action: VersionAction,
    pub recorded_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LearningParameter {
    pub parameter: String,
    pub value: String,
    pub origin: ProposalOrigin,
    pub updated_at: String,
}

/// Outcome counts grouped by film, language, genre, VAD engine and cue source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutcomeMetricRow {
    pub film: String,
    pub language: String,
    pub genre: Option<String>,
    pub vad_engine: String,
    pub cue_source: String,
    pub decisions: usize,
    pub corrections: usize,
    pub accepted: usize,
    pub rejected: usize,
    pub label_counts: BTreeMap<String, usize>,
}

/// Evaluation fixture that the learning path must never write to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HoldoutFixture {
    pub fixture_id: String,
    pub path: String,
    pub checksum: String,
    pub registered_at: String,
}

/// Artifact classes that must never leave the machine without explicit consent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactKind {
    SourceMedia,
    VoiceSample,
    ReviewArtifact,
}

/// Default-deny policy for source media, voice samples and review artifacts.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrivacyPolicy {
    #[serde(default)]
    pub allow_media_upload: bool,
    #[serde(default)]
    pub allow_voice_sample_upload: bool,
    #[serde(default)]
    pub allow_review_artifact_upload: bool,
}

impl PrivacyPolicy {
    pub fn assert_upload_allowed(&self, kind: ArtifactKind) -> Result<()> {
        let allowed = match kind {
            ArtifactKind::SourceMedia => self.allow_media_upload,
            ArtifactKind::VoiceSample => self.allow_voice_sample_upload,
            ArtifactKind::ReviewArtifact => self.allow_review_artifact_upload,
        };
        if !allowed {
            bail!("upload of {kind:?} is disabled by the review feedback privacy policy");
        }
        Ok(())
    }
}

/// Portable export of review decisions and their correction history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewFeedbackExport {
    pub schema_version: u32,
    pub decisions: Vec<ReviewDecision>,
    pub correction_events: Vec<CorrectionEvent>,
    pub privacy: PrivacyPolicy,
}
