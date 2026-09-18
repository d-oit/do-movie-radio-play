use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::ops::RangeInclusive;
use std::path::PathBuf;

/// Maximum accepted text length per synthesis request, in characters.
pub const MAX_REQUEST_TEXT_CHARS: usize = 10_000;
/// Maximum accepted voice ID length, in characters.
pub const MAX_VOICE_ID_CHARS: usize = 128;
/// Maximum accepted language tag length, in characters.
pub const MAX_LANGUAGE_CHARS: usize = 32;
/// Inclusive bounds for `sample_rate_hz`, in Hz.
pub const SAMPLE_RATE_RANGE_HZ: RangeInclusive<u32> = 8_000..=48_000;
/// Inclusive bounds for `speed`.
pub const SPEED_RANGE: RangeInclusive<f32> = 0.25..=4.0;

pub mod audio_cpp;
pub mod elevenlabs;
pub mod kokoro;
pub mod modal;
pub mod openai;
pub mod orpheus;
pub mod qwen3;

#[async_trait]
pub trait VoiceSynthesizer: Send + Sync {
    /// Synthesize text with emotion to audio samples
    async fn synthesize(&self, request: &SynthesisRequest) -> Result<AudioOutput>;

    /// Provider capabilities
    fn capabilities(&self) -> ProviderCapabilities;

    /// Estimated cost for a request (0.0 for local)
    fn estimate_cost(&self, text_len: usize) -> f64;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthesisRequest {
    pub text: String,
    pub emotion: Emotion,
    pub voice_id: Option<String>,
    /// Optional reference-audio clip for voice cloning (ADR-0125).
    /// Honored by the audio.cpp provider: staged to `--voice-ref` for CLI
    /// synthesis, read and base64-embedded for its remote endpoint (never a
    /// raw local path). Providers without cloning support are skipped for
    /// such requests.
    #[serde(default)]
    pub reference_audio: Option<PathBuf>,
    #[serde(default = "default_language")]
    pub language: String,
    pub speed: f32,          // 0.25 - 4.0
    pub sample_rate_hz: u32, // output sample rate
}

/// Errors returned when a [`SynthesisRequest`] fails pre-dispatch validation.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum SynthesisValidationError {
    #[error("sample_rate_hz {0} Hz outside supported range 8_000..=48_000 Hz")]
    SampleRateOutOfRange(u32),
    #[error("speed {0} outside supported finite range 0.25..=4.0")]
    SpeedOutOfRange(f32),
    #[error("text length {0} chars exceeds maximum of {MAX_REQUEST_TEXT_CHARS}")]
    TextTooLong(usize),
    #[error("voice_id is empty, contains invalid characters, or exceeds maximum length of {MAX_VOICE_ID_CHARS}")]
    InvalidVoiceId,
    #[error("language tag contains invalid characters or exceeds maximum length of {MAX_LANGUAGE_CHARS}")]
    InvalidLanguage,
}

fn is_valid_voice_id_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')
}

pub(crate) fn is_valid_voice_id(id: &str) -> bool {
    !id.is_empty()
        && id.chars().count() <= MAX_VOICE_ID_CHARS
        && id.chars().all(is_valid_voice_id_char)
}

fn is_valid_language_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '_')
}

fn is_valid_language(lang: &str) -> bool {
    !lang.is_empty()
        && lang.chars().count() <= MAX_LANGUAGE_CHARS
        && lang.chars().all(is_valid_language_char)
}

impl SynthesisRequest {
    /// Caller-side validation. Optional but recommended: only the voice ID
    /// is defensively re-validated by providers at dispatch; speed, sample
    /// rate, text length, and language rely on this call.
    pub fn validate(&self) -> Result<(), SynthesisValidationError> {
        if !SAMPLE_RATE_RANGE_HZ.contains(&self.sample_rate_hz) {
            return Err(SynthesisValidationError::SampleRateOutOfRange(
                self.sample_rate_hz,
            ));
        }
        if !SPEED_RANGE.contains(&self.speed) {
            return Err(SynthesisValidationError::SpeedOutOfRange(self.speed));
        }
        let len = self.text.chars().count();
        if len > MAX_REQUEST_TEXT_CHARS {
            return Err(SynthesisValidationError::TextTooLong(len));
        }
        if let Some(ref voice_id) = self.voice_id {
            if !is_valid_voice_id(voice_id) {
                return Err(SynthesisValidationError::InvalidVoiceId);
            }
        }
        if !is_valid_language(&self.language) {
            return Err(SynthesisValidationError::InvalidLanguage);
        }
        Ok(())
    }
}

fn default_language() -> String {
    "de".to_string()
}

impl Default for SynthesisRequest {
    fn default() -> Self {
        Self {
            text: String::default(),
            emotion: Emotion::Neutral,
            voice_id: None,
            reference_audio: None,
            language: default_language(),
            speed: 1.0,
            sample_rate_hz: 16000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Emotion {
    Neutral,
    Excited,
    Sad,
    Tense,
    Mysterious,
    Joyful,
    Whisper,
    Angry,
    Custom(String),
}

#[derive(Debug, Clone)]
pub struct AudioOutput {
    pub samples: Vec<f32>,
    pub sample_rate_hz: u32,
}

pub struct ProviderCapabilities {
    pub supports_emotion: bool,
    pub supports_voice_cloning: bool,
    pub supports_streaming: bool,
    pub max_text_length: usize,
    pub languages: Vec<String>,
    pub requires_gpu: bool,
}

pub struct SynthesisOrchestrator {
    providers: std::collections::HashMap<String, Box<dyn VoiceSynthesizer>>,
    fallback_chain: Vec<String>,
}

impl SynthesisOrchestrator {
    pub fn new(config: crate::config::VoiceSynthesisConfig) -> Self {
        let mut providers: std::collections::HashMap<String, Box<dyn VoiceSynthesizer>> =
            std::collections::HashMap::new();

        if let Some(c) = config.providers.kokoro {
            providers.insert(
                "kokoro".to_string(),
                Box::new(kokoro::KokoroProvider::new(c)),
            );
        }
        if let Some(c) = config.providers.qwen3 {
            providers.insert("qwen3".to_string(), Box::new(qwen3::Qwen3Provider::new(c)));
        }
        if let Some(c) = config.providers.orpheus {
            providers.insert(
                "orpheus".to_string(),
                Box::new(orpheus::OrpheusProvider::new(c)),
            );
        }
        if let Some(c) = config.providers.elevenlabs {
            providers.insert(
                "elevenlabs".to_string(),
                Box::new(elevenlabs::ElevenLabsProvider::new(c)),
            );
        }
        if let Some(c) = config.providers.modal {
            providers.insert(
                "modal".to_string(),
                Box::new(modal::ModalTtsProvider::new(c)),
            );
        }
        if let Some(c) = config.providers.openai {
            providers.insert(
                "openai".to_string(),
                Box::new(openai::OpenAiTtsProvider::new(c)),
            );
        }
        if let Some(c) = config.providers.audio_cpp {
            providers.insert(
                "audio_cpp".to_string(),
                Box::new(audio_cpp::AudioCppProvider::new(c)),
            );
        }

        Self {
            providers,
            fallback_chain: config.fallback_chain,
        }
    }

    pub async fn synthesize(&self, request: &SynthesisRequest) -> Result<AudioOutput> {
        let (output, _) = self.synthesize_with_provider(request).await?;
        Ok(output)
    }

    /// Build an orchestrator from prebuilt providers (tests and trace attribution).
    pub fn from_test_providers(
        providers: std::collections::HashMap<String, Box<dyn VoiceSynthesizer>>,
        chain: &[&str],
    ) -> Self {
        Self {
            providers,
            fallback_chain: chain.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// Synthesize, reporting which fallback-chain provider served the
    /// request so learning traces can attribute outcomes honestly.
    pub async fn synthesize_with_provider(
        &self,
        request: &SynthesisRequest,
    ) -> Result<(AudioOutput, String)> {
        request.validate()?;
        let text_chars = request.text.chars().count();
        let mut last_err = anyhow::anyhow!("No provider available in fallback chain");

        let wants_clone = request
            .reference_audio
            .as_deref()
            .is_some_and(|p| !p.as_os_str().is_empty());
        for provider_id in &self.fallback_chain {
            if let Some(provider) = self.providers.get(provider_id) {
                // Only the audio.cpp provider consumes `reference_audio`
                // (CLI --voice-ref / remote base64); every other provider
                // would silently drop the clip, so skip them for clone
                // requests instead of returning uncloned audio.
                if wants_clone && provider_id != "audio_cpp" {
                    tracing::warn!(
                        provider_id,
                        "Skipping provider that ignores reference_audio for clone request"
                    );
                    last_err = anyhow::anyhow!(
                        "provider '{}' does not honor reference_audio",
                        provider_id
                    );
                    continue;
                }
                let cap = provider.capabilities().max_text_length;
                if text_chars > cap {
                    tracing::warn!(
                        provider_id,
                        cap,
                        text_chars,
                        "Text exceeds provider cap; trying next in fallback chain"
                    );
                    last_err = anyhow::anyhow!(
                        "text length {} exceeds provider '{}' cap of {} chars",
                        text_chars,
                        provider_id,
                        cap
                    );
                    continue;
                }
                match provider.synthesize(request).await {
                    Ok(output) => return Ok((output, provider_id.clone())),
                    Err(e) => {
                        tracing::warn!("Provider {} failed: {}", provider_id, e);
                        last_err = e;
                    }
                }
            }
        }

        Err(last_err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(text: &str, speed: f32, sample_rate_hz: u32) -> SynthesisRequest {
        SynthesisRequest {
            text: text.to_string(),
            emotion: Emotion::Neutral,
            voice_id: None,
            reference_audio: None,
            language: default_language(),
            speed,
            sample_rate_hz,
        }
    }

    #[test]
    fn test_default_request_is_valid() {
        assert_eq!(SynthesisRequest::default().validate(), Ok(()));
    }

    #[test]
    fn test_sample_rate_boundaries() {
        // Absolute pins against the issue contract (8_000..=48_000 Hz).
        assert_eq!(request("hi", 1.0, 8_000).validate(), Ok(()));
        assert_eq!(request("hi", 1.0, 48_000).validate(), Ok(()));
        for rate in [7_999, 48_001] {
            assert_eq!(
                request("hi", 1.0, rate).validate(),
                Err(SynthesisValidationError::SampleRateOutOfRange(rate))
            );
        }
    }

    #[test]
    fn test_speed_boundaries() {
        // Absolute pins against the issue contract (0.25..=4.0), independent
        // of SPEED_RANGE so a wrong constant cannot self-validate.
        assert_eq!(request("hi", 0.25, 16_000).validate(), Ok(()));
        assert_eq!(request("hi", 4.0, 16_000).validate(), Ok(()));
        for speed in [0.24, 4.5] {
            assert!(matches!(
                request("hi", speed, 16_000).validate(),
                Err(SynthesisValidationError::SpeedOutOfRange(_))
            ));
        }
    }

    #[test]
    fn test_speed_must_be_finite() {
        for speed in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(matches!(
                request("hi", speed, 16_000).validate(),
                Err(SynthesisValidationError::SpeedOutOfRange(_))
            ));
        }
    }

    #[test]
    fn test_text_length_cap() {
        let max = "a".repeat(MAX_REQUEST_TEXT_CHARS);
        assert_eq!(request(&max, 1.0, 16_000).validate(), Ok(()));

        let over = "a".repeat(MAX_REQUEST_TEXT_CHARS + 1);
        assert_eq!(
            request(&over, 1.0, 16_000).validate(),
            Err(SynthesisValidationError::TextTooLong(
                MAX_REQUEST_TEXT_CHARS + 1
            ))
        );
    }

    #[test]
    fn test_multibyte_chars_counted_as_chars_not_bytes() {
        // 'ä' is 2 bytes in UTF-8; 10_000 chars must pass despite 20_000 bytes.
        let text = "ä".repeat(MAX_REQUEST_TEXT_CHARS);
        assert_eq!(request(&text, 1.0, 16_000).validate(), Ok(()));
    }

    #[test]
    fn test_sample_rate_checked_before_text() {
        let over = "a".repeat(MAX_REQUEST_TEXT_CHARS + 1);
        assert!(matches!(
            request(&over, 1.0, 192_000).validate(),
            Err(SynthesisValidationError::SampleRateOutOfRange(192_000))
        ));
    }

    #[test]
    fn test_voice_id_validation() {
        let mut req = SynthesisRequest::default();

        for valid_id in ["pNInz6obpgDQGcFmaJgB", "onyx", "voice-123_abc.v1"] {
            req.voice_id = Some(valid_id.to_string());
            assert_eq!(req.validate(), Ok(()));
        }

        let long_id = "a".repeat(MAX_VOICE_ID_CHARS + 1);
        for invalid_id in [
            "",
            &long_id,
            "../admin",
            "voice/id",
            "voice\nid",
            "voice?param=1",
            "voice@host",
            "voice:scheme",
        ] {
            req.voice_id = Some(invalid_id.to_string());
            assert_eq!(
                req.validate(),
                Err(SynthesisValidationError::InvalidVoiceId)
            );
        }
    }

    #[test]
    fn test_language_validation() {
        let mut req = SynthesisRequest::default();

        for valid_lang in ["de", "en", "en-US", "zh_CN"] {
            req.language = valid_lang.to_string();
            assert_eq!(req.validate(), Ok(()));
        }

        let long_lang = "a".repeat(MAX_LANGUAGE_CHARS + 1);
        for invalid_lang in ["", &long_lang, "de;cat /etc/passwd", "de\r\n", "en.US"] {
            req.language = invalid_lang.to_string();
            assert_eq!(
                req.validate(),
                Err(SynthesisValidationError::InvalidLanguage)
            );
        }
    }
}
