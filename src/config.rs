use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{env, fs, path::PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisConfig {
    pub sample_rate_hz: u32,
    pub frame_ms: u32,
    pub speech_hangover_ms: u32,
    pub merge_gap_ms: u32,
    pub min_speech_ms: u32,
    pub min_non_voice_ms: u32,
    pub energy_threshold: f32,
    pub vad_threshold_delta: f32,
    pub prompt_min_duration_ms: u64,
    pub prompt_min_confidence: f32,
    pub vad_engine: String,
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
            vad_threshold_delta: 0.0,
            prompt_min_duration_ms: 2500,
            prompt_min_confidence: 0.65,
            vad_engine: "energy".to_string(),
        }
    }
}

impl AnalysisConfig {
    pub fn from_args(
        config_path: Option<PathBuf>,
        threshold_override: Option<f32>,
        min_speech_override: Option<u32>,
        min_silence_override: Option<u32>,
        vad_engine_override: Option<String>,
        threshold_delta_override: Option<f32>,
    ) -> Result<Self> {
        let cfg = if let Some(path) = config_path {
            let data = fs::read_to_string(&path).context("failed to read config file")?;
            serde_json::from_str(&data).context("failed to parse config file")?
        } else {
            Self::default()
        };

        let mut cfg = apply_env_overrides(cfg);
        if let Some(t) = threshold_override {
            cfg.energy_threshold = t;
        }
        if let Some(ms) = min_speech_override {
            cfg.min_speech_ms = ms;
        }
        if let Some(ms) = min_silence_override {
            cfg.min_non_voice_ms = ms;
        }
        if let Some(engine) = vad_engine_override {
            cfg.vad_engine = engine;
        }
        if let Some(d) = threshold_delta_override {
            cfg.vad_threshold_delta = d;
        }

        validate(&cfg)?;
        Ok(cfg)
    }
}

fn apply_env_overrides(mut cfg: AnalysisConfig) -> AnalysisConfig {
    if let Ok(v) = env::var("TIMELINE_SAMPLE_RATE") {
        if let Ok(sr) = v.parse() {
            cfg.sample_rate_hz = sr;
        }
    }
    if let Ok(v) = env::var("TIMELINE_FRAME_MS") {
        if let Ok(fm) = v.parse() {
            cfg.frame_ms = fm;
        }
    }
    if let Ok(v) = env::var("TIMELINE_MIN_SPEECH_MS") {
        if let Ok(ms) = v.parse() {
            cfg.min_speech_ms = ms;
        }
    }
    if let Ok(v) = env::var("TIMELINE_MIN_SILENCE_MS") {
        if let Ok(ms) = v.parse() {
            cfg.min_non_voice_ms = ms;
        }
    }
    if let Ok(v) = env::var("TIMELINE_ENERGY_THRESHOLD") {
        if let Ok(t) = v.parse() {
            cfg.energy_threshold = t;
        }
    }
    cfg
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

    #[test]
    fn from_args_with_overrides() {
        let cfg = AnalysisConfig::from_args(
            None,
            Some(0.5),
            Some(500),
            Some(2000),
            Some("silero".to_string()),
            Some(0.01),
        )
        .unwrap();
        assert_eq!(cfg.energy_threshold, 0.5);
        assert_eq!(cfg.min_speech_ms, 500);
        assert_eq!(cfg.min_non_voice_ms, 2000);
        assert_eq!(cfg.vad_engine, "silero");
        assert_eq!(cfg.vad_threshold_delta, 0.01);
    }
}
