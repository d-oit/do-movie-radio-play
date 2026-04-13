use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StageDurations {
    pub decode_ms: u128,
    pub resample_ms: u128,
    pub frame_ms: u128,
    pub vad_ms: u128,
    pub smooth_ms: u128,
    pub speech_ms: u128,
    pub merge_ms: u128,
    pub invert_ms: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResult {
    pub input_file: String,
    pub total_ms: u128,
    pub decode_ms: u128,
    pub frame_count: usize,
    pub segment_count: usize,
    pub stage_ms: StageDurations,
}
