use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineOutput {
    pub file: String,
    pub analysis_sample_rate: u32,
    pub frame_ms: u32,
    pub segments: Vec<Segment>,
}
