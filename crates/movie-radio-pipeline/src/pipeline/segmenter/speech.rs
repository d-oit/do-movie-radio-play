use movie_radio_types::{Segment, SegmentKind};

use super::confidence::slice_confidence;

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
