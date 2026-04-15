#![allow(unused_imports)]

use anyhow::{Context, Result};
use serde::{de::DeserializeOwned, Serialize};
use std::{collections::HashSet, fs, path::Path};

use crate::types::{SegmentKind, TimelineOutput};

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

#[derive(Debug, Clone, Serialize)]
pub struct ExportData {
    pub file: String,
    pub analysis_sample_rate: u32,
    pub frame_ms: u32,
    pub segments: Vec<ExportSegment>,
    pub verified_keys: Option<Vec<(u64, u64)>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportSegment {
    pub start_ms: u64,
    pub end_ms: u64,
    pub duration_ms: u64,
    pub kind: String,
    pub confidence: f32,
    pub tags: Vec<String>,
    pub is_verified: bool,
}

impl ExportData {
    pub fn from_timeline(
        timeline: &TimelineOutput,
        verified: Option<&HashSet<(u64, u64)>>,
    ) -> Self {
        let segments: Vec<ExportSegment> = timeline
            .segments
            .iter()
            .map(|s| {
                let is_verified = verified
                    .map(|v| v.contains(&(s.start_ms, s.end_ms)))
                    .unwrap_or(false);
                ExportSegment {
                    start_ms: s.start_ms,
                    end_ms: s.end_ms,
                    duration_ms: s.end_ms.saturating_sub(s.start_ms),
                    kind: match s.kind {
                        SegmentKind::Speech => "speech".to_string(),
                        SegmentKind::NonVoice => "non_voice".to_string(),
                    },
                    confidence: s.confidence,
                    tags: s.tags.clone(),
                    is_verified,
                }
            })
            .collect();

        ExportData {
            file: timeline.file.clone(),
            analysis_sample_rate: timeline.analysis_sample_rate,
            frame_ms: timeline.frame_ms,
            segments,
            verified_keys: verified.map(|v| v.iter().copied().collect()),
        }
    }
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
