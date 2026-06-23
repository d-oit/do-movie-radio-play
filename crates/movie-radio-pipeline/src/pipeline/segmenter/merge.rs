use movie_radio_types::{MergeOptions, MergeStrategy, Segment};

pub fn apply_non_voice_merge_policy(segments: &[Segment], options: &MergeOptions) -> Vec<Segment> {
    if segments.is_empty() {
        return Vec::new();
    }

    let merge_gap_ms = options.min_gap_to_merge.max(options.min_silence_duration) as u64;

    match options.merge_strategy {
        MergeStrategy::All | MergeStrategy::Longest => {
            merge_by_gap_threshold(segments, merge_gap_ms)
        }
        MergeStrategy::Sparse => merge_sparse_non_voice(segments, merge_gap_ms),
    }
}

fn merge_by_gap_threshold(segments: &[Segment], min_gap_ms: u64) -> Vec<Segment> {
    if segments.is_empty() {
        return Vec::new();
    }

    let mut merged = Vec::new();
    let mut current = segments[0].clone();

    for segment in segments.iter().skip(1) {
        let gap = segment.start_ms.saturating_sub(current.end_ms);
        if gap >= min_gap_ms {
            merged.push(current);
            current = segment.clone();
        } else {
            current.end_ms = segment.end_ms;
        }
    }

    merged.push(current);
    merged
}

fn merge_sparse_non_voice(segments: &[Segment], min_gap_ms: u64) -> Vec<Segment> {
    if segments.is_empty() {
        return Vec::new();
    }

    let mut merged = Vec::new();
    let mut current = segments[0].clone();

    for segment in segments.iter().skip(1) {
        let gap = segment.start_ms.saturating_sub(current.end_ms);
        if gap >= min_gap_ms {
            merged.push(current);
            current = segment.clone();
        } else {
            current.end_ms = segment.end_ms;
            current.confidence = (current.confidence + segment.confidence) / 2.0;
            current.tags.extend(segment.tags.clone());
        }
    }

    merged.push(current);
    merged
}
