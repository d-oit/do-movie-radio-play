use anyhow::{Context, Result};
use rodio::source::noise::{Pink, WhiteUniform};
use rodio::Source;
use serde::{Deserialize, Serialize};
use std::num::NonZeroU32;

/// Type of noise to generate.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoiseType {
    White,
    Pink,
}

/// Configuration for noise track generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoiseTrackConfig {
    pub noise_type: NoiseType,
    pub duration_ms: u64,
    pub sample_rate: u32,
    #[serde(default = "default_volume")]
    pub volume: f32,
    #[serde(default)]
    pub low_pass_hz: Option<u32>,
}

fn default_volume() -> f32 {
    1.0
}

/// Generate noise samples based on the provided configuration.
pub fn generate_noise_samples(cfg: &NoiseTrackConfig) -> Result<Vec<f32>> {
    if cfg.volume == 0.0 {
        let num_samples = (cfg.duration_ms * cfg.sample_rate as u64 / 1000) as usize;
        return Ok(vec![0.0; num_samples]);
    }

    let sample_rate_nz =
        NonZeroU32::new(cfg.sample_rate).context("sample_rate must be non-zero")?;
    let num_samples = (cfg.duration_ms * cfg.sample_rate as u64 / 1000) as usize;

    let samples = match cfg.noise_type {
        NoiseType::White => collect_noise::<WhiteUniform>(
            WhiteUniform::new(sample_rate_nz),
            num_samples,
            cfg.low_pass_hz,
        ),
        NoiseType::Pink => {
            collect_noise::<Pink>(Pink::new(sample_rate_nz), num_samples, cfg.low_pass_hz)
        }
    };

    if cfg.volume != 1.0 {
        Ok(samples.into_iter().map(|s| s * cfg.volume).collect())
    } else {
        Ok(samples)
    }
}

fn collect_noise<S: Source<Item = f32>>(
    source: S,
    num_samples: usize,
    low_pass_hz: Option<u32>,
) -> Vec<f32> {
    if let Some(lp) = low_pass_hz {
        source.low_pass(lp).take(num_samples).collect()
    } else {
        source.take(num_samples).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn white_noise_length_matches_duration() -> Result<()> {
        let cfg = NoiseTrackConfig {
            noise_type: NoiseType::White,
            duration_ms: 1000,
            sample_rate: 44100,
            volume: 1.0,
            low_pass_hz: None,
        };
        let samples = generate_noise_samples(&cfg)?;
        assert_eq!(samples.len(), 44100);
        Ok(())
    }

    #[test]
    fn pink_noise_with_low_pass_is_finite() -> Result<()> {
        let cfg = NoiseTrackConfig {
            noise_type: NoiseType::Pink,
            duration_ms: 500,
            sample_rate: 44100,
            volume: 0.8,
            low_pass_hz: Some(2000),
        };
        let samples = generate_noise_samples(&cfg)?;
        assert!(samples.iter().all(|s| s.is_finite()));
        Ok(())
    }

    #[test]
    fn zero_volume_is_silent() -> Result<()> {
        let cfg = NoiseTrackConfig {
            noise_type: NoiseType::White,
            duration_ms: 500,
            sample_rate: 44100,
            volume: 0.0,
            low_pass_hz: None,
        };
        let samples = generate_noise_samples(&cfg)?;
        assert!(samples.iter().all(|s| *s == 0.0));
        Ok(())
    }
}
