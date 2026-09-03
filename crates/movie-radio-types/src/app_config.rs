use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::config::{AudioCppConfig, GpuPolicyConfig, GpuPoolEndpoint};
use crate::sfx_types::SoundEffectsConfig;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub providers: ProvidersConfig,
    #[serde(default)]
    pub voice: VoiceConfig,
    #[serde(default)]
    pub voice_clone: VoiceCloneConfig,
    #[serde(default)]
    pub sound_effects: SoundEffectsConfig,
    #[serde(default)]
    pub narrator: NarratorConfig,
    #[serde(default)]
    pub pipeline: PipelineConfig,
    #[serde(default)]
    pub characters: Vec<CharacterConfig>,
    #[serde(default)]
    pub output: OutputConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProvidersConfig {
    #[serde(default)]
    pub tts: String,
    #[serde(default)]
    pub transcription: Option<String>,
    #[serde(default)]
    pub sfx: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VoiceConfig {
    #[serde(default)]
    pub audio_cpp: AudioCppConfig,
    #[serde(default)]
    pub gpu_pool: Vec<GpuPoolEndpoint>,
    #[serde(default)]
    pub gpu_policy: GpuPolicyConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceCloneConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_voice_clone_runtime")]
    pub runtime: String,
    #[serde(default = "default_audio_cpp_family")]
    pub family: String,
    #[serde(default)]
    pub model: String,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_min_sample_secs")]
    pub min_sample_seconds: f32,
    #[serde(default = "default_max_samples")]
    pub max_samples_per_character: u32,
    #[serde(default = "default_true")]
    pub normalize_samples: bool,
    #[serde(default)]
    pub routing: VoiceCloneRoutingConfig,
}

impl Default for VoiceCloneConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            runtime: default_voice_clone_runtime(),
            family: default_audio_cpp_family(),
            model: "models/Qwen3-TTS-12Hz-1.7B-Base".to_string(),
            language: default_language(),
            min_sample_seconds: default_min_sample_secs(),
            max_samples_per_character: default_max_samples(),
            normalize_samples: true,
            routing: VoiceCloneRoutingConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceCloneRoutingConfig {
    #[serde(default = "default_audio_cpp_mode")]
    pub mode: String,
    #[serde(default = "default_true")]
    pub prefer_free: bool,
    #[serde(default)]
    pub allow_paid: bool,
}

impl Default for VoiceCloneRoutingConfig {
    fn default() -> Self {
        Self {
            mode: default_audio_cpp_mode(),
            prefer_free: true,
            allow_paid: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NarratorConfig {
    #[serde(default = "default_narrator_backend")]
    pub backend: String,
    #[serde(default = "default_narrator_language")]
    pub language: String,
    #[serde(default = "default_narrator_style")]
    pub style: String,
    #[serde(default = "default_narrator_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_narrator_temperature")]
    pub temperature: f32,
    #[serde(default = "default_narrator_template")]
    pub prompt_template: String,
    #[serde(default)]
    pub openai: NarratorOpenAiConfig,
    #[serde(default)]
    pub ollama_local: NarratorOllamaConfig,
    #[serde(default)]
    pub anthropic: NarratorAnthropicConfig,
}

impl Default for NarratorConfig {
    fn default() -> Self {
        Self {
            backend: default_narrator_backend(),
            language: default_narrator_language(),
            style: default_narrator_style(),
            max_tokens: default_narrator_max_tokens(),
            temperature: default_narrator_temperature(),
            prompt_template: default_narrator_template(),
            openai: NarratorOpenAiConfig::default(),
            ollama_local: NarratorOllamaConfig::default(),
            anthropic: NarratorAnthropicConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NarratorOpenAiConfig {
    #[serde(default = "default_openai_api_env")]
    pub api_key_env: String,
    #[serde(default = "default_openai_model")]
    pub model: String,
    #[serde(default = "default_openai_base_url")]
    pub base_url: String,
}

impl Default for NarratorOpenAiConfig {
    fn default() -> Self {
        Self {
            api_key_env: default_openai_api_env(),
            model: default_openai_model(),
            base_url: default_openai_base_url(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NarratorOllamaConfig {
    #[serde(default = "default_ollama_base_url")]
    pub base_url: String,
    #[serde(default = "default_ollama_model")]
    pub model: String,
    #[serde(default = "default_ollama_ctx")]
    pub num_ctx: u32,
}

impl Default for NarratorOllamaConfig {
    fn default() -> Self {
        Self {
            base_url: default_ollama_base_url(),
            model: default_ollama_model(),
            num_ctx: default_ollama_ctx(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NarratorAnthropicConfig {
    #[serde(default = "default_anthropic_api_env")]
    pub api_key_env: String,
    #[serde(default = "default_anthropic_model")]
    pub model: String,
}

impl Default for NarratorAnthropicConfig {
    fn default() -> Self {
        Self {
            api_key_env: default_anthropic_api_env(),
            model: default_anthropic_model(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PipelineConfig {
    #[serde(default)]
    pub stages: Vec<String>,
    #[serde(default)]
    pub parallel: bool,
    #[serde(default)]
    pub checkpoint_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterConfig {
    pub name: String,
    #[serde(default)]
    pub voice_id: Option<String>,
    #[serde(default)]
    pub voice_ref: Option<String>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OutputConfig {
    #[serde(default = "default_output_dir")]
    pub dir: String,
    #[serde(default = "default_output_format")]
    pub format: String,
}

fn default_true() -> bool {
    true
}
fn default_voice_clone_runtime() -> String {
    "audio_cpp".to_string()
}
fn default_audio_cpp_family() -> String {
    "qwen3_tts".to_string()
}
fn default_language() -> String {
    "de".to_string()
}
fn default_min_sample_secs() -> f32 {
    6.0
}
fn default_max_samples() -> u32 {
    20
}
fn default_audio_cpp_mode() -> String {
    "auto".to_string()
}
fn default_narrator_backend() -> String {
    "openai".to_string()
}
fn default_narrator_language() -> String {
    "en-US".to_string()
}
fn default_narrator_style() -> String {
    "radio_drama".to_string()
}
fn default_narrator_max_tokens() -> u32 {
    200
}
#[allow(clippy::approx_constant)]
fn default_narrator_temperature() -> f32 {
    0.7
}
fn default_narrator_template() -> String {
    "templates/narrator_prompt.md".to_string()
}
fn default_openai_api_env() -> String {
    "OPENAI_API_KEY".to_string()
}
fn default_openai_model() -> String {
    "gpt-4o".to_string()
}
fn default_openai_base_url() -> String {
    "https://api.openai.com/v1".to_string()
}
fn default_ollama_base_url() -> String {
    "http://localhost:11434".to_string()
}
fn default_ollama_model() -> String {
    "llama3.1:8b".to_string()
}
fn default_ollama_ctx() -> u32 {
    2048
}
fn default_anthropic_api_env() -> String {
    "ANTHROPIC_API_KEY".to_string()
}
fn default_anthropic_model() -> String {
    "claude-3-5-haiku-20241022".to_string()
}
fn default_output_dir() -> String {
    "output".to_string()
}
fn default_output_format() -> String {
    "wav".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_app_config_serializes() {
        let cfg = AppConfig::default();
        let s = serde_json::to_string(&cfg).expect("serialize");
        let v: AppConfig = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(v.voice_clone.runtime, "audio_cpp");
    }
}
