use realfft::RealFftPlanner;
use std::collections::HashMap;

pub use movie_radio_types::Fingerprint;

pub const DEFAULT_SAMPLE_RATE: f32 = 16000.0;
const WINDOW_SIZE: usize = 512;
const HOP_SIZE: usize = 256;
const FAN_OUT: usize = 5;
const MAX_DELTA_T_MS: u32 = 4095;
const MIN_DELTA_T_MS: u32 = 10; // Avoid pairing with same frame or very close

/// Generates Wang combinatorial hashes for a given audio segment.
pub fn fingerprint_segment(samples: &[f32], sample_rate: f32) -> Vec<Fingerprint> {
    if samples.len() < WINDOW_SIZE {
        return Vec::new();
    }

    // 1. Spectrogram generation
    let spectrogram = compute_spectrogram(samples, WINDOW_SIZE, HOP_SIZE);
    if spectrogram.is_empty() {
        return Vec::new();
    }

    // 2. Peak detection (constellation map)
    let peaks = find_peaks(&spectrogram, sample_rate, WINDOW_SIZE);

    // 3. Combinatorial hashing
    generate_hashes(&peaks)
}

fn compute_spectrogram(samples: &[f32], window_size: usize, hop_size: usize) -> Vec<Vec<f32>> {
    let mut planner = RealFftPlanner::new();
    let fft = planner.plan_fft_forward(window_size);
    let mut input = fft.make_input_vec();
    let mut output = fft.make_output_vec();

    let mut spectrogram = Vec::new();

    for i in (0..=samples.len().saturating_sub(window_size)).step_by(hop_size) {
        let window = &samples[i..i + window_size];
        input.copy_from_slice(window);

        // Apply Hann window
        for (j, val) in input.iter_mut().enumerate() {
            let multiplier = 0.5
                * (1.0 - (2.0 * std::f32::consts::PI * j as f32 / (window_size - 1) as f32).cos());
            *val *= multiplier;
        }

        if fft.process(&mut input, &mut output).is_ok() {
            let magnitudes: Vec<f32> = output.iter().map(|c| c.norm()).collect();
            spectrogram.push(magnitudes);
        }
    }

    spectrogram
}

#[derive(Debug, Clone, Copy)]
struct Peak {
    time_ms: u32,
    freq_hz: f32,
}

fn find_peaks(spectrogram: &[Vec<f32>], sample_rate: f32, fft_size: usize) -> Vec<Peak> {
    let num_frames = spectrogram.len();
    let num_bins = spectrogram[0].len();
    let bin_width = sample_rate / fft_size as f32;
    let ms_per_hop = (HOP_SIZE as f32 / sample_rate * 1000.0) as u32;

    let mut peaks = Vec::new();

    // Simple local maxima detection in 3x3 neighborhood
    for t in 1..num_frames - 1 {
        // Only consider bins up to 4kHz for stability in 10-bit quantization
        let max_bin = (4000.0 / bin_width) as usize;
        let end_bin = num_bins.min(max_bin).saturating_sub(1);

        for f in 1..end_bin {
            let val = spectrogram[t][f];
            if val > 0.01 && // Minimum magnitude threshold
               val > spectrogram[t-1][f-1] && val > spectrogram[t-1][f] && val > spectrogram[t-1][f+1] &&
               val > spectrogram[t][f-1]   &&                          val > spectrogram[t][f+1]   &&
               val > spectrogram[t+1][f-1] && val > spectrogram[t+1][f] && val > spectrogram[t+1][f+1]
            {
                peaks.push(Peak {
                    time_ms: t as u32 * ms_per_hop,
                    freq_hz: f as f32 * bin_width,
                });
            }
        }
    }

    // Sort peaks by magnitude if we wanted to limit number of peaks,
    // but for now let's keep all local maxima.
    // Wang paper suggests around 30 peaks per second.

    peaks
}

fn generate_hashes(peaks: &[Peak]) -> Vec<Fingerprint> {
    let mut fingerprints = Vec::new();

    for (i, anchor) in peaks.iter().enumerate() {
        let mut targets_found = 0;
        for target in peaks.iter().skip(i + 1) {
            let delta_t = target.time_ms.saturating_sub(anchor.time_ms);

            if delta_t > MAX_DELTA_T_MS {
                break; // Peaks are sorted by time, so we can break
            }

            if delta_t < MIN_DELTA_T_MS {
                continue;
            }

            // pack(f_anchor, f_target, Δt)
            // [10 bits: f_anchor / 4Hz] [10 bits: f_target / 4Hz] [12 bits: Δt in ms]
            let f_anchor_q = ((anchor.freq_hz / 4.0) as u32).min(1023);
            let f_target_q = ((target.freq_hz / 4.0) as u32).min(1023);
            let dt_q = delta_t.min(4095);

            let hash = (f_anchor_q << 22) | (f_target_q << 12) | dt_q;

            fingerprints.push(Fingerprint {
                hash,
                offset_ms: anchor.time_ms,
            });

            targets_found += 1;
            if targets_found >= FAN_OUT {
                break;
            }
        }
    }

    fingerprints
}

/// Matches query fingerprints against a set of stored fingerprints and returns a map of segment_id to best match score.
/// score is the peak of the Δt histogram.
pub fn match_fingerprints(
    query_fps: &[Fingerprint],
    stored_fps: Vec<(i64, u32, u32)>, // (segment_id, hash, offset_ms)
) -> HashMap<i64, u32> {
    let mut segment_histograms: HashMap<i64, HashMap<i32, u32>> = HashMap::new();

    // Organize stored fingerprints by hash for faster lookup
    let mut hash_to_stored: HashMap<u32, Vec<(i64, u32)>> = HashMap::new();
    for (seg_id, hash, offset_ms) in stored_fps {
        hash_to_stored
            .entry(hash)
            .or_default()
            .push((seg_id, offset_ms));
    }

    for q_fp in query_fps {
        if let Some(matches) = hash_to_stored.get(&q_fp.hash) {
            for (seg_id, s_offset) in matches {
                let diff = *s_offset as i32 - q_fp.offset_ms as i32;
                let histogram = segment_histograms.entry(*seg_id).or_default();
                *histogram.entry(diff).or_insert(0) += 1;
            }
        }
    }

    let mut results = HashMap::new();
    for (seg_id, histogram) in segment_histograms {
        let max_count = histogram.values().cloned().max().unwrap_or(0);
        results.insert(seg_id, max_count);
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fingerprint_empty() {
        let fps = fingerprint_segment(&[], 16000.0);
        assert!(fps.is_empty());
    }

    #[test]
    fn test_fingerprint_sine() {
        let sample_rate = 16000.0;
        let duration_s = 2.0;
        let mut samples = vec![0.0f32; (sample_rate * duration_s) as usize];
        for (i, s) in samples.iter_mut().enumerate() {
            *s = (i as f32 * 2.0 * std::f32::consts::PI * 440.0 / sample_rate).sin();
        }

        let fps = fingerprint_segment(&samples, sample_rate);
        // A pure sine wave might not have many "peaks" if it's too stable,
        // but it should have at least some due to windowing and noise.
        // Actually it might have a peak at 440Hz in every frame.
        assert!(!fps.is_empty());
    }
}
