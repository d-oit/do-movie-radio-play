pub mod compare;
pub mod dataset;
pub mod srt;
pub mod synthetic;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::io::json::{read_timeline, write_json_pretty};
use crate::pipeline::extract_timeline;
use crate::types::{Segment, SegmentKind, TimelineOutput};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationReport {
    pub profile: String,
    pub tolerance_ms: u64,
    pub expected_segments: usize,
    pub predicted_segments: usize,
    pub overlap_ratio: f32,
    pub boundary_error_ms: f32,
    pub speech_precision: f32,
    pub speech_recall: f32,
    pub non_voice_precision: f32,
    pub non_voice_recall: f32,
}

pub fn validate_against_timeline(
    predicted: &TimelineOutput,
    truth: &TimelineOutput,
    profile: &str,
    tolerance_ms: u64,
) -> ValidationReport {
    let metrics = compare::score_segments(&predicted.segments, &truth.segments, tolerance_ms);
    ValidationReport {
        profile: profile.to_string(),
        tolerance_ms,
        expected_segments: truth.segments.len(),
        predicted_segments: predicted.segments.len(),
        overlap_ratio: metrics.overlap_ratio,
        boundary_error_ms: metrics.boundary_error_ms,
        speech_precision: metrics.speech_precision,
        speech_recall: metrics.speech_recall,
        non_voice_precision: metrics.non_voice_precision,
        non_voice_recall: metrics.non_voice_recall,
    }
}

pub fn validate_file(
    input_media: &Path,
    truth_json: &Path,
    output_report: &Path,
    cfg: &crate::config::AnalysisConfig,
    tolerance_ms: u64,
    profile: &str,
) -> Result<()> {
    let predicted = extract_timeline(input_media, cfg)
        .with_context(|| format!("extract failed for {}", input_media.display()))?;
    let truth = read_timeline(truth_json)
        .with_context(|| format!("cannot read truth json {}", truth_json.display()))?;
    let report = validate_against_timeline(&predicted, &truth, profile, tolerance_ms);
    write_json_pretty(output_report, &report)
        .with_context(|| format!("cannot write {}", output_report.display()))?;
    Ok(())
}

pub fn timeline_from_speech_segments(
    file: String,
    sample_rate: u32,
    frame_ms: u32,
    speech: &[Segment],
    total_ms: u64,
    min_non_voice_ms: u32,
) -> TimelineOutput {
    let non_voice = crate::pipeline::segmenter::invert_to_non_voice(
        speech,
        total_ms,
        min_non_voice_ms,
        frame_ms,
        &[],
    );
    TimelineOutput {
        file,
        analysis_sample_rate: sample_rate,
        frame_ms,
        segments: non_voice,
    }
}

pub fn speech_segment(start_ms: u64, end_ms: u64) -> Segment {
    Segment {
        start_ms,
        end_ms,
        kind: SegmentKind::Speech,
        confidence: 1.0,
        tags: vec![],
        prompt: None,
    }
}
