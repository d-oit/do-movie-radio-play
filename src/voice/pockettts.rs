use anyhow::Result;
use async_trait::async_trait;

use crate::config::PocketTtsConfig;
use crate::voice::{AudioOutput, ProviderCapabilities, SynthesisRequest, VoiceSynthesizer};

pub struct PocketTtsProvider {
    _config: PocketTtsConfig,
}

impl PocketTtsProvider {
    pub fn new(config: PocketTtsConfig) -> Self {
        Self { _config: config }
    }
}

#[async_trait]
impl VoiceSynthesizer for PocketTtsProvider {
    async fn synthesize(&self, request: &SynthesisRequest) -> Result<AudioOutput> {
        // Implementation for PocketTTS (CPU-first German baseline)
        Ok(AudioOutput {
            samples: vec![0.0; 16000],
            sample_rate_hz: request.sample_rate_hz,
        })
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_emotion: false,
            supports_voice_cloning: true,
            supports_streaming: true,
            max_text_length: 5000,
            languages: vec![
                "de".to_string(),
                "en".to_string(),
                "es".to_string(),
                "fr".to_string(),
            ],
            requires_gpu: false,
        }
    }

    fn estimate_cost(&self, _text_len: usize) -> f64 {
        0.0
    }

    fn max_monthly_cost(&self) -> f64 {
        0.0
    }
}
