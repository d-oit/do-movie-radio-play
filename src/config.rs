use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisConfig {
    pub sample_rate_hz: u32,
    pub frame_ms: u32,
    pub speech_hangover_ms: u32,
    pub merge_gap_ms: u32,
    pub min_speech_ms: u32,
    pub min_non_voice_ms: u32,
    pub energy_threshold: f32,
    pub prompt_min_duration_ms: u64,
    pub prompt_min_confidence: f32,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            sample_rate_hz: 16000,
            frame_ms: 20,
            speech_hangover_ms: 300,
            merge_gap_ms: 250,
            min_speech_ms: 120,
            min_non_voice_ms: 1000,
            energy_threshold: 0.015,
            prompt_min_duration_ms: 2500,
            prompt_min_confidence: 0.65,
        }
    }
}

pub fn load_config(path: Option<PathBuf>) -> Result<AnalysisConfig> {
    let cfg = if let Some(path) = path {
        let data = fs::read_to_string(&path)?;
        serde_json::from_str(&data)?
    } else {
        AnalysisConfig::default()
    };
    validate(&cfg)?;
    Ok(cfg)
}

fn validate(cfg: &AnalysisConfig) -> Result<()> {
    if cfg.sample_rate_hz == 0 || cfg.frame_ms == 0 {
        bail!("invalid config: sample_rate_hz and frame_ms must be > 0");
    }
    if cfg.prompt_min_confidence <= 0.0 || cfg.prompt_min_confidence > 1.0 {
        bail!("invalid config: prompt_min_confidence must be in (0, 1]");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_valid() {
        assert!(validate(&AnalysisConfig::default()).is_ok());
    }
}
