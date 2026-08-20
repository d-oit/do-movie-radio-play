use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub mod elevenlabs;
pub mod kokoro;
pub mod modal;
pub mod openai;
pub mod orpheus;
pub mod pockettts;
pub mod qwen3;

#[async_trait]
pub trait VoiceSynthesizer: Send + Sync {
    /// Synthesize text with emotion to audio samples
    async fn synthesize(&self, request: &SynthesisRequest) -> Result<AudioOutput>;

    /// Provider capabilities
    fn capabilities(&self) -> ProviderCapabilities;

    /// Estimated cost for a request (0.0 for local)
    fn estimate_cost(&self, text_len: usize) -> f64;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthesisRequest {
    pub text: String,
    pub emotion: Emotion,
    pub voice_id: Option<String>,
    #[serde(default = "default_language")]
    pub language: String,
    pub speed: f32,          // 0.5 - 2.0
    pub sample_rate_hz: u32, // output sample rate
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

impl SynthesisRequest {
    /// Validate synthesis parameters to prevent DoS / invalid memory allocation.
    pub fn validate(&self) -> Result<()> {
        if !(8000..=48000).contains(&self.sample_rate_hz) {
            anyhow::bail!(
                "sample_rate_hz must be between 8000 and 48000 Hz, got {}",
                self.sample_rate_hz
            );
        }

        if !self.speed.is_finite() || !(0.25..=4.0).contains(&self.speed) {
            anyhow::bail!(
                "speed must be a finite float between 0.25 and 4.0, got {}",
                self.speed
            );
        }

        if self.text.len() > 10000 {
            anyhow::bail!(
                "text length exceeds maximum limit of 10000 characters, got {}",
                self.text.len()
            );
        }

        Ok(())
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

pub struct SynthesisOrchestrator {
    providers: std::collections::HashMap<String, Box<dyn VoiceSynthesizer>>,
    fallback_chain: Vec<String>,
}

impl SynthesisOrchestrator {
    pub fn new(config: crate::config::VoiceSynthesisConfig) -> Self {
        let mut providers: std::collections::HashMap<String, Box<dyn VoiceSynthesizer>> =
            std::collections::HashMap::new();

        if let Some(c) = config.providers.kokoro {
            providers.insert(
                "kokoro".to_string(),
                Box::new(kokoro::KokoroProvider::new(c)),
            );
        }
        if let Some(c) = config.providers.pockettts {
            providers.insert(
                "pockettts".to_string(),
                Box::new(pockettts::PocketTtsProvider::new(c)),
            );
        }
        if let Some(c) = config.providers.qwen3 {
            providers.insert("qwen3".to_string(), Box::new(qwen3::Qwen3Provider::new(c)));
        }
        if let Some(c) = config.providers.orpheus {
            providers.insert(
                "orpheus".to_string(),
                Box::new(orpheus::OrpheusProvider::new(c)),
            );
        }
        if let Some(c) = config.providers.elevenlabs {
            providers.insert(
                "elevenlabs".to_string(),
                Box::new(elevenlabs::ElevenLabsProvider::new(c)),
            );
        }
        if let Some(c) = config.providers.modal {
            providers.insert(
                "modal".to_string(),
                Box::new(modal::ModalTtsProvider::new(c)),
            );
        }
        if let Some(c) = config.providers.openai {
            providers.insert(
                "openai".to_string(),
                Box::new(openai::OpenAiTtsProvider::new(c)),
            );
        }

        Self {
            providers,
            fallback_chain: config.fallback_chain,
        }
    }

    pub async fn synthesize(&self, request: &SynthesisRequest) -> Result<AudioOutput> {
        request.validate()?;

        let mut last_err = anyhow::anyhow!("No provider available in fallback chain");

        for provider_id in &self.fallback_chain {
            if let Some(provider) = self.providers.get(provider_id) {
                match provider.synthesize(request).await {
                    Ok(output) => return Ok(output),
                    Err(e) => {
                        tracing::warn!("Provider {} failed: {}", provider_id, e);
                        last_err = e;
                    }
                }
            }
        }

        Err(last_err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_synthesis_request_validate_valid() {
        let req = SynthesisRequest {
            text: "Hallo Welt".to_string(),
            emotion: Emotion::Neutral,
            voice_id: None,
            language: "de".to_string(),
            speed: 1.0,
            sample_rate_hz: 16000,
        };
        assert!(req.validate().is_ok());
    }

    #[test]
    fn test_synthesis_request_validate_sample_rate() {
        let mut req = SynthesisRequest {
            text: "Hallo".to_string(),
            ..Default::default()
        };

        req.sample_rate_hz = 7999;
        assert!(req.validate().is_err());

        req.sample_rate_hz = 48001;
        assert!(req.validate().is_err());

        req.sample_rate_hz = 8000;
        assert!(req.validate().is_ok());

        req.sample_rate_hz = 48000;
        assert!(req.validate().is_ok());
    }

    #[test]
    fn test_synthesis_request_validate_speed() {
        let mut req = SynthesisRequest {
            text: "Hallo".to_string(),
            ..Default::default()
        };

        req.speed = 0.2;
        assert!(req.validate().is_err());

        req.speed = 4.1;
        assert!(req.validate().is_err());

        req.speed = f32::NAN;
        assert!(req.validate().is_err());

        req.speed = f32::INFINITY;
        assert!(req.validate().is_err());

        req.speed = 0.25;
        assert!(req.validate().is_ok());

        req.speed = 4.0;
        assert!(req.validate().is_ok());
    }

    #[test]
    fn test_synthesis_request_validate_text_length() {
        let mut req = SynthesisRequest {
            text: "a".repeat(10001),
            ..Default::default()
        };
        assert!(req.validate().is_err());

        req.text = "a".repeat(10000);
        assert!(req.validate().is_ok());
    }
}
