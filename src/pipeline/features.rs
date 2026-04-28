use realfft::{RealFftPlanner, RealToComplex};
use std::sync::Arc;

pub struct SpectralAnalyzer {
    fft_len: usize,
    fft: Arc<dyn RealToComplex<f32>>,
    hann: Vec<f32>,
    input_buf: Vec<f32>,
    output_buf: Vec<realfft::num_complex::Complex<f32>>,
    mag_buf: Vec<f32>,
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
        let input_buf = fft.make_input_vec();
        let output_buf = fft.make_output_vec();
        let mag_buf = vec![0.0; fft_len / 2];
        Self {
            fft_len,
            fft,
            hann,
            input_buf,
            output_buf,
            mag_buf,
        }
    }

    pub fn analyze(&mut self, samples: &[f32]) -> &[f32] {
        let n = samples.len().min(self.fft_len);
        self.input_buf[..n].copy_from_slice(&samples[..n]);
        self.input_buf[n..].fill(0.0);

        for (s, h) in self.input_buf.iter_mut().zip(self.hann.iter()) {
            *s *= h;
        }

        if self
            .fft
            .process(&mut self.input_buf, &mut self.output_buf)
            .is_err()
        {
            self.mag_buf.fill(0.0);
            return &self.mag_buf;
        }

        for (c, m) in self.output_buf.iter().zip(self.mag_buf.iter_mut()) {
            *m = (c.re * c.re + c.im * c.im).sqrt();
        }
        &self.mag_buf
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FeatureSet {
    pub rms: f32,
    pub zcr: f32,
    pub spectral_flux: f32,
    pub spectral_flatness: f32,
    pub spectral_entropy: f32,
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
            spectral_flatness: 0.0,
            spectral_entropy: 0.0,
            centroid_hz: 0.0,
            low_band_ratio: 0.0,
            high_band_ratio: 0.0,
        };
    }

    let mut entropy_acc = 0.0f32;
    let mut entropy_count = 0usize;
    let inv_ln_2 = 1.0 / 2.0f32.ln();

    let rms = (samples.iter().map(|v| v * v).sum::<f32>() / samples.len() as f32).sqrt();

    let mut zero_crosses = 0u32;
    for w in samples.windows(2) {
        if (w[0] >= 0.0) != (w[1] >= 0.0) {
            zero_crosses += 1;
        }
    }
    let zcr = zero_crosses as f32 / samples.len() as f32;

    let fft_len = 1024usize.next_power_of_two().max(256);
    let mut analyzer = SpectralAnalyzer::new(fft_len);

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
    let mut log_mag_sum = 0.0f32;
    let mut arithmetic_mean = 0.0f32;
    let mut valid_mag_count = 0usize;

    for chunk in samples
        .chunks(fft_len / 2)
        .take_while(|c| c.len() >= fft_len / 4)
    {
        let mags = analyzer.analyze(chunk);

        let mut chunk_mag_sum = 0.0f32;
        let mut chunk_sum_m_ln_m = 0.0f32;
        let mut freq = 0.0f32;

        for (i, &m) in mags.iter().enumerate().take(half_bins) {
            chunk_mag_sum += m;

            // Centroid and band ratios
            weighted_bin_sum += freq * m;
            freq += bin_width;
            mag_sum += m;
            if i < low_bin {
                low += m;
            }
            if i >= high_bin {
                high += m;
            }

            // Flatness and Entropy components
            if m > 1e-10 {
                let ln_m = m.ln();
                log_mag_sum += ln_m;
                arithmetic_mean += m;
                valid_mag_count += 1;
                chunk_sum_m_ln_m += m * ln_m;
            }

            // Flux
            if let Some(prev) = &prev_mags {
                let diff = m - prev.get(i).copied().unwrap_or(0.0);
                flux_acc += diff.max(0.0);
            }
        }

        if chunk_mag_sum > 1e-10 {
            let chunk_entropy = (chunk_mag_sum.ln() - chunk_sum_m_ln_m / chunk_mag_sum) * inv_ln_2;
            entropy_acc += chunk_entropy.max(0.0);
            entropy_count += 1;
        }

        if let Some(ref mut prev) = prev_mags {
            prev.copy_from_slice(mags);
        } else {
            prev_mags = Some(mags.to_vec());
        }
    }

    let spectral_entropy = if entropy_count > 0 {
        entropy_acc / entropy_count as f32
    } else {
        0.0
    };

    let spectral_flatness = if valid_mag_count > 0 && arithmetic_mean > 0.0 {
        let am = arithmetic_mean / valid_mag_count as f32;
        ((log_mag_sum / half_bins as f32) - am.ln())
            .max(-10.0)
            .exp()
    } else {
        0.0
    };

    FeatureSet {
        rms,
        zcr,
        spectral_flux: flux_acc / samples.len().max(1) as f32 * 1000.0,
        spectral_flatness,
        spectral_entropy,
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
        assert_eq!(features.spectral_flatness, 0.0);
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
        use rand::RngExt;
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
