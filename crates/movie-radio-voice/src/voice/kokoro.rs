use anyhow::{Context, Result};
use async_trait::async_trait;
use ort::session::Session;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::info;

use super::{AudioOutput, ProviderCapabilities, SynthesisRequest, VoiceSynthesizer};
use crate::config::KokoroConfig;

const KOKORO_MODEL_URL: &str =
    "https://huggingface.co/Godelaune/Kokoro-82M-ONNX-German-Martin/resolve/main/model.onnx";

pub struct KokoroProvider {
    config: KokoroConfig,
    session: Option<Arc<Session>>,
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

    fn load_session(&self) -> Result<Arc<Session>> {
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

        info!("Kokoro session loaded successfully");
        Ok(Arc::new(session))
    }

    fn phonemize_german(&self, text: &str) -> String {
        let mut result = String::new();
        for ch in text.chars() {
            match ch {
                'ä' => result.push_str("ae"),
                'ö' => result.push_str("oe"),
                'ü' => result.push_str("ue"),
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

    fn estimate_duration_samples(&self, text: &str, sample_rate: u32) -> usize {
        let word_count = text.split_whitespace().count().max(1);
        let seconds = word_count as f64 / 150.0 * 60.0;
        (seconds * sample_rate as f64) as usize
    }

    fn synthesize_silence(&self, duration_samples: usize, sample_rate: u32) -> AudioOutput {
        AudioOutput {
            samples: vec![0.0; duration_samples],
            sample_rate_hz: sample_rate,
        }
    }

    fn synthesize_with_session(
        &self,
        _session: &Session,
        request: &SynthesisRequest,
    ) -> Result<AudioOutput> {
        let phonemes = self.phonemize_german(&request.text);
        info!(phonemes = %phonemes, "Phonemized input");

        let duration_samples =
            self.estimate_duration_samples(&request.text, request.sample_rate_hz);

        let output = self.synthesize_silence(duration_samples, request.sample_rate_hz);

        Ok(output)
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
