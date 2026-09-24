use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use std::env;
use std::time::Duration;
use tracing::warn;

use super::{AudioOutput, ProviderCapabilities, SynthesisRequest, VoiceSynthesizer};
use crate::config::{KokoroConfig, ENV_KOKORO_ENDPOINT_URL};

const MAX_CONNECT_ATTEMPTS: usize = 3;
const RETRY_DELAY: Duration = Duration::from_millis(250);
const DEFAULT_ENDPOINT_URL: &str = "http://127.0.0.1:8881";
const KOKORO_VOICE_ID: &str = "martin";

pub struct KokoroProvider {
    endpoint_url: String,
    client: Client,
}

impl KokoroProvider {
    pub fn new(_config: KokoroConfig) -> Self {
        let endpoint_override = env::var(ENV_KOKORO_ENDPOINT_URL)
            .ok()
            .filter(|url| !url.trim().is_empty());
        Self {
            endpoint_url: Self::resolve_endpoint_url(endpoint_override.as_deref()),
            client: Client::new(),
        }
    }

    fn resolve_endpoint_url(endpoint_override: Option<&str>) -> String {
        endpoint_override
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| DEFAULT_ENDPOINT_URL.to_string())
    }

    fn endpoint(&self) -> Result<String> {
        let base_url = self.endpoint_url.trim_end_matches('/');
        if base_url.is_empty() {
            anyhow::bail!("Kokoro endpoint_url must not be empty");
        }
        Ok(format!("{base_url}/v1/audio/speech"))
    }

    fn build_request(&self, request: &SynthesisRequest) -> Result<reqwest::RequestBuilder> {
        let speed = request.emotion.effective_speed(request.speed);
        let voice = request.voice_id.as_deref().unwrap_or(KOKORO_VOICE_ID);
        if voice != KOKORO_VOICE_ID {
            anyhow::bail!(
                "Kokoro German Martin sidecar supports only voice_id '{KOKORO_VOICE_ID}'"
            );
        }
        if request.language != "de" {
            anyhow::bail!(
                "Kokoro German Martin sidecar supports only language 'de', got '{}'",
                request.language
            );
        }

        Ok(self.client.post(self.endpoint()?).json(&serde_json::json!({
            "model": "kokoro-german-martin",
            "voice": voice,
            "input": request.text,
            "language": request.language,
            "response_format": "wav",
            "speed": speed,
        })))
    }

    async fn send_with_retry(&self, request: &SynthesisRequest) -> Result<reqwest::Response> {
        let endpoint = self.endpoint()?;
        let mut last_err = None;
        for attempt in 1..=MAX_CONNECT_ATTEMPTS {
            match self.build_request(request)?.send().await {
                Ok(response) => return Ok(response),
                Err(error) if error.is_connect() || error.is_timeout() => {
                    warn!(attempt, endpoint = %endpoint, error = %error, "Transient Kokoro sidecar failure");
                    last_err = Some(error);
                    if attempt < MAX_CONNECT_ATTEMPTS {
                        tokio::time::sleep(RETRY_DELAY).await;
                    }
                }
                Err(error) => return Err(error).context("Failed to send Kokoro synthesis request"),
            }
        }
        match last_err {
            Some(error) => Err(error).with_context(|| {
                format!(
                    "Kokoro sidecar unreachable after {MAX_CONNECT_ATTEMPTS} attempts: {endpoint}"
                )
            }),
            None => {
                anyhow::bail!("Kokoro retry loop exhausted without a transport error: {endpoint}")
            }
        }
    }

    fn validate_audio(samples: &[f32]) -> Result<()> {
        if samples.is_empty() {
            anyhow::bail!("Kokoro sidecar returned empty audio");
        }
        if samples.iter().any(|sample| !sample.is_finite()) {
            anyhow::bail!("Kokoro sidecar returned invalid audio");
        }
        if !samples.iter().any(|sample| *sample != 0.0) {
            anyhow::bail!("Kokoro sidecar returned silent audio");
        }
        Ok(())
    }
}

#[async_trait]
impl VoiceSynthesizer for KokoroProvider {
    async fn synthesize(&self, request: &SynthesisRequest) -> Result<AudioOutput> {
        let response = self.send_with_retry(request).await?;
        if !response.status().is_success() {
            let error_text = match response.text().await {
                Ok(text) => text,
                Err(error) => format!("failed to read sidecar error response: {error}"),
            };
            anyhow::bail!(
                "Kokoro sidecar error at {}: {}",
                self.endpoint()?,
                error_text
            );
        }

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        if !content_type.contains("audio") {
            anyhow::bail!("Kokoro sidecar returned unexpected content-type: {content_type}");
        }

        let bytes = response
            .bytes()
            .await
            .context("Failed to read Kokoro audio response")?;
        let samples = super::elevenlabs::decode_audio_bytes(&bytes, request.sample_rate_hz)
            .context("Failed to decode Kokoro audio response")?;
        Self::validate_audio(&samples)?;

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
    use std::io::{Read, Write};

    fn provider(endpoint_url: &str) -> KokoroProvider {
        KokoroProvider {
            endpoint_url: endpoint_url.to_string(),
            client: Client::new(),
        }
    }

    fn wav_bytes(samples: &[i16]) -> Vec<u8> {
        let data_len = std::mem::size_of_val(samples) as u32;
        let mut wav = Vec::with_capacity(44 + data_len as usize);
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data_len).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16_u32.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&24_000_u32.to_le_bytes());
        wav.extend_from_slice(&48_000_u32.to_le_bytes());
        wav.extend_from_slice(&2_u16.to_le_bytes());
        wav.extend_from_slice(&16_u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_len.to_le_bytes());
        for sample in samples {
            wav.extend_from_slice(&sample.to_le_bytes());
        }
        wav
    }

    #[test]
    fn rejects_empty_or_silent_audio() {
        assert!(KokoroProvider::validate_audio(&[]).is_err());
        assert!(KokoroProvider::validate_audio(&[0.0, f32::NAN]).is_err());
        assert!(KokoroProvider::validate_audio(&[0.25, f32::INFINITY]).is_err());
        assert!(KokoroProvider::validate_audio(&[0.0, 0.25]).is_ok());
    }

    #[test]
    fn endpoint_normalizes_trailing_slash() -> Result<()> {
        let provider = provider("http://127.0.0.1:8881/");
        assert_eq!(
            provider.endpoint()?,
            "http://127.0.0.1:8881/v1/audio/speech"
        );
        Ok(())
    }

    #[test]
    fn default_sidecar_endpoint_is_used_without_an_override() {
        assert_eq!(
            KokoroProvider::resolve_endpoint_url(None),
            DEFAULT_ENDPOINT_URL
        );
    }

    #[test]
    fn sidecar_endpoint_override_is_used_when_configured() {
        assert_eq!(
            KokoroProvider::resolve_endpoint_url(Some("http://127.0.0.1:9999")),
            "http://127.0.0.1:9999"
        );
    }

    #[test]
    fn rejects_unsupported_voice() {
        let provider = provider("http://127.0.0.1:8881");
        let request = SynthesisRequest {
            voice_id: Some("other-voice".to_string()),
            ..SynthesisRequest::default()
        };
        assert!(provider.build_request(&request).is_err());
    }

    #[test]
    fn rejects_unsupported_language() {
        let provider = provider("http://127.0.0.1:8881");
        let request = SynthesisRequest {
            language: "en".to_string(),
            ..SynthesisRequest::default()
        };
        assert!(provider.build_request(&request).is_err());
    }

    #[tokio::test]
    async fn requests_and_decodes_audio_from_sidecar() -> Result<()> {
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        let source_samples: Vec<i16> = (0..240)
            .map(|index| if index % 2 == 0 { 1_000 } else { -1_000 })
            .collect();
        let body = wav_bytes(&source_samples);
        let server = std::thread::spawn(move || -> Result<()> {
            let (mut stream, _) = listener.accept()?;
            let request = read_request(&mut stream)?;
            anyhow::ensure!(request.contains("POST /v1/audio/speech"));
            anyhow::ensure!(request.contains("\"voice\":\"martin\""));
            anyhow::ensure!(request.contains("\"input\":\"Hallo, Straße!\""));
            anyhow::ensure!(request.contains("\"language\":\"de\""));
            anyhow::ensure!(request.contains("\"response_format\":\"wav\""));
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: audio/wav\r\nContent-Length: {}\r\n\r\n",
                body.len()
            )?;
            stream.write_all(&body)?;
            Ok(())
        });

        let provider = provider(&format!("http://127.0.0.1:{port}"));
        let audio = provider
            .synthesize(&SynthesisRequest {
                text: "Hallo, Straße!".to_string(),
                ..SynthesisRequest::default()
            })
            .await?;
        assert!(!audio.samples.is_empty());
        assert_eq!(audio.sample_rate_hz, 16_000);
        server
            .join()
            .map_err(|_| anyhow::anyhow!("Kokoro test server panicked"))??;
        Ok(())
    }

    fn read_request(stream: &mut std::net::TcpStream) -> Result<String> {
        let mut request = Vec::new();
        let mut chunk = [0_u8; 1_024];
        loop {
            let len = stream.read(&mut chunk)?;
            if len == 0 {
                break;
            }
            request.extend_from_slice(&chunk[..len]);

            if let Some(header_end) = request.windows(4).position(|part| part == b"\r\n\r\n") {
                let header_end = header_end + 4;
                let headers = std::str::from_utf8(&request[..header_end])?;
                let content_len = headers
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length: "))
                    .context("sidecar request has no content-length")?
                    .parse::<usize>()?;
                if request.len() >= header_end + content_len {
                    break;
                }
            }
        }
        String::from_utf8(request).context("sidecar request must be UTF-8")
    }
}
