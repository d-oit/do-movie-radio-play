use crate::types::{Segment, SegmentKind};

#[derive(Debug, Clone, Copy)]
pub struct CompareMetrics {
    pub overlap_ratio: f32,
    pub boundary_error_ms: f32,
    pub speech_precision: f32,
    pub speech_recall: f32,
    pub non_voice_precision: f32,
    pub non_voice_recall: f32,
}

pub fn score_segments(pred: &[Segment], truth: &[Segment], tolerance_ms: u64) -> CompareMetrics {
    let overlap_ratio = overlap_ratio(pred, truth);
    let boundary_error_ms = boundary_error(pred, truth);
    let (speech_precision, speech_recall) =
        precision_recall(pred, truth, SegmentKind::Speech, tolerance_ms);
    let (non_voice_precision, non_voice_recall) =
        precision_recall(pred, truth, SegmentKind::NonVoice, tolerance_ms);
    CompareMetrics {
        overlap_ratio,
        boundary_error_ms,
        speech_precision,
        speech_recall,
        non_voice_precision,
        non_voice_recall,
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
    let p: Vec<_> = pred.iter().map(|s| (s.start_ms, s.end_ms)).collect();
    let t: Vec<_> = truth.iter().map(|s| (s.start_ms, s.end_ms)).collect();
    let intersection: u64 = p
        .iter()
        .flat_map(|(ps, pe)| t.iter().map(move |(ts, te)| overlap_ms(*ps, *pe, *ts, *te)))
        .sum();
    let pred_total: u64 = p.iter().map(|(s, e)| e.saturating_sub(*s)).sum();
    let truth_total: u64 = t.iter().map(|(s, e)| e.saturating_sub(*s)).sum();
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
        assert!(m.overlap_ratio >= 0.99);
    }
}
