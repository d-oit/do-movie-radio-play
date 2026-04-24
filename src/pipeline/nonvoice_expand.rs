use crate::types::Segment;

const AMBIGUOUS_LOW: f32 = 0.35;
const AMBIGUOUS_HIGH: f32 = 0.65;
const AMBIGUOUS_SPEECH_GUARD: f32 = 0.52;

pub fn expand_non_voice_segments_into_ambiguous(
    segments: &[Segment],
    frame_likelihoods: &[f32],
    frame_ms: u32,
    max_expand_ms: Option<u64>,
) -> Vec<Segment> {
    if segments.is_empty() || frame_likelihoods.is_empty() {
        return segments.to_vec();
    }

    let frame_ms = frame_ms.max(1) as u64;
    let max_expand_frames =
        max_expand_ms.map(|ms| ((ms + frame_ms.saturating_sub(1)) / frame_ms).max(1) as usize);
    let mut expanded = Vec::with_capacity(segments.len());

    for segment in segments {
        let mut updated = segment.clone();

        let mut start_idx = (updated.start_ms / frame_ms) as usize;
        let mut back_steps = 0usize;
        while start_idx > 0 {
            if let Some(max) = max_expand_frames {
                if back_steps >= max {
                    break;
                }
            }
            let idx = start_idx - 1;
            let likelihood = frame_likelihoods[idx];
            if !is_ambiguous(likelihood) || likelihood >= AMBIGUOUS_SPEECH_GUARD {
                break;
            }
            start_idx = idx;
            back_steps += 1;
        }

        let mut end_idx = (updated.end_ms.div_ceil(frame_ms)) as usize;
        let mut forward_steps = 0usize;
        while end_idx < frame_likelihoods.len() {
            if let Some(max) = max_expand_frames {
                if forward_steps >= max {
                    break;
                }
            }
            let likelihood = frame_likelihoods[end_idx];
            if !is_ambiguous(likelihood) || likelihood >= AMBIGUOUS_SPEECH_GUARD {
                break;
            }
            end_idx += 1;
            forward_steps += 1;
        }

        updated.start_ms = start_idx as u64 * frame_ms;
        updated.end_ms = end_idx as u64 * frame_ms;
        expanded.push(updated);
    }

    expanded
}

fn is_ambiguous(likelihood: f32) -> bool {
    (AMBIGUOUS_LOW..AMBIGUOUS_HIGH).contains(&likelihood)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SegmentKind;

    #[test]
    fn expands_into_adjacent_ambiguous_frames() {
        let segments = vec![Segment {
            start_ms: 40,
            end_ms: 80,
            kind: SegmentKind::NonVoice,
            confidence: 0.8,
            tags: vec![],
            prompt: None,
        }];
        let likelihoods = vec![0.2, 0.4, 0.1, 0.5, 0.8];

        let expanded = expand_non_voice_segments_into_ambiguous(&segments, &likelihoods, 20, None);
        assert_eq!(expanded[0].start_ms, 20);
        assert_eq!(expanded[0].end_ms, 80);
    }

    #[test]
    fn does_not_expand_into_speech_leaning_ambiguous_frames() {
        let segments = vec![Segment {
            start_ms: 40,
            end_ms: 80,
            kind: SegmentKind::NonVoice,
            confidence: 0.8,
            tags: vec![],
            prompt: None,
        }];
        let likelihoods = vec![0.2, 0.6, 0.1, 0.58, 0.8];

        let expanded = expand_non_voice_segments_into_ambiguous(&segments, &likelihoods, 20, None);
        assert_eq!(expanded[0].start_ms, 40);
        assert_eq!(expanded[0].end_ms, 80);
    }
}
