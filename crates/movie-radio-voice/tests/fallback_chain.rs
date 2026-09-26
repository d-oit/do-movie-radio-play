use anyhow::Result;
use movie_radio_voice::voice::{
    AudioOutput, ProviderCapabilities, SynthesisOrchestrator, SynthesisRequest, VoiceSynthesizer,
};

fn request(text: &str, speed: f32, sample_rate_hz: u32) -> SynthesisRequest {
    SynthesisRequest {
        text: text.to_string(),
        emotion: movie_radio_voice::Emotion::Neutral,
        voice_id: None,
        reference_audio: None,
        language: "de".to_string(),
        speed,
        sample_rate_hz,
    }
}

struct FakeProvider {
    cap: usize,
    fail: bool,
}

#[async_trait::async_trait]
impl VoiceSynthesizer for FakeProvider {
    async fn synthesize(&self, request: &SynthesisRequest) -> Result<AudioOutput> {
        if self.fail {
            anyhow::bail!("injected failure");
        }
        Ok(AudioOutput {
            samples: vec![0.0; 8],
            sample_rate_hz: request.sample_rate_hz,
            is_synthetic_placeholder: false,
        })
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_emotion: false,
            supports_voice_cloning: false,
            supports_streaming: false,
            max_text_length: self.cap,
            languages: vec!["de".to_string()],
            requires_gpu: false,
        }
    }

    fn estimate_cost(&self, _text_len: usize) -> f64 {
        0.0
    }
}

fn orchestrator_with(
    providers: Vec<(&str, FakeProvider)>,
    chain: &[&str],
) -> SynthesisOrchestrator {
    let mut map = std::collections::HashMap::new();
    for (id, provider) in providers {
        map.insert(
            id.to_string(),
            Box::new(provider) as Box<dyn VoiceSynthesizer>,
        );
    }
    SynthesisOrchestrator::from_test_providers(map, chain)
}

#[tokio::test]
async fn test_synthesize_reports_serving_provider() {
    let orchestrator = orchestrator_with(
        vec![(
            "a",
            FakeProvider {
                cap: 5,
                fail: false,
            },
        )],
        &["a"],
    );
    // `synthesize` keeps working through the delegating wrapper.
    let out = orchestrator
        .synthesize(&request("abc", 1.0, 16_000))
        .await
        .expect("serve");
    assert_eq!(out.sample_rate_hz, 16_000);
    let (_, provider) = orchestrator
        .synthesize_with_provider(&request("abc", 1.0, 16_000))
        .await
        .expect("serve with label");
    assert_eq!(provider, "a");
}

#[tokio::test]
async fn test_falls_back_when_text_exceeds_provider_cap() {
    let orchestrator = orchestrator_with(
        vec![
            (
                "small",
                FakeProvider {
                    cap: 5,
                    fail: false,
                },
            ),
            (
                "big",
                FakeProvider {
                    cap: 10_000,
                    fail: false,
                },
            ),
        ],
        &["small", "big"],
    );
    let (output, provider) = orchestrator
        .synthesize_with_provider(&request("a".repeat(10).as_str(), 1.0, 16_000))
        .await
        .expect("second provider must serve the request");
    assert_eq!(output.sample_rate_hz, 16_000);
    assert_eq!(provider, "big");
}

#[tokio::test]
async fn test_errors_when_no_provider_cap_fits() {
    let orchestrator = orchestrator_with(
        vec![
            (
                "a",
                FakeProvider {
                    cap: 5,
                    fail: false,
                },
            ),
            (
                "b",
                FakeProvider {
                    cap: 6,
                    fail: false,
                },
            ),
        ],
        &["a", "b"],
    );
    let err = orchestrator
        .synthesize(&request("abcdefg", 1.0, 16_000))
        .await
        .expect_err("no provider can fit the text");
    assert!(err.to_string().contains("cap of"), "{}", err);
}
