use rayon::prelude::*;
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

pub struct ConstellationMap {
    pub peaks: Vec<(usize, usize)>,
    pub density: f32,
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
    pub constellation_density: f32,
    pub constellation_speech_density: f32,
}

pub struct FeatureExtractor {
    analyzer: SpectralAnalyzer,
    prev_mags: Vec<f32>,
    fft_len: usize,
}

impl FeatureExtractor {
    pub fn new(fft_len: usize) -> Self {
        Self {
            analyzer: SpectralAnalyzer::new(fft_len),
            prev_mags: vec![0.0; fft_len / 2],
            fft_len,
        }
    }

    pub fn extract(&mut self, samples: &[f32], sample_rate: u32) -> FeatureSet {
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
                constellation_density: 0.0,
                constellation_speech_density: 0.0,
            };
        }

        let inv_ln_2 = 1.0 / 2.0f32.ln();

        let sum_sq: f32 = samples.iter().map(|v| v * v).sum();
        let rms = (sum_sq / samples.len() as f32).sqrt();

        let mut zero_crosses = 0u32;
        for w in samples.windows(2) {
            if (w[0] >= 0.0) != (w[1] >= 0.0) {
                zero_crosses += 1;
            }
        }
        let zcr = zero_crosses as f32 / samples.len() as f32;

        let bin_width = sample_rate as f32 / self.fft_len as f32;
        let low_bin = (300.0 / bin_width) as usize;
        let high_bin = (2000.0 / bin_width) as usize;
        let half_bins = self.fft_len / 2;
        let inv_half_bins = 1.0 / half_bins as f32;

        let mut flux_acc = 0.0;
        let mut weighted_bin_sum = 0.0;
        let mut mag_sum = 0.0;
        let mut low = 0.0;
        let mut high = 0.0;
        let mut log_mag_sum = 0.0f32;
        let mut arithmetic_mean = 0.0f32;
        let mut valid_mag_count = 0usize;
        let mut entropy_acc = 0.0f32;
        let mut entropy_count = 0usize;
        let mut has_prev = false;

        let mut all_mags = Vec::new();

        for chunk in samples
            .chunks(self.fft_len / 2)
            .take_while(|c| c.len() >= self.fft_len / 4)
        {
            let mags = self.analyzer.analyze(chunk);
            all_mags.push(mags.to_vec());

            let mut chunk_mag_sum = 0.0f32;
            let mut chunk_sum_m_ln_m = 0.0f32;

            for (i, &m) in mags.iter().enumerate().take(half_bins) {
                chunk_mag_sum += m;

                let freq = i as f32 * bin_width;
                weighted_bin_sum += freq * m;
                mag_sum += m;
                if i < low_bin {
                    low += m;
                }
                if i >= high_bin {
                    high += m;
                }

                if m > 1e-10 {
                    let ln_m = m.ln();
                    log_mag_sum += ln_m;
                    arithmetic_mean += m;
                    valid_mag_count += 1;
                    chunk_sum_m_ln_m += m * ln_m;
                }

                if has_prev {
                    let diff = m - self.prev_mags[i];
                    flux_acc += diff.max(0.0);
                }
            }

            if chunk_mag_sum > 1e-10 {
                let chunk_entropy =
                    (chunk_mag_sum.ln() - chunk_sum_m_ln_m / chunk_mag_sum) * inv_ln_2;
                entropy_acc += chunk_entropy.max(0.0);
                entropy_count += 1;
            }

            self.prev_mags.copy_from_slice(mags);
            has_prev = true;
        }

        let spectral_entropy = if entropy_count > 0 {
            entropy_acc / entropy_count as f32
        } else {
            0.0
        };

        let spectral_flatness = if valid_mag_count > 0 && arithmetic_mean > 0.0 {
            let am = arithmetic_mean / valid_mag_count as f32;
            ((log_mag_sum * inv_half_bins) - am.ln()).max(-10.0).exp()
        } else {
            0.0
        };

        let constellation = extract_constellation(&all_mags, 5, 10, 0.01);
        let mut speech_peaks = 0;
        for &(_, f) in &constellation.peaks {
            let freq = f as f32 * bin_width;
            if (300.0..=3000.0).contains(&freq) {
                speech_peaks += 1;
            }
        }
        let constellation_speech_density = if !all_mags.is_empty() {
            speech_peaks as f32 * 5.0 / all_mags.len() as f32
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
            constellation_density: constellation.density,
            constellation_speech_density,
        }
    }
}

pub fn extract_constellation(
    spectrogram: &[Vec<f32>],
    neighborhood_frames: usize,
    neighborhood_bins: usize,
    magnitude_threshold: f32,
) -> ConstellationMap {
    if spectrogram.is_empty() {
        return ConstellationMap {
            peaks: Vec::new(),
            density: 0.0,
        };
    }

    let num_frames = spectrogram.len();
    let num_bins = spectrogram[0].len();

    // Pass 1: max along frequency axis
    let mut freq_max = vec![vec![0.0f32; num_bins]; num_frames];
    for (t, frame_mags) in spectrogram.iter().enumerate() {
        for (f, item) in freq_max[t].iter_mut().enumerate() {
            let start = f.saturating_sub(neighborhood_bins);
            let end = (f + neighborhood_bins).min(num_bins - 1);
            let mut m = 0.0f32;
            for mag in frame_mags.iter().take(end + 1).skip(start) {
                if *mag > m {
                    m = *mag;
                }
            }
            *item = m;
        }
    }

    // Pass 2: max along time axis
    let mut peaks = Vec::new();
    for f in 0..num_bins {
        for (t, frame_mags) in spectrogram.iter().enumerate() {
            let start = t.saturating_sub(neighborhood_frames);
            let end = (t + neighborhood_frames).min(num_frames - 1);
            let mut m = 0.0f32;
            for row in freq_max.iter().take(end + 1).skip(start) {
                if row[f] > m {
                    m = row[f];
                }
            }

            let val = frame_mags[f];
            if val > magnitude_threshold && (val - m).abs() < 1e-6 {
                peaks.push((t, f));
            }
        }
    }

    // Density filter: max 50 peaks per 100ms window (5 frames)
    let window_size = 5;
    let mut filtered_peaks = Vec::new();
    for window_start in (0..num_frames).step_by(window_size) {
        let window_end = (window_start + window_size).min(num_frames);
        let mut window_peaks: Vec<_> = peaks
            .iter()
            .filter(|(t, _)| *t >= window_start && *t < window_end)
            .copied()
            .collect();

        if window_peaks.len() > 50 {
            window_peaks.sort_by(|&(t1, f1), &(t2, f2)| {
                spectrogram[t2][f2]
                    .partial_cmp(&spectrogram[t1][f1])
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            window_peaks.truncate(50);
        }
        filtered_peaks.extend(window_peaks);
    }
    let peaks = filtered_peaks;

    let density = if num_frames > 0 {
        peaks.len() as f32 * 5.0 / num_frames as f32
    } else {
        0.0
    };

    ConstellationMap { peaks, density }
}

#[allow(dead_code)]
pub fn compute_features(samples: &[f32], sample_rate: u32) -> FeatureSet {
    let fft_len = 1024usize.next_power_of_two().max(256);
    let mut extractor = FeatureExtractor::new(fft_len);
    extractor.extract(samples, sample_rate)
}

pub fn feature_set_to_frame(f: FeatureSet) -> crate::types::frame::Frame {
    crate::types::frame::Frame {
        rms: f.rms,
        zcr: f.zcr,
        spectral_flux: f.spectral_flux,
        spectral_flatness: f.spectral_flatness,
        spectral_entropy: f.spectral_entropy,
        centroid_hz: f.centroid_hz,
        low_band_ratio: f.low_band_ratio,
        high_band_ratio: f.high_band_ratio,
        constellation_peaks: (f.constellation_density * 0.2).round() as u32, // peaks per 20ms frame
        constellation_speech_peaks: (f.constellation_speech_density * 0.2).round() as u32,
    }
}

pub fn compute_features_parallel(frames: &[&[f32]], sample_rate: u32) -> Vec<FeatureSet> {
    if frames.is_empty() {
        return Vec::new();
    }
    let fft_len = frames[0].len().next_power_of_two().max(256);
    let half_bins = fft_len / 2;

    let mut all_mags = vec![0.0f32; frames.len() * half_bins];
    all_mags
        .par_chunks_mut(half_bins)
        .zip(frames.par_iter())
        .for_each_init(
            || SpectralAnalyzer::new(fft_len),
            |analyzer, (mags_out, chunk)| {
                let mags = analyzer.analyze(chunk);
                let n = mags.len().min(half_bins);
                mags_out[..n].copy_from_slice(&mags[..n]);
            },
        );

    let spectrogram: Vec<Vec<f32>> = all_mags.chunks(half_bins).map(|c| c.to_vec()).collect();

    let constellation = extract_constellation(&spectrogram, 5, 10, 0.01);
    let mut frame_peak_counts = vec![0u32; frames.len()];
    let mut frame_speech_peak_counts = vec![0u32; frames.len()];

    let bin_width = sample_rate as f32 / fft_len as f32;

    for &(t, f) in &constellation.peaks {
        if t < frames.len() {
            frame_peak_counts[t] += 1;
            let freq = f as f32 * bin_width;
            if (300.0..=3000.0).contains(&freq) {
                frame_speech_peak_counts[t] += 1;
            }
        }
    }

    frames
        .par_iter()
        .enumerate()
        .map(|(i, chunk)| {
            let start = i * half_bins;
            let mags = &all_mags[start..start + half_bins];
            let prev_mags = if i > 0 {
                let prev_start = (i - 1) * half_bins;
                Some(&all_mags[prev_start..prev_start + half_bins])
            } else {
                None
            };
            let mut fset =
                compute_frame_features_impl(chunk, mags, prev_mags, sample_rate, fft_len);
            fset.constellation_density = frame_peak_counts[i] as f32 * 5.0; // scale to peaks per 100ms
            fset.constellation_speech_density = frame_speech_peak_counts[i] as f32 * 5.0;
            fset
        })
        .collect()
}

fn compute_frame_features_impl(
    samples: &[f32],
    mags: &[f32],
    prev_mags: Option<&[f32]>,
    sample_rate: u32,
    fft_len: usize,
) -> FeatureSet {
    let inv_ln_2 = 1.0 / 2.0f32.ln();

    let sum_sq: f32 = samples.iter().map(|v| v * v).sum();
    let rms = (sum_sq / samples.len() as f32).sqrt();

    let mut zero_crosses = 0u32;
    for w in samples.windows(2) {
        if (w[0] >= 0.0) != (w[1] >= 0.0) {
            zero_crosses += 1;
        }
    }
    let zcr = zero_crosses as f32 / samples.len().max(1) as f32;

    let bin_width = sample_rate as f32 / fft_len as f32;
    let half_bins = fft_len / 2;
    let inv_half_bins = 1.0 / half_bins.max(1) as f32;
    let low_bin = (300.0 / bin_width).round() as usize;
    let high_bin = (2000.0 / bin_width).round() as usize;
    let low_bin = low_bin.min(half_bins);
    let high_bin = high_bin.min(half_bins);

    let mut weighted_bin_sum = 0.0;
    let mut mag_sum = 0.0;
    let mut low = 0.0;
    let mut high = 0.0;
    let mut log_mag_sum = 0.0f32;
    let mut arithmetic_mean = 0.0f32;
    let mut valid_mag_count = 0usize;
    let mut flux_acc = 0.0f32;
    let mut sum_m_ln_m = 0.0f32;

    for (i, &m) in mags.iter().enumerate().take(half_bins) {
        let freq = i as f32 * bin_width;
        weighted_bin_sum += freq * m;
        mag_sum += m;
        if i <= low_bin {
            low += m;
        }
        if i >= high_bin {
            high += m;
        }

        if m > 1e-10 {
            let ln_m = m.ln();
            log_mag_sum += ln_m;
            arithmetic_mean += m;
            valid_mag_count += 1;
            sum_m_ln_m += m * ln_m;
        }

        if let Some(prev) = prev_mags {
            flux_acc += (m - prev[i]).max(0.0);
        }
    }

    let spectral_entropy = if mag_sum > 1e-10 {
        ((mag_sum.ln() - sum_m_ln_m / mag_sum) * inv_ln_2).max(0.0)
    } else {
        0.0
    };

    let spectral_flatness = if valid_mag_count > 0 && arithmetic_mean > 0.0 {
        let am = arithmetic_mean / valid_mag_count as f32;
        ((log_mag_sum * inv_half_bins) - am.ln()).max(-10.0).exp()
    } else {
        0.0
    };

    FeatureSet {
        rms,
        zcr,
        spectral_flux: flux_acc * inv_half_bins,
        spectral_flatness,
        spectral_entropy,
        centroid_hz: if mag_sum > 0.0 {
            weighted_bin_sum / mag_sum
        } else {
            0.0
        },
        low_band_ratio: if mag_sum > 0.0 { low / mag_sum } else { 0.0 },
        high_band_ratio: if mag_sum > 0.0 { high / mag_sum } else { 0.0 },
        constellation_density: 0.0,
        constellation_speech_density: 0.0,
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

    #[test]
    fn test_extract_constellation_basic() {
        let mut spectrogram = vec![vec![0.0f32; 100]; 20];
        // Create a clear peak at (5, 50)
        spectrogram[5][50] = 1.0;
        // Some noise below threshold
        spectrogram[10][10] = 0.005;

        let map = extract_constellation(&spectrogram, 5, 10, 0.01);
        assert_eq!(map.peaks.len(), 1);
        assert_eq!(map.peaks[0], (5, 50));
    }

    #[test]
    fn test_extract_constellation_density_filter() {
        let mut spectrogram = vec![vec![0.0f32; 100]; 5];
        // Create 60 peaks in a single 5-frame window
        for i in 0..60 {
            spectrogram[i % 5][i] = 1.0 + i as f32 * 0.01;
        }

        // Use 0, 0 neighborhood to avoid interference between these close peaks
        let map = extract_constellation(&spectrogram, 0, 0, 0.01);
        // Should be limited to 50
        assert_eq!(map.peaks.len(), 50);
        // Should keep highest magnitudes (the ones with higher indices)
        let mut min_idx = 100;
        for &(_, f) in &map.peaks {
            if f < min_idx {
                min_idx = f;
            }
        }
        assert!(min_idx >= 10);
    }
}
