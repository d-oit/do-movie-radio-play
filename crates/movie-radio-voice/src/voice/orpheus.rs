use anyhow::{Context, Result};
use async_trait::async_trait;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::token::LlamaToken;
use once_cell::sync::OnceCell;
use ort::session::Session;
use std::sync::{Arc, Mutex};
use tracing::{info, warn};

use super::{AudioOutput, Emotion, ProviderCapabilities, SynthesisRequest, VoiceSynthesizer};
use crate::config::OrpheusConfig;

static LLAMA_BACKEND: OnceCell<Mutex<LlamaBackend>> = OnceCell::new();

pub struct OrpheusProvider {
    config: OrpheusConfig,
    model: OnceCell<Arc<LlamaModel>>,
    vocoder_session: OnceCell<Option<Arc<Mutex<Session>>>>,
}

impl OrpheusProvider {
    pub fn new(config: OrpheusConfig) -> Self {
        Self {
            config,
            model: OnceCell::new(),
            vocoder_session: OnceCell::new(),
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

    fn ensure_vocoder(&self) -> Option<Arc<Mutex<Session>>> {
        self.vocoder_session
            .get_or_init(|| {
                let vocoder_path = self.config.vocoder_path.as_ref()?;
                if !vocoder_path.exists() {
                    warn!(
                        path = %vocoder_path.display(),
                        "Configured SNAC vocoder path does not exist. Fallback will be used."
                    );
                    return None;
                }

                let session_res = Session::builder()
                    .map_err(|e| anyhow::anyhow!("Failed session builder: {:?}", e))
                    .and_then(|b| {
                        b.with_intra_threads(2)
                            .map_err(|e| anyhow::anyhow!("Failed intra threads: {:?}", e))
                    })
                    .and_then(|mut b| {
                        b.commit_from_file(vocoder_path)
                            .map_err(|e| anyhow::anyhow!("Failed commit file: {:?}", e))
                    });

                match session_res {
                    Ok(session) => {
                        info!(
                            path = %vocoder_path.display(),
                            inputs = session.inputs().len(),
                            outputs = session.outputs().len(),
                            "Loaded SNAC ONNX vocoder session"
                        );
                        Some(Arc::new(Mutex::new(session)))
                    }
                    Err(e) => {
                        warn!(
                            path = %vocoder_path.display(),
                            error = %e,
                            "Failed to load SNAC ONNX vocoder model. Fallback will be used."
                        );
                        None
                    }
                }
            })
            .clone()
    }

    /// De-interleaves sequential Orpheus SNAC speech tokens into 3 hierarchical codebook levels.
    /// SNAC 24kHz uses 7 tokens per frame: Level 0 (1 token), Level 1 (2 tokens), Level 2 (4 tokens).
    fn parse_snac_levels(tokens: &[LlamaToken]) -> (Vec<i64>, Vec<i64>, Vec<i64>) {
        let frame_size = 7;
        let num_frames = tokens.len() / frame_size;

        let mut l0 = Vec::with_capacity(num_frames);
        let mut l1 = Vec::with_capacity(num_frames * 2);
        let mut l2 = Vec::with_capacity(num_frames * 4);

        for chunk in tokens[..num_frames * frame_size].chunks_exact(frame_size) {
            l0.push(chunk[0].0 as i64);
            l1.push(chunk[1].0 as i64);
            l1.push(chunk[2].0 as i64);
            l2.push(chunk[3].0 as i64);
            l2.push(chunk[4].0 as i64);
            l2.push(chunk[5].0 as i64);
            l2.push(chunk[6].0 as i64);
        }

        (l0, l1, l2)
    }

    /// Runs ONNX inference on SNAC codebook levels using the loaded vocoder session.
    fn decode_snac_onnx(
        &self,
        session: &Mutex<Session>,
        tokens: &[LlamaToken],
    ) -> Result<Vec<f32>> {
        let (l0, l1, l2) = Self::parse_snac_levels(tokens);
        let num_frames = l0.len();

        if num_frames == 0 {
            return Ok(Vec::new());
        }

        let mut guard = session
            .lock()
            .map_err(|_| anyhow::anyhow!("Vocoder session lock poisoned"))?;

        let inputs = guard.inputs();
        if inputs.is_empty() {
            anyhow::bail!("SNAC ONNX vocoder model has no input tensors");
        }

        let output_name = guard
            .outputs()
            .first()
            .context("Vocoder model has no output tensor")?
            .name()
            .to_owned();

        let outputs = if inputs.len() >= 3 {
            let name0 = inputs[0].name().to_owned();
            let name1 = inputs[1].name().to_owned();
            let name2 = inputs[2].name().to_owned();

            let t0_shape = [1usize, 1, num_frames];
            let t1_shape = [1usize, 2, num_frames];
            let t2_shape = [1usize, 4, num_frames];

            let val0 = ort::value::TensorRef::from_array_view((t0_shape, l0.as_slice()))?;
            let val1 = ort::value::TensorRef::from_array_view((t1_shape, l1.as_slice()))?;
            let val2 = ort::value::TensorRef::from_array_view((t2_shape, l2.as_slice()))?;

            guard.run(ort::inputs![
                name0.as_str() => val0.into_dyn(),
                name1.as_str() => val1.into_dyn(),
                name2.as_str() => val2.into_dyn()
            ])?
        } else {
            let name = inputs[0].name().to_owned();
            let mut combined = Vec::with_capacity(7 * num_frames);

            for f in 0..num_frames {
                combined.push(l0[f]);
                combined.push(l1[f * 2]);
                combined.push(l1[f * 2 + 1]);
                combined.push(l2[f * 4]);
                combined.push(l2[f * 4 + 1]);
                combined.push(l2[f * 4 + 2]);
                combined.push(l2[f * 4 + 3]);
            }

            let shape = [1usize, 7, num_frames];
            let val = ort::value::TensorRef::from_array_view((shape, combined.as_slice()))?;

            guard.run(ort::inputs![name.as_str() => val.into_dyn()])?
        };

        let output_tensor = outputs
            .get(output_name.as_str())
            .context("Failed to get output tensor from vocoder session")?;

        let (_shape, data) = output_tensor
            .try_extract_tensor::<f32>()
            .context("Failed to extract float audio samples from vocoder output")?;

        Ok(data.to_vec())
    }

    /// Decodes Orpheus-3B speech tokens into PCM samples.
    ///
    /// If an ONNX SNAC vocoder model is configured via `vocoder_path` in `OrpheusConfig`,
    /// this function decodes the multi-level SNAC tokens into real speech audio.
    /// Otherwise, it logs a warning documenting the missing dependency (`hubertsiuzdak/snac_24khz`
    /// / ONNX vocoder) and falls back to synthetic tone generation.
    fn decode_snac_tokens(&self, tokens: &[LlamaToken]) -> Vec<f32> {
        if tokens.is_empty() {
            return Vec::new();
        }

        if let Some(session) = self.ensure_vocoder() {
            match self.decode_snac_onnx(&session, tokens) {
                Ok(samples) if !samples.is_empty() => {
                    info!(
                        sample_count = samples.len(),
                        token_count = tokens.len(),
                        "Decoded SNAC tokens to speech PCM using ONNX vocoder"
                    );
                    return samples;
                }
                Ok(_) => {
                    warn!("SNAC ONNX vocoder returned empty audio samples");
                }
                Err(e) => {
                    warn!(
                        error = %e,
                        "SNAC ONNX vocoder inference failed, falling back to synthetic audio"
                    );
                }
            }
        }

        warn!(
            token_count = tokens.len(),
            "Orpheus SNAC decode: no valid ONNX vocoder model loaded (configure `vocoder_path` with e.g. `snac_24khz.onnx` from `hubertsiuzdak/snac_24khz`); using synthetic fallback"
        );

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
            vocoder_path: None,
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
            vocoder_path: None,
        };
        let provider = OrpheusProvider::new(config);
        let caps = provider.capabilities();

        assert!(caps.supports_emotion);
        assert!(caps.languages.contains(&"de".to_string()));
        assert!(caps.languages.contains(&"en".to_string()));
    }

    #[test]
    fn test_parse_snac_levels() {
        // 14 tokens = 2 frames of 7 codes each
        let tokens: Vec<LlamaToken> = (10..24).map(LlamaToken).collect();
        let (l0, l1, l2) = OrpheusProvider::parse_snac_levels(&tokens);

        assert_eq!(l0, vec![10, 17]);
        assert_eq!(l1, vec![11, 12, 18, 19]);
        assert_eq!(l2, vec![13, 14, 15, 16, 20, 21, 22, 23]);
    }

    #[test]
    fn test_decode_snac_tokens_fallback() {
        let config = OrpheusConfig {
            model_path: "dummy.gguf".into(),
            device: "cpu".into(),
            vocoder_path: Some("nonexistent_snac_model.onnx".into()),
        };
        let provider = OrpheusProvider::new(config);

        let tokens: Vec<LlamaToken> = (0..7).map(LlamaToken).collect();
        let samples = provider.decode_snac_tokens(&tokens);

        // Should return synthetic samples when vocoder model file is missing
        assert_eq!(samples.len(), 7 * 320);
    }

    #[test]
    fn test_decode_snac_tokens_empty() {
        let config = OrpheusConfig {
            model_path: "dummy.gguf".into(),
            device: "cpu".into(),
            vocoder_path: None,
        };
        let provider = OrpheusProvider::new(config);

        let samples = provider.decode_snac_tokens(&[]);
        assert!(samples.is_empty());
    }
}
