use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

use crate::validation::{speech_segment, timeline_from_speech_segments};

pub fn build_truth_from_manifest(
    manifest_csv: &Path,
    media_name: &str,
    sample_rate: u32,
    frame_ms: u32,
    total_ms: u64,
    min_non_voice_ms: u32,
) -> Result<crate::types::TimelineOutput> {
    let data = fs::read_to_string(manifest_csv)
        .with_context(|| format!("cannot read manifest {}", manifest_csv.display()))?;
    let mut speech = Vec::new();
    for (idx, line) in data.lines().enumerate() {
        if idx == 0 && line.contains("start_ms") {
            continue;
        }
        let cols: Vec<_> = line.split(',').collect();
        if cols.len() < 2 {
            continue;
        }
        let start_ms: u64 = cols[0].trim().parse().unwrap_or(0);
        let end_ms: u64 = cols[1].trim().parse().unwrap_or(start_ms);
        speech.push(speech_segment(start_ms, end_ms));
    }
    Ok(timeline_from_speech_segments(
        media_name.to_string(),
        sample_rate,
        frame_ms,
        &speech,
        total_ms,
        min_non_voice_ms,
    ))
}
