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
    #[serde(default)]
    pub sound_effects: Option<crate::sfx_types::SoundEffectsConfig>,
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
    #[serde(default)]
    pub kokoro: Option<KokoroConfig>,
    #[serde(default)]
    pub pockettts: Option<PocketTtsConfig>,
    #[serde(default)]
    pub qwen3: Option<Qwen3Config>,
    #[serde(default)]
    pub orpheus: Option<OrpheusConfig>,
    #[serde(default)]
    pub elevenlabs: Option<ElevenLabsConfig>,
    #[serde(default)]
    pub audio_cpp: Option<AudioCppConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioCppConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_audio_cpp_mode")]
    pub mode: String,
    #[serde(default)]
    pub local: AudioCppLocalConfig,
    #[serde(default)]
    pub remote: AudioCppRemoteConfig,
    #[serde(default = "default_audio_cpp_family")]
    pub family: String,
    #[serde(default)]
    pub model: String,
    #[serde(default = "default_audio_cpp_backend")]
    pub backend: String,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default)]
    pub voice_id: Option<String>,
    #[serde(default)]
    pub voice_ref: Option<String>,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    #[serde(default)]
    pub gpu_pool: Vec<GpuPoolEndpoint>,
    #[serde(default)]
    pub gpu_policy: GpuPolicyConfig,
}

impl Default for AudioCppConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            mode: default_audio_cpp_mode(),
            local: AudioCppLocalConfig::default(),
            remote: AudioCppRemoteConfig::default(),
            family: default_audio_cpp_family(),
            model: "models/Qwen3-TTS-12Hz-1.7B-Base".to_string(),
            backend: default_audio_cpp_backend(),
            language: default_language(),
            voice_id: None,
            voice_ref: None,
            timeout_secs: default_timeout_secs(),
            gpu_pool: Vec::default(),
            gpu_policy: GpuPolicyConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioCppLocalConfig {
    #[serde(default = "default_local_mode")]
    pub mode: String,
    #[serde(default = "default_local_binary")]
    pub binary: String,
    #[serde(default = "default_local_server_url")]
    pub server_url: String,
}

impl Default for AudioCppLocalConfig {
    fn default() -> Self {
        Self {
            mode: default_local_mode(),
            binary: default_local_binary(),
            server_url: default_local_server_url(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioCppRemoteConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub server_url: String,
    #[serde(default)]
    pub auth_env: Option<String>,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
}

impl Default for AudioCppRemoteConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            server_url: String::default(),
            auth_env: None,
            timeout_secs: default_timeout_secs(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuPoolEndpoint {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub auth_env: Option<String>,
    #[serde(default)]
    pub priority: u32,
    #[serde(default)]
    pub cost_per_hour: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuPolicyConfig {
    #[serde(default = "default_true")]
    pub prefer_free: bool,
    #[serde(default)]
    pub allow_paid: bool,
    #[serde(default = "default_max_cost_job")]
    pub max_cost_per_job: f64,
    #[serde(default = "default_max_cost_day")]
    pub max_cost_per_day: f64,
}

impl Default for GpuPolicyConfig {
    fn default() -> Self {
        Self {
            prefer_free: true,
            allow_paid: false,
            max_cost_per_job: default_max_cost_job(),
            max_cost_per_day: default_max_cost_day(),
        }
    }
}

fn default_audio_cpp_mode() -> String {
    "auto".to_string()
}

fn default_audio_cpp_family() -> String {
    "qwen3_tts".to_string()
}

fn default_audio_cpp_backend() -> String {
    "best".to_string()
}

fn default_language() -> String {
    "de".to_string()
}

fn default_timeout_secs() -> u64 {
    300
}

fn default_local_mode() -> String {
    "server".to_string()
}

fn default_local_binary() -> String {
    "audiocpp_cli".to_string()
}

fn default_local_server_url() -> String {
    "http://127.0.0.1:8080".to_string()
}

fn default_max_cost_job() -> f64 {
    0.50
}

fn default_max_cost_day() -> f64 {
    5.00
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
            sound_effects: None,
        }
    }
}

fn default_true() -> bool {
    true
}
