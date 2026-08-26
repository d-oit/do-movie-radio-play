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
            .json(&serde_json::json!({
                "text": request.text,
                "language": request.language,
            }))
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

        let samples = parse_pcm16_mono_wav(&bytes).context("Failed to parse Modal WAV response")?;

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

/// Validates that a `fmt ` chunk contains 16-bit mono PCM audio settings.
fn validate_fmt_chunk(chunk_body: &[u8]) -> Result<()> {
    if chunk_body.len() < 16 {
        anyhow::bail!("Invalid WAV fmt chunk size: must be at least 16 bytes");
    }
    let audio_format = u16::from_le_bytes([chunk_body[0], chunk_body[1]]);
    let num_channels = u16::from_le_bytes([chunk_body[2], chunk_body[3]]);
    let bits_per_sample = u16::from_le_bytes([chunk_body[14], chunk_body[15]]);

    if audio_format != 1 {
        anyhow::bail!(
            "Unsupported WAV format: expected uncompressed PCM (1), got {}",
            audio_format
        );
    }
    if num_channels != 1 {
        anyhow::bail!(
            "Unsupported WAV channel count: expected mono (1), got {}",
            num_channels
        );
    }
    if bits_per_sample != 16 {
        anyhow::bail!(
            "Unsupported WAV bit depth: expected 16-bit, got {}",
            bits_per_sample
        );
    }
    Ok(())
}

/// Converts raw 16-bit LE PCM byte slice to normalized `f32` audio samples.
fn decode_pcm16_samples(pcm: &[u8]) -> Vec<f32> {
    let mut samples = Vec::with_capacity(pcm.len() / 2);
    for chunk in pcm.as_chunks::<2>().0 {
        let s = i16::from_le_bytes([chunk[0], chunk[1]]) as f32 / i16::MAX as f32;
        samples.push(s);
    }
    samples
}

/// Parses a RIFF/WAVE byte buffer containing 16-bit PCM mono audio.
///
/// Validates container signatures and format metadata (`fmt ` chunk) and locates
/// the `data` chunk to extract normalized `f32` samples.
fn parse_pcm16_mono_wav(bytes: &[u8]) -> Result<Vec<f32>> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        anyhow::bail!("Invalid WAV container: missing or short RIFF/WAVE header");
    }

    let mut cursor = 12;
    let mut fmt_checked = false;
    let mut pcm_data: Option<&[u8]> = None;

    while cursor + 8 <= bytes.len() {
        let chunk_id = &bytes[cursor..cursor + 4];
        let chunk_size = u32::from_le_bytes([
            bytes[cursor + 4],
            bytes[cursor + 5],
            bytes[cursor + 6],
            bytes[cursor + 7],
        ]) as usize;
        cursor += 8;

        if cursor + chunk_size > bytes.len() {
            anyhow::bail!("Invalid WAV chunk bounds: chunk exceeds response buffer");
        }

        let chunk_body = &bytes[cursor..cursor + chunk_size];

        match chunk_id {
            b"fmt " => {
                validate_fmt_chunk(chunk_body)?;
                fmt_checked = true;
            }
            b"data" => {
                pcm_data = Some(chunk_body);
                break;
            }
            _ => {}
        }

        // Chunk sizes in RIFF are padded to word boundaries (2 bytes)
        let pad = if chunk_size % 2 == 1 { 1 } else { 0 };
        cursor += chunk_size + pad;
    }

    if !fmt_checked {
        anyhow::bail!("Invalid WAV container: missing or malformed fmt chunk");
    }

    let pcm = pcm_data.context("Invalid WAV container: missing data chunk")?;
    Ok(decode_pcm16_samples(pcm))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_valid_wav(samples_i16: &[i16]) -> Vec<u8> {
        let pcm_bytes: Vec<u8> = samples_i16.iter().flat_map(|s| s.to_le_bytes()).collect();
        let data_size = pcm_bytes.len() as u32;
        let riff_size = 36 + data_size;

        let mut header = Vec::new();
        header.extend_from_slice(b"RIFF");
        header.extend_from_slice(&riff_size.to_le_bytes());
        header.extend_from_slice(b"WAVE");

        header.extend_from_slice(b"fmt ");
        header.extend_from_slice(&(16u32.to_le_bytes())); // chunk size
        header.extend_from_slice(&(1u16.to_le_bytes())); // PCM format
        header.extend_from_slice(&(1u16.to_le_bytes())); // mono
        header.extend_from_slice(&(16000u32.to_le_bytes())); // sample rate
        header.extend_from_slice(&(32000u32.to_le_bytes())); // byte rate
        header.extend_from_slice(&(2u16.to_le_bytes())); // block align
        header.extend_from_slice(&(16u16.to_le_bytes())); // bits per sample

        header.extend_from_slice(b"data");
        header.extend_from_slice(&data_size.to_le_bytes());
        header.extend_from_slice(&pcm_bytes);

        header
    }

    #[test]
    fn test_parse_valid_wav() {
        let raw_samples = vec![0i16, 16384, -16384, i16::MAX];
        let wav_bytes = make_valid_wav(&raw_samples);

        let parsed = parse_pcm16_mono_wav(&wav_bytes).unwrap();
        assert_eq!(parsed.len(), 4);
        assert_eq!(parsed[0], 0.0);
        assert!((parsed[1] - (16384.0 / i16::MAX as f32)).abs() < 1e-5);
        assert!((parsed[2] - (-16384.0 / i16::MAX as f32)).abs() < 1e-5);
        assert_eq!(parsed[3], 1.0);
    }

    #[test]
    fn test_parse_sub_44_byte_body_err() {
        let short_bytes = vec![0u8; 20];
        let err = parse_pcm16_mono_wav(&short_bytes).unwrap_err();
        assert!(err.to_string().contains("Invalid WAV container"));
    }

    #[test]
    fn test_parse_invalid_riff_header() {
        let mut wav_bytes = make_valid_wav(&[100, 200]);
        wav_bytes[0..4].copy_from_slice(b"FORM");
        let err = parse_pcm16_mono_wav(&wav_bytes).unwrap_err();
        assert!(err
            .to_string()
            .contains("missing or short RIFF/WAVE header"));
    }

    #[test]
    fn test_parse_non_pcm_wav() {
        let mut wav_bytes = make_valid_wav(&[100, 200]);
        // Modify audio_format to 3 (IEEE float)
        wav_bytes[20..22].copy_from_slice(&(3u16.to_le_bytes()));
        let err = parse_pcm16_mono_wav(&wav_bytes).unwrap_err();
        assert!(err.to_string().contains("expected uncompressed PCM"));
    }

    #[test]
    fn test_parse_stereo_wav() {
        let mut wav_bytes = make_valid_wav(&[100, 200]);
        // Modify num_channels to 2
        wav_bytes[22..24].copy_from_slice(&(2u16.to_le_bytes()));
        let err = parse_pcm16_mono_wav(&wav_bytes).unwrap_err();
        assert!(err.to_string().contains("expected mono"));
    }

    #[test]
    fn test_parse_truncated_chunk_bounds() {
        let mut wav_bytes = make_valid_wav(&[100, 200]);
        // Truncate the buffer in the middle of the data chunk
        wav_bytes.pop();
        wav_bytes.pop();
        let err = parse_pcm16_mono_wav(&wav_bytes).unwrap_err();
        assert!(err.to_string().contains("chunk exceeds response buffer"));
    }
}
