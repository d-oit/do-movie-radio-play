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
        let line_number = idx + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if idx == 0 && trimmed.contains("start_ms") {
            continue;
        }
        let cols: Vec<_> = trimmed.split(',').collect();
        if cols.len() < 2 {
            anyhow::bail!(
                "invalid manifest row at line {line_number}: expected at least 2 columns"
            );
        }
        let start_ms: u64 = cols[0]
            .trim()
            .parse()
            .with_context(|| format!("invalid start_ms at line {line_number}"))?;
        let end_ms: u64 = cols[1]
            .trim()
            .parse()
            .with_context(|| format!("invalid end_ms at line {line_number}"))?;
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
