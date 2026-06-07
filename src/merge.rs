use crate::config::{MergeOptions, MergeStrategy};
use crate::types::{Segment, SegmentKind, TimelineOutput};
use std::collections::HashSet;

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
        MergeStrategy::All => merge_all(non_voice_segments),
        MergeStrategy::Longest => {
            merge_by_gap_threshold(non_voice_segments, options.min_gap_to_merge as u64)
        }
        MergeStrategy::Sparse => {
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
