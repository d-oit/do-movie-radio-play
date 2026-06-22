use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fingerprint {
    pub hash: u32,
    pub offset_ms: u32,
}
