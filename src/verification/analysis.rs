use realfft::RealFftPlanner;
use serde::{Deserialize, Serialize};

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
    let sum_squares: f32 = samples.iter().map(|s| s * s).sum();
    (sum_squares / samples.len() as f32).sqrt()
}

fn compute_zcr(samples: &[f32]) -> f32 {
    if samples.len() < 2 {
        return 0.0;
    }
    let crossings: usize = samples
        .windows(2)
        .filter(|w| w[0].signum() != w[1].signum())
        .count();
    crossings as f32 / (samples.len() - 1) as f32
}

fn compute_spectral_features(samples: &[f32]) -> anyhow::Result<(f32, f32, f32, f32, f32)> {
    let fft_size = next_power_of_2(samples.len().max(512));

    let mut planner = RealFftPlanner::new();
    let fft = planner.plan_fft_forward(fft_size);
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

    // Reuse a single buffer for magnitudes to avoid extra allocations
    let spectrum: Vec<f32> = output.iter().map(|c| c.norm()).collect();

    let entropy = compute_spectral_entropy(&spectrum);
    let flatness = compute_spectral_flatness(&spectrum);
    let (centroid, low_ratio, high_ratio) = compute_spectral_centroid(&spectrum);

    Ok((entropy, flatness, centroid, low_ratio, high_ratio))
}

fn next_power_of_2(n: usize) -> usize {
    let n = n.saturating_sub(1);
    let shift = std::mem::size_of::<usize>() * 8 - n.leading_zeros() as usize;
    1 << shift
}

fn compute_spectral_entropy(spectrum: &[f32]) -> f32 {
    let sum: f32 = spectrum.iter().sum();
    if sum == 0.0 {
        return 7.0;
    }
    let inv_sum = 1.0 / sum;
    spectrum
        .iter()
        .filter(|&&x| x > 0.0)
        .map(|&x| {
            let p = x * inv_sum;
            -p * p.log2()
        })
        .sum()
}

fn compute_spectral_flatness(spectrum: &[f32]) -> f32 {
    let n = spectrum.len();
    if n == 0 {
        return 1.0;
    }

    let mut log_sum = 0.0f32;
    let mut pos_count = 0usize;
    let mut sum = 0.0f32;

    for &x in spectrum {
        sum += x;
        if x > 0.0 {
            log_sum += x.ln();
            pos_count += 1;
        }
    }

    if pos_count == 0 || sum == 0.0 {
        return 1.0;
    }

    let geometric_mean = (log_sum / pos_count as f32).exp();
    let arithmetic_mean = sum / n as f32;

    (geometric_mean / arithmetic_mean).min(1.0)
}

fn compute_spectral_centroid(spectrum: &[f32]) -> (f32, f32, f32) {
    let sample_rate = 16000.0f32;
    // realfft output length is n/2 + 1
    let n = (spectrum.len().saturating_sub(1)) * 2;
    let bin_width = sample_rate / n.max(1) as f32;

    let mut weighted_sum = 0.0f32;
    let mut total = 0.0f32;
    let mut low_sum = 0.0f32;
    let mut high_sum = 0.0f32;

    for (i, &mag) in spectrum.iter().enumerate() {
        let freq = i as f32 * bin_width;
        weighted_sum += freq * mag;
        total += mag;

        if freq < 250.0 {
            low_sum += mag;
        } else if freq > 4000.0 {
            high_sum += mag;
        }
    }

    let centroid = if total > 0.0 {
        weighted_sum / total
    } else {
        0.0
    };

    let low_ratio = if total > 0.0 { low_sum / total } else { 0.0 };
    let high_ratio = if total > 0.0 { high_sum / total } else { 0.0 };

    (centroid, low_ratio, high_ratio)
}

fn compute_spectral_flux(samples: &[f32]) -> f32 {
    if samples.len() < 2 {
        return 0.0;
    }

    let window_size = 512;
    let hop_size = 256;

    let mut flux = 0.0f32;
    let mut count = 0usize;
    let mut has_prev = false;

    let mut planner = RealFftPlanner::new();
    let fft = planner.plan_fft_forward(window_size);
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
            for (c, m) in output.iter().zip(spectrum_a.iter_mut()) {
                *m = c.norm();
            }
            if has_prev {
                let diff: f32 = spectrum_a
                    .iter()
                    .zip(spectrum_b.iter())
                    .map(|(s, p)| (s - p).max(0.0))
                    .sum();
                flux += diff;
                count += 1;
            }
        } else {
            for (c, m) in output.iter().zip(spectrum_b.iter_mut()) {
                *m = c.norm();
            }
            if has_prev {
                let diff: f32 = spectrum_b
                    .iter()
                    .zip(spectrum_a.iter())
                    .map(|(s, p)| (s - p).max(0.0))
                    .sum();
                flux += diff;
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

        let (entropy, _, _, _, _) = compute_spectral_features(&samples).unwrap();
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
        for i in 0..1024 {
            sine[i] = (i as f32 * 0.1).sin();
        }

        let mut noise = vec![0.0f32; 1024];
        use rand::rngs::StdRng;
        use rand::{RngExt, SeedableRng};
        let mut rng = StdRng::seed_from_u64(42);
        for s in noise.iter_mut() {
            *s = rng.random::<f32>() * 2.0 - 1.0;
        }

        let (_, flatness_sine, _, _, _) = compute_spectral_features(&sine).unwrap();
        let (_, flatness_noise, _, _, _) = compute_spectral_features(&noise).unwrap();

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
        for i in 512..768 {
            samples2[i] = 0.5; // Change second half
        }
        let flux = compute_spectral_flux(&samples2);
        assert!(
            flux > 0.0,
            "Flux should be non-zero for 2 different windows, got {}",
            flux
        );
    }
}
