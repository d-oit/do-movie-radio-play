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
    pub kokoro: Option<KokoroConfig>,
    pub pockettts: Option<PocketTtsConfig>,
    pub qwen3: Option<Qwen3Config>,
    pub orpheus: Option<OrpheusConfig>,
    pub elevenlabs: Option<ElevenLabsConfig>,
    pub modal: Option<ModalConfig>,
    pub openai: Option<OpenAiConfig>,
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
