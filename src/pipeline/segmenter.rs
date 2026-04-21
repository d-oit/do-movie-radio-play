use crate::config::MergeOptions;
use crate::types::{Segment, SegmentKind};

const SHORT_SPEECH_CONFIDENCE_KEEP_FLOOR: f32 = 0.78;
const POST_FILTER_BRIDGE_MAX_GAP_MS: u64 = 2_500;
pub fn smooth_speech(raw: &[bool], frame_ms: u32, hangover_ms: u32) -> Vec<bool> {
    let mut out = raw.to_vec();
    let hang = (hangover_ms / frame_ms) as usize;
    let mut last_speech: Option<usize> = None;
    for (i, &b) in raw.iter().enumerate() {
        if b {
            last_speech = Some(i);
            continue;
        }
        if let Some(last) = last_speech {
            if i.saturating_sub(last) <= hang {
                out[i] = true;
            }
        }
    }

    // remove isolated one-frame flicker
    for i in 1..out.len().saturating_sub(1) {
        if out[i] && !out[i - 1] && !out[i + 1] {
            out[i] = false;
        }
    }
    out
}

pub fn speech_segments(
    smoothed: &[bool],
    frame_ms: u32,
    min_speech_ms: u32,
    frame_likelihoods: &[f32],
) -> Vec<Segment> {
    let mut segs = Vec::new();
    let mut start: Option<usize> = None;
    for (i, &v) in smoothed.iter().enumerate() {
        match (start, v) {
            (None, true) => start = Some(i),
            (Some(s), false) => {
                let end = i;
                let duration_ms = (end.saturating_sub(s) as u64) * frame_ms as u64;
                if duration_ms >= min_speech_ms as u64 {
                    segs.push(speech_seg(s, end, frame_ms, frame_likelihoods));
                }
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        let end = smoothed.len();
        let duration_ms = (end.saturating_sub(s) as u64) * frame_ms as u64;
        if duration_ms >= min_speech_ms as u64 {
            segs.push(speech_seg(s, end, frame_ms, frame_likelihoods));
        }
    }
    segs
}

pub fn merge_close_segments(segments: &[Segment], merge_gap_ms: u32) -> Vec<Segment> {
    if segments.is_empty() {
        return Vec::new();
    }
    let mut merged = vec![segments[0].clone()];
    for seg in segments.iter().skip(1) {
        if let Some(last) = merged.last_mut() {
            if seg.start_ms <= last.end_ms + merge_gap_ms as u64 && last.kind == seg.kind {
                last.end_ms = last.end_ms.max(seg.end_ms);
                last.confidence = last.confidence.min(seg.confidence);
            } else {
                merged.push(seg.clone());
            }
        }
    }
    merged
}

pub fn prune_short_speech_segments(segments: &[Segment], min_speech_ms: u32) -> Vec<Segment> {
    segments
        .iter()
        .filter(|seg| {
            if seg.kind != SegmentKind::Speech {
                return true;
            }
            let duration_ms = seg.end_ms.saturating_sub(seg.start_ms);
            duration_ms >= min_speech_ms as u64
                || seg.confidence >= SHORT_SPEECH_CONFIDENCE_KEEP_FLOOR
        })
        .cloned()
        .collect()
}

pub fn invert_to_non_voice(
    speech: &[Segment],
    total_ms: u64,
    min_non_voice_ms: u32,
    frame_ms: u32,
    frame_likelihoods: &[f32],
) -> Vec<Segment> {
    let mut out = Vec::new();
    let mut cursor = 0u64;
    for s in speech {
        if s.start_ms > cursor {
            push_nv(
                &mut out,
                cursor,
                s.start_ms,
                min_non_voice_ms,
                frame_ms,
                frame_likelihoods,
            );
        }
        cursor = s.end_ms;
    }
    if cursor < total_ms {
        push_nv(
            &mut out,
            cursor,
            total_ms,
            min_non_voice_ms,
            frame_ms,
            frame_likelihoods,
        );
    }
    out
}

pub fn bridge_non_voice_segments(segments: &[Segment], max_speech_bridge_ms: u32) -> Vec<Segment> {
    if segments.is_empty() || max_speech_bridge_ms == 0 {
        return segments.to_vec();
    }
    let mut merged = Vec::with_capacity(segments.len());
    merged.push(segments[0].clone());

    for seg in segments.iter().skip(1) {
        if let Some(last) = merged.last_mut() {
            let gap_ms = seg.start_ms.saturating_sub(last.end_ms);
            if gap_ms <= max_speech_bridge_ms as u64 {
                last.end_ms = seg.end_ms;
                last.confidence = last.confidence.min(seg.confidence);
                continue;
            }
        }
        merged.push(seg.clone());
    }
    merged
}

pub fn apply_non_voice_merge_policy(segments: &[Segment], options: &MergeOptions) -> Vec<Segment> {
    if segments.is_empty() {
        return Vec::new();
    }

    match options.merge_strategy.as_str() {
        "all" => merge_all_non_voice(segments),
        "longest" => merge_by_gap_threshold(
            segments,
            options.min_gap_to_merge.max(options.min_silence_duration) as u64,
        ),
        "sparse" => merge_sparse_non_voice(
            segments,
            options.min_gap_to_merge.max(options.min_silence_duration) as u64,
        ),
        _ => segments.to_vec(),
    }
}

pub fn bridge_residual_non_voice_gaps(segments: &[Segment]) -> Vec<Segment> {
    if segments.is_empty() {
        return Vec::new();
    }

    let mut merged = Vec::with_capacity(segments.len());
    merged.push(segments[0].clone());

    for segment in segments.iter().skip(1) {
        if let Some(last) = merged.last_mut() {
            let gap_ms = segment.start_ms.saturating_sub(last.end_ms);
            if gap_ms <= POST_FILTER_BRIDGE_MAX_GAP_MS {
                last.end_ms = segment.end_ms;
                last.confidence = last.confidence.min(segment.confidence);
                continue;
            }
        }
        merged.push(segment.clone());
    }

    merged
}

pub fn split_long_segments(
    segments: Vec<Segment>,
    max_non_voice_ms: u32,
    min_non_voice_ms: u32,
    frame_ms: u32,
    frame_likelihoods: &[f32],
) -> Vec<Segment> {
    let mut out = Vec::new();
    for seg in segments {
        let duration_ms = seg.end_ms.saturating_sub(seg.start_ms);
        if duration_ms <= max_non_voice_ms as u64 {
            out.push(seg);
        } else {
            let mut cursor = seg.start_ms;
            while cursor < seg.end_ms {
                let remaining = seg.end_ms - cursor;
                if remaining <= max_non_voice_ms as u64 {
                    push_nv(
                        &mut out,
                        cursor,
                        seg.end_ms,
                        min_non_voice_ms,
                        frame_ms,
                        frame_likelihoods,
                    );
                    break;
                } else {
                    let split_end = cursor + max_non_voice_ms as u64;
                    push_nv(
                        &mut out,
                        cursor,
                        split_end,
                        min_non_voice_ms,
                        frame_ms,
                        frame_likelihoods,
                    );
                    cursor = split_end;
                }
            }
        }
    }
    out
}

fn speech_seg(
    start_idx: usize,
    end_idx: usize,
    frame_ms: u32,
    frame_likelihoods: &[f32],
) -> Segment {
    let frame_ms_u64 = frame_ms.max(1) as u64;
    let start_ms = start_idx as u64 * frame_ms_u64;
    let end_ms = end_idx as u64 * frame_ms_u64;
    let confidence = slice_confidence(frame_likelihoods, start_idx, end_idx, false);
    Segment {
        start_ms,
        end_ms,
        kind: SegmentKind::Speech,
        confidence,
        tags: vec![],
        prompt: None,
    }
}

fn push_nv(
    out: &mut Vec<Segment>,
    start: u64,
    end: u64,
    min_ms: u32,
    frame_ms: u32,
    frame_likelihoods: &[f32],
) {
    if end.saturating_sub(start) >= min_ms as u64 {
        let confidence = confidence_for_range(frame_likelihoods, frame_ms, start, end, true);
        out.push(Segment {
            start_ms: start,
            end_ms: end,
            kind: SegmentKind::NonVoice,
            confidence,
            tags: vec![],
            prompt: None,
        });
    }
}

fn merge_all_non_voice(segments: &[Segment]) -> Vec<Segment> {
    if segments.is_empty() {
        return Vec::new();
    }

    let first_start = segments.first().map(|s| s.start_ms).unwrap_or(0);
    let last_end = segments.last().map(|s| s.end_ms).unwrap_or(0);
    let avg_confidence = segments.iter().map(|s| s.confidence).sum::<f32>() / segments.len() as f32;
    let all_tags = segments.iter().flat_map(|s| s.tags.clone()).collect();

    vec![Segment {
        start_ms: first_start,
        end_ms: last_end,
        kind: SegmentKind::NonVoice,
        confidence: avg_confidence,
        tags: all_tags,
        prompt: None,
    }]
}

fn merge_by_gap_threshold(segments: &[Segment], min_gap_ms: u64) -> Vec<Segment> {
    if segments.is_empty() {
        return Vec::new();
    }

    let mut merged = Vec::new();
    let mut current = segments[0].clone();

    for segment in segments.iter().skip(1) {
        let gap = segment.start_ms.saturating_sub(current.end_ms);
        if gap >= min_gap_ms {
            merged.push(current);
            current = segment.clone();
        } else {
            current.end_ms = segment.end_ms;
        }
    }

    merged.push(current);
    merged
}

fn merge_sparse_non_voice(segments: &[Segment], min_gap_ms: u64) -> Vec<Segment> {
    if segments.is_empty() {
        return Vec::new();
    }

    let mut merged = Vec::new();
    let mut current = segments[0].clone();

    for segment in segments.iter().skip(1) {
        let gap = segment.start_ms.saturating_sub(current.end_ms);
        if gap >= min_gap_ms {
            merged.push(current);
            current = segment.clone();
        } else {
            current.end_ms = segment.end_ms;
            current.confidence = (current.confidence + segment.confidence) / 2.0;
            current.tags.extend(segment.tags.clone());
        }
    }

    merged.push(current);
    merged
}

fn slice_confidence(
    frame_likelihoods: &[f32],
    start_idx: usize,
    end_idx: usize,
    invert: bool,
) -> f32 {
    if frame_likelihoods.is_empty() || end_idx <= start_idx {
        return 0.5;
    }
    let end_idx = end_idx.min(frame_likelihoods.len());
    if end_idx <= start_idx {
        return 0.5;
    }
    let slice = &frame_likelihoods[start_idx..end_idx];
    if slice.is_empty() {
        return 0.5;
    }
    let avg = slice.iter().copied().sum::<f32>() / slice.len() as f32;
    let base_score = if invert { 1.0 - avg } else { avg };
    let frame_count = slice.len();
    let duration_adjustment = duration_confidence_adjustment(frame_count);
    let adjusted = base_score * duration_adjustment;
    adjusted.clamp(0.0, 1.0)
}

fn duration_confidence_adjustment(frame_count: usize) -> f32 {
    const MIN_FRAMES_FOR_FULL_CONFIDENCE: usize = 50;
    const MIN_FRAMES_FOR_REDUCED: usize = 10;
    if frame_count >= MIN_FRAMES_FOR_FULL_CONFIDENCE {
        1.0
    } else if frame_count >= MIN_FRAMES_FOR_REDUCED {
        0.85 + (0.15 * (frame_count - MIN_FRAMES_FOR_REDUCED) as f32
            / (MIN_FRAMES_FOR_FULL_CONFIDENCE - MIN_FRAMES_FOR_REDUCED) as f32)
    } else {
        0.85 * (frame_count as f32 / MIN_FRAMES_FOR_REDUCED as f32)
    }
}

fn confidence_for_range(
    frame_likelihoods: &[f32],
    frame_ms: u32,
    start_ms: u64,
    end_ms: u64,
    invert: bool,
) -> f32 {
    if frame_likelihoods.is_empty() || end_ms <= start_ms {
        return 0.5;
    }
    let frame_ms = frame_ms.max(1) as u64;
    let len = frame_likelihoods.len();
    let mut start_idx = (start_ms / frame_ms) as usize;
    if start_idx >= len {
        start_idx = len.saturating_sub(1);
    }
    let mut end_idx = end_ms.div_ceil(frame_ms) as usize;
    end_idx = end_idx.clamp(start_idx + 1, len);
    slice_confidence(frame_likelihoods, start_idx, end_idx, invert)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_speech(start_ms: u64, end_ms: u64) -> Segment {
        Segment {
            start_ms,
            end_ms,
            kind: SegmentKind::Speech,
            confidence: 0.8,
            tags: vec![],
            prompt: None,
        }
    }

    #[test]
    fn merge_gap_works() {
        let speech = vec![fake_speech(0, 200), fake_speech(300, 500)];
        let merged = merge_close_segments(&speech, 120);
        assert_eq!(merged.len(), 1);
    }

    #[test]
    fn prune_short_low_confidence_speech_segments() {
        let mut short_low = fake_speech(100, 260);
        short_low.confidence = 0.55;
        let mut short_high = fake_speech(300, 450);
        short_high.confidence = 0.9;
        let long = fake_speech(600, 1200);

        let pruned = prune_short_speech_segments(&[short_low, short_high, long], 300);
        assert_eq!(pruned.len(), 2);
        assert_eq!(pruned[0].start_ms, 300);
        assert_eq!(pruned[1].start_ms, 600);
    }

    #[test]
    fn bridges_non_voice_across_short_speech_gap() {
        let nv = vec![
            Segment {
                start_ms: 0,
                end_ms: 2000,
                kind: SegmentKind::NonVoice,
                confidence: 0.8,
                tags: vec![],
                prompt: None,
            },
            Segment {
                start_ms: 2150,
                end_ms: 4000,
                kind: SegmentKind::NonVoice,
                confidence: 0.7,
                tags: vec![],
                prompt: None,
            },
        ];

        let bridged = bridge_non_voice_segments(&nv, 200);
        assert_eq!(bridged.len(), 1);
        assert_eq!(bridged[0].start_ms, 0);
        assert_eq!(bridged[0].end_ms, 4000);
    }

    #[test]
    fn sparse_merge_policy_merges_close_non_voice_segments() {
        let opts = MergeOptions {
            min_gap_to_merge: 400,
            merge_strategy: "sparse".to_string(),
            min_speech_duration: 500,
            min_silence_duration: 300,
            silence_threshold_db: -42,
        };
        let segments = vec![
            Segment {
                start_ms: 0,
                end_ms: 1000,
                kind: SegmentKind::NonVoice,
                confidence: 0.8,
                tags: vec![],
                prompt: None,
            },
            Segment {
                start_ms: 1200,
                end_ms: 2000,
                kind: SegmentKind::NonVoice,
                confidence: 0.6,
                tags: vec![],
                prompt: None,
            },
        ];

        let merged = apply_non_voice_merge_policy(&segments, &opts);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].start_ms, 0);
        assert_eq!(merged[0].end_ms, 2000);
    }

    #[test]
    fn residual_gap_bridge_merges_tiny_final_gap() {
        let segments = vec![
            Segment {
                start_ms: 1000,
                end_ms: 2000,
                kind: SegmentKind::NonVoice,
                confidence: 0.8,
                tags: vec![],
                prompt: None,
            },
            Segment {
                start_ms: 3500,
                end_ms: 5000,
                kind: SegmentKind::NonVoice,
                confidence: 0.7,
                tags: vec![],
                prompt: None,
            },
        ];

        let bridged = bridge_residual_non_voice_gaps(&segments);
        assert_eq!(bridged.len(), 1);
        assert_eq!(bridged[0].start_ms, 1000);
        assert_eq!(bridged[0].end_ms, 5000);
    }
}
