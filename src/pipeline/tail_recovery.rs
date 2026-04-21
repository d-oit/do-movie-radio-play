use crate::types::Segment;

const TAIL_NEAR_END_MS: u64 = 5_000;
const MAX_LEFT_EXTENSION_MS: u64 = 70_000;
const STRONG_SPEECH_LIKELIHOOD: f32 = 0.82;
const STRONG_SPEECH_STOP_FRAMES: usize = 40;

pub fn extend_terminal_non_voice_segment(
    segments: &[Segment],
    frame_likelihoods: &[f32],
    frame_ms: u32,
    total_ms: u64,
) -> Vec<Segment> {
    if segments.len() != 1 || frame_likelihoods.is_empty() {
        return segments.to_vec();
    }
    let segment = &segments[0];
    if total_ms.saturating_sub(segment.end_ms) > TAIL_NEAR_END_MS {
        return segments.to_vec();
    }

    let frame_ms = frame_ms.max(1) as u64;
    let max_extend_frames = (MAX_LEFT_EXTENSION_MS / frame_ms) as usize;
    let mut start_idx = (segment.start_ms / frame_ms) as usize;
    let mut extended = 0usize;
    let mut strong_run = 0usize;

    while start_idx > 0 && extended < max_extend_frames {
        let idx = start_idx - 1;
        if frame_likelihoods.get(idx).copied().unwrap_or(1.0) > STRONG_SPEECH_LIKELIHOOD {
            strong_run += 1;
            if strong_run >= STRONG_SPEECH_STOP_FRAMES {
                break;
            }
        } else {
            strong_run = 0;
        }
        start_idx = idx;
        extended += 1;
    }

    let mut updated = segment.clone();
    updated.start_ms = start_idx as u64 * frame_ms;
    vec![updated]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SegmentKind;

    #[test]
    fn extends_single_terminal_segment_left() {
        let segments = vec![Segment {
            start_ms: 100_000,
            end_ms: 120_000,
            kind: SegmentKind::NonVoice,
            confidence: 0.8,
            tags: vec![],
            prompt: None,
        }];
        let likelihoods = vec![0.4; 6000];
        let out = extend_terminal_non_voice_segment(&segments, &likelihoods, 20, 120_000);
        assert!(out[0].start_ms < 100_000);
        assert_eq!(out[0].end_ms, 120_000);
    }
}
