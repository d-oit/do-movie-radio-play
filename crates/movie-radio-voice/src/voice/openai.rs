use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use std::env;

use super::{AudioOutput, ProviderCapabilities, SynthesisRequest, VoiceSynthesizer};
use crate::config::OpenAiConfig;

pub struct OpenAiTtsProvider {
    config: OpenAiConfig,
    client: Client,
}

impl OpenAiTtsProvider {
    pub fn new(config: OpenAiConfig) -> Self {
        Self {
            config,
            client: Client::new(),
        }
    }
}

#[async_trait]
impl VoiceSynthesizer for OpenAiTtsProvider {
    async fn synthesize(&self, request: &SynthesisRequest) -> Result<AudioOutput> {
        let api_key = env::var(&self.config.api_key_env)
            .with_context(|| format!("Environment variable {} not set", self.config.api_key_env))?;

        let voice = if let Some(ref voice_id) = request.voice_id {
            voice_id.clone()
        } else {
            self.config.voice.clone()
        };

        let response = self
            .client
            .post("https://api.openai.com/v1/audio/speech")
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&serde_json::json!({
                "model": self.config.model,
                "voice": voice,
                "input": request.text,
                "response_format": self.config.response_format,
                "speed": request.speed,
            }))
            .send()
            .await
            .context("Failed to send request to OpenAI TTS API")?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("OpenAI TTS API error: {}", error_text);
        }

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        let bytes = response
            .bytes()
            .await
            .context("Failed to read OpenAI TTS response bytes")?;

        let samples = if content_type.contains("audio") {
            super::elevenlabs::decode_audio_bytes(&bytes, request.sample_rate_hz)
                .context("Failed to decode OpenAI audio response")?
        } else {
            anyhow::bail!("Unexpected response content-type: {}", content_type);
        };

        Ok(AudioOutput {
            samples,
            sample_rate_hz: request.sample_rate_hz,
        })
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_emotion: false,
            supports_voice_cloning: false,
            supports_streaming: true,
            max_text_length: 4096,
            languages: vec![
                "de".to_string(),
                "en".to_string(),
                "es".to_string(),
                "fr".to_string(),
                "it".to_string(),
                "pt".to_string(),
                "pl".to_string(),
                "tr".to_string(),
                "ru".to_string(),
                "nl".to_string(),
                "cs".to_string(),
                "ar".to_string(),
                "zh".to_string(),
                "ja".to_string(),
                "ko".to_string(),
            ],
            requires_gpu: false,
        }
    }

    fn estimate_cost(&self, text_len: usize) -> f64 {
        let price_per_char = if self.config.model == "tts-1-hd" {
            0.000030
        } else {
            0.000015
        };
        (text_len as f64) * price_per_char
    }
}
