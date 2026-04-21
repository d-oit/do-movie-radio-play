use crate::types::Segment;

const AMBIGUOUS_LOW: f32 = 0.35;
const AMBIGUOUS_HIGH: f32 = 0.65;

pub fn expand_non_voice_segments_into_ambiguous(
    segments: &[Segment],
    frame_likelihoods: &[f32],
    frame_ms: u32,
) -> Vec<Segment> {
    if segments.is_empty() || frame_likelihoods.is_empty() {
        return segments.to_vec();
    }

    let frame_ms = frame_ms.max(1) as u64;
    let mut expanded = Vec::with_capacity(segments.len());

    for segment in segments {
        let mut updated = segment.clone();

        let mut start_idx = (updated.start_ms / frame_ms) as usize;
        while start_idx > 0 {
            let idx = start_idx - 1;
            if !is_ambiguous(frame_likelihoods[idx]) {
                break;
            }
            start_idx = idx;
        }

        let mut end_idx = (updated.end_ms.div_ceil(frame_ms)) as usize;
        while end_idx < frame_likelihoods.len() {
            if !is_ambiguous(frame_likelihoods[end_idx]) {
                break;
            }
            end_idx += 1;
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

        let expanded = expand_non_voice_segments_into_ambiguous(&segments, &likelihoods, 20);
        assert_eq!(expanded[0].start_ms, 20);
        assert_eq!(expanded[0].end_ms, 80);
    }
}
