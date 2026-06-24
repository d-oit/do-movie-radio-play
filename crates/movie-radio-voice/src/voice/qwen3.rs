use anyhow::{Context, Result};
use async_trait::async_trait;
use candle_core::Device;
use qwen_tts::model::loader::{LoaderConfig, ModelLoader};
use std::sync::{Arc, Mutex};
use tracing::info;

use super::{AudioOutput, Emotion, ProviderCapabilities, SynthesisRequest, VoiceSynthesizer};
use crate::config::Qwen3Config;

pub struct Qwen3Provider {
    config: Qwen3Config,
    model: Arc<Mutex<Option<qwen_tts::model::Model>>>,
}

impl Qwen3Provider {
    pub fn new(config: Qwen3Config) -> Self {
        Self {
            config,
            model: Arc::new(Mutex::new(None)),
        }
    }

    fn ensure_model(&self) -> Result<()> {
        let mut guard = self
            .model
            .lock()
            .map_err(|_| anyhow::anyhow!("Model lock poisoned"))?;

        if guard.is_some() {
            return Ok(());
        }

        let model_dir = &self.config.model_path;
        if !model_dir.exists() {
            anyhow::bail!("Qwen3 model directory not found: {}", model_dir.display());
        }

        info!(path = %model_dir.display(), "Loading Qwen3-TTS model");

        let device = if self.config.device.to_lowercase() == "cpu" {
            Device::Cpu
        } else {
            Device::new_cuda(0).unwrap_or_else(|_| {
                info!("CUDA not available, falling back to CPU");
                Device::Cpu
            })
        };

        let loader = ModelLoader::from_local_dir(model_dir)
            .context("Failed to create Qwen3 model loader")?;

        let tts_model = loader
            .load_tts_model(&device, &LoaderConfig::default())
            .context("Failed to load Qwen3 TTS model")?;

        *guard = Some(tts_model);
        info!("Qwen3-TTS model loaded successfully");

        Ok(())
    }

    fn get_emotion_prompt(&self, emotion: &Emotion) -> String {
        match emotion {
            Emotion::Neutral => "Sprich in einem neutralen Ton.".to_string(),
            Emotion::Excited => "Sprich mit Begeisterung und Energie.".to_string(),
            Emotion::Sad => "Sprich traurig und langsam.".to_string(),
            Emotion::Tense => "Sprich angespannt und flüsternd.".to_string(),
            Emotion::Mysterious => "Sprich geheimnisvoll und dunkel.".to_string(),
            Emotion::Joyful => "Sprich fröhlich und hell.".to_string(),
            Emotion::Whisper => "Sprich sehr leise, flüsternd.".to_string(),
            Emotion::Angry => "Sprich wütend und bestimmt.".to_string(),
            Emotion::Custom(p) => p.clone(),
        }
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
}

#[async_trait]
impl VoiceSynthesizer for Qwen3Provider {
    async fn synthesize(&self, request: &SynthesisRequest) -> Result<AudioOutput> {
        self.ensure_model()?;

        let guard = self
            .model
            .lock()
            .map_err(|_| anyhow::anyhow!("Model lock poisoned"))?;

        let model = guard.as_ref().context("Qwen3 model not loaded")?;

        let prompt = self.get_emotion_prompt(&request.emotion);
        let text_with_prompt = format!("{} {}", prompt, request.text);

        let language = match request.language.as_str() {
            "de" => "german",
            "en" => "english",
            "es" => "spanish",
            "fr" => "french",
            "zh" => "chinese",
            _ => "english",
        };

        info!(
            text_len = request.text.len(),
            language = language,
            "Running Qwen3-TTS inference"
        );

        let result = model
            .generate_custom_voice_from_text(
                &text_with_prompt,
                &self.config.voice_description,
                language,
                None,
                None,
            )
            .context("Qwen3-TTS inference failed")?;

        let raw_samples: Vec<f32> = result
            .audio
            .to_vec1()
            .context("Failed to extract audio samples from tensor")?;
        let source_rate = result.sample_rate as u32;

        let samples = self.resample(&raw_samples, source_rate, request.sample_rate_hz);

        info!(
            raw_len = raw_samples.len(),
            output_len = samples.len(),
            source_rate,
            "Qwen3-TTS inference complete"
        );

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
            max_text_length: 2000,
            languages: vec!["de".to_string(), "en".to_string(), "zh".to_string()],
            requires_gpu: true,
        }
    }

    fn estimate_cost(&self, _text_len: usize) -> f64 {
        0.0
    }
}
