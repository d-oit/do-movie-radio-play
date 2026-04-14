use realfft::{RealFftPlanner, RealToComplex};
use std::sync::Arc;

pub struct SpectralAnalyzer {
    fft_len: usize,
    fft: Arc<dyn RealToComplex<f32>>,
    hann: Vec<f32>,
}

impl SpectralAnalyzer {
    pub fn new(fft_len: usize) -> Self {
        let mut planner = RealFftPlanner::new();
        let fft = planner.plan_fft_forward(fft_len);
        let hann: Vec<f32> = (0..fft_len)
            .map(|i| {
                0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / (fft_len - 1) as f32).cos())
            })
            .collect();
        Self { fft_len, fft, hann }
    }

    pub fn analyze(&self, samples: &[f32]) -> Vec<f32> {
        let mut input: Vec<f32> = samples.iter().take(self.fft_len).cloned().collect();
        input.resize(self.fft_len, 0.0);

        for (s, h) in input.iter_mut().zip(self.hann.iter()) {
            *s *= h;
        }

        let mut spectrum = self.fft.make_output_vec();
        let bin_count = self.fft_len / 2;
        if self.fft.process(&mut input, &mut spectrum).is_err() {
            return vec![0.0; bin_count];
        }

        let mut magnitudes = Vec::with_capacity(bin_count);
        for c in spectrum.iter().take(bin_count) {
            magnitudes.push((c.re * c.re + c.im * c.im).sqrt());
        }
        magnitudes
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FeatureSet {
    pub rms: f32,
    pub zcr: f32,
    pub spectral_flux: f32,
    pub centroid_hz: f32,
    pub low_band_ratio: f32,
    pub high_band_ratio: f32,
}

pub fn compute_features(samples: &[f32], sample_rate: u32) -> FeatureSet {
    if samples.is_empty() {
        return FeatureSet {
            rms: 0.0,
            zcr: 0.0,
            spectral_flux: 0.0,
            centroid_hz: 0.0,
            low_band_ratio: 0.0,
            high_band_ratio: 0.0,
        };
    }

    let rms = (samples.iter().map(|v| v * v).sum::<f32>() / samples.len() as f32).sqrt();

    let mut zero_crosses = 0u32;
    for w in samples.windows(2) {
        if (w[0] >= 0.0) != (w[1] >= 0.0) {
            zero_crosses += 1;
        }
    }
    let zcr = zero_crosses as f32 / samples.len() as f32;

    let fft_len = 1024usize.next_power_of_two().max(256);
    let analyzer = SpectralAnalyzer::new(fft_len);

    let bin_width = sample_rate as f32 / fft_len as f32;
    let low_bin = (300.0 / bin_width) as usize;
    let high_bin = (2000.0 / bin_width) as usize;
    let half_bins = fft_len / 2;

    let mut flux_acc = 0.0;
    let mut weighted_bin_sum = 0.0;
    let mut mag_sum = 0.0;
    let mut low = 0.0;
    let mut high = 0.0;
    let mut prev_mags: Option<Vec<f32>> = None;

    for chunk in samples
        .chunks(fft_len / 2)
        .take_while(|c| c.len() >= fft_len / 4)
    {
        let mags = analyzer.analyze(chunk);

        if let Some(prev) = prev_mags {
            for (i, &m) in mags.iter().enumerate().take(half_bins) {
                let diff = m - prev.get(i).copied().unwrap_or(0.0);
                flux_acc += diff.max(0.0);
            }
        }

        for (i, &m) in mags.iter().enumerate().take(half_bins) {
            let freq = i as f32 * bin_width;
            weighted_bin_sum += freq * m;
            mag_sum += m;
            if i < low_bin {
                low += m;
            }
            if i >= high_bin {
                high += m;
            }
        }

        prev_mags = Some(mags);
    }

    FeatureSet {
        rms,
        zcr,
        spectral_flux: flux_acc / samples.len().max(1) as f32 * 1000.0,
        centroid_hz: if mag_sum > 0.0 {
            weighted_bin_sum / mag_sum
        } else {
            0.0
        },
        low_band_ratio: if mag_sum > 0.0 { low / mag_sum } else { 0.0 },
        high_band_ratio: if mag_sum > 0.0 { high / mag_sum } else { 0.0 },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_returns_zeros() {
        let features = compute_features(&[], 16000);
        assert_eq!(features.rms, 0.0);
        assert_eq!(features.zcr, 0.0);
        assert_eq!(features.spectral_flux, 0.0);
        assert_eq!(features.centroid_hz, 0.0);
    }

    #[test]
    fn dc_signal_has_low_centroid() {
        let samples = vec![0.5f32; 1024];
        let features = compute_features(&samples, 16000);
        assert!(
            features.centroid_hz >= 0.0,
            "DC should have non-negative centroid, got {}",
            features.centroid_hz
        );
        assert!(
            features.high_band_ratio < features.low_band_ratio,
            "DC should have more low-band energy, low={} high={}",
            features.low_band_ratio,
            features.high_band_ratio
        );
    }

    #[test]
    fn noise_has_high_flux() {
        use rand::rngs::StdRng;
        use rand::Rng;
        use rand::SeedableRng;
        let mut rng = StdRng::seed_from_u64(42);
        let samples: Vec<f32> = (0..2048).map(|_| rng.random::<f32>() * 2.0 - 1.0).collect();
        let features = compute_features(&samples, 16000);
        assert!(
            features.spectral_flux > 0.0,
            "Noise should produce spectral flux"
        );
    }
}
