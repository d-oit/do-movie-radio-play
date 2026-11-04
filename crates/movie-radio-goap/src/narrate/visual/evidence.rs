//! Per-window evidence and the review report the visual stage emits.

use movie_radio_types::VisualGap;
use serde::{Deserialize, Serialize};

use super::frames::SamplingPolicy;
use super::grounding::RejectedClaim;
use super::NarrationScript;

/// Minimum gap duration the acoustic generator narrates; the visual stage uses
/// the same bound so both paths cover identical windows.
pub(super) const MIN_GAP_DURATION_MS: u64 = 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NarrationWindow {
    pub start_ms: u64,
    pub end_ms: u64,
}

impl NarrationWindow {
    pub fn from_gap(gap: &VisualGap) -> Self {
        Self {
            start_ms: gap.start_ms,
            end_ms: gap.end_ms,
        }
    }

    pub fn duration_ms(&self) -> u64 {
        self.end_ms.saturating_sub(self.start_ms)
    }

    pub(super) fn is_narratable(&self) -> bool {
        self.duration_ms() >= MIN_GAP_DURATION_MS
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceSource {
    Visual,
    AudioOnly,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrameEvidence {
    pub frame_ref: String,
    pub timestamp_ms: u64,
    pub confidence: f32,
    pub description: String,
    pub uncertainty: Vec<String>,
    /// On-disk sampled frame used as evidence.
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindowEvidence {
    pub gap_start_ms: u64,
    pub gap_end_ms: u64,
    pub source: EvidenceSource,
    pub narration_text: String,
    pub frames: Vec<FrameEvidence>,
    pub rejected_claims: Vec<RejectedClaim>,
    pub fallback_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VisualNarrationReport {
    pub movie: String,
    pub language: String,
    pub provider: String,
    pub model: String,
    pub sampling: SamplingPolicy,
    pub visual_windows: usize,
    pub audio_only_windows: usize,
    pub windows: Vec<WindowEvidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualNarrationOutcome {
    pub scripts: Vec<NarrationScript>,
    pub report: VisualNarrationReport,
}
