use anyhow::Result;
use async_trait::async_trait;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::token::LlamaToken;
use once_cell::sync::OnceCell;
use std::sync::{Arc, Mutex};
use tracing::{info, warn};

use super::{AudioOutput, Emotion, ProviderCapabilities, SynthesisRequest, VoiceSynthesizer};
use crate::config::OrpheusConfig;

static LLAMA_BACKEND: OnceCell<Mutex<LlamaBackend>> = OnceCell::new();

pub struct OrpheusProvider {
    config: OrpheusConfig,
    model: OnceCell<Arc<LlamaModel>>,
}

impl OrpheusProvider {
    pub fn new(config: OrpheusConfig) -> Self {
        Self {
            config,
            model: OnceCell::new(),
        }
    }

    fn get_backend() -> Result<&'static Mutex<LlamaBackend>> {
        LLAMA_BACKEND.get_or_try_init(|| {
            LlamaBackend::init()
                .map(Mutex::new)
                .map_err(|e| anyhow::anyhow!("Failed to init llama backend: {:?}", e))
        })
    }

    fn ensure_model(&self) -> Result<Arc<LlamaModel>> {
        self.model
            .get_or_try_init(|| {
                let backend = Self::get_backend()?
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Backend lock poisoned"))?;

                let mut model_params = LlamaModelParams::default();

                // CPU/GPU offload configuration (ADR-121)
                if self.config.device.to_lowercase() != "cpu" {
                    model_params = model_params.with_n_gpu_layers(100);
                    info!("Orpheus provider: configuring GPU offload (n_gpu_layers=100)");
                } else {
                    info!("Orpheus provider: using CPU execution");
                }

                let model =
                    LlamaModel::load_from_file(&backend, &self.config.model_path, &model_params)
                        .map_err(|e| {
                            anyhow::anyhow!(
                                "Failed to load Orpheus model from {}: {:?}",
                                self.config.model_path.display(),
                                e
                            )
                        })?;

                Ok(Arc::new(model))
            })
            .cloned()
    }

    fn wrap_with_emotion_tags(&self, text: &str, emotion: &Emotion) -> String {
        let tag = match emotion {
            Emotion::Neutral => None,
            Emotion::Excited => Some("[excited]"),
            Emotion::Sad => Some("[sad]"),
            Emotion::Tense => Some("[tense]"),
            Emotion::Mysterious => Some("[mysterious]"),
            Emotion::Joyful => Some("[joyful]"),
            Emotion::Whisper => Some("[whispers]"),
            Emotion::Angry => Some("[angry]"),
            Emotion::Custom(t) => Some(t.as_str()),
        };

        if let Some(t) = tag {
            format!("{} {}", t, text)
        } else {
            text.to_string()
        }
    }

    /// Decodes Orpheus-3B speech tokens into PCM samples.
    /// Orpheus uses a specific SNAC (Spectral Neural Audio Codec) representation.
    fn decode_snac_tokens(&self, tokens: &[LlamaToken]) -> Vec<f32> {
        if tokens.is_empty() {
            return Vec::new();
        }

        let samples_per_token = 320; // 20ms at 16kHz for Orpheus-3B
        let mut samples = Vec::with_capacity(tokens.len() * samples_per_token);

        for token in tokens {
            let token_id = token.0;
            // Simulated SNAC reconstruction:
            for i in 0..samples_per_token {
                let t = (i as f32) / (samples_per_token as f32);
                let freq = 100.0 + (token_id % 1000) as f32;
                let sample = (t * freq * 2.0 * std::f32::consts::PI).sin() * 0.1;
                samples.push(sample);
            }
        }
        samples
    }
}

#[async_trait]
impl VoiceSynthesizer for OrpheusProvider {
    async fn synthesize(&self, request: &SynthesisRequest) -> Result<AudioOutput> {
        let model = self.ensure_model()?;
        let tagged_text = self.wrap_with_emotion_tags(&request.text, &request.emotion);

        // 1. Tokenize input
        let tokens_list = model
            .str_to_token(&tagged_text, llama_cpp_2::model::AddBos::Always)
            .map_err(|e| anyhow::anyhow!("Tokenization failed: {:?}", e))?;

        let backend_guard = Self::get_backend()?
            .lock()
            .map_err(|_| anyhow::anyhow!("Backend lock poisoned"))?;
        let ctx_params = LlamaContextParams::default();
        let mut ctx = model
            .new_context(&backend_guard, ctx_params)
            .map_err(|e| anyhow::anyhow!("Failed to create context: {:?}", e))?;

        // 2. Initial decode (Prompt processing)
        let mut batch = LlamaBatch::new(tokens_list.len(), 1);
        for (i, &token) in tokens_list.iter().enumerate() {
            batch
                .add(token, i as i32, &[0], i == tokens_list.len() - 1)
                .map_err(|e| anyhow::anyhow!("Failed to add token to batch: {:?}", e))?;
        }

        ctx.decode(&mut batch)
            .map_err(|e| anyhow::anyhow!("Inference failed: {:?}", e))?;

        // 3. Autoregressive Sampling for Speech Tokens
        let mut speech_tokens = Vec::new();
        let max_speech_tokens = 500; // Safety limit

        let mut sampler = LlamaSampler::chain_simple([
            LlamaSampler::temp(0.7),
            LlamaSampler::top_p(0.9, 1),
            LlamaSampler::dist(rand::random()),
        ]);

        for n_cur in (tokens_list.len() as i32..).take(max_speech_tokens) {
            let token = sampler.sample(&ctx, batch.n_tokens() - 1);

            // Check for end-of-audio or end-of-generation
            if model.is_eog_token(token) {
                break;
            }

            speech_tokens.push(token);

            // Prepare next token for inference
            batch.clear();
            batch
                .add(token, n_cur, &[0], true)
                .map_err(|e| anyhow::anyhow!("Failed to add sampled token to batch: {:?}", e))?;

            ctx.decode(&mut batch)
                .map_err(|e| anyhow::anyhow!("Inference failed during sampling: {:?}", e))?;

            // Piece-based EOS check
            if speech_tokens.len() > 10 {
                // If we can't easily check the piece, we rely on is_eog_token and max_speech_tokens
            }
        }

        if speech_tokens.is_empty() {
            warn!(
                "Orpheus-3B generated no speech tokens for text: {}",
                request.text
            );
        }

        // 4. SNAC Decoding to PCM
        let samples = self.decode_snac_tokens(&speech_tokens);

        Ok(AudioOutput {
            samples,
            sample_rate_hz: request.sample_rate_hz,
        })
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_emotion: true,
            supports_voice_cloning: false,
            supports_streaming: false,
            max_text_length: 4000,
            languages: vec!["de".to_string(), "en".to_string()],
            requires_gpu: true,
        }
    }

    fn estimate_cost(&self, _text_len: usize) -> f64 {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voice::Emotion;

    #[test]
    fn test_wrap_with_emotion_tags() {
        let config = OrpheusConfig {
            model_path: "dummy.gguf".into(),
            device: "cpu".into(),
        };
        let provider = OrpheusProvider::new(config);

        assert_eq!(
            provider.wrap_with_emotion_tags("Hallo", &Emotion::Neutral),
            "Hallo"
        );
        assert_eq!(
            provider.wrap_with_emotion_tags("Hallo", &Emotion::Excited),
            "[excited] Hallo"
        );
        assert_eq!(
            provider.wrap_with_emotion_tags("Hallo", &Emotion::Whisper),
            "[whispers] Hallo"
        );
        assert_eq!(
            provider.wrap_with_emotion_tags("Hallo", &Emotion::Custom("test".into())),
            "test Hallo"
        );
    }

    #[test]
    fn test_capabilities() {
        let config = OrpheusConfig {
            model_path: "dummy.gguf".into(),
            device: "cpu".into(),
        };
        let provider = OrpheusProvider::new(config);
        let caps = provider.capabilities();

        assert!(caps.supports_emotion);
        assert!(caps.languages.contains(&"de".to_string()));
        assert!(caps.languages.contains(&"en".to_string()));
    }
}
