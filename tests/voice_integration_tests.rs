use movie_nonvoice_timeline::config::{
    ElevenLabsConfig, KokoroConfig, ModalConfig, OrpheusConfig, PocketTtsConfig, Qwen3Config,
};
use movie_nonvoice_timeline::voice::{
    elevenlabs::ElevenLabsProvider, kokoro::KokoroProvider, modal::ModalTtsProvider,
    orpheus::OrpheusProvider, pockettts::PocketTtsProvider, qwen3::Qwen3Provider, Emotion,
    SynthesisRequest, VoiceSynthesizer,
};
use std::path::PathBuf;

#[tokio::test]
async fn test_kokoro_synthesis() {
    let config = KokoroConfig {
        model_path: PathBuf::from("models/kokoro.onnx"),
        device: "cpu".to_string(),
    };
    let provider = KokoroProvider::new(config);
    let request = SynthesisRequest {
        text: "Hallo Welt".to_string(),
        emotion: Emotion::Joyful,
        language: "de".to_string(),
        ..Default::default()
    };
    let output = provider.synthesize(&request).await.unwrap();
    assert!(!output.samples.is_empty());
    assert_eq!(output.sample_rate_hz, 16000);
}

#[tokio::test]
async fn test_pockettts_synthesis() {
    let config = PocketTtsConfig {
        model_path: PathBuf::from("models/pockettts"),
        device: "cpu".to_string(),
    };
    let provider = PocketTtsProvider::new(config);
    let request = SynthesisRequest {
        text: "Guten Tag".to_string(),
        language: "de".to_string(),
        ..Default::default()
    };
    let output = provider.synthesize(&request).await.unwrap();
    assert!(!output.samples.is_empty());
}

#[tokio::test]
async fn test_qwen3_synthesis() {
    let config = Qwen3Config {
        model_path: PathBuf::from("models/qwen3.gguf"),
        vocoder_path: PathBuf::from("models/vocoder.onnx"),
        device: "auto".to_string(),
        voice_description: "Narrator".to_string(),
    };
    let provider = Qwen3Provider::new(config);
    let request = SynthesisRequest {
        text: "Es war einmal".to_string(),
        emotion: Emotion::Mysterious,
        language: "de".to_string(),
        ..Default::default()
    };
    let output = provider.synthesize(&request).await.unwrap();
    assert!(!output.samples.is_empty());
}

#[tokio::test]
async fn test_orpheus_synthesis() {
    let config = OrpheusConfig {
        model_path: PathBuf::from("models/orpheus.gguf"),
        device: "gpu".to_string(),
    };
    let provider = OrpheusProvider::new(config);
    let request = SynthesisRequest {
        text: "Vorsicht!".to_string(),
        emotion: Emotion::Angry,
        language: "de".to_string(),
        ..Default::default()
    };
    let output = provider.synthesize(&request).await.unwrap();
    assert!(!output.samples.is_empty());
}

#[tokio::test]
async fn test_elevenlabs_synthesis_no_api_key() {
    let config = ElevenLabsConfig {
        api_key_env: "NON_EXISTENT_KEY".to_string(),
        voice_id: "test_voice".to_string(),
        model: "eleven_multilingual_v3".to_string(),
        stability: 0.5,
        similarity_boost: 0.5,
    };
    let provider = ElevenLabsProvider::new(config);
    let request = SynthesisRequest {
        text: "Cloud synthesis".to_string(),
        ..Default::default()
    };
    let result = provider.synthesize(&request).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_orchestrator_fallback() {
    use movie_nonvoice_timeline::config::{VoiceProvidersConfig, VoiceSynthesisConfig};
    use movie_nonvoice_timeline::voice::SynthesisOrchestrator;

    let config = VoiceSynthesisConfig {
        provider: "elevenlabs".to_string(),
        fallback_chain: vec!["elevenlabs".to_string(), "kokoro".to_string()],
        emotion_mapping: true,
        language: "de".to_string(),
        voice_id: None,
        max_cost_per_run_usd: 1.0,
        providers: VoiceProvidersConfig {
            kokoro: Some(KokoroConfig {
                model_path: PathBuf::from("models/kokoro.onnx"),
                device: "cpu".to_string(),
            }),
            elevenlabs: Some(ElevenLabsConfig {
                api_key_env: "NON_EXISTENT_KEY".to_string(),
                voice_id: "test".to_string(),
                model: "model".to_string(),
                stability: 0.5,
                similarity_boost: 0.5,
            }),
            pockettts: None,
            qwen3: None,
            orpheus: None,
            modal: None,
        },
    };

    let orchestrator = SynthesisOrchestrator::new(config);
    let request = SynthesisRequest {
        text: "Testing fallback".to_string(),
        ..Default::default()
    };

    // ElevenLabs should fail (no API key), falling back to Kokoro (which returns mock audio)
    let output = orchestrator.synthesize(&request, None).await.unwrap();
    assert_eq!(output.samples[0], 0.1);
}

#[tokio::test]
async fn test_modal_synthesis_no_endpoint() {
    let config = ModalConfig {
        endpoint_url_env: "NON_EXISTENT_MODAL_URL".to_string(),
        max_monthly_cost: 25.0,
    };
    let provider = ModalTtsProvider::new(config);
    let request = SynthesisRequest {
        text: "Modal test".to_string(),
        ..Default::default()
    };
    let result = provider.synthesize(&request).await;
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Environment variable NON_EXISTENT_MODAL_URL not set"));
}
