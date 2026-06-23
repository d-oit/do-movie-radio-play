use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use std::env;
use std::io::Cursor;
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::packet::Packet;

use super::{AudioOutput, ProviderCapabilities, SynthesisRequest, VoiceSynthesizer};
use crate::config::ElevenLabsConfig;

pub struct ElevenLabsProvider {
    config: ElevenLabsConfig,
    client: Client,
}

impl ElevenLabsProvider {
    pub fn new(config: ElevenLabsConfig) -> Self {
        Self {
            config,
            client: Client::new(),
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
            .client
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

        let bytes = response
            .bytes()
            .await
            .context("Failed to read ElevenLabs response bytes")?;

        let samples = decode_audio_bytes(&bytes, request.sample_rate_hz)
            .context("Failed to decode ElevenLabs audio response")?;

        Ok(AudioOutput {
            samples,
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
}

pub fn decode_audio_bytes(bytes: &[u8], target_sample_rate: u32) -> Result<Vec<f32>> {
    let cursor = Cursor::new(bytes.to_vec());
    let mss = MediaSourceStream::new(Box::new(cursor), Default::default());

    let mut format = symphonia::default::get_probe()
        .probe(
            &Hint::new(),
            mss,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .context("Failed to probe audio format")?;

    let track = format
        .tracks()
        .iter()
        .find(|t| {
            t.codec_params
                .as_ref()
                .is_some_and(|cp| cp.audio().is_some())
        })
        .context("No audio track found")?;

    let track_id = track.id;
    let audio_params = track
        .codec_params
        .as_ref()
        .and_then(|cp| cp.audio())
        .context("Track has no audio codec parameters")?;

    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(audio_params, &AudioDecoderOptions::default())
        .context("Failed to create decoder")?;

    let mut all_samples = Vec::new();
    let mut source_rate = 0u32;
    let mut spec_set = false;

    loop {
        let packet: Packet = match format.next_packet() {
            Ok(Some(p)) => p,
            Ok(None) => break,
            Err(symphonia::core::errors::Error::IoError(ref e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(e) => return Err(e).context("Failed to read audio packet"),
        };

        if packet.track_id != track_id {
            continue;
        }

        match decoder.decode(&packet) {
            Ok(audio_buf) => {
                if !spec_set {
                    let buf_spec = audio_buf.spec();
                    source_rate = buf_spec.rate();
                    spec_set = true;
                }
                let mut buf = vec![0.0f32; audio_buf.samples_interleaved()];
                audio_buf.copy_to_vec_interleaved(&mut buf);
                all_samples.extend_from_slice(&buf);
            }
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
            Err(e) => return Err(e).context("Failed to decode audio"),
        }
    }

    if !spec_set || all_samples.is_empty() {
        anyhow::bail!("No audio frames decoded");
    }

    if source_rate == target_sample_rate {
        return Ok(all_samples);
    }

    let ratio = target_sample_rate as f64 / source_rate as f64;
    let output_len = (all_samples.len() as f64 * ratio) as usize;
    let mut resampled = Vec::with_capacity(output_len);

    for i in 0..output_len {
        let src_idx = i as f64 / ratio;
        let idx = src_idx as usize;
        let frac = src_idx - idx as f64;

        let s0 = all_samples[idx.min(all_samples.len() - 1)];
        let s1 = all_samples[(idx + 1).min(all_samples.len() - 1)];
        resampled.push(s0 + (s1 - s0) * frac as f32);
    }

    Ok(resampled)
}
