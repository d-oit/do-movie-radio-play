//! Deterministic loudness normalization and true-peak limiting.

use std::collections::VecDeque;

use super::measure::{measure, true_peak, MeasurementConfig, SILENCE_LUFS};
use super::{validate, MasteringProfile, MasteringReport};

#[derive(Debug, Clone)]
pub struct MasteringOutcome {
    pub samples: Vec<f32>,
    pub report: MasteringReport,
    pub applied_gain_db: f32,
    pub limiting_reduction_db: f32,
}

/// Normalizes to the profile's integrated target, then limits to its true-peak
/// ceiling. The same input and profile always produce the same output.
pub fn master(
    samples: &[f32],
    sample_rate: u32,
    channels: u16,
    profile: &MasteringProfile,
) -> MasteringOutcome {
    let channels = channels.max(1);
    if samples.is_empty() || sample_rate == 0 {
        let report = validate(samples, sample_rate, channels, profile);
        return MasteringOutcome {
            samples: samples.to_vec(),
            report,
            applied_gain_db: 0.0,
            limiting_reduction_db: 0.0,
        };
    }

    let config = MeasurementConfig {
        silence_threshold_dbfs: profile.silence_threshold_dbfs,
        clipping_threshold: profile.clipping_threshold,
    };
    let before = measure(samples, sample_rate, channels, &config);
    let initial_gain_db = if before.integrated_lufs <= SILENCE_LUFS + 1.0 {
        0.0
    } else {
        (profile.target_lufs - before.integrated_lufs).clamp(-24.0, 24.0)
    };
    let gain = 10f32.powf(initial_gain_db / 20.0);
    let mut output: Vec<f32> = samples.iter().map(|&s| s * gain).collect();
    let mut applied_gain_db = initial_gain_db;

    let ceiling = 10f32.powf(profile.max_true_peak_dbtp / 20.0);
    let mut limiting_reduction_db = 0.0;
    // Limiting removes peaks and therefore some loudness. Re-measure and
    // top up a few times so the delivered file still reaches the target.
    for _ in 0..3 {
        let peak_before = true_peak(&output, channels as usize);
        if peak_before > ceiling {
            limiting_reduction_db += 20.0 * (ceiling / peak_before).log10();
            limit(&mut output, channels as usize, sample_rate, ceiling);
            let peak_after = true_peak(&output, channels as usize);
            if peak_after > ceiling {
                let safety = ceiling / peak_after;
                for s in &mut output {
                    *s *= safety;
                }
                limiting_reduction_db += 20.0 * safety.log10();
            }
        }
        let measured = measure(&output, sample_rate, channels, &config).integrated_lufs;
        if measured <= SILENCE_LUFS + 1.0 {
            break;
        }
        let delta = profile.target_lufs - measured;
        if delta.abs() <= 0.3 {
            break;
        }
        let extra = delta.clamp(-6.0, 6.0);
        applied_gain_db += extra;
        let extra_gain = 10f32.powf(extra / 20.0);
        for s in &mut output {
            *s *= extra_gain;
        }
    }
    // The last loop iteration may have topped up the gain without a following
    // limit pass; guarantee the ceiling on the final samples.
    let final_peak = true_peak(&output, channels as usize);
    if final_peak > ceiling {
        limiting_reduction_db += 20.0 * (ceiling / final_peak).log10();
        limit(&mut output, channels as usize, sample_rate, ceiling);
        let after = true_peak(&output, channels as usize);
        if after > ceiling {
            let safety = ceiling / after;
            for s in &mut output {
                *s *= safety;
            }
            limiting_reduction_db += 20.0 * safety.log10();
        }
    }

    let mut report = validate(&output, sample_rate, channels, profile);
    report.applied_gain_db = applied_gain_db;
    report.limiting_reduction_db = limiting_reduction_db;
    MasteringOutcome {
        samples: output,
        report,
        applied_gain_db,
        limiting_reduction_db,
    }
}

fn limit(samples: &mut [f32], channels: usize, sample_rate: u32, ceiling: f32) {
    let frames = samples.len() / channels;
    if frames == 0 {
        return;
    }
    let lookahead = ((sample_rate as f32 * 0.005).round() as usize).max(1);
    let release = ((sample_rate as f32 * 0.05).round() as usize).max(1);

    // Target gain from the envelope, then instant attack / exponential release
    // smoothed in place so long deliverables need only one gain buffer.
    let mut gains = vec![1.0f32; frames];
    for (i, frame) in samples.chunks_exact(channels).enumerate() {
        let peak = frame.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
        if peak > ceiling {
            gains[i] = ceiling / peak;
        }
    }
    let mut gain = 1.0f32;
    for slot in &mut gains {
        let t = *slot;
        if t < gain {
            gain = t;
        } else {
            gain += (t - gain) / release as f32;
        }
        *slot = gain;
    }

    // Scan right to left: each frame then sees the minimum gain of its
    // look-ahead window [i, i + lookahead], which is a trailing window of the
    // reversed gain sequence. A deque keeps this O(n) with O(lookahead) memory.
    let mut window: VecDeque<usize> = VecDeque::new();
    for j in 0..frames {
        let i = frames - 1 - j;
        while let Some(&back) = window.back() {
            if gains[frames - 1 - back] >= gains[i] {
                window.pop_back();
            } else {
                break;
            }
        }
        window.push_back(j);
        while let Some(&front) = window.front() {
            if front + lookahead < j {
                window.pop_front();
            } else {
                break;
            }
        }
        let g = window
            .front()
            .map_or(1.0, |&front| gains[frames - 1 - front]);
        let frame = &mut samples[i * channels..(i + 1) * channels];
        for s in frame {
            *s *= g;
        }
    }
}
