use crate::types::{Segment, SegmentKind};

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

pub fn speech_segments(smoothed: &[bool], frame_ms: u32, min_speech_ms: u32) -> Vec<Segment> {
    let mut segs = Vec::new();
    let mut start = None;
    for (i, &v) in smoothed.iter().enumerate() {
        match (start, v) {
            (None, true) => start = Some(i as u64 * frame_ms as u64),
            (Some(s), false) => {
                let end = i as u64 * frame_ms as u64;
                if end - s >= min_speech_ms as u64 {
                    segs.push(speech_seg(s, end));
                }
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        let end = smoothed.len() as u64 * frame_ms as u64;
        if end - s >= min_speech_ms as u64 {
            segs.push(speech_seg(s, end));
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

pub fn invert_to_non_voice(
    speech: &[Segment],
    total_ms: u64,
    min_non_voice_ms: u32,
) -> Vec<Segment> {
    let mut out = Vec::new();
    let mut cursor = 0u64;
    for s in speech {
        if s.start_ms > cursor {
            push_nv(&mut out, cursor, s.start_ms, min_non_voice_ms);
        }
        cursor = s.end_ms;
    }
    if cursor < total_ms {
        push_nv(&mut out, cursor, total_ms, min_non_voice_ms);
    }
    out
}

fn speech_seg(start_ms: u64, end_ms: u64) -> Segment {
    Segment {
        start_ms,
        end_ms,
        kind: SegmentKind::Speech,
        confidence: 0.8,
        tags: vec![],
        prompt: None,
    }
}

fn push_nv(out: &mut Vec<Segment>, start: u64, end: u64, min_ms: u32) {
    if end.saturating_sub(start) >= min_ms as u64 {
        out.push(Segment {
            start_ms: start,
            end_ms: end,
            kind: SegmentKind::NonVoice,
            confidence: 0.9,
            tags: vec![],
            prompt: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_gap_works() {
        let speech = vec![speech_seg(0, 200), speech_seg(300, 500)];
        let merged = merge_close_segments(&speech, 120);
        assert_eq!(merged.len(), 1);
    }
}
