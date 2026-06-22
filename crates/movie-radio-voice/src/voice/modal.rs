use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use std::env;

use super::{AudioOutput, ProviderCapabilities, SynthesisRequest, VoiceSynthesizer};
use crate::config::ModalConfig;

pub struct ModalTtsProvider {
    config: ModalConfig,
    client: Client,
}

impl ModalTtsProvider {
    pub fn new(config: ModalConfig) -> Self {
        Self {
            config,
            client: Client::new(),
        }
    }
}

#[async_trait]
impl VoiceSynthesizer for ModalTtsProvider {
    async fn synthesize(&self, request: &SynthesisRequest) -> Result<AudioOutput> {
        let endpoint_url = env::var(&self.config.endpoint_url_env).with_context(|| {
            format!(
                "Environment variable {} not set",
                self.config.endpoint_url_env
            )
        })?;

        let response = self
            .client
            .post(&endpoint_url)
            .query(&[
                ("text", request.text.as_str()),
                ("language", request.language.as_str()),
            ])
            .send()
            .await
            .context("Failed to send request to Modal endpoint")?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Modal API error: {}", error_text);
        }

        let bytes = response
            .bytes()
            .await
            .context("Failed to read Modal response bytes")?;

        if bytes.len() < 44 {
            anyhow::bail!("Modal response too short to be a valid WAV");
        }

        let pcm_data = &bytes[44..];
        let mut samples = Vec::with_capacity(pcm_data.len() / 2);
        for chunk in pcm_data.chunks_exact(2) {
            let s = i16::from_le_bytes([chunk[0], chunk[1]]) as f32 / i16::MAX as f32;
            samples.push(s);
        }

        Ok(AudioOutput {
            samples,
            sample_rate_hz: request.sample_rate_hz,
        })
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_emotion: true,
            supports_voice_cloning: true,
            supports_streaming: false,
            max_text_length: 5000,
            languages: vec!["de".to_string(), "en".to_string()],
            requires_gpu: true,
        }
    }

    fn estimate_cost(&self, text_len: usize) -> f64 {
        (text_len as f64) * 0.0000006
    }
}
