use crate::types::{Segment, SegmentKind};

#[derive(Debug, Clone, Copy)]
pub struct CompareMetrics {
    pub overlap_ratio: f32,
    pub boundary_error_ms: f32,
    pub speech_precision: f32,
    pub speech_recall: f32,
    pub non_voice_precision: f32,
    pub non_voice_recall: f32,
    pub speech_time_precision: f32,
    pub speech_time_recall: f32,
    pub non_voice_time_precision: f32,
    pub non_voice_time_recall: f32,
    pub speech_overlap_ms: u64,
    pub speech_predicted_ms: u64,
    pub speech_expected_ms: u64,
    pub non_voice_overlap_ms: u64,
    pub non_voice_predicted_ms: u64,
    pub non_voice_expected_ms: u64,
}

struct DurationMetrics {
    precision: f32,
    recall: f32,
    overlap_ms: u64,
    predicted_ms: u64,
    expected_ms: u64,
}

pub fn score_segments(pred: &[Segment], truth: &[Segment], tolerance_ms: u64) -> CompareMetrics {
    let overlap_ratio = overlap_ratio(pred, truth);
    let boundary_error_ms = boundary_error(pred, truth);
    let (speech_precision, speech_recall) =
        precision_recall(pred, truth, SegmentKind::Speech, tolerance_ms);
    let (non_voice_precision, non_voice_recall) =
        precision_recall(pred, truth, SegmentKind::NonVoice, tolerance_ms);
    let speech_duration = duration_metrics(pred, truth, &SegmentKind::Speech);
    let non_voice_duration = duration_metrics(pred, truth, &SegmentKind::NonVoice);
    CompareMetrics {
        overlap_ratio,
        boundary_error_ms,
        speech_precision,
        speech_recall,
        non_voice_precision,
        non_voice_recall,
        speech_time_precision: speech_duration.precision,
        speech_time_recall: speech_duration.recall,
        non_voice_time_precision: non_voice_duration.precision,
        non_voice_time_recall: non_voice_duration.recall,
        speech_overlap_ms: speech_duration.overlap_ms,
        speech_predicted_ms: speech_duration.predicted_ms,
        speech_expected_ms: speech_duration.expected_ms,
        non_voice_overlap_ms: non_voice_duration.overlap_ms,
        non_voice_predicted_ms: non_voice_duration.predicted_ms,
        non_voice_expected_ms: non_voice_duration.expected_ms,
    }
}

fn duration_metrics(pred: &[Segment], truth: &[Segment], kind: &SegmentKind) -> DurationMetrics {
    let pred_intervals = merged_intervals_for_kind(pred, kind);
    let truth_intervals = merged_intervals_for_kind(truth, kind);
    let pred_total = total_duration(&pred_intervals);
    let truth_total = total_duration(&truth_intervals);
    if pred_total == 0 && truth_total == 0 {
        return DurationMetrics {
            precision: 1.0,
            recall: 1.0,
            overlap_ms: 0,
            predicted_ms: 0,
            expected_ms: 0,
        };
    }

    let intersection = interval_intersection_duration(&pred_intervals, &truth_intervals);
    let precision = if pred_total == 0 {
        1.0
    } else {
        intersection as f32 / pred_total as f32
    };
    let recall = if truth_total == 0 {
        1.0
    } else {
        intersection as f32 / truth_total as f32
    };
    DurationMetrics {
        precision,
        recall,
        overlap_ms: intersection,
        predicted_ms: pred_total,
        expected_ms: truth_total,
    }
}

fn precision_recall(
    pred: &[Segment],
    truth: &[Segment],
    kind: SegmentKind,
    tol: u64,
) -> (f32, f32) {
    let pred_k: Vec<_> = pred.iter().filter(|s| s.kind == kind).collect();
    let truth_k: Vec<_> = truth.iter().filter(|s| s.kind == kind).collect();
    if pred_k.is_empty() && truth_k.is_empty() {
        return (1.0, 1.0);
    }
    let mut matched_truth = vec![false; truth_k.len()];
    let mut tp = 0u32;
    for p in &pred_k {
        if let Some(idx) = truth_k.iter().enumerate().find_map(|(i, t)| {
            if matched_truth[i] {
                return None;
            }
            let start_ok = p.start_ms.abs_diff(t.start_ms) <= tol;
            let end_ok = p.end_ms.abs_diff(t.end_ms) <= tol;
            if start_ok && end_ok {
                Some(i)
            } else {
                None
            }
        }) {
            matched_truth[idx] = true;
            tp += 1;
        }
    }
    let precision = if pred_k.is_empty() {
        1.0
    } else {
        tp as f32 / pred_k.len() as f32
    };
    let recall = if truth_k.is_empty() {
        1.0
    } else {
        tp as f32 / truth_k.len() as f32
    };
    (precision, recall)
}

fn overlap_ratio(pred: &[Segment], truth: &[Segment]) -> f32 {
    let p = merged_intervals(pred.iter().map(|s| (s.start_ms, s.end_ms)).collect());
    let t = merged_intervals(truth.iter().map(|s| (s.start_ms, s.end_ms)).collect());
    let intersection = interval_intersection_duration(&p, &t);
    let pred_total = total_duration(&p);
    let truth_total = total_duration(&t);
    let union = pred_total + truth_total;
    if union == 0 {
        1.0
    } else {
        (2 * intersection) as f32 / union as f32
    }
}

fn boundary_error(pred: &[Segment], truth: &[Segment]) -> f32 {
    if pred.is_empty() || truth.is_empty() {
        return 0.0;
    }
    let mut total = 0u64;
    let mut count = 0u64;
    for p in pred {
        if let Some(t) = truth.iter().min_by_key(|t| p.start_ms.abs_diff(t.start_ms)) {
            total += p.start_ms.abs_diff(t.start_ms) + p.end_ms.abs_diff(t.end_ms);
            count += 2;
        }
    }
    if count == 0 {
        0.0
    } else {
        total as f32 / count as f32
    }
}

fn overlap_ms(a_start: u64, a_end: u64, b_start: u64, b_end: u64) -> u64 {
    let start = a_start.max(b_start);
    let end = a_end.min(b_end);
    end.saturating_sub(start)
}

fn merged_intervals_for_kind(segments: &[Segment], kind: &SegmentKind) -> Vec<(u64, u64)> {
    let intervals: Vec<(u64, u64)> = segments
        .iter()
        .filter(|s| &s.kind == kind)
        .map(|s| (s.start_ms, s.end_ms))
        .collect();
    merged_intervals(intervals)
}

fn merged_intervals(mut intervals: Vec<(u64, u64)>) -> Vec<(u64, u64)> {
    if intervals.is_empty() {
        return intervals;
    }
    intervals.sort_by_key(|(start, _)| *start);
    let mut merged: Vec<(u64, u64)> = Vec::with_capacity(intervals.len());
    for (start, end) in intervals {
        if end <= start {
            continue;
        }
        match merged.last_mut() {
            Some((_, prev_end)) if start <= *prev_end => {
                *prev_end = (*prev_end).max(end);
            }
            _ => merged.push((start, end)),
        }
    }
    merged
}

fn total_duration(intervals: &[(u64, u64)]) -> u64 {
    intervals
        .iter()
        .map(|(start, end)| end.saturating_sub(*start))
        .sum()
}

fn interval_intersection_duration(a: &[(u64, u64)], b: &[(u64, u64)]) -> u64 {
    let mut i = 0usize;
    let mut j = 0usize;
    let mut total = 0u64;
    while i < a.len() && j < b.len() {
        let (a_start, a_end) = a[i];
        let (b_start, b_end) = b[j];
        total += overlap_ms(a_start, a_end, b_start, b_end);
        if a_end <= b_end {
            i += 1;
        } else {
            j += 1;
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(start_ms: u64, end_ms: u64, kind: SegmentKind) -> Segment {
        Segment {
            start_ms,
            end_ms,
            kind,
            confidence: 1.0,
            tags: vec![],
            prompt: None,
        }
    }

    #[test]
    fn metrics_are_stable() {
        let pred = vec![
            seg(0, 1000, SegmentKind::Speech),
            seg(1000, 3000, SegmentKind::NonVoice),
        ];
        let truth = pred.clone();
        let m = score_segments(&pred, &truth, 100);
        assert_eq!(m.speech_precision, 1.0);
        assert_eq!(m.non_voice_recall, 1.0);
        assert_eq!(m.non_voice_time_precision, 1.0);
        assert_eq!(m.non_voice_time_recall, 1.0);
        assert!(m.overlap_ratio >= 0.99);
    }

    #[test]
    fn duration_metrics_handle_many_to_one_matches() {
        let pred = vec![
            seg(0, 1000, SegmentKind::NonVoice),
            seg(1000, 2000, SegmentKind::NonVoice),
            seg(2000, 3000, SegmentKind::NonVoice),
        ];
        let truth = vec![seg(0, 3000, SegmentKind::NonVoice)];

        let m = score_segments(&pred, &truth, 100);
        assert_eq!(m.non_voice_precision, 0.0);
        assert_eq!(m.non_voice_recall, 0.0);
        assert_eq!(m.non_voice_time_precision, 1.0);
        assert_eq!(m.non_voice_time_recall, 1.0);
    }
}
