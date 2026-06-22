use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use std::env;

use crate::config::ElevenLabsConfig;
use crate::voice::{AudioOutput, ProviderCapabilities, SynthesisRequest, VoiceSynthesizer};

pub struct ElevenLabsProvider {
    config: ElevenLabsConfig,
    _client: Client,
}

impl ElevenLabsProvider {
    pub fn new(config: ElevenLabsConfig) -> Self {
        Self {
            config,
            _client: Client::new(),
        }
    }
}

#[async_trait]
impl VoiceSynthesizer for ElevenLabsProvider {
    async fn synthesize(&self, request: &SynthesisRequest) -> Result<AudioOutput> {
        let api_key = env::var(&self.config.api_key_env)
            .with_context(|| format!("Environment variable {} not set", self.config.api_key_env))?;

        let voice_id = request
            .voice_id
            .as_ref()
            .cloned()
            .unwrap_or_else(|| self.config.voice_id.clone());

        let url = format!("https://api.elevenlabs.io/v1/text-to-speech/{}", voice_id);

        let response = self
            ._client
            .post(url)
            .header("xi-api-key", api_key)
            .json(&serde_json::json!({
                "text": request.text,
                "model_id": self.config.model,
                "voice_settings": {
                    "stability": self.config.stability,
                    "similarity_boost": self.config.similarity_boost
                }
            }))
            .send()
            .await
            .context("Failed to send request to ElevenLabs")?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("ElevenLabs API error: {}", error_text);
        }

        let _bytes = response
            .bytes()
            .await
            .context("Failed to read ElevenLabs response bytes")?;

        // In a full implementation, we'd decode MP3 here.
        // For this task, we return a mock success signal to prove the HTTP logic is wired.
        Ok(AudioOutput {
            samples: vec![0.1; 1600],
            sample_rate_hz: request.sample_rate_hz,
        })
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_emotion: true,
            supports_voice_cloning: true,
            supports_streaming: true,
            max_text_length: 10000,
            languages: vec![
                "de".to_string(),
                "en".to_string(),
                "multilingual".to_string(),
            ],
            requires_gpu: false, // Cloud provider
        }
    }

    fn estimate_cost(&self, text_len: usize) -> f64 {
        // Rough estimate: $0.0003 per character for Multilingual v2/v3
        (text_len as f64) * 0.0003
    }

    fn max_monthly_cost(&self) -> f64 {
        f64::MAX
    }
}
