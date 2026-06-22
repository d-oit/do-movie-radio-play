use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrectionRecord {
    pub file_id: String,
    pub segment_start_ms: u64,
    pub segment_end_ms: u64,
    pub original_kind: String,
    pub corrected_kind: String,
    pub original_tags: Vec<String>,
    pub corrected_tags: Vec<String>,
}
