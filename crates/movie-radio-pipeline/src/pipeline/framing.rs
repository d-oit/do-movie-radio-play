use crate::pipeline::features::{compute_features_parallel, feature_set_to_frame};
use movie_radio_types::frame::Frame;

/// Incremental framing that keeps the spectral analyser state between calls,
/// so frames match whole-file sequential framing however the input is split.
/// The streaming extractor relies on this to keep spectral flux continuous
/// across chunk boundaries.
pub struct FrameStreamer {
    sample_rate: u32,
    frame_len: usize,
    extractor: crate::pipeline::features::FeatureExtractor,
    prev_mags: Vec<f32>,
    has_prev: bool,
    parked: Vec<f32>,
}

impl FrameStreamer {
    pub fn new(sample_rate: u32, frame_ms: u32) -> Self {
        let frame_len = (sample_rate as usize * frame_ms as usize) / 1000;
        let fft_len = frame_len.next_power_of_two().max(256);
        Self {
            sample_rate,
            frame_len,
            extractor: crate::pipeline::features::FeatureExtractor::new(fft_len),
            prev_mags: vec![0.0f32; fft_len / 2],
            has_prev: false,
            parked: Vec::new(),
        }
    }

    pub fn frame_len(&self) -> usize {
        self.frame_len
    }

    /// Appends samples and returns the frames completed by them. A trailing
    /// partial frame is held until [`FrameStreamer::finish`].
    pub fn push(&mut self, samples: &[f32]) -> Vec<Frame> {
        if self.frame_len == 0 {
            return Vec::new();
        }
        let mut pending = std::mem::take(&mut self.parked);
        pending.extend_from_slice(samples);
        let usable = pending.len() - pending.len() % self.frame_len;
        if usable == 0 {
            self.parked = pending;
            return Vec::new();
        }
        let frames = self.frames_from(&pending[..usable]);
        self.parked = pending.split_off(usable);
        frames
    }

    /// Flushes the held remainder as the final (partial) frame, matching how
    /// whole-file framing consumes the last short chunk.
    pub fn finish(&mut self) -> Vec<Frame> {
        if self.parked.is_empty() || self.frame_len == 0 {
            return Vec::new();
        }
        let tail = std::mem::take(&mut self.parked);
        self.frames_from(&tail)
    }

    fn frames_from(&mut self, samples: &[f32]) -> Vec<Frame> {
        let mut out = Vec::with_capacity(samples.len() / self.frame_len + 1);
        for chunk in samples.chunks(self.frame_len) {
            if chunk.is_empty() {
                continue;
            }
            let prev = if self.has_prev {
                Some(&self.prev_mags[..])
            } else {
                None
            };
            let (features, mags) = self.extractor.extract_frame(chunk, self.sample_rate, prev);
            let copy_len = mags.len().min(self.prev_mags.len());
            self.prev_mags[..copy_len].copy_from_slice(&mags[..copy_len]);
            self.has_prev = true;
            out.push(feature_set_to_frame(features));
        }
        out
    }
}

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
    let mut prev_mags = vec![0.0f32; fft_len / 2];
    let mut has_prev = false;

    for chunk in samples.chunks(frame_len) {
        if chunk.is_empty() {
            continue;
        }
        let prev = if has_prev { Some(&prev_mags[..]) } else { None };
        let (features, mags) = extractor.extract_frame(chunk, sample_rate, prev);
        let copy_len = mags.len().min(prev_mags.len());
        prev_mags[..copy_len].copy_from_slice(&mags[..copy_len]);
        has_prev = true;

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

    fn noisy_signal(len: usize) -> Vec<f32> {
        let mut state: u32 = 99;
        (0..len)
            .map(|i| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let noise = (state >> 8) as f32 / u32::MAX as f32 - 0.5;
                let t = i as f32 / 16_000.0;
                0.2 * (2.0 * std::f32::consts::PI * 440.0 * t).sin() + 0.05 * noise
            })
            .collect()
    }

    fn assert_frames_equal(left: &[Frame], right: &[Frame]) {
        assert_eq!(left.len(), right.len(), "frame count differs");
        for (idx, (a, b)) in left.iter().zip(right).enumerate() {
            let same = a.rms == b.rms
                && a.zcr == b.zcr
                && a.spectral_flux == b.spectral_flux
                && a.spectral_flatness == b.spectral_flatness
                && a.spectral_entropy == b.spectral_entropy
                && a.centroid_hz == b.centroid_hz
                && a.low_band_ratio == b.low_band_ratio
                && a.high_band_ratio == b.high_band_ratio;
            assert!(same, "frame {idx} differs: {a:?} vs {b:?}");
        }
    }

    #[test]
    fn parallel_matches_sequential_framing() {
        let samples = noisy_signal(16_000 * 3 + 137);
        let sequential = build_frames(&samples, 16_000, 20, false);
        let parallel = build_frames(&samples, 16_000, 20, true);
        assert_frames_equal(&sequential, &parallel);
    }

    #[test]
    fn streaming_framing_matches_whole_file_across_uneven_chunks() {
        let samples = noisy_signal(16_000 * 3 + 137);
        let whole = build_frames(&samples, 16_000, 20, false);

        let mut streamer = FrameStreamer::new(16_000, 20);
        let mut streamed = Vec::new();
        let mut offset = 0;
        for chunk_len in [1_007usize, 5_000, 11_111, 31_007] {
            let end = (offset + chunk_len).min(samples.len());
            streamed.extend(streamer.push(&samples[offset..end]));
            offset = end;
        }
        streamed.extend(streamer.push(&samples[offset..]));
        streamed.extend(streamer.finish());

        assert_frames_equal(&whole, &streamed);
    }
}
