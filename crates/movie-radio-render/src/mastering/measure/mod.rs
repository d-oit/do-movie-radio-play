//! Loudness, peak and continuity measurement for mastering validation.

use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

mod meter;

pub use meter::LoudnessMeter;

/// Floor used instead of `-inf` so reports stay valid JSON.
pub const SILENCE_LUFS: f32 = -100.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoudnessMeasurement {
    pub integrated_lufs: f32,
    pub loudness_range_lu: f32,
    pub momentary_max_lufs: f32,
    pub short_term_max_lufs: f32,
    pub sample_peak_dbfs: f32,
    pub true_peak_dbtp: f32,
    pub dc_offset: f32,
    pub clipped_samples: usize,
    pub longest_silence_ms: u64,
    pub max_discontinuity: f32,
    pub max_gain_change_lu: f32,
    pub channel_imbalance_db: f32,
    pub duration_s: f64,
}

impl Default for LoudnessMeasurement {
    fn default() -> Self {
        Self {
            integrated_lufs: SILENCE_LUFS,
            loudness_range_lu: 0.0,
            momentary_max_lufs: SILENCE_LUFS,
            short_term_max_lufs: SILENCE_LUFS,
            sample_peak_dbfs: SILENCE_LUFS,
            true_peak_dbtp: SILENCE_LUFS,
            dc_offset: 0.0,
            clipped_samples: 0,
            longest_silence_ms: 0,
            max_discontinuity: 0.0,
            max_gain_change_lu: 0.0,
            channel_imbalance_db: 0.0,
            duration_s: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MeasurementConfig {
    pub silence_threshold_dbfs: f32,
    pub clipping_threshold: f32,
}

impl Default for MeasurementConfig {
    fn default() -> Self {
        Self {
            silence_threshold_dbfs: -60.0,
            clipping_threshold: 0.999,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Biquad {
    pub(super) fn new(b0: f32, b1: f32, b2: f32, a0: f32, a1: f32, a2: f32) -> Self {
        let inv = if a0.abs() > f32::EPSILON {
            1.0 / a0
        } else {
            1.0
        };
        Self {
            b0: b0 * inv,
            b1: b1 * inv,
            b2: b2 * inv,
            a1: a1 * inv,
            a2: a2 * inv,
            z1: 0.0,
            z2: 0.0,
        }
    }

    pub(super) fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

fn high_shelf(sample_rate: f32, freq: f32, gain_db: f32, q: f32) -> Biquad {
    let a = 10f32.powf(gain_db / 40.0);
    let w0 = 2.0 * std::f32::consts::PI * freq / sample_rate;
    let cos_w0 = w0.cos();
    let sin_w0 = w0.sin();
    let alpha = sin_w0 / (2.0 * q);
    let sqrt_a = a.sqrt();
    let b0 = a * ((a + 1.0) + (a - 1.0) * cos_w0 + 2.0 * sqrt_a * alpha);
    let b1 = -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0);
    let b2 = a * ((a + 1.0) + (a - 1.0) * cos_w0 - 2.0 * sqrt_a * alpha);
    let a0 = (a + 1.0) - (a - 1.0) * cos_w0 + 2.0 * sqrt_a * alpha;
    let a1 = 2.0 * ((a - 1.0) - (a + 1.0) * cos_w0);
    let a2 = (a + 1.0) - (a - 1.0) * cos_w0 - 2.0 * sqrt_a * alpha;
    Biquad::new(b0, b1, b2, a0, a1, a2)
}

fn high_pass(sample_rate: f32, freq: f32, q: f32) -> Biquad {
    let w0 = 2.0 * std::f32::consts::PI * freq / sample_rate;
    let cos_w0 = w0.cos();
    let sin_w0 = w0.sin();
    let alpha = sin_w0 / (2.0 * q);
    let b0 = (1.0 + cos_w0) / 2.0;
    let b1 = -(1.0 + cos_w0);
    let b2 = (1.0 + cos_w0) / 2.0;
    let a0 = 1.0 + alpha;
    let a1 = -2.0 * cos_w0;
    let a2 = 1.0 - alpha;
    Biquad::new(b0, b1, b2, a0, a1, a2)
}

pub(super) fn k_weighting(sample_rate: f32) -> (Biquad, Biquad) {
    (
        high_shelf(sample_rate, 1681.97, 4.0, 0.707_106_77),
        high_pass(sample_rate, 38.13, 0.5),
    )
}

pub(super) fn loudness_from_energy(mean_energy: f64) -> f32 {
    if mean_energy <= 0.0 {
        SILENCE_LUFS
    } else {
        (-0.691 + 10.0 * mean_energy.log10()) as f32
    }
}

fn energy_from_loudness(loudness: f32) -> f64 {
    10f64.powf((f64::from(loudness) + 0.691) / 10.0)
}

pub(super) fn integrated_loudness(blocks: &[f32]) -> f32 {
    let gated: Vec<f64> = blocks
        .iter()
        .copied()
        .filter(|&l| l > -70.0)
        .map(energy_from_loudness)
        .collect();
    if gated.is_empty() {
        return SILENCE_LUFS;
    }
    let mean = gated.iter().sum::<f64>() / gated.len() as f64;
    let relative_gate = loudness_from_energy(mean) - 10.0;
    let gated: Vec<f64> = blocks
        .iter()
        .copied()
        .filter(|&l| l > -70.0 && l > relative_gate)
        .map(energy_from_loudness)
        .collect();
    if gated.is_empty() {
        return SILENCE_LUFS;
    }
    let mean = gated.iter().sum::<f64>() / gated.len() as f64;
    loudness_from_energy(mean)
}

pub(super) fn loudness_range(blocks: &[f32]) -> f32 {
    let gated: Vec<f32> = blocks.iter().copied().filter(|&l| l > -70.0).collect();
    if gated.is_empty() {
        return 0.0;
    }
    let mean_energy =
        gated.iter().copied().map(energy_from_loudness).sum::<f64>() / gated.len() as f64;
    let relative_gate = loudness_from_energy(mean_energy) - 20.0;
    let mut values: Vec<f32> = gated.into_iter().filter(|&l| l > relative_gate).collect();
    if values.len() < 2 {
        return 0.0;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    let p10 = percentile(&values, 0.10);
    let p95 = percentile(&values, 0.95);
    (p95 - p10).max(0.0)
}

fn percentile(sorted: &[f32], fraction: f64) -> f32 {
    if sorted.is_empty() {
        return 0.0;
    }
    let pos = fraction * (sorted.len() - 1) as f64;
    let lower = pos.floor() as usize;
    let upper = pos.ceil() as usize;
    if lower == upper {
        sorted[lower]
    } else {
        let weight = (pos - lower as f64) as f32;
        sorted[lower] * (1.0 - weight) + sorted[upper] * weight
    }
}

pub(super) fn channel_imbalance(sum_sq: &[f64], frames: usize) -> f32 {
    if sum_sq.len() < 2 || frames == 0 {
        return 0.0;
    }
    let rms: Vec<f32> = sum_sq
        .iter()
        .map(|&s| (s / frames as f64).sqrt() as f32)
        .collect();
    let max = rms.iter().copied().fold(0.0f32, f32::max);
    let min = rms.iter().copied().fold(f32::INFINITY, f32::min);
    if min <= 0.0 || max <= 0.0 {
        return 0.0;
    }
    20.0 * (max / min).log10()
}

/// Estimates the inter-sample true peak with 4x windowed-sinc interpolation.
///
/// Only local maxima near the sample peak are interpolated, which keeps the
/// cost bounded on long deliverables while still catching the peaks that can
/// exceed the sample peak.
pub(crate) fn true_peak(samples: &[f32], channels: usize) -> f32 {
    let frames = samples.len() / channels;
    if frames == 0 {
        return 0.0;
    }
    let mut sample_peak = 0.0f32;
    for &s in samples {
        sample_peak = sample_peak.max(s.abs());
    }
    if sample_peak <= 0.0 {
        return 0.0;
    }
    let threshold = (sample_peak * 0.5).max(0.01);
    let mut candidates: Vec<(f32, usize, usize)> = Vec::new();
    for c in 0..channels {
        for i in 0..frames {
            let x = samples[i * channels + c].abs();
            if x < threshold {
                continue;
            }
            let prev = if i > 0 {
                samples[(i - 1) * channels + c].abs()
            } else {
                0.0
            };
            let next = if i + 1 < frames {
                samples[(i + 1) * channels + c].abs()
            } else {
                0.0
            };
            if x < prev || x < next {
                continue;
            }
            candidates.push((x, i, c));
            if candidates.len() > 4096 {
                candidates.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(Ordering::Equal));
                candidates.truncate(1024);
            }
        }
    }
    let mut peak = sample_peak;
    let half = 8i32;
    for (_, i, c) in candidates {
        for phase in 1..4 {
            let frac = phase as f32 / 4.0;
            let mut acc = 0.0f32;
            for k in -half + 1..=half {
                let idx = i as i32 + k;
                if idx < 0 || idx >= frames as i32 {
                    continue;
                }
                let t = frac - k as f32;
                let sinc = if t.abs() < 1e-6 {
                    1.0
                } else {
                    let arg = std::f32::consts::PI * t;
                    arg.sin() / arg
                };
                let window = if t.abs() < half as f32 {
                    0.5 * (1.0 + (std::f32::consts::PI * t / half as f32).cos())
                } else {
                    0.0
                };
                acc += samples[idx as usize * channels + c] * sinc * window;
            }
            peak = peak.max(acc.abs());
        }
    }
    peak
}

pub fn measure(
    samples: &[f32],
    sample_rate: u32,
    channels: u16,
    config: &MeasurementConfig,
) -> LoudnessMeasurement {
    let mut meter = LoudnessMeter::new(sample_rate, channels, config);
    meter.push(samples);
    meter.finish()
}
