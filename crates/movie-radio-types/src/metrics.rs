use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StageDurations {
    pub decode_ms: u64,
    pub resample_ms: u64,
    pub framing_ms: u64,
    pub frame_ms: u64,
    pub features_ms: u64,
    pub vad_ms: u64,
    pub smooth_ms: u64,
    pub speech_ms: u64,
    pub segmenter_ms: u64,
    pub merge_ms: u64,
    pub invert_ms: u64,
    pub total_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResult {
    pub input_file: String,
    pub total_ms: u64,
    pub decode_ms: u64,
    pub frame_count: usize,
    pub segment_count: usize,
    pub stage_ms: StageDurations,
}
