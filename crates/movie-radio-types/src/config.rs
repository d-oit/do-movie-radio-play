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

impl VoiceSynthesisConfig {
    /// Builds a consolidated `VoiceSynthesisConfig` from `AnalysisConfig`.
    ///
    /// If `cfg.voice_synthesis` is provided, its settings are used as base.
    /// Missing provider configurations are automatically populated from the
    /// environment when available (`ELEVENLABS_API_KEY`, `MODAL_TTS_ENDPOINT`,
    /// `OPENAI_API_KEY` / `OPENAI_TTS_BASE_URL`, `AudioCppConfig`).
    /// The language defaults to `"de"` if empty.
    pub fn from_analysis_config(cfg: &AnalysisConfig) -> Self {
        let mut voice_cfg = cfg.voice_synthesis.clone().unwrap_or_else(|| Self {
            provider: "modal".to_string(),
            fallback_chain: vec![
                "audio_cpp".to_string(),
                "modal".to_string(),
                "elevenlabs".to_string(),
                "openai".to_string(),
            ],
            emotion_mapping: true,
            language: "de".to_string(),
            voice_id: None,
            max_cost_per_run_usd: 25.0,
            providers: VoiceProvidersConfig::default(),
        });

        if voice_cfg.language.trim().is_empty() {
            voice_cfg.language = "de".to_string();
        }

        if voice_cfg.providers.elevenlabs.is_none() && std::env::var("ELEVENLABS_API_KEY").is_ok() {
            voice_cfg.providers.elevenlabs = Some(ElevenLabsConfig {
                api_key_env: "ELEVENLABS_API_KEY".to_string(),
                voice_id: "pNInz6obpgDQGcFmaJgB".to_string(),
                model: "eleven_multilingual_v2".to_string(),
                stability: 0.5,
                similarity_boost: 0.75,
            });
        }

        if voice_cfg.providers.modal.is_none()
            && (std::env::var("MODAL_TTS_ENDPOINT").is_ok() || cfg.voice_synthesis.is_none())
        {
            voice_cfg.providers.modal = Some(ModalConfig {
                endpoint_url_env: "MODAL_TTS_ENDPOINT".to_string(),
                max_monthly_cost: 25.0,
            });
        }

        if voice_cfg.providers.openai.is_none() {
            if std::env::var("OPENAI_API_KEY").is_ok() {
                voice_cfg.providers.openai = Some(OpenAiConfig {
                    api_key_env: Some("OPENAI_API_KEY".to_string()),
                    base_url: default_openai_base_url(),
                    model: "tts-1-hd".to_string(),
                    voice: "onyx".to_string(),
                    response_format: "mp3".to_string(),
                });
            } else if let Ok(base_url) = std::env::var("OPENAI_TTS_BASE_URL") {
                voice_cfg.providers.openai = Some(OpenAiConfig {
                    api_key_env: None,
                    base_url,
                    model: "pocket-tts".to_string(),
                    voice: "alba".to_string(),
                    response_format: "wav".to_string(),
                });
            }
        }

        if voice_cfg.providers.audio_cpp.is_none() {
            voice_cfg.providers.audio_cpp = Some(AudioCppConfig::default());
        }

        voice_cfg
    }
}

impl Default for VoiceSynthesisConfig {
    fn default() -> Self {
        Self::from_analysis_config(&AnalysisConfig::default())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VoiceProvidersConfig {
    #[serde(default)]
    pub kokoro: Option<KokoroConfig>,
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
pub struct ModalConfig {
    pub endpoint_url_env: String,
    pub max_monthly_cost: f64,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_voice_synthesis_config_defaults() {
        let analysis_cfg = AnalysisConfig::default();
        let voice_cfg = VoiceSynthesisConfig::from_analysis_config(&analysis_cfg);

        assert_eq!(voice_cfg.language, "de");
        assert_eq!(voice_cfg.voice_id, None);
        assert_eq!(
            voice_cfg.fallback_chain,
            vec!["audio_cpp", "modal", "elevenlabs", "openai"]
        );
        assert!(voice_cfg.providers.audio_cpp.is_some());
    }

    #[test]
    fn test_voice_synthesis_config_custom_override() {
        let analysis_cfg = AnalysisConfig {
            voice_synthesis: Some(VoiceSynthesisConfig {
                provider: "elevenlabs".to_string(),
                fallback_chain: vec!["elevenlabs".to_string()],
                emotion_mapping: false,
                language: "en".to_string(),
                voice_id: Some("custom_voice_123".to_string()),
                max_cost_per_run_usd: 10.0,
                providers: VoiceProvidersConfig::default(),
            }),
            ..Default::default()
        };

        let voice_cfg = VoiceSynthesisConfig::from_analysis_config(&analysis_cfg);

        assert_eq!(voice_cfg.provider, "elevenlabs");
        assert_eq!(voice_cfg.language, "en");
        assert_eq!(voice_cfg.voice_id.as_deref(), Some("custom_voice_123"));
        assert_eq!(voice_cfg.fallback_chain, vec!["elevenlabs"]);
    }
}
