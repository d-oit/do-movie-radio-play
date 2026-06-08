use anyhow::{bail, Context, Result};
use std::collections::HashSet;

use crate::config::{MergeOptions, MergeStrategy};
use crate::types::{Segment, SegmentKind, TimelineOutput};

/// Merge non-voice segments according to strategy and verified keys.
pub fn merge_nonvoice_segments(
    timeline: &TimelineOutput,
    options: &MergeOptions,
    verified_keys: Option<&HashSet<(u64, u64)>>,
) -> TimelineOutput {
    let non_voice_segments: Vec<_> = timeline
        .segments
        .iter()
        .filter(|s| {
            if s.kind != SegmentKind::NonVoice {
                return false;
            }
            if let Some(keys) = verified_keys {
                keys.contains(&(s.start_ms, s.end_ms))
            } else {
                true
            }
        })
        .collect();

    if non_voice_segments.is_empty() {
        return TimelineOutput {
            file: timeline.file.clone(),
            analysis_sample_rate: timeline.analysis_sample_rate,
            frame_ms: timeline.frame_ms,
            segments: vec![],
        };
    }

    let merged = match options.merge_strategy {
        crate::config::MergeStrategy::All => merge_all(non_voice_segments),
        crate::config::MergeStrategy::Longest => {
            merge_by_gap_threshold(non_voice_segments, options.min_gap_to_merge as u64)
        }
        crate::config::MergeStrategy::Sparse => {
            merge_sparse_segments(non_voice_segments, options.min_gap_to_merge as u64)
        }
    };

    TimelineOutput {
        file: timeline.file.clone(),
        analysis_sample_rate: timeline.analysis_sample_rate,
        frame_ms: timeline.frame_ms,
        segments: merged,
    }
}

fn merge_all(segments: Vec<&Segment>) -> Vec<Segment> {
    if segments.is_empty() {
        return vec![];
    }

    let first_start = segments.first().map(|s| s.start_ms).unwrap_or(0);
    let last_end = segments.last().map(|s| s.end_ms).unwrap_or(0);

    let avg_confidence: f32 =
        segments.iter().map(|s| s.confidence).sum::<f32>() / segments.len() as f32;

    let all_tags: Vec<String> = segments.iter().flat_map(|s| s.tags.clone()).collect();

    vec![Segment {
        start_ms: first_start,
        end_ms: last_end,
        kind: SegmentKind::NonVoice,
        confidence: avg_confidence,
        tags: all_tags,
        prompt: None,
    }]
}

fn merge_by_gap_threshold(segments: Vec<&Segment>, min_gap_ms: u64) -> Vec<Segment> {
    let Some(first) = segments.first() else {
        return vec![];
    };

    let mut merged = Vec::new();
    let mut current_start = first.start_ms;
    let mut current_end = first.end_ms;
    let mut current_confidence = first.confidence;
    let mut current_tags: Vec<String> = first.tags.clone();

    for segment in segments.iter().skip(1) {
        let gap = segment.start_ms - current_end;
        if gap >= min_gap_ms {
            merged.push(Segment {
                start_ms: current_start,
                end_ms: current_end,
                kind: SegmentKind::NonVoice,
                confidence: current_confidence,
                tags: std::mem::take(&mut current_tags),
                prompt: None,
            });
            current_start = segment.start_ms;
            current_confidence = segment.confidence;
            current_tags = segment.tags.clone();
        }
        current_end = segment.end_ms;
    }

    merged.push(Segment {
        start_ms: current_start,
        end_ms: current_end,
        kind: SegmentKind::NonVoice,
        confidence: current_confidence,
        tags: current_tags,
        prompt: None,
    });

    merged
}

fn merge_sparse_segments(segments: Vec<&Segment>, min_gap_ms: u64) -> Vec<Segment> {
    let Some(first) = segments.first() else {
        return vec![];
    };

    let mut merged = Vec::new();
    let mut current_start = first.start_ms;
    let mut current_end = first.end_ms;
    let mut current_confidence = first.confidence;
    let mut current_tags: Vec<String> = first.tags.clone();

    for segment in segments.iter().skip(1) {
        let gap = segment.start_ms - current_end;
        if gap >= min_gap_ms {
            merged.push(Segment {
                start_ms: current_start,
                end_ms: current_end,
                kind: SegmentKind::NonVoice,
                confidence: current_confidence,
                tags: std::mem::take(&mut current_tags),
                prompt: None,
            });
            current_start = segment.start_ms;
            current_confidence = segment.confidence;
            current_tags = segment.tags.clone();
        } else {
            current_confidence = (current_confidence + segment.confidence) / 2.0;
            current_tags.extend(segment.tags.clone());
        }
        current_end = segment.end_ms;
    }

    merged.push(Segment {
        start_ms: current_start,
        end_ms: current_end,
        kind: SegmentKind::NonVoice,
        confidence: current_confidence,
        tags: current_tags,
        prompt: None,
    });

    merged
}

/// Load merge options from an optional config file and CLI overrides.
pub fn load_merge_options(
    config_path: Option<&std::path::Path>,
    min_gap_override: Option<u32>,
    strategy_override: Option<String>,
) -> Result<MergeOptions> {
    let cfg = if let Some(path) = config_path {
        let data = std::fs::read_to_string(path).context("failed to read config file")?;
        let analysis_cfg: crate::config::AnalysisConfig =
            serde_json::from_str(&data).context("failed to parse config file")?;
        analysis_cfg.merge_options.unwrap_or_default()
    } else {
        MergeOptions::default()
    };

    let mut opts = cfg;
    if let Some(min_gap) = min_gap_override {
        opts.min_gap_to_merge = min_gap;
    }
    if let Some(strategy) = strategy_override {
        opts.merge_strategy = match strategy.as_str() {
            "all" => MergeStrategy::All,
            "longest" => MergeStrategy::Longest,
            "sparse" => MergeStrategy::Sparse,
            _ => bail!("invalid merge_strategy: must be one of all, longest, sparse"),
        };
    }

    if opts.min_gap_to_merge == 0 {
        bail!("invalid min_gap_to_merge: must be > 0");
    }
    Ok(opts)
}
