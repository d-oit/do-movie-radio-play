use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
    pub modal: Option<ModalConfig>,
    #[serde(default)]
    pub openai: Option<OpenAiConfig>,
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

fn default_true() -> bool {
    true
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiConfig {
    /// Environment variable holding the bearer token. `None` disables the
    /// Authorization header entirely — for OpenAI-compatible local servers
    /// such as an audio.cpp sidecar.
    #[serde(default)]
    pub api_key_env: Option<String>,
    /// API root. Defaults to the public OpenAI API; point it at a local
    /// OpenAI-compatible TTS server (e.g. audio.cpp) to switch engines.
    #[serde(default = "default_openai_base_url")]
    pub base_url: String,
    pub model: String,
    pub voice: String,
    pub response_format: String,
}

pub fn default_openai_base_url() -> String {
    "https://api.openai.com/v1".to_string()
}
