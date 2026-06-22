use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{env, fmt, fs, path::PathBuf};

const VALID_VAD_ENGINES: [&str; 3] = ["energy", "spectral", "hybrid"];

/// Merge strategy for combining non-voice segments.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MergeStrategy {
    All,
    Longest,
    Sparse,
}

impl fmt::Display for MergeStrategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MergeStrategy::All => write!(f, "all"),
            MergeStrategy::Longest => write!(f, "longest"),
            MergeStrategy::Sparse => write!(f, "sparse"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MergeOptions {
    pub min_gap_to_merge: u32,
    pub merge_strategy: MergeStrategy,
    pub min_speech_duration: u32,
    pub min_silence_duration: u32,
    pub silence_threshold_db: i32,
}

impl Default for MergeOptions {
    fn default() -> Self {
        Self {
            min_gap_to_merge: 400,
            merge_strategy: MergeStrategy::All,
            min_speech_duration: 250,
            min_silence_duration: 300,
            silence_threshold_db: -42,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisConfig {
    pub sample_rate_hz: u32,
    pub frame_ms: u32,
    pub speech_hangover_ms: u32,
    pub merge_gap_ms: u32,
    pub min_speech_ms: u32,
    pub min_non_voice_ms: u32,
    pub max_non_voice_ms: Option<u32>,
    pub energy_threshold: f32,
    pub vad_threshold_delta: f32,
    pub prompt_min_duration_ms: u64,
    pub prompt_min_confidence: f32,
    pub vad_engine: String,
    #[serde(default = "default_true")]
    pub parallel_features: bool,
    #[serde(default)]
    pub merge_options: Option<MergeOptions>,
    #[serde(default)]
    pub spectral_flatness_max: Option<f32>,
    #[serde(default)]
    pub spectral_entropy_min: Option<f32>,
    #[serde(default)]
    pub spectral_centroid_min: Option<f32>,
    #[serde(default)]
    pub spectral_centroid_max: Option<f32>,
    #[serde(default)]
    pub voice_synthesis: Option<VoiceSynthesisConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceSynthesisConfig {
    pub provider: String,
    pub fallback_chain: Vec<String>,
    pub emotion_mapping: bool,
    pub language: String,
    pub voice_id: Option<String>,
    pub max_cost_per_run_usd: f64,
    pub providers: VoiceProvidersConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceProvidersConfig {
    pub kokoro: Option<KokoroConfig>,
    pub pockettts: Option<PocketTtsConfig>,
    pub qwen3: Option<Qwen3Config>,
    pub orpheus: Option<OrpheusConfig>,
    pub elevenlabs: Option<ElevenLabsConfig>,
    pub modal: Option<ModalConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModalConfig {
    pub endpoint_url_env: String,
    pub max_monthly_cost: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KokoroConfig {
    pub model_path: PathBuf,
    pub device: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PocketTtsConfig {
    pub model_path: PathBuf,
    pub device: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Qwen3Config {
    pub model_path: PathBuf,
    pub vocoder_path: PathBuf,
    pub device: String,
    pub voice_description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrpheusConfig {
    pub model_path: PathBuf,
    pub device: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElevenLabsConfig {
    pub api_key_env: String,
    pub voice_id: String,
    pub model: String,
    pub stability: f32,
    pub similarity_boost: f32,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            sample_rate_hz: 16000,
            frame_ms: 20,
            speech_hangover_ms: 300,
            merge_gap_ms: 250,
            min_speech_ms: 120,
            min_non_voice_ms: 10000,
            max_non_voice_ms: None,
            energy_threshold: 0.015,
            vad_threshold_delta: 0.0,
            prompt_min_duration_ms: 2500,
            prompt_min_confidence: 0.65,
            vad_engine: "energy".to_string(),
            parallel_features: true,
            merge_options: None,
            spectral_flatness_max: None,
            spectral_entropy_min: None,
            spectral_centroid_min: None,
            spectral_centroid_max: None,
            voice_synthesis: None,
        }
    }
}

impl AnalysisConfig {
    #[allow(clippy::too_many_arguments)]
    pub fn from_args(
        config_path: Option<PathBuf>,
        threshold_override: Option<f32>,
        min_speech_override: Option<u32>,
        min_silence_override: Option<u32>,
        max_non_voice_override: Option<u32>,
        vad_engine_override: Option<String>,
        threshold_delta_override: Option<f32>,
        parallel_features_override: Option<bool>,
    ) -> Result<Self> {
        let cfg = if let Some(path) = config_path {
            let data = fs::read_to_string(&path).context("failed to read config file")?;
            serde_json::from_str(&data).context("failed to parse config file")?
        } else {
            Self::default()
        };

        let mut cfg = apply_env_overrides(cfg)?;
        if let Some(t) = threshold_override {
            cfg.energy_threshold = t;
        }
        if let Some(ms) = min_speech_override {
            cfg.min_speech_ms = ms;
        }
        if let Some(ms) = min_silence_override {
            cfg.min_non_voice_ms = ms;
        }
        if let Some(max) = max_non_voice_override {
            cfg.max_non_voice_ms = Some(max);
        }
        if let Some(engine) = vad_engine_override {
            cfg.vad_engine = engine;
        }
        if let Some(d) = threshold_delta_override {
            cfg.vad_threshold_delta = d;
        }
        if let Some(p) = parallel_features_override {
            cfg.parallel_features = p;
        }

        validate(&cfg)?;
        Ok(cfg)
    }
}

fn apply_env_overrides(mut cfg: AnalysisConfig) -> Result<AnalysisConfig> {
    if let Ok(v) = env::var("TIMELINE_SAMPLE_RATE") {
        cfg.sample_rate_hz = parse_env_value("TIMELINE_SAMPLE_RATE", &v)?;
    }
    if let Ok(v) = env::var("TIMELINE_FRAME_MS") {
        cfg.frame_ms = parse_env_value("TIMELINE_FRAME_MS", &v)?;
    }
    if let Ok(v) = env::var("TIMELINE_MIN_SPEECH_MS") {
        cfg.min_speech_ms = parse_env_value("TIMELINE_MIN_SPEECH_MS", &v)?;
    }
    if let Ok(v) = env::var("TIMELINE_MIN_SILENCE_MS") {
        cfg.min_non_voice_ms = parse_env_value("TIMELINE_MIN_SILENCE_MS", &v)?;
    }
    if let Ok(v) = env::var("TIMELINE_ENERGY_THRESHOLD") {
        cfg.energy_threshold = parse_env_value("TIMELINE_ENERGY_THRESHOLD", &v)?;
    }
    if let Ok(v) = env::var("TIMELINE_PARALLEL_FEATURES") {
        cfg.parallel_features = parse_env_value("TIMELINE_PARALLEL_FEATURES", &v)?;
    }
    Ok(cfg)
}

fn default_true() -> bool {
    true
}

fn parse_env_value<T>(key: &str, value: &str) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    value
        .parse::<T>()
        .map_err(|err| anyhow::anyhow!("invalid env var {key}={value:?}: {err}"))
}

fn validate(cfg: &AnalysisConfig) -> Result<()> {
    if cfg.sample_rate_hz == 0 {
        bail!("invalid config: sample_rate_hz must be > 0");
    }
    if cfg.frame_ms == 0 {
        bail!("invalid config: frame_ms must be > 0");
    }
    if cfg.speech_hangover_ms < cfg.frame_ms {
        bail!("invalid config: speech_hangover_ms must be >= frame_ms");
    }
    if cfg.min_speech_ms < cfg.frame_ms {
        bail!("invalid config: min_speech_ms must be >= frame_ms");
    }
    if cfg.min_non_voice_ms < cfg.frame_ms {
        bail!("invalid config: min_non_voice_ms must be >= frame_ms");
    }
    if let Some(max) = cfg.max_non_voice_ms {
        if max < cfg.frame_ms {
            bail!("invalid config: max_non_voice_ms must be >= frame_ms");
        }
        if cfg.min_non_voice_ms > max {
            bail!("invalid config: min_non_voice_ms must be <= max_non_voice_ms");
        }
    }
    if !(0.0..=1.0).contains(&cfg.energy_threshold) {
        bail!("invalid config: energy_threshold must be in [0, 1]");
    }
    if !(-1.0..=1.0).contains(&cfg.vad_threshold_delta) {
        bail!("invalid config: vad_threshold_delta must be in [-1, 1]");
    }
    if cfg.prompt_min_confidence <= 0.0 || cfg.prompt_min_confidence > 1.0 {
        bail!("invalid config: prompt_min_confidence must be in (0, 1]");
    }
    if !VALID_VAD_ENGINES.contains(&cfg.vad_engine.as_str()) {
        bail!(
            "invalid config: vad_engine must be one of {}",
            VALID_VAD_ENGINES.join(", ")
        );
    }
    if let Some(ref merge_opts) = cfg.merge_options {
        validate_merge_options(merge_opts, cfg.frame_ms)?;
    }
    Ok(())
}

fn validate_merge_options(opts: &MergeOptions, frame_ms: u32) -> Result<()> {
    if opts.min_gap_to_merge < frame_ms {
        bail!("invalid merge_options: min_gap_to_merge must be >= frame_ms");
    }
    if opts.min_speech_duration < frame_ms {
        bail!("invalid merge_options: min_speech_duration must be >= frame_ms");
    }
    if opts.min_silence_duration < frame_ms {
        bail!("invalid merge_options: min_silence_duration must be >= frame_ms");
    }
    if !(opts.silence_threshold_db >= -80 && opts.silence_threshold_db <= -20) {
        bail!("invalid merge_options: silence_threshold_db must be in [-80, -20]");
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
            Some(30000),
            Some("energy".to_string()),
            Some(0.01),
            Some(false),
        )
        .unwrap();
        assert_eq!(cfg.energy_threshold, 0.5);
        assert!(!cfg.parallel_features);
        assert_eq!(cfg.min_speech_ms, 500);
        assert_eq!(cfg.min_non_voice_ms, 2000);
        assert_eq!(cfg.max_non_voice_ms, Some(30000));
        assert_eq!(cfg.vad_engine, "energy");
        assert_eq!(cfg.vad_threshold_delta, 0.01);
    }

    #[test]
    fn hybrid_vad_engine_is_accepted() {
        let cfg = AnalysisConfig {
            vad_engine: "hybrid".to_string(),
            ..AnalysisConfig::default()
        };
        assert!(validate(&cfg).is_ok());
    }

    #[test]
    fn invalid_energy_threshold_is_rejected() {
        let cfg = AnalysisConfig {
            energy_threshold: 1.5,
            ..AnalysisConfig::default()
        };
        assert!(validate(&cfg).is_err());
    }

    #[test]
    fn min_non_voice_must_be_at_least_frame_ms() {
        let cfg = AnalysisConfig {
            frame_ms: 20,
            min_non_voice_ms: 10,
            ..AnalysisConfig::default()
        };
        assert!(validate(&cfg).is_err());
    }

    #[test]
    fn max_non_voice_must_be_at_least_frame_ms() {
        let cfg = AnalysisConfig {
            frame_ms: 20,
            max_non_voice_ms: Some(10),
            ..AnalysisConfig::default()
        };
        assert!(validate(&cfg).is_err());
    }

    #[test]
    fn min_must_not_exceed_max() {
        let cfg = AnalysisConfig {
            min_non_voice_ms: 15000,
            max_non_voice_ms: Some(10000),
            ..AnalysisConfig::default()
        };
        assert!(validate(&cfg).is_err());
    }

    #[test]
    fn invalid_vad_engine_is_rejected() {
        let cfg = AnalysisConfig {
            vad_engine: "bogus".to_string(),
            ..AnalysisConfig::default()
        };
        assert!(validate(&cfg).is_err());
    }
}
