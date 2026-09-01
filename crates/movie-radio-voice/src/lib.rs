pub mod config;
pub mod voice;

pub use config::{
    AudioCppConfig, AudioCppLocalConfig, AudioCppRemoteConfig, ElevenLabsConfig, GpuPolicyConfig,
    GpuPoolEndpoint, KokoroConfig, ModalConfig, OpenAiConfig, OrpheusConfig, PocketTtsConfig,
    Qwen3Config, VoiceProvidersConfig, VoiceSynthesisConfig,
};
pub use voice::{
    AudioOutput, Emotion, ProviderCapabilities, SynthesisRequest, SynthesisValidationError,
    VoiceSynthesizer,
};
