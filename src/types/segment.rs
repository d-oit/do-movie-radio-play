use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SegmentKind {
    Speech,
    NonVoice,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SegmentTag {
    Music,
    MusicWithVoice,
    Speech,
    Ambience,
}

impl std::fmt::Display for SegmentTag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            SegmentTag::Music => "music",
            SegmentTag::MusicWithVoice => "music_with_voice",
            SegmentTag::Speech => "speech",
            SegmentTag::Ambience => "ambience",
        };
        write!(f, "{}", s)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    pub start_ms: u64,
    pub end_ms: u64,
    pub kind: SegmentKind,
    pub confidence: f32,
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constellation_density: Option<f32>,
    pub prompt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineOutput {
    pub file: String,
    pub analysis_sample_rate: u32,
    pub frame_ms: u32,
    pub segments: Vec<Segment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiVoiceOutput {
    pub file: String,
    pub analysis_sample_rate: u32,
    pub frame_ms: u32,
    pub segments: Vec<Segment>,
}
