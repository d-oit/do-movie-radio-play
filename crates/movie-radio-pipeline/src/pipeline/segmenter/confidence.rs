pub(crate) const SHORT_SPEECH_CONFIDENCE_KEEP_FLOOR: f32 = 0.78;

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
