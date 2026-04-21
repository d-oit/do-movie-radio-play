use crate::types::{frame::Frame, Segment, SegmentKind};

const MAX_REVIEW_SPEECH_MS: u64 = 12000;
const MIN_CONFIDENCE_TO_KEEP: f32 = 0.9;

pub fn filter_implausible_speech_segments(
    segments: &[Segment],
    frames: &[Frame],
    frame_ms: u32,
) -> Vec<Segment> {
    segments
        .iter()
        .filter(|segment| {
            if segment.kind != SegmentKind::Speech {
                return true;
            }
            let duration_ms = segment.end_ms.saturating_sub(segment.start_ms);
            if segment.confidence >= MIN_CONFIDENCE_TO_KEEP || duration_ms > MAX_REVIEW_SPEECH_MS {
                return true;
            }
            let stats = average_frame_stats(frames, frame_ms, segment.start_ms, segment.end_ms);
            !looks_implausible_for_speech(&stats)
        })
        .cloned()
        .collect()
}

#[derive(Default)]
struct AvgStats {
    zcr: f32,
    flatness: f32,
    entropy: f32,
    centroid_hz: f32,
    low_band_ratio: f32,
    high_band_ratio: f32,
}

fn average_frame_stats(frames: &[Frame], frame_ms: u32, start_ms: u64, end_ms: u64) -> AvgStats {
    if frames.is_empty() || end_ms <= start_ms {
        return AvgStats::default();
    }
    let frame_ms = frame_ms.max(1) as u64;
    let start_idx = (start_ms / frame_ms) as usize;
    let end_idx = (end_ms.div_ceil(frame_ms) as usize)
        .min(frames.len())
        .max(start_idx + 1);
    let slice = &frames[start_idx.min(frames.len().saturating_sub(1))..end_idx];
    let n = slice.len().max(1) as f32;

    AvgStats {
        zcr: slice.iter().map(|f| f.zcr).sum::<f32>() / n,
        flatness: slice.iter().map(|f| f.spectral_flatness).sum::<f32>() / n,
        entropy: slice.iter().map(|f| f.spectral_entropy).sum::<f32>() / n,
        centroid_hz: slice.iter().map(|f| f.centroid_hz).sum::<f32>() / n,
        low_band_ratio: slice.iter().map(|f| f.low_band_ratio).sum::<f32>() / n,
        high_band_ratio: slice.iter().map(|f| f.high_band_ratio).sum::<f32>() / n,
    }
}

fn looks_implausible_for_speech(stats: &AvgStats) -> bool {
    let music_like = stats.low_band_ratio > 0.45 && stats.high_band_ratio < 0.15;
    let noisy = stats.flatness > 0.42 || stats.entropy > 6.2;
    let centroid_outside = stats.centroid_hz < 120.0 || stats.centroid_hz > 5000.0;
    let zcr_outside = stats.zcr < 0.025 || stats.zcr > 0.38;
    (music_like && zcr_outside) || (noisy && centroid_outside) || (noisy && zcr_outside)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn speech_segment(start_ms: u64, end_ms: u64, confidence: f32) -> Segment {
        Segment {
            start_ms,
            end_ms,
            kind: SegmentKind::Speech,
            confidence,
            tags: vec![],
            prompt: None,
        }
    }

    fn frame(
        zcr: f32,
        flatness: f32,
        entropy: f32,
        centroid_hz: f32,
        low: f32,
        high: f32,
    ) -> Frame {
        Frame {
            rms: 0.01,
            zcr,
            spectral_flux: 0.01,
            spectral_flatness: flatness,
            spectral_entropy: entropy,
            centroid_hz,
            low_band_ratio: low,
            high_band_ratio: high,
        }
    }

    #[test]
    fn filters_low_confidence_non_speech_like_segments() {
        let segments = vec![speech_segment(0, 400, 0.6), speech_segment(1000, 2000, 0.8)];
        let frames = vec![
            frame(0.01, 0.5, 6.8, 80.0, 0.55, 0.05),
            frame(0.01, 0.5, 6.8, 80.0, 0.55, 0.05),
            frame(0.12, 0.2, 4.0, 1800.0, 0.2, 0.2),
            frame(0.12, 0.2, 4.0, 1800.0, 0.2, 0.2),
        ];
        let kept = filter_implausible_speech_segments(&segments, &frames, 500);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].start_ms, 1000);
    }
}
