use crate::pipeline::features::{compute_features_parallel, feature_set_to_frame};
use movie_radio_types::frame::Frame;

pub fn build_frames(
    samples: &[f32],
    sample_rate: u32,
    frame_ms: u32,
    parallel: bool,
) -> Vec<Frame> {
    let frame_len = (sample_rate as usize * frame_ms as usize) / 1000;
    if frame_len == 0 {
        return Vec::new();
    }

    if parallel {
        let chunks: Vec<&[f32]> = samples.chunks(frame_len).collect();
        return compute_features_parallel(&chunks, sample_rate)
            .into_iter()
            .map(feature_set_to_frame)
            .collect();
    }

    let mut out = Vec::with_capacity(samples.len() / frame_len + 1);
    let fft_len = frame_len.next_power_of_two().max(256);
    let mut extractor = crate::pipeline::features::FeatureExtractor::new(fft_len);
    let mut prev_mags: Option<Vec<f32>> = None;

    for chunk in samples.chunks(frame_len) {
        if chunk.is_empty() {
            continue;
        }
        let (features, mags) = extractor.extract_frame(chunk, sample_rate, prev_mags.as_deref());
        if let Some(ref mut prev) = prev_mags {
            prev.copy_from_slice(mags);
        } else {
            prev_mags = Some(mags.to_vec());
        }

        out.push(feature_set_to_frame(features));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_count_is_stable() {
        let s = vec![0.0f32; 3200];
        let frames = build_frames(&s, 16000, 20, false);
        assert_eq!(frames.len(), 10);
    }

    #[test]
    fn frame_count_parallel_is_stable() {
        let s = vec![0.0f32; 3200];
        let frames = build_frames(&s, 16000, 20, true);
        assert_eq!(frames.len(), 10);
    }
}
