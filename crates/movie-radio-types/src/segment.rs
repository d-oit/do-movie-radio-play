use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SegmentKind {
    Speech,
    NonVoice,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    pub start_ms: u64,
    pub end_ms: u64,
    pub kind: SegmentKind,
    pub confidence: f32,
    pub tags: Vec<String>,
    pub prompt: Option<String>,
}

impl PartialEq for Segment {
    fn eq(&self, other: &Self) -> bool {
        self.start_ms == other.start_ms
            && self.end_ms == other.end_ms
            && self.kind == other.kind
            && (self.confidence - other.confidence).abs() < 1e-5
            && self.tags == other.tags
            && self.prompt == other.prompt
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SegmentEvent {
    SegmentDetected {
        segment: Segment,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineOutput {
    pub file: String,
    pub analysis_sample_rate: u32,
    pub frame_ms: u32,
    pub segments: Vec<Segment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualGap {
    pub start_ms: u64,
    pub end_ms: u64,
    pub confidence: f32,
    pub reason: String,
    pub priority: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GapAnalysisOutput {
    pub file: String,
    pub gaps: Vec<VisualGap>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiVoiceOutput {
    pub file: String,
    pub analysis_sample_rate: u32,
    pub frame_ms: u32,
    pub segments: Vec<Segment>,
}
