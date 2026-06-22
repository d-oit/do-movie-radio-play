use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub mod elevenlabs;
pub mod kokoro;
pub mod modal;
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

    /// Max monthly cost allowed for this provider
    fn max_monthly_cost(&self) -> f64;
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
    max_cost_per_run: f64,
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

        Self {
            providers,
            fallback_chain: config.fallback_chain,
            max_cost_per_run: config.max_cost_per_run_usd,
        }
    }

    pub async fn synthesize(
        &self,
        request: &SynthesisRequest,
        db: Option<&crate::learning::database::LearningDb>,
    ) -> Result<AudioOutput> {
        let mut last_err = anyhow::anyhow!("No provider available in fallback chain");

        for provider_id in &self.fallback_chain {
            if let Some(provider) = self.providers.get(provider_id) {
                let cost = provider.estimate_cost(request.text.len());
                if cost > self.max_cost_per_run {
                    tracing::warn!(
                        "Provider {} exceeds max cost per run: {} > {}",
                        provider_id,
                        cost,
                        self.max_cost_per_run
                    );
                    continue;
                }

                // Monthly cost guard for cloud/serverless providers
                if let Some(db) = db {
                    let max_monthly = provider.max_monthly_cost();
                    if max_monthly > 0.0 {
                        let monthly_spend = db.get_monthly_spend(provider_id).await.unwrap_or(0.0);
                        if monthly_spend >= max_monthly {
                            tracing::warn!(
                                "Provider {} monthly spend limit reached: ${} >= ${}",
                                provider_id,
                                monthly_spend,
                                max_monthly
                            );
                            continue;
                        }
                    }
                }

                match provider.synthesize(request).await {
                    Ok(output) => {
                        if let Some(db) = db {
                            let _ = db.record_usage(provider_id, cost).await;
                        }
                        return Ok(output);
                    }
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
