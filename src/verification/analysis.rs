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
    let padded = if samples.len() >= fft_size {
        samples.to_vec()
    } else {
        let mut p = samples.to_vec();
        p.resize(fft_size, 0.0);
        p
    };

    let spectrum = compute_fft_magnitude(&padded);

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

fn compute_fft_magnitude(samples: &[f32]) -> Vec<f32> {
    let n = samples.len();
    let mut real = samples.to_vec();
    let mut imag = vec![0.0; n];

    fft_in_place(&mut real, &mut imag);

    real.iter()
        .zip(imag.iter())
        .map(|(r, i)| (r * r + i * i).sqrt())
        .collect()
}

fn fft_in_place(real: &mut [f32], imag: &mut [f32]) {
    let n = real.len();
    if n <= 1 {
        return;
    }

    for i in 0..n {
        let j = bit_reverse(i, n);
        if i < j {
            real.swap(i, j);
            imag.swap(i, j);
        }
    }

    let mut len = 2;
    while len <= n {
        let half_len = len / 2;
        let theta = -2.0 * std::f32::consts::PI / len as f32;
        let w_real = theta.cos();
        let w_imag = theta.sin();

        for i in (0..n).step_by(len) {
            let mut wjr = 1.0f32;
            let mut wji = 0.0f32;
            for j in 0..half_len {
                let k = i + j;
                let l = k + half_len;
                let t_real = wjr * real[l] - wji * imag[l];
                let t_imag = wjr * imag[l] + wji * real[l];
                real[l] = real[k] - t_real;
                imag[l] = imag[k] - t_imag;
                real[k] += t_real;
                imag[k] += t_imag;

                let new_wjr = wjr * w_real - wji * w_imag;
                wji = wjr * w_imag + wji * w_real;
                wjr = new_wjr;
            }
        }
        len *= 2;
    }
}

fn bit_reverse(mut n: usize, size: usize) -> usize {
    let bits = size.next_power_of_two().trailing_zeros();
    n = ((n & 0x5555_5555) << 1) | ((n & 0xAAAAAAAA) >> 1);
    n = ((n & 0x3333_3333) << 2) | ((n & 0xCCCC_CCCC) >> 2);
    n = ((n & 0x0F0F_0F0F) << 4) | ((n & 0xF0F0_F0F0) >> 4);
    n = ((n & 0x00FF_00FF) << 8) | ((n & 0xFF00_FF00) >> 8);
    n = ((n & 0x0000_FFFF) << 16) | ((n & 0xFFFF_0000) >> 16);
    n >>= 32 - bits;
    n
}

fn compute_spectral_entropy(spectrum: &[f32]) -> f32 {
    let sum: f32 = spectrum.iter().sum();
    if sum == 0.0 {
        return 7.0;
    }
    let normalized: Vec<f32> = spectrum.iter().map(|&x| x / sum).collect();

    let entropy: f32 = normalized
        .iter()
        .filter(|&&p| p > 0.0)
        .map(|&p| -p * p.log2().max(0.0))
        .sum();

    entropy
}

fn compute_spectral_flatness(spectrum: &[f32]) -> f32 {
    let n = spectrum.len();
    if n == 0 {
        return 1.0;
    }

    let geometric_mean = {
        let pos: Vec<f32> = spectrum.iter().filter(|&&x| x > 0.0).cloned().collect();
        if pos.is_empty() {
            return 1.0;
        }
        let log_sum: f32 = pos.iter().map(|&x| x.ln()).sum();
        (log_sum / pos.len() as f32).exp()
    };

    let arithmetic_mean = spectrum.iter().sum::<f32>() / n as f32;
    if arithmetic_mean == 0.0 {
        return 1.0;
    }

    (geometric_mean / arithmetic_mean).min(1.0)
}

fn compute_spectral_centroid(spectrum: &[f32]) -> (f32, f32, f32) {
    let sample_rate = 16000.0f32;
    let bin_width = sample_rate / (2.0 * spectrum.len() as f32);

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

    for i in (0..samples.len().saturating_sub(window_size)).step_by(hop_size) {
        let window = &samples[i..i + window_size];
        let spectrum = compute_fft_magnitude(window);

        if i >= hop_size {
            let prev_start = i - hop_size;
            let prev_window = &samples[prev_start..prev_start + window_size];
            let prev_spectrum = compute_fft_magnitude(prev_window);

            let diff: f32 = spectrum
                .iter()
                .zip(prev_spectrum.iter())
                .map(|(s, p)| (s - p).max(0.0))
                .sum();
            flux += diff;
            count += 1;
        }
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
}
