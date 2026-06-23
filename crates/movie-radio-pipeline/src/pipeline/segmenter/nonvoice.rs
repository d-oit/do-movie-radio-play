use movie_radio_types::{Segment, SegmentKind};

use super::confidence::confidence_for_range;

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

pub fn bridge_residual_non_voice_gaps(segments: &[Segment], max_gap_ms: u64) -> Vec<Segment> {
    if segments.is_empty() {
        return Vec::new();
    }
    if max_gap_ms == 0 {
        return segments.to_vec();
    }

    let mut merged = Vec::with_capacity(segments.len());
    merged.push(segments[0].clone());

    for segment in segments.iter().skip(1) {
        if let Some(last) = merged.last_mut() {
            let gap_ms = segment.start_ms.saturating_sub(last.end_ms);
            if gap_ms <= max_gap_ms {
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
