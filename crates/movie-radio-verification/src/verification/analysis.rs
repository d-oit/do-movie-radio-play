use realfft::num_complex::Complex;
use realfft::{RealFftPlanner, RealToComplex};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

struct AnalysisCache {
    planner: RealFftPlanner<f32>,
    plans: HashMap<usize, Arc<dyn RealToComplex<f32>>>,
    input: Vec<f32>,
    output: Vec<Complex<f32>>,
    spectrum_a: Vec<f32>,
    spectrum_b: Vec<f32>,
    mags: Vec<f32>,
}

impl AnalysisCache {
    fn new() -> Self {
        Self {
            planner: RealFftPlanner::new(),
            plans: HashMap::new(),
            input: Vec::new(),
            output: Vec::new(),
            spectrum_a: Vec::new(),
            spectrum_b: Vec::new(),
            mags: Vec::new(),
        }
    }

    fn get_plan(&mut self, size: usize) -> Arc<dyn RealToComplex<f32>> {
        self.plans
            .entry(size)
            .or_insert_with(|| self.planner.plan_fft_forward(size))
            .clone()
    }

    fn ensure_buffers(&mut self, fft_size: usize) {
        if self.input.len() < fft_size {
            self.input.resize(fft_size, 0.0);
        }
        let output_size = fft_size / 2 + 1;
        if self.output.len() < output_size {
            self.output.resize(output_size, Complex::new(0.0, 0.0));
        }
        if self.mags.len() < output_size {
            self.mags.resize(output_size, 0.0);
        }
    }

    fn ensure_flux_buffers(&mut self, output_size: usize) {
        if self.spectrum_a.len() < output_size {
            self.spectrum_a.resize(output_size, 0.0);
        }
        if self.spectrum_b.len() < output_size {
            self.spectrum_b.resize(output_size, 0.0);
        }
    }
}

thread_local! {
    static CACHE: RefCell<AnalysisCache> = RefCell::new(AnalysisCache::new());
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    Verified,
    Suspicious,
    Rejected,
    Inconclusive,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SpectralFeatures {
    pub rms: f32,
    pub zcr: f32,
    pub spectral_entropy: f32,
    pub spectral_flatness: f32,
    pub spectral_flux: f32,
    pub centroid_hz: f32,
    pub low_band_ratio: f32,
    pub high_band_ratio: f32,
}

#[derive(Debug, Clone)]
pub struct SegmentAnalysis {
    pub status: VerificationStatus,
    pub features: SpectralFeatures,
    pub reason: Option<String>,
}

pub fn analyze_audio_features(samples: &[f32]) -> anyhow::Result<SpectralFeatures> {
    if samples.is_empty() {
        return Err(anyhow::anyhow!("empty audio samples"));
    }

    let (rms, zcr) = compute_rms_and_zcr(samples);
    let (spectral_entropy, spectral_flatness, centroid_hz, low_band_ratio, high_band_ratio) =
        compute_spectral_features(samples)?;
    let spectral_flux = compute_spectral_flux(samples);

    Ok(SpectralFeatures {
        rms,
        zcr,
        spectral_entropy,
        spectral_flatness,
        spectral_flux,
        centroid_hz,
        low_band_ratio,
        high_band_ratio,
    })
}

/// Single-pass computation of RMS and ZCR to minimize buffer iteration overhead.
fn compute_rms_and_zcr(samples: &[f32]) -> (f32, f32) {
    if samples.is_empty() {
        return (0.0, 0.0);
    }
    let mut sum_squares = samples[0] * samples[0];
    let mut crossings = 0usize;
    let mut prev_sign = samples[0] >= 0.0;

    for &s in &samples[1..] {
        sum_squares += s * s;
        let sign = s >= 0.0;
        if sign != prev_sign {
            crossings += 1;
            prev_sign = sign;
        }
    }

    let rms = (sum_squares / samples.len() as f32).sqrt();
    let zcr = if samples.len() > 1 {
        crossings as f32 / (samples.len() - 1) as f32
    } else {
        0.0
    };
    (rms, zcr)
}

/// Fills `mags` with the magnitude spectrum computed from the FFT `output`.
fn fill_magnitudes(output: &[Complex<f32>], mags: &mut [f32]) {
    for (c, m) in output.iter().zip(mags.iter_mut()) {
        *m = (c.re * c.re + c.im * c.im).sqrt();
    }
}

/// Single-pass spectral moments over the magnitude spectrum using a running float index.
fn spectral_stats(mags: &[f32]) -> (f32, f32, f32, f32, usize) {
    let mut weighted_sum = 0.0f32;
    let mut total_mag = 0.0f32;
    let mut log_mag_sum = 0.0f32;
    let mut mag_log_mag_sum = 0.0f32;
    let mut pos_count = 0usize;
    let mut i_f32 = 0.0f32;

    for &mag in mags {
        weighted_sum += i_f32 * mag;
        total_mag += mag;

        if mag > 1e-10 {
            let ln_mag = mag.ln();
            log_mag_sum += ln_mag;
            mag_log_mag_sum += mag * ln_mag;
            pos_count += 1;
        }
        i_f32 += 1.0;
    }

    (
        weighted_sum,
        total_mag,
        log_mag_sum,
        mag_log_mag_sum,
        pos_count,
    )
}

fn compute_spectral_features(samples: &[f32]) -> anyhow::Result<(f32, f32, f32, f32, f32)> {
    let fft_size = next_power_of_2(samples.len().max(512));

    let (entropy, flatness, centroid, low_ratio, high_ratio) = CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let fft = cache.get_plan(fft_size);
        cache.ensure_buffers(fft_size);
        let output_size = fft_size / 2 + 1;

        let cache_ptr = &mut *cache;
        let input = &mut cache_ptr.input;
        let output = &mut cache_ptr.output;

        if samples.len() >= fft_size {
            input[..fft_size].copy_from_slice(&samples[..fft_size]);
        } else {
            input[..samples.len()].copy_from_slice(samples);
            input[samples.len()..fft_size].fill(0.0);
        };

        if fft
            .process(&mut input[..fft_size], &mut output[..output_size])
            .is_err()
        {
            return Err(anyhow::anyhow!("FFT processing failed"));
        }

        let sample_rate = 16000.0f32;
        let bin_width = sample_rate / fft_size as f32;
        let inv_ln_2 = 1.0 / 2.0f32.ln();

        let low_bin_limit = (250.0 / bin_width).floor() as usize;
        let high_bin_limit = (4000.0 / bin_width).ceil() as usize;

        // Populate pre-allocated mags buffer
        let mags = &mut cache_ptr.mags[..output_size];
        fill_magnitudes(&output[..output_size], mags);

        // Slice-based summation to remove inner loop branching entirely
        let low_limit = low_bin_limit.min(output_size);
        let low_mag_sum: f32 = mags[..low_limit].iter().sum();

        let high_start = (high_bin_limit + 1).min(output_size);
        let high_mag_sum: f32 = mags[high_start..output_size].iter().sum();

        let (weighted_sum, total_mag, log_mag_sum, mag_log_mag_sum, pos_count) =
            spectral_stats(mags);

        if total_mag > 0.0 {
            let entropy = ((total_mag.ln() - mag_log_mag_sum / total_mag) * inv_ln_2).max(0.0);
            let flatness = if pos_count > 0 {
                let geometric_mean = (log_mag_sum / pos_count as f32).exp();
                let arithmetic_mean = total_mag / output_size as f32;
                (geometric_mean / arithmetic_mean).min(1.0)
            } else {
                1.0
            };
            let centroid = (weighted_sum * bin_width) / total_mag;
            let low_ratio = low_mag_sum / total_mag;
            let high_ratio = high_mag_sum / total_mag;
            Ok((entropy, flatness, centroid, low_ratio, high_ratio))
        } else {
            Ok((7.0, 1.0, 0.0, 0.0, 0.0))
        }
    })?;

    Ok((entropy, flatness, centroid, low_ratio, high_ratio))
}

fn next_power_of_2(n: usize) -> usize {
    let n = n.saturating_sub(1);
    let shift = usize::BITS - n.leading_zeros();
    1 << shift
}

fn compute_spectral_flux(samples: &[f32]) -> f32 {
    let window_size = 512;
    if samples.len() < window_size {
        return 0.0;
    }

    let hop_size = 256;
    let output_size = window_size / 2 + 1;

    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let fft = cache.get_plan(window_size);
        cache.ensure_buffers(window_size);
        cache.ensure_flux_buffers(output_size);

        let mut flux = 0.0f32;
        let mut count = 0usize;
        let mut has_prev = false;
        let mut current_is_a = true;

        for i in (0..=samples.len().saturating_sub(window_size)).step_by(hop_size) {
            let window = &samples[i..i + window_size];

            let cache_ptr = &mut *cache;
            cache_ptr.input[..window_size].copy_from_slice(window);

            if fft
                .process(
                    &mut cache_ptr.input[..window_size],
                    &mut cache_ptr.output[..output_size],
                )
                .is_err()
            {
                continue;
            }

            // Select active spectrum buffer and previous spectrum buffer
            let (curr_spec, prev_spec) = if current_is_a {
                (
                    &mut cache_ptr.spectrum_a[..output_size],
                    &cache_ptr.spectrum_b[..output_size],
                )
            } else {
                (
                    &mut cache_ptr.spectrum_b[..output_size],
                    &cache_ptr.spectrum_a[..output_size],
                )
            };

            let output_slice = &cache_ptr.output[..output_size];

            if has_prev {
                // Compute magnitude, store in current spectrum buffer, and accumulate positive flux diff against previous frame
                let mut diff_sum = 0.0f32;
                for (c, (m, &p)) in output_slice
                    .iter()
                    .zip(curr_spec.iter_mut().zip(prev_spec.iter()))
                {
                    let mag = (c.re * c.re + c.im * c.im).sqrt();
                    *m = mag;
                    diff_sum += (mag - p).max(0.0);
                }
                flux += diff_sum;
                count += 1;
            } else {
                // First frame: populate magnitude spectrum without computing unneeded flux difference
                fill_magnitudes(output_slice, curr_spec);
                has_prev = true;
            }

            current_is_a = !current_is_a;
        }

        if count > 0 {
            flux / count as f32
        } else {
            0.0
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rms_computation() {
        let samples = vec![0.5, -0.5, 0.5, -0.5];
        let (rms, _) = compute_rms_and_zcr(&samples);
        assert!((rms - 0.5).abs() < 0.001);
    }

    #[test]
    fn zcr_of_silence_is_zero() {
        let samples = vec![0.0f32; 100];
        let (_, zcr) = compute_rms_and_zcr(&samples);
        assert_eq!(zcr, 0.0);
    }

    #[test]
    fn spectral_features_empty_input() {
        let result = analyze_audio_features(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn spectral_features_silence() {
        let samples = vec![0.001f32; 16000];
        let features = analyze_audio_features(&samples).unwrap();

        assert!(features.rms < 0.01);
    }

    fn generate_white_noise(len: usize) -> Vec<f32> {
        use rand::rngs::StdRng;
        use rand::{RngExt, SeedableRng};
        let mut rng = StdRng::seed_from_u64(42);
        (0..len).map(|_| rng.random::<f32>() * 2.0 - 1.0).collect()
    }

    #[test]
    fn test_spectral_entropy_white_noise() {
        let samples = generate_white_noise(1024);
        let (entropy, ..) = compute_spectral_features(&samples).unwrap();
        assert!(
            entropy > 7.0,
            "White noise should have high entropy, got {entropy}"
        );
    }

    #[test]
    fn test_spectral_flatness_sine_vs_noise() {
        let sine: Vec<f32> = (0..1024).map(|i| (i as f32 * 0.1).sin()).collect();
        let noise = generate_white_noise(1024);
        let (_, flatness_sine, ..) = compute_spectral_features(&sine).unwrap();
        let (_, flatness_noise, ..) = compute_spectral_features(&noise).unwrap();
        assert!(
            flatness_noise > flatness_sine,
            "Noise flatness {flatness_noise} should be > sine flatness {flatness_sine}"
        );
        assert!(
            flatness_sine < 0.35,
            "Sine flatness too high: {flatness_sine}"
        );
        assert!(
            flatness_noise > 0.45,
            "Noise flatness too low: {flatness_noise}"
        );
    }

    #[test]
    fn test_spectral_flux_loop_range() {
        // window=512, hop=256.
        // 512 samples: exactly 1 window. flux should be 0 because no prev.
        let samples1 = vec![0.1f32; 512];
        assert_eq!(compute_spectral_flux(&samples1), 0.0);

        // 768 samples: exactly 2 windows (0..512 and 256..768).
        // If we use 0..512 (exclusive), it only sees 1 window.
        // If we use 0..=256, it sees 2 windows.
        let mut samples2 = vec![0.1f32; 768];
        for s in samples2.iter_mut().skip(512) {
            *s = 0.5; // Change second half
        }
        let flux = compute_spectral_flux(&samples2);
        assert!(
            flux > 0.0,
            "Flux should be non-zero for 2 different windows, got {}",
            flux
        );
    }

    #[test]
    fn test_zcr_sine_wave() {
        // 1kHz sine at 16kHz sample rate
        // 16 samples per cycle.
        let mut samples = vec![0.0f32; 1600];
        for (i, s) in samples.iter_mut().enumerate() {
            *s = (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / 16000.0).sin();
        }
        let (_, zcr) = compute_rms_and_zcr(&samples);
        // 100 cycles, 2 crossings per cycle = 200 crossings.
        // ZCR = 200 / 1599 approx 0.125
        assert!((zcr - 0.125).abs() < 0.01);
    }

    #[test]
    fn test_spectral_centroid_sine() {
        // 2kHz sine at 16kHz sample rate
        let mut samples = vec![0.0f32; 1024];
        for (i, s) in samples.iter_mut().enumerate() {
            *s = (2.0 * std::f32::consts::PI * 2000.0 * i as f32 / 16000.0).sin();
        }
        let (_, _, centroid, ..) = compute_spectral_features(&samples).unwrap();
        // Centroid should be very close to 2000Hz.
        assert!((centroid - 2000.0).abs() < 100.0);
    }

    #[test]
    fn test_rms_known_values_and_edge_cases() {
        let val = std::f32::consts::FRAC_1_SQRT_2;
        assert!((compute_rms_and_zcr(&vec![val; 1000]).0 - val).abs() < 0.001);
        assert!((compute_rms_and_zcr(&[0.0, 1.0, 0.0, -1.0]).0 - val).abs() < 0.001);
        assert_eq!(compute_rms_and_zcr(&[]), (0.0, 0.0));
        assert_eq!(compute_rms_and_zcr(&[-0.5]), (0.5, 0.0));
        assert_eq!(compute_rms_and_zcr(&[1.0, -0.0]).1, 0.0);
        assert_eq!(compute_rms_and_zcr(&[-1.0, -0.0]).1, 1.0);
        assert!(compute_rms_and_zcr(&[0.5f32, f32::NAN, -0.5]).0.is_nan());
    }
}
