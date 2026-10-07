use movie_radio_types::{Segment, SegmentKind};

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
    score_segments_with_total(pred, truth, tolerance_ms, None)
}

/// Extraction timelines hold only non-voice segments, which made every speech
/// metric a vacuous 0/0 = 1.0. With the film length known and no speech
/// segments on either side, speech is the complement of non-voice over
/// `[0, total_ms)`. `overlap_ratio` and `boundary_error_ms` are unchanged.
pub fn score_segments_with_total(
    pred: &[Segment],
    truth: &[Segment],
    tolerance_ms: u64,
    total_ms: Option<u64>,
) -> CompareMetrics {
    let mut metrics = score_all(pred, truth, tolerance_ms);
    let has_speech = |s: &[Segment]| s.iter().any(|x| x.kind == SegmentKind::Speech);
    if let Some(total) = total_ms.filter(|_| !has_speech(pred) && !has_speech(truth)) {
        let pred_speech = complement_speech(pred, total);
        let truth_speech = complement_speech(truth, total);
        let (precision, recall) = precision_recall(
            &pred_speech,
            &truth_speech,
            SegmentKind::Speech,
            tolerance_ms,
        );
        let duration = duration_metrics(&pred_speech, &truth_speech, &SegmentKind::Speech);
        metrics.speech_precision = precision;
        metrics.speech_recall = recall;
        metrics.speech_time_precision = duration.precision;
        metrics.speech_time_recall = duration.recall;
        metrics.speech_overlap_ms = duration.overlap_ms;
        metrics.speech_predicted_ms = duration.predicted_ms;
        metrics.speech_expected_ms = duration.expected_ms;
    }
    metrics
}

fn complement_speech(segments: &[Segment], total_ms: u64) -> Vec<Segment> {
    let non_voice = merged_intervals_for_kind(segments, &SegmentKind::NonVoice);
    let mut out = Vec::new();
    let mut cursor = 0u64;
    for (start, end) in non_voice {
        let start = start.min(total_ms);
        if start > cursor {
            out.push(speech_between(cursor, start));
        }
        cursor = cursor.max(end.min(total_ms));
    }
    if cursor < total_ms {
        out.push(speech_between(cursor, total_ms));
    }
    out
}

fn speech_between(start_ms: u64, end_ms: u64) -> Segment {
    Segment {
        start_ms,
        end_ms,
        kind: SegmentKind::Speech,
        confidence: 1.0,
        tags: vec![],
        prompt: None,
        sfx_trigger: None,
    }
}

fn score_all(pred: &[Segment], truth: &[Segment], tolerance_ms: u64) -> CompareMetrics {
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
            sfx_trigger: None,
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

    #[test]
    fn speech_is_the_complement_of_non_voice_when_total_is_known() {
        // Truth: speech 0-2000 and 6000-10000. Prediction: speech 0-3000 and 7000-10000.
        let truth = vec![seg(2_000, 6_000, SegmentKind::NonVoice)];
        let pred = vec![seg(3_000, 7_000, SegmentKind::NonVoice)];
        let vacuous = score_segments(&pred, &truth, 100);
        assert_eq!(vacuous.speech_time_precision, 1.0);
        assert_eq!(vacuous.speech_expected_ms, 0);

        let m = score_segments_with_total(&pred, &truth, 100, Some(10_000));
        assert_eq!(m.speech_expected_ms, 6_000);
        assert_eq!(m.speech_predicted_ms, 6_000);
        assert_eq!(m.speech_overlap_ms, 5_000);
        assert!((m.speech_time_precision - 5.0 / 6.0).abs() < 1e-6);
        assert!((m.speech_time_recall - 5.0 / 6.0).abs() < 1e-6);
        // Non-voice figures and overlap_ratio are untouched.
        assert_eq!(m.overlap_ratio, vacuous.overlap_ratio);
        assert_eq!(m.non_voice_time_recall, vacuous.non_voice_time_recall);
    }

    #[test]
    fn everything_is_a_gap_is_no_longer_a_perfect_speech_score() {
        let truth = vec![seg(2_000, 6_000, SegmentKind::NonVoice)];
        let pred = vec![seg(0, 10_000, SegmentKind::NonVoice)];
        let m = score_segments_with_total(&pred, &truth, 100, Some(10_000));
        assert_eq!(m.speech_predicted_ms, 0);
        assert_eq!(m.speech_time_recall, 0.0);
    }

    #[test]
    fn explicit_speech_segments_disable_the_complement() {
        let both = vec![
            seg(0, 1_000, SegmentKind::Speech),
            seg(1_000, 3_000, SegmentKind::NonVoice),
        ];
        let a = score_segments_with_total(&both, &both, 100, Some(9_000));
        assert_eq!(a.speech_expected_ms, 1_000);
    }
}
