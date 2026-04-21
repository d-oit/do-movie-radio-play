use crate::types::Segment;

const TAIL_NEAR_END_MS: u64 = 5_000;
const MIN_LEFT_EXTENSION_MS: u64 = 60_000;
const MAX_AGGRESSIVE_MIN_NON_VOICE_MS: u32 = 1_000;
const MAX_LEFT_EXTENSION_MS: u64 = 180_000;
const STRONG_SPEECH_LIKELIHOOD: f32 = 0.82;
const STRONG_SPEECH_STOP_FRAMES: usize = 60;
const ROLLING_WINDOW_FRAMES: usize = 120;
const ROLLING_SPEECH_AVG_STOP: f32 = 0.78;

pub fn extend_terminal_non_voice_segment(
    segments: &[Segment],
    frame_likelihoods: &[f32],
    frame_ms: u32,
    total_ms: u64,
    min_non_voice_ms: u32,
) -> Vec<Segment> {
    if segments.len() != 1 || frame_likelihoods.is_empty() {
        return segments.to_vec();
    }
    let segment = &segments[0];
    if total_ms.saturating_sub(segment.end_ms) > TAIL_NEAR_END_MS {
        return segments.to_vec();
    }

    let frame_ms = frame_ms.max(1) as u64;
    let min_left_extension_ms = if min_non_voice_ms <= MAX_AGGRESSIVE_MIN_NON_VOICE_MS {
        MIN_LEFT_EXTENSION_MS
    } else {
        0
    };
    let min_extend_frames = (min_left_extension_ms / frame_ms) as usize;
    let max_extend_frames = (MAX_LEFT_EXTENSION_MS / frame_ms) as usize;
    let mut start_idx = (segment.start_ms / frame_ms) as usize;
    let mut extended = 0usize;
    let mut strong_run = 0usize;
    let mut rolling_count = 0usize;
    let mut rolling_sum = 0.0f32;
    let mut rolling = vec![0.0f32; ROLLING_WINDOW_FRAMES];
    let mut rolling_pos = 0usize;

    while start_idx > 0 && extended < max_extend_frames {
        let idx = start_idx - 1;
        let likelihood = frame_likelihoods.get(idx).copied().unwrap_or(1.0);
        if likelihood > STRONG_SPEECH_LIKELIHOOD {
            strong_run += 1;
            if extended >= min_extend_frames && strong_run >= STRONG_SPEECH_STOP_FRAMES {
                break;
            }
        } else {
            strong_run = 0;
        }

        if rolling_count < ROLLING_WINDOW_FRAMES {
            rolling_count += 1;
        } else {
            rolling_sum -= rolling[rolling_pos];
        }
        rolling[rolling_pos] = likelihood;
        rolling_sum += likelihood;
        rolling_pos = (rolling_pos + 1) % ROLLING_WINDOW_FRAMES;

        if extended >= min_extend_frames
            && rolling_count == ROLLING_WINDOW_FRAMES
            && rolling_sum / ROLLING_WINDOW_FRAMES as f32 >= ROLLING_SPEECH_AVG_STOP
        {
            break;
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
        let out = extend_terminal_non_voice_segment(&segments, &likelihoods, 20, 120_000, 800);
        assert!(out[0].start_ms < 100_000);
        assert_eq!(out[0].end_ms, 120_000);
    }

    #[test]
    fn enforces_minimum_left_extension_before_stopping() {
        let segments = vec![Segment {
            start_ms: 100_000,
            end_ms: 120_000,
            kind: SegmentKind::NonVoice,
            confidence: 0.8,
            tags: vec![],
            prompt: None,
        }];
        let likelihoods = vec![0.95; 7000];
        let out = extend_terminal_non_voice_segment(&segments, &likelihoods, 20, 120_000, 800);
        assert_eq!(out[0].start_ms, 40_000);
    }

    #[test]
    fn skips_minimum_extension_floor_for_large_min_non_voice() {
        let segments = vec![Segment {
            start_ms: 100_000,
            end_ms: 120_000,
            kind: SegmentKind::NonVoice,
            confidence: 0.8,
            tags: vec![],
            prompt: None,
        }];
        let likelihoods = vec![0.95; 7000];
        let out = extend_terminal_non_voice_segment(&segments, &likelihoods, 20, 120_000, 10_000);
        assert!(out[0].start_ms < 100_000);
        assert!(out[0].start_ms >= 98_000);
    }
}
