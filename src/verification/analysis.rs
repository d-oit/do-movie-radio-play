use realfft::RealFftPlanner;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;

thread_local! {
    static FFT_PLANNER: RefCell<RealFftPlanner<f32>> = RefCell::new(RealFftPlanner::new());
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

    let rms = compute_rms(samples);
    let zcr = compute_zcr(samples);
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

fn compute_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    // Optimization: Manual loop for efficiency
    let mut sum_squares = 0.0f32;
    for &s in samples {
        sum_squares += s * s;
    }
    (sum_squares / samples.len() as f32).sqrt()
}

fn compute_zcr(samples: &[f32]) -> f32 {
    if samples.len() < 2 {
        return 0.0;
    }
    // Optimization: true single pass manual loop
    let mut crossings = 0usize;
    let mut prev_sign = samples[0] >= 0.0;
    for &s in &samples[1..] {
        let sign = s >= 0.0;
        if sign != prev_sign {
            crossings += 1;
        }
        prev_sign = sign;
    }
    crossings as f32 / (samples.len() - 1) as f32
}

fn compute_spectral_features(samples: &[f32]) -> anyhow::Result<(f32, f32, f32, f32, f32)> {
    let fft_size = next_power_of_2(samples.len().max(512));

    let fft = FFT_PLANNER.with(|p| p.borrow_mut().plan_fft_forward(fft_size));
    let mut input = fft.make_input_vec();

    if samples.len() >= fft_size {
        input.copy_from_slice(&samples[..fft_size]);
    } else {
        input[..samples.len()].copy_from_slice(samples);
        input[samples.len()..].fill(0.0);
    };

    let mut output = fft.make_output_vec();
    if fft.process(&mut input, &mut output).is_err() {
        return Err(anyhow::anyhow!("FFT processing failed"));
    }

    // Optimization: Fuse multiple spectral feature calculations into a single pass over the FFT output.
    // This avoids an intermediate Vec<f32> allocation for magnitudes and reduces iterations.
    let sample_rate = 16000.0f32;
    let bin_width = sample_rate / fft_size as f32;
    let inv_ln_2 = 1.0 / 2.0f32.ln();

    let mut weighted_sum = 0.0f32;
    let mut total_mag = 0.0f32;
    let mut low_mag_sum = 0.0f32;
    let mut high_mag_sum = 0.0f32;
    let mut log_mag_sum = 0.0f32;
    let mut mag_log_mag_sum = 0.0f32;
    let mut pos_count = 0usize;

    for (i, c) in output.iter().enumerate() {
        let mag = (c.re * c.re + c.im * c.im).sqrt();
        let freq = i as f32 * bin_width;

        weighted_sum += freq * mag;
        total_mag += mag;

        if freq < 250.0 {
            low_mag_sum += mag;
        } else if freq > 4000.0 {
            high_mag_sum += mag;
        }

        if mag > 1e-10 {
            let ln_mag = mag.ln();
            log_mag_sum += ln_mag;
            mag_log_mag_sum += mag * ln_mag;
            pos_count += 1;
        }
    }

    if total_mag > 0.0 {
        let entropy = ((total_mag.ln() - mag_log_mag_sum / total_mag) * inv_ln_2).max(0.0);
        let flatness = if pos_count > 0 {
            let geometric_mean = (log_mag_sum / pos_count as f32).exp();
            let arithmetic_mean = total_mag / output.len() as f32;
            (geometric_mean / arithmetic_mean).min(1.0)
        } else {
            1.0
        };
        let centroid = weighted_sum / total_mag;
        let low_ratio = low_mag_sum / total_mag;
        let high_ratio = high_mag_sum / total_mag;
        Ok((entropy, flatness, centroid, low_ratio, high_ratio))
    } else {
        Ok((7.0, 1.0, 0.0, 0.0, 0.0))
    }
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

    let mut flux = 0.0f32;
    let mut count = 0usize;
    let mut has_prev = false;

    let fft = FFT_PLANNER.with(|p| p.borrow_mut().plan_fft_forward(window_size));
    let mut input = fft.make_input_vec();
    let mut output = fft.make_output_vec();

    // Use two buffers to avoid allocations in the loop
    let mut spectrum_a = vec![0.0f32; window_size / 2 + 1];
    let mut spectrum_b = vec![0.0f32; window_size / 2 + 1];
    let mut current_is_a = true;

    for i in (0..=samples.len().saturating_sub(window_size)).step_by(hop_size) {
        let window = &samples[i..i + window_size];
        input.copy_from_slice(window);
        if fft.process(&mut input, &mut output).is_err() {
            continue;
        }

        if current_is_a {
            let mut diff_sum = 0.0f32;
            for (c, (m, &p)) in output
                .iter()
                .zip(spectrum_a.iter_mut().zip(spectrum_b.iter()))
            {
                let mag = (c.re * c.re + c.im * c.im).sqrt();
                *m = mag;
                diff_sum += (mag - p).max(0.0);
            }
            if has_prev {
                flux += diff_sum;
                count += 1;
            }
        } else {
            let mut diff_sum = 0.0f32;
            for (c, (m, &p)) in output
                .iter()
                .zip(spectrum_b.iter_mut().zip(spectrum_a.iter()))
            {
                let mag = (c.re * c.re + c.im * c.im).sqrt();
                *m = mag;
                diff_sum += (mag - p).max(0.0);
            }
            if has_prev {
                flux += diff_sum;
                count += 1;
            }
        }

        has_prev = true;
        current_is_a = !current_is_a;
    }

    if count > 0 {
        flux / count as f32
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rms_computation() {
        let samples = vec![0.5, -0.5, 0.5, -0.5];
        let rms = compute_rms(&samples);
        assert!((rms - 0.5).abs() < 0.001);
    }

    #[test]
    fn zcr_of_silence_is_zero() {
        let samples = vec![0.0f32; 100];
        let zcr = compute_zcr(&samples);
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

    #[test]
    fn test_spectral_entropy_white_noise() {
        // White noise should have high entropy
        let mut samples = vec![0.0f32; 1024];
        use rand::rngs::StdRng;
        use rand::{RngExt, SeedableRng};
        let mut rng = StdRng::seed_from_u64(42);
        for s in samples.iter_mut() {
            *s = rng.random::<f32>() * 2.0 - 1.0;
        }

        let (entropy, ..) = compute_spectral_features(&samples).unwrap();
        // For 512 bins (next_power_of_2 of 1024 is 1024, but realfft gives 513 bins),
        // max entropy is log2(513) approx 9.0.
        assert!(
            entropy > 7.0,
            "White noise should have high entropy, got {}",
            entropy
        );
    }

    #[test]
    fn test_spectral_flatness_sine_vs_noise() {
        let mut sine = vec![0.0f32; 1024];
        for (i, s) in sine.iter_mut().enumerate() {
            *s = (i as f32 * 0.1).sin();
        }

        let mut noise = vec![0.0f32; 1024];
        use rand::rngs::StdRng;
        use rand::{RngExt, SeedableRng};
        let mut rng = StdRng::seed_from_u64(42);
        for s in noise.iter_mut() {
            *s = rng.random::<f32>() * 2.0 - 1.0;
        }

        let (_, flatness_sine, ..) = compute_spectral_features(&sine).unwrap();
        let (_, flatness_noise, ..) = compute_spectral_features(&noise).unwrap();

        assert!(
            flatness_noise > flatness_sine,
            "Noise flatness {} should be > sine flatness {}",
            flatness_noise,
            flatness_sine
        );
        // Pure sine has 1 peak, white noise is flat. Flatness of noise should be close to 1.
        // Flatness of a pure sine should be low.
        assert!(
            flatness_sine < 0.35,
            "Sine flatness too high: {}",
            flatness_sine
        );
        assert!(
            flatness_noise > 0.45,
            "Noise flatness too low: {}",
            flatness_noise
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
        let zcr = compute_zcr(&samples);
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
    fn test_rms_known_values() {
        let val = std::f32::consts::FRAC_1_SQRT_2;
        let samples = vec![val; 1000];
        let rms = compute_rms(&samples);
        assert!((rms - val).abs() < 0.001);

        let samples2 = vec![0.0f32, 1.0f32, 0.0f32, -1.0f32];
        // Squares: 0, 1, 0, 1. Sum=2. Avg=0.5. Sqrt=0.7071
        let rms2 = compute_rms(&samples2);
        assert!((rms2 - val).abs() < 0.001);
    }
}
