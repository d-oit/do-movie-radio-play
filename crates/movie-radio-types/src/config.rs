use serde::{Deserialize, Serialize};
use std::{fmt, path::PathBuf};

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
    #[serde(default)]
    pub chunk_duration_sec: Option<u64>,
    #[serde(default)]
    pub profile_id: Option<String>,
    #[serde(default)]
    pub version: Option<u32>,
    #[serde(default)]
    pub experiment_tags: Vec<String>,
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
            chunk_duration_sec: None,
            profile_id: None,
            version: None,
            experiment_tags: vec![],
        }
    }
}

fn default_true() -> bool {
    true
}
