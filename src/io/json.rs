use anyhow::{Context, Result};
use serde::{de::DeserializeOwned, Serialize};
use std::{fs, path::Path};

use crate::types::TimelineOutput;

pub fn write_json_pretty<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    let bytes = serde_json::to_vec_pretty(value)?;
    fs::write(path, bytes)?;
    Ok(())
}

pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let data = fs::read(path)?;
    Ok(serde_json::from_slice(&data)?)
}

pub fn read_timeline(path: &Path) -> Result<TimelineOutput> {
    read_json(path)
}

#[cfg(test)]
mod tests {
    use crate::types::{Segment, SegmentKind, TimelineOutput};

    #[test]
    fn serde_roundtrip() {
        let out = TimelineOutput {
            file: "a.wav".into(),
            analysis_sample_rate: 16000,
            frame_ms: 20,
            segments: vec![Segment {
                start_ms: 0,
                end_ms: 1000,
                kind: SegmentKind::NonVoice,
                confidence: 0.9,
                tags: vec!["ambience".to_string()],
                prompt: None,
            }],
        };
        let v = serde_json::to_string(&out).unwrap_or_default();
        let parsed: TimelineOutput = serde_json::from_str(&v).unwrap_or_else(|_| TimelineOutput {
            file: String::new(),
            analysis_sample_rate: 0,
            frame_ms: 0,
            segments: vec![],
        });
        assert_eq!(parsed.segments.len(), 1);
    }
}
