use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResult {
    pub input_file: String,
    pub total_ms: u128,
    pub decode_ms: u128,
    pub frame_count: usize,
    pub segment_count: usize,
}
