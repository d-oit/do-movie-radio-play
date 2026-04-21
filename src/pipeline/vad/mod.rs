mod energy;
mod engine;
mod spectral;

use anyhow::{bail, Result};

use crate::types::frame::Frame;

pub use energy::EnergyVad;
pub use engine::VadEngine;
pub use spectral::SpectralVad;

const MIN_ADAPTIVE_THRESHOLD: f32 = 0.0001;
const MAX_ADAPTIVE_THRESHOLD: f32 = 0.05;

#[derive(Debug, Clone, Copy)]
pub struct SpectralThresholds {
    pub threshold: f32,
    pub flatness_max: f32,
    pub entropy_min: f32,
    pub centroid_min: f32,
    pub centroid_max: f32,
}

pub fn create_engine(
    name: &str,
    threshold: f32,
    flatness_max: Option<f32>,
    entropy_min: Option<f32>,
    centroid_min: Option<f32>,
    centroid_max: Option<f32>,
) -> Result<Box<dyn VadEngine>> {
    match name {
        "energy" => Ok(Box::new(EnergyVad::new(threshold))),
        "spectral" => {
            let engine = match (flatness_max, entropy_min, centroid_min, centroid_max) {
                (Some(f), Some(e), Some(c_min), Some(c_max)) => {
                    SpectralVad::with_thresholds(threshold, f, e, c_min, c_max)
                }
                _ => SpectralVad::new(threshold),
            };
            Ok(Box::new(engine))
        }
        _ => bail!("unknown VAD engine '{name}'"),
    }
}

pub fn adapt_spectral_thresholds(
    frames: &[Frame],
    threshold: f32,
    flatness_max: Option<f32>,
    entropy_min: Option<f32>,
    centroid_min: Option<f32>,
    centroid_max: Option<f32>,
) -> SpectralThresholds {
    let base_flatness = flatness_max.unwrap_or(0.45);
    let base_entropy = entropy_min.unwrap_or(3.5);
    let base_centroid_min = centroid_min.unwrap_or(100.0);
    let base_centroid_max = centroid_max.unwrap_or(6000.0);
    if frames.is_empty() {
        return SpectralThresholds {
            threshold,
            flatness_max: base_flatness,
            entropy_min: base_entropy,
            centroid_min: base_centroid_min,
            centroid_max: base_centroid_max,
        };
    }

    let rms_p35 = percentile(frames.iter().map(|f| f.rms).collect(), 0.35);
    let flatness_p60 = percentile(frames.iter().map(|f| f.spectral_flatness).collect(), 0.60);
    let entropy_p60 = percentile(frames.iter().map(|f| f.spectral_entropy).collect(), 0.60);
    let centroid_p10 = percentile(frames.iter().map(|f| f.centroid_hz).collect(), 0.10);
    let centroid_p90 = percentile(frames.iter().map(|f| f.centroid_hz).collect(), 0.90);

    let adapted_threshold = threshold
        .min((rms_p35 * 0.85).max(MIN_ADAPTIVE_THRESHOLD))
        .clamp(MIN_ADAPTIVE_THRESHOLD, MAX_ADAPTIVE_THRESHOLD);
    let adapted_flatness = base_flatness.max(flatness_p60 * 1.15).clamp(0.15, 0.8);
    let adapted_entropy = base_entropy.max(entropy_p60 - 1.5).clamp(1.0, 7.5);
    let adapted_centroid_min = base_centroid_min.min(centroid_p10 * 0.8).clamp(0.0, 4000.0);
    let adapted_centroid_max = base_centroid_max
        .max(centroid_p90 * 1.1)
        .clamp(1000.0, 8000.0);

    SpectralThresholds {
        threshold: adapted_threshold,
        flatness_max: adapted_flatness,
        entropy_min: adapted_entropy,
        centroid_min: adapted_centroid_min,
        centroid_max: adapted_centroid_max.max(adapted_centroid_min + 100.0),
    }
}

fn percentile(mut values: Vec<f32>, q: f32) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let idx = ((values.len() - 1) as f32 * q.clamp(0.0, 1.0)).round() as usize;
    values[idx]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(rms: f32, flatness: f32, entropy: f32, centroid_hz: f32) -> Frame {
        Frame {
            rms,
            zcr: 0.1,
            spectral_flux: 0.01,
            spectral_flatness: flatness,
            spectral_entropy: entropy,
            centroid_hz,
            low_band_ratio: 0.2,
            high_band_ratio: 0.2,
        }
    }

    #[test]
    fn adaptive_thresholds_relax_for_noisy_frames() {
        let frames = vec![
            frame(0.01, 0.5, 5.8, 1600.0),
            frame(0.012, 0.55, 6.1, 1800.0),
            frame(0.014, 0.52, 5.9, 2000.0),
            frame(0.016, 0.48, 5.4, 2200.0),
        ];
        let adapted = adapt_spectral_thresholds(
            &frames,
            0.015,
            Some(0.38),
            Some(3.0),
            Some(180.0),
            Some(3800.0),
        );
        assert!(adapted.threshold <= 0.015);
        assert!(adapted.flatness_max >= 0.38);
        assert!(adapted.entropy_min >= 3.0);
        assert!(adapted.centroid_max >= 3800.0);
    }
}
