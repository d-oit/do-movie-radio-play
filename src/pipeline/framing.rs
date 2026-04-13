use crate::pipeline::features::SpectralAnalyzer;
use crate::types::frame::Frame;

pub fn build_frames(samples: &[f32], sample_rate: u32, frame_ms: u32) -> Vec<Frame> {
    let frame_len = (sample_rate as usize * frame_ms as usize) / 1000;
    if frame_len == 0 {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(samples.len() / frame_len + 1);
    let fft_len = frame_len.next_power_of_two().max(256);
    let analyzer = SpectralAnalyzer::new(fft_len);
    let bin_width = sample_rate as f32 / fft_len as f32;
    let half_bins = fft_len / 2;
    let low_bin = (300.0 / bin_width).round() as usize;
    let high_bin = (2000.0 / bin_width).round() as usize;
    let low_bin = low_bin.min(half_bins);
    let high_bin = high_bin.min(half_bins);
    let mut prev_mags: Option<Vec<f32>> = None;
    for chunk in samples.chunks(frame_len) {
        if chunk.is_empty() {
            continue;
        }
        let sum_sq = chunk.iter().map(|v| v * v).sum::<f32>();
        let rms = (sum_sq / chunk.len() as f32).sqrt();

        let mut zero_crosses = 0u32;
        for w in chunk.windows(2) {
            if (w[0] >= 0.0) != (w[1] >= 0.0) {
                zero_crosses += 1;
            }
        }
        let zcr = zero_crosses as f32 / chunk.len().max(1) as f32;

        let mags = analyzer.analyze(chunk);
        let mut weighted = 0.0f32;
        let mut mag_sum = 0.0f32;
        let mut low = 0.0f32;
        let mut high = 0.0f32;
        for (i, &m) in mags.iter().enumerate() {
            let freq = i as f32 * bin_width;
            weighted += freq * m;
            mag_sum += m;
            if i <= low_bin {
                low += m;
            }
            if i >= high_bin {
                high += m;
            }
        }
        let centroid_hz = if mag_sum > 0.0 {
            weighted / mag_sum
        } else {
            0.0
        };
        let low_band_ratio = if mag_sum > 0.0 { low / mag_sum } else { 0.0 };
        let high_band_ratio = if mag_sum > 0.0 { high / mag_sum } else { 0.0 };
        let spectral_flux = if let Some(prev) = &prev_mags {
            mags.iter()
                .zip(prev.iter())
                .map(|(m, p)| (m - p).max(0.0))
                .sum::<f32>()
                / mags.len().max(1) as f32
        } else {
            0.0
        };
        prev_mags = Some(mags);

        out.push(Frame {
            rms,
            zcr,
            spectral_flux,
            centroid_hz,
            low_band_ratio,
            high_band_ratio,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_count_is_stable() {
        let s = vec![0.0f32; 3200];
        let frames = build_frames(&s, 16000, 20);
        assert_eq!(frames.len(), 10);
    }
}
