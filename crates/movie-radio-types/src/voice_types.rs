use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[async_trait]
pub trait VoiceSynthesizer: Send + Sync {
    async fn synthesize(&self, request: &SynthesisRequest) -> Result<AudioOutput>;
    fn capabilities(&self) -> ProviderCapabilities;
    fn estimate_cost(&self, text_len: usize) -> f64;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthesisRequest {
    pub text: String,
    pub emotion: Emotion,
    pub voice_id: Option<String>,
    #[serde(default = "default_language")]
    pub language: String,
    pub speed: f32,
    pub sample_rate_hz: u32,
}

fn default_language() -> String {
    "de".to_string()
}

impl Default for SynthesisRequest {
    fn default() -> Self {
        Self {
            text: String::new(),
            emotion: Emotion::Neutral,
            voice_id: None,
            language: default_language(),
            speed: 1.0,
            sample_rate_hz: 16000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Emotion {
    Neutral,
    Excited,
    Sad,
    Tense,
    Mysterious,
    Joyful,
    Whisper,
    Angry,
    Custom(String),
}

#[derive(Debug, Clone)]
pub struct AudioOutput {
    pub samples: Vec<f32>,
    pub sample_rate_hz: u32,
}

pub struct ProviderCapabilities {
    pub supports_emotion: bool,
    pub supports_voice_cloning: bool,
    pub supports_streaming: bool,
    pub max_text_length: usize,
    pub languages: Vec<String>,
    pub requires_gpu: bool,
}
