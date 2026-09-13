use anyhow::{Context, Result};
use async_trait::async_trait;
use ort::session::Session;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing::{debug, info, warn};

use super::{AudioOutput, ProviderCapabilities, SynthesisRequest, VoiceSynthesizer};
use crate::config::KokoroConfig;

const KOKORO_MODEL_URL: &str =
    "https://huggingface.co/Godelaune/Kokoro-82M-ONNX-German-Martin/resolve/main/model.onnx";

const KOKORO_SAMPLE_RATE: u32 = 24000;

pub struct KokoroProvider {
    config: KokoroConfig,
    session: Option<Arc<Mutex<Session>>>,
}

impl KokoroProvider {
    pub fn new(config: KokoroConfig) -> Self {
        Self {
            config,
            session: None,
        }
    }

    fn model_dir(&self) -> PathBuf {
        self.config
            .model_path
            .parent()
            .unwrap_or(&PathBuf::from("."))
            .to_path_buf()
    }

    fn onnx_path(&self) -> PathBuf {
        self.model_dir().join("kokoro-german-martin.onnx")
    }

    async fn ensure_model(&self) -> Result<()> {
        let onnx_path = self.onnx_path();
        if onnx_path.exists() {
            info!(path = %onnx_path.display(), "Kokoro model found");
            return Ok(());
        }

        info!("Downloading Kokoro German Martin model...");
        let model_dir = self.model_dir();
        std::fs::create_dir_all(&model_dir)?;

        let client = reqwest::Client::new();
        let response = client
            .get(KOKORO_MODEL_URL)
            .send()
            .await
            .context("Failed to download model")?;

        let bytes = response
            .bytes()
            .await
            .context("Failed to read model bytes")?;

        std::fs::write(&onnx_path, &bytes)?;
        info!(
            path = %onnx_path.display(),
            size_mb = bytes.len() / (1024 * 1024),
            "Model downloaded"
        );

        Ok(())
    }

    fn load_session(&self) -> Result<Arc<Mutex<Session>>> {
        if let Some(session) = &self.session {
            return Ok(Arc::clone(session));
        }

        let onnx_path = self.onnx_path();
        if !onnx_path.exists() {
            anyhow::bail!("Model not found at {}", onnx_path.display());
        }

        let session = Session::builder()
            .map_err(|e| anyhow::anyhow!("Failed to create session builder: {}", e))?
            .with_intra_threads(2)
            .map_err(|e| anyhow::anyhow!("Failed to set threads: {}", e))?
            .commit_from_file(&onnx_path)
            .map_err(|e| anyhow::anyhow!("Failed to load model: {}", e))?;

        info!(
            inputs = session.inputs().len(),
            outputs = session.outputs().len(),
            "Kokoro session loaded"
        );
        for input in session.inputs() {
            debug!(name = %input.name(), "model input");
        }
        for output in session.outputs() {
            debug!(name = %output.name(), "model output");
        }

        Ok(Arc::new(Mutex::new(session)))
    }

    fn phonemize_german(&self, text: &str) -> String {
        let mut result = String::new();
        for ch in text.chars() {
            match ch {
                'ä' => result.push_str("ae"),
                'ö' => result.push_str("oe"),
                'ü' => result.push_str("ue"),
                'Ä' => result.push_str("Ae"),
                'Ö' => result.push_str("Oe"),
                'Ü' => result.push_str("Ue"),
                'ß' => result.push_str("ss"),
                'é' | 'è' | 'ê' => result.push('e'),
                'á' | 'à' => result.push('a'),
                'ô' => result.push('o'),
                'î' => result.push('i'),
                'û' => result.push('u'),
                _ => result.push(ch),
            }
        }
        result
    }

    fn text_to_tokens(&self, text: &str) -> Vec<i64> {
        let phonemes = self.phonemize_german(text);
        phonemes.chars().map(|c| c as i64).collect()
    }

    fn resample(&self, samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
        if from_rate == to_rate {
            return samples.to_vec();
        }
        let ratio = to_rate as f64 / from_rate as f64;
        let output_len = (samples.len() as f64 * ratio) as usize;
        let mut resampled = Vec::with_capacity(output_len);
        for i in 0..output_len {
            let src_idx = i as f64 / ratio;
            let idx = src_idx as usize;
            let frac = src_idx - idx as f64;
            let s0 = samples[idx.min(samples.len() - 1)];
            let s1 = samples[(idx + 1).min(samples.len() - 1)];
            resampled.push(s0 + (s1 - s0) * frac as f32);
        }
        resampled
    }

    fn synthesize_with_session(
        &self,
        session: &Mutex<Session>,
        request: &SynthesisRequest,
    ) -> Result<AudioOutput> {
        let tokens = self.text_to_tokens(&request.text);
        info!(token_count = tokens.len(), "Running Kokoro inference");

        let mut guard = session
            .lock()
            .map_err(|_| anyhow::anyhow!("Session lock poisoned"))?;

        let input_name = guard
            .inputs()
            .first()
            .context("Model has no inputs")?
            .name()
            .to_owned();

        let output_name = guard
            .outputs()
            .first()
            .context("Model has no outputs")?
            .name()
            .to_owned();

        let input_shape = [1usize, tokens.len()];
        let input_value = ort::value::TensorRef::from_array_view((input_shape, tokens.as_slice()))
            .context("Failed to create input tensor")?;

        let outputs = guard
            .run(ort::inputs![input_name.as_str() => input_value.into_dyn()])
            .context("ONNX inference failed")?;

        let output_tensor = outputs
            .get(output_name.as_str())
            .context("Output tensor not found")?;

        let (shape, data) = output_tensor
            .try_extract_tensor::<f32>()
            .context("Failed to extract f32 output tensor")?;

        let raw_samples: Vec<f32> = data.to_vec();

        if raw_samples.iter().all(|&s| s == 0.0) {
            warn!(
                shape = ?shape,
                "Kokoro produced all-zero output, model may not be loaded correctly"
            );
        }

        let samples = self.resample(&raw_samples, KOKORO_SAMPLE_RATE, request.sample_rate_hz);

        info!(
            raw_len = raw_samples.len(),
            output_len = samples.len(),
            "Kokoro inference complete"
        );

        Ok(AudioOutput {
            samples,
            sample_rate_hz: request.sample_rate_hz,
        })
    }
}

#[async_trait]
impl VoiceSynthesizer for KokoroProvider {
    async fn synthesize(&self, request: &SynthesisRequest) -> Result<AudioOutput> {
        self.ensure_model().await?;
        let session = self.load_session()?;

        self.synthesize_with_session(&session, request)
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_emotion: true,
            supports_voice_cloning: false,
            supports_streaming: false,
            max_text_length: 1000,
            languages: vec!["de".to_string()],
            requires_gpu: false,
        }
    }

    fn estimate_cost(&self, _text_len: usize) -> f64 {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phonemize_german() {
        let config = KokoroConfig {
            model_path: "models/dummy.onnx".into(),
            device: "cpu".into(),
        };
        let provider = KokoroProvider::new(config);

        assert_eq!(provider.phonemize_german("Hallo"), "Hallo");
        assert_eq!(provider.phonemize_german("Über"), "Ueber");
        assert_eq!(provider.phonemize_german("über"), "ueber");
        assert_eq!(provider.phonemize_german("Straße"), "Strasse");
        assert_eq!(provider.phonemize_german("schön"), "schoen");
    }

    #[test]
    fn test_text_to_tokens() {
        let config = KokoroConfig {
            model_path: "models/dummy.onnx".into(),
            device: "cpu".into(),
        };
        let provider = KokoroProvider::new(config);

        let tokens = provider.text_to_tokens("Hi");
        assert_eq!(tokens, vec![72, 105]);
    }

    #[test]
    fn test_resample() {
        let config = KokoroConfig {
            model_path: "models/dummy.onnx".into(),
            device: "cpu".into(),
        };
        let provider = KokoroProvider::new(config);

        let input = vec![1.0, 2.0, 3.0, 4.0];
        let output = provider.resample(&input, 24000, 16000);
        assert!(!output.is_empty());
        assert!((output.len() as f64 - 4.0 * 16000.0 / 24000.0).abs() < 2.0);
    }
}
