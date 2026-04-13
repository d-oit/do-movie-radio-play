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
                    segs.push(Segment {
                        start_ms: s,
                        end_ms: end,
                        kind: SegmentKind::Speech,
                        confidence: 0.8,
                        tags: vec![],
                        prompt: None,
                    });
                }
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        let end = smoothed.len() as u64 * frame_ms as u64;
        if end - s >= min_speech_ms as u64 {
            segs.push(Segment {
                start_ms: s,
                end_ms: end,
                kind: SegmentKind::Speech,
                confidence: 0.8,
                tags: vec![],
                prompt: None,
            });
        }
    }
    segs
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
    fn inversion_works() {
        let speech = vec![Segment {
            start_ms: 1000,
            end_ms: 2000,
            kind: SegmentKind::Speech,
            confidence: 1.0,
            tags: vec![],
            prompt: None,
        }];
        let inv = invert_to_non_voice(&speech, 4000, 500);
        assert_eq!(inv.len(), 2);
        assert_eq!(inv[0].start_ms, 0);
    }
}
