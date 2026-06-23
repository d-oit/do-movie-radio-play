use movie_radio_types::{Segment, SegmentKind};

const SHORT_SPEECH_CONFIDENCE_KEEP_FLOOR: f32 = 0.78;

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

pub(crate) fn speech_seg(
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

pub(crate) fn slice_confidence(
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

pub(crate) fn duration_confidence_adjustment(frame_count: usize) -> f32 {
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

pub(crate) fn confidence_for_range(
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
