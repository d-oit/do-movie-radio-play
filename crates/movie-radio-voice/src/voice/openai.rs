use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use std::env;
use std::time::Duration;

use super::{
    is_valid_voice_id, AudioOutput, ProviderCapabilities, SynthesisRequest,
    SynthesisValidationError, VoiceSynthesizer,
};
use crate::config::OpenAiConfig;

/// Attempts per synthesis for transient transport failures (connect/timeout).
const MAX_CONNECT_ATTEMPTS: usize = 3;
/// Backoff between connection retries.
const RETRY_DELAY: Duration = Duration::from_millis(250);

pub struct OpenAiTtsProvider {
    config: OpenAiConfig,
    client: Client,
}

impl OpenAiTtsProvider {
    pub fn new(config: OpenAiConfig) -> Self {
        Self {
            config,
            client: Client::new(),
        }
    }

    /// Full TTS endpoint derived from the configured API root.
    fn endpoint(&self) -> String {
        let base = self.config.base_url.trim_end_matches('/');
        format!("{}/audio/speech", base)
    }

    /// Resolves the bearer token from `api_key_env`, if one is configured.
    /// `Ok(None)` means "send no Authorization header" (local sidecars).
    fn auth_header(&self) -> Result<Option<String>> {
        match &self.config.api_key_env {
            Some(env_name) => env::var(env_name)
                .map(|token| Some(format!("Bearer {}", token)))
                .with_context(|| format!("Environment variable {} not set", env_name)),
            None => Ok(None),
        }
    }

    fn build_request(&self, request: &SynthesisRequest) -> Result<reqwest::RequestBuilder> {
        let voice = if let Some(ref voice_id) = request.voice_id {
            voice_id.clone()
        } else {
            self.config.voice.clone()
        };

        if !is_valid_voice_id(&voice) {
            return Err(SynthesisValidationError::InvalidVoiceId.into());
        }

        let mut req = self.client.post(self.endpoint()).json(&serde_json::json!({
            "model": self.config.model,
            "voice": voice,
            "input": request.text,
            "response_format": self.config.response_format,
            "speed": request.speed,
        }));
        if let Some(auth) = self.auth_header()? {
            req = req.header("Authorization", auth);
        }
        Ok(req)
    }

    /// Sends the request, retrying only transport-level failures
    /// (connection refused/reset, timeouts). HTTP error responses are
    /// never retried.
    async fn send_with_retry(&self, request: &SynthesisRequest) -> Result<reqwest::Response> {
        let endpoint = self.endpoint();
        let mut last_err: Option<reqwest::Error> = None;
        for attempt in 1..=MAX_CONNECT_ATTEMPTS {
            match self.build_request(request)?.send().await {
                Ok(response) => return Ok(response),
                Err(e) if e.is_connect() || e.is_timeout() => {
                    tracing::warn!(
                        attempt,
                        endpoint = %endpoint,
                        error = %e,
                        "Transient TTS transport failure"
                    );
                    last_err = Some(e);
                    if attempt < MAX_CONNECT_ATTEMPTS {
                        tokio::time::sleep(RETRY_DELAY).await;
                    }
                }
                Err(e) => {
                    return Err(e).context(format!("failed to send request to {}", endpoint));
                }
            }
        }
        match last_err {
            Some(err) => Err(err).with_context(|| {
                format!(
                    "TTS endpoint unreachable after {} attempts: {}",
                    MAX_CONNECT_ATTEMPTS, endpoint
                )
            }),
            None => anyhow::bail!(
                "TTS retry loop at {} exhausted without a captured transport error",
                endpoint
            ),
        }
    }
}

#[async_trait]
impl VoiceSynthesizer for OpenAiTtsProvider {
    async fn synthesize(&self, request: &SynthesisRequest) -> Result<AudioOutput> {
        let response = self.send_with_retry(request).await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!(
                "OpenAI-compatible TTS API error at {}: {}",
                self.endpoint(),
                error_text
            );
        }

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        let bytes = response
            .bytes()
            .await
            .context("Failed to read OpenAI TTS response bytes")?;

        let samples = if content_type.contains("audio") {
            super::elevenlabs::decode_audio_bytes(&bytes, request.sample_rate_hz)
                .context("Failed to decode OpenAI audio response")?
        } else {
            anyhow::bail!("Unexpected response content-type: {}", content_type);
        };

        Ok(AudioOutput {
            samples,
            sample_rate_hz: request.sample_rate_hz,
        })
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_emotion: false,
            supports_voice_cloning: false,
            supports_streaming: true,
            max_text_length: 4096,
            languages: vec![
                "de".to_string(),
                "en".to_string(),
                "es".to_string(),
                "fr".to_string(),
                "it".to_string(),
                "pt".to_string(),
                "pl".to_string(),
                "tr".to_string(),
                "ru".to_string(),
                "nl".to_string(),
                "cs".to_string(),
                "ar".to_string(),
                "zh".to_string(),
                "ja".to_string(),
                "ko".to_string(),
            ],
            requires_gpu: false,
        }
    }

    fn estimate_cost(&self, text_len: usize) -> f64 {
        let price_per_char = if self.config.model == "tts-1-hd" {
            0.000030
        } else {
            0.000015
        };
        (text_len as f64) * price_per_char
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::default_openai_base_url;

    fn config(base_url: Option<&str>, api_key_env: Option<&str>) -> OpenAiConfig {
        OpenAiConfig {
            api_key_env: api_key_env.map(str::to_string),
            base_url: base_url
                .map(str::to_string)
                .unwrap_or_else(default_openai_base_url),
            model: "tts-1-hd".to_string(),
            voice: "onyx".to_string(),
            response_format: "mp3".to_string(),
        }
    }

    #[test]
    fn test_build_request_rejects_invalid_voice_id() {
        let invalid_cfg = OpenAiConfig {
            api_key_env: None,
            base_url: "https://api.openai.com/v1".to_string(),
            model: "tts-1".to_string(),
            voice: "../invalid_voice".to_string(),
            response_format: "mp3".to_string(),
        };
        let provider = OpenAiTtsProvider::new(invalid_cfg);
        let req = SynthesisRequest::default();
        let res = provider.build_request(&req);
        assert!(res.is_err());
        assert_eq!(
            res.err()
                .unwrap()
                .downcast::<SynthesisValidationError>()
                .unwrap(),
            SynthesisValidationError::InvalidVoiceId
        );

        let valid_cfg = config(None, None);
        let provider = OpenAiTtsProvider::new(valid_cfg);
        let req_override = SynthesisRequest {
            voice_id: Some("voice/path/traversal".to_string()),
            ..SynthesisRequest::default()
        };
        let res_override = provider.build_request(&req_override);
        assert!(res_override.is_err());
        assert_eq!(
            res_override
                .err()
                .unwrap()
                .downcast::<SynthesisValidationError>()
                .unwrap(),
            SynthesisValidationError::InvalidVoiceId
        );
    }

    #[test]
    fn test_build_request_accepts_valid_voice_id() {
        let provider = OpenAiTtsProvider::new(config(None, None));
        let req = SynthesisRequest::default();
        assert!(provider.build_request(&req).is_ok());

        let req_override = SynthesisRequest {
            voice_id: Some("alloy".to_string()),
            ..SynthesisRequest::default()
        };
        assert!(provider.build_request(&req_override).is_ok());
    }

    #[test]
    fn test_default_endpoint_is_public_api() {
        let provider = OpenAiTtsProvider::new(config(None, None));
        assert_eq!(
            provider.endpoint(),
            "https://api.openai.com/v1/audio/speech"
        );
    }

    #[test]
    fn test_endpoint_tolerates_trailing_slash() {
        let provider = OpenAiTtsProvider::new(config(Some("http://127.0.0.1:8080/v1/"), None));
        assert_eq!(provider.endpoint(), "http://127.0.0.1:8080/v1/audio/speech");
    }

    #[test]
    fn test_no_auth_when_api_key_env_absent() {
        let provider = OpenAiTtsProvider::new(config(None, None));
        assert_eq!(provider.auth_header().unwrap(), None);
    }

    #[test]
    fn test_auth_header_from_env() {
        const ENV_NAME: &str = "OPENAI_TTS_TEST_TOKEN";
        std::env::set_var(ENV_NAME, "secret-token");
        let provider = OpenAiTtsProvider::new(config(None, Some(ENV_NAME)));
        assert_eq!(
            provider.auth_header().unwrap(),
            Some("Bearer secret-token".to_string())
        );
        std::env::remove_var(ENV_NAME);
        assert!(provider.auth_header().is_err());
    }

    #[test]
    fn test_serde_defaults_apply_for_local_sidecar() {
        let json = r#"{
            "model": "pocket-tts",
            "voice": "alba",
            "response_format": "wav"
        }"#;
        let cfg: OpenAiConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.base_url, "https://api.openai.com/v1");
        assert_eq!(cfg.api_key_env, None);
    }

    fn sidecar_provider(port: u16) -> OpenAiTtsProvider {
        OpenAiTtsProvider::new(OpenAiConfig {
            api_key_env: None,
            base_url: format!("http://127.0.0.1:{port}/v1"),
            model: "pocket-tts".to_string(),
            voice: "alba".to_string(),
            response_format: "wav".to_string(),
        })
    }

    #[tokio::test]
    async fn test_connection_failure_retries_then_reports_endpoint() {
        // Bind then drop: nothing listens on this port anymore.
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let provider = sidecar_provider(port);
        let request = SynthesisRequest {
            text: "Hallo".to_string(),
            ..SynthesisRequest::default()
        };

        let err = provider
            .send_with_retry(&request)
            .await
            .expect_err("dead endpoint must fail");

        let msg = err.to_string();
        assert!(msg.contains("unreachable after 3 attempts"), "{}", msg);
        assert!(msg.contains(&format!("127.0.0.1:{port}")), "{}", msg);
    }

    #[tokio::test]
    async fn test_http_error_response_is_not_retried() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().expect("exactly one connection expected");
            let mut stream = stream;
            use std::io::Write;
            stream
                .write_all(b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 3\r\n\r\nbad")
                .unwrap();
        });

        let provider = sidecar_provider(port);
        let request = SynthesisRequest {
            text: "Hallo".to_string(),
            ..SynthesisRequest::default()
        };
        let err = provider
            .synthesize(&request)
            .await
            .expect_err("HTTP 500 must surface as error");

        assert!(
            err.to_string()
                .contains(&format!("http://127.0.0.1:{port}/v1/audio/speech")),
            "{}",
            err
        );
        server.join().unwrap();
    }
}
