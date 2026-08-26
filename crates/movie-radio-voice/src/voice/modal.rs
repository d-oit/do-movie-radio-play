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

        if bytes.len() < 44 {
            anyhow::bail!("Modal response too short to be a valid WAV");
        }

        let data = wav_pcm16_mono_data(&bytes)
            .context("Modal response is not a supported PCM16 mono WAV")?;

        let mut samples = Vec::with_capacity(data.len() / 2);
        for chunk in data.as_chunks::<2>().0 {
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

struct WavLayout {
    data: std::ops::Range<usize>,
}

fn read_chunk_header(bytes: &[u8], offset: usize) -> Option<(&[u8], usize)> {
    if offset + 8 > bytes.len() {
        return None;
    }
    let id = &bytes[offset..offset + 4];
    let size = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().ok()?) as usize;
    Some((id, size))
}

fn parse_fmt_chunk(bytes: &[u8], data_start: usize, size: usize) -> Result<()> {
    const FMT_MIN_LEN: usize = 16;
    if size < FMT_MIN_LEN || data_start + FMT_MIN_LEN > bytes.len() {
        anyhow::bail!("WAVE fmt chunk truncated");
    }
    let fmt = &bytes[data_start..data_start + FMT_MIN_LEN];
    let audio_format = u16::from_le_bytes([fmt[0], fmt[1]]);
    let channels = u16::from_le_bytes([fmt[2], fmt[3]]);
    let bits_per_sample = u16::from_le_bytes([fmt[14], fmt[15]]);
    if audio_format != 1 {
        anyhow::bail!(
            "Unsupported WAVE format tag {} (expected PCM=1)",
            audio_format
        );
    }
    if channels != 1 {
        anyhow::bail!("Unsupported channel count {} (expected mono)", channels);
    }
    if bits_per_sample != 16 {
        anyhow::bail!(
            "Unsupported bit depth {} (expected 16-bit)",
            bits_per_sample
        );
    }
    Ok(())
}

fn parse_wav_layout(bytes: &[u8]) -> Result<WavLayout> {
    const HEADER_LEN: usize = 12;
    if bytes.len() < HEADER_LEN || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        anyhow::bail!("Not a RIFF/WAVE container");
    }

    let mut fmt_ok = false;
    let mut data_range: Option<std::ops::Range<usize>> = None;
    let mut offset = HEADER_LEN;
    while let Some((id, size)) = read_chunk_header(bytes, offset) {
        let data_start = offset + 8;
        let data_end = data_start
            .checked_add(size)
            .filter(|&end| end <= bytes.len())
            .ok_or_else(|| anyhow::anyhow!("WAVE chunk overruns buffer"))?;
        match id {
            b"fmt " => {
                parse_fmt_chunk(bytes, data_start, size)?;
                fmt_ok = true;
            }
            b"data" => data_range = Some(data_start..data_end),
            _ => {}
        }
        offset = data_end + (size & 1);
    }

    if !fmt_ok {
        anyhow::bail!("WAVE container has no valid fmt chunk");
    }
    let data = data_range.ok_or_else(|| anyhow::anyhow!("WAVE container has no data chunk"))?;
    Ok(WavLayout { data })
}

fn wav_pcm16_mono_data(bytes: &[u8]) -> Result<&[u8]> {
    let layout = parse_wav_layout(bytes)?;
    Ok(&bytes[layout.data])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt_chunk(channels: u16, bits: u16, audio_format: u16) -> Vec<u8> {
        let mut f = Vec::with_capacity(16);
        f.extend_from_slice(&audio_format.to_le_bytes());
        f.extend_from_slice(&channels.to_le_bytes());
        f.extend_from_slice(&16000u32.to_le_bytes());
        f.extend_from_slice(&32000u32.to_le_bytes());
        f.extend_from_slice(&(channels * bits / 8).to_le_bytes());
        f.extend_from_slice(&bits.to_le_bytes());
        f
    }

    fn riff(chunks: &[(&[u8], Vec<u8>)]) -> Vec<u8> {
        let mut body = b"WAVE".to_vec();
        for (id, data) in chunks {
            body.extend_from_slice(id);
            body.extend_from_slice(&(data.len() as u32).to_le_bytes());
            body.extend_from_slice(data);
            if data.len() % 2 == 1 {
                body.push(0);
            }
        }
        let mut out = b"RIFF".to_vec();
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&body);
        out
    }

    fn canonical_wav(samples: &[i16]) -> Vec<u8> {
        let data: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        riff(&[(b"fmt ", fmt_chunk(1, 16, 1)), (b"data", data)])
    }

    #[test]
    fn accepts_canonical_pcm16_mono_wav() {
        let wav = canonical_wav(&[100, -200, 300]);
        let expected: Vec<u8> = [100i16, -200, 300]
            .iter()
            .flat_map(|s| s.to_le_bytes())
            .collect();
        assert_eq!(wav_pcm16_mono_data(&wav).unwrap(), &expected[..]);
    }

    #[test]
    fn tolerates_unknown_chunks_and_padding() {
        let wav = riff(&[
            (b"LIST", b"INFOx".to_vec()),
            (b"fmt ", fmt_chunk(1, 16, 1)),
            (b"data", vec![0xAB, 0xCD]),
        ]);
        assert!(wav_pcm16_mono_data(&wav).is_ok());
    }

    #[test]
    fn rejects_non_riff_and_truncated_headers() {
        assert!(wav_pcm16_mono_data(b"\x00\x01\x02\x03 short").is_err());
        let mut wav = canonical_wav(&[1]);
        wav.truncate(10);
        assert!(wav_pcm16_mono_data(&wav).is_err());
    }

    #[test]
    fn rejects_stereo_and_non_pcm_and_missing_data() {
        let stereo = riff(&[(b"fmt ", fmt_chunk(2, 16, 1)), (b"data", vec![0, 0])]);
        assert!(wav_pcm16_mono_data(&stereo).is_err());

        let float32 = riff(&[(b"fmt ", fmt_chunk(1, 32, 3)), (b"data", vec![0; 4])]);
        assert!(wav_pcm16_mono_data(&float32).is_err());

        let no_data = riff(&[(b"fmt ", fmt_chunk(1, 16, 1))]);
        assert!(wav_pcm16_mono_data(&no_data).is_err());

        let no_fmt = riff(&[(b"data", vec![0, 0])]);
        assert!(wav_pcm16_mono_data(&no_fmt).is_err());
    }

    #[test]
    fn rejects_chunk_size_overrun() {
        let mut wav = canonical_wav(&[7]);
        let data_len_pos = wav.len() - 5;
        wav[data_len_pos..data_len_pos + 4].copy_from_slice(&9999u32.to_le_bytes());
        assert!(wav_pcm16_mono_data(&wav).is_err());
    }
}
