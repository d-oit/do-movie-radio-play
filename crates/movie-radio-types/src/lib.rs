pub mod app_config;
pub mod compute;
pub mod config;
pub mod error;
pub mod features;
pub mod fingerprint;
pub mod frame;
pub mod metrics;
pub mod narrator_types;
pub mod provider_registry;
pub mod segment;
pub mod sfx_types;
pub mod validation;
pub mod voice_clone;
pub mod voice_types;

pub use app_config::{
    AppConfig, CharacterConfig, NarratorAnthropicConfig, NarratorConfig, NarratorOllamaConfig,
    NarratorOpenAiConfig, OutputConfig, PipelineConfig, ProvidersConfig, VoiceCloneConfig,
    VoiceCloneRoutingConfig, VoiceConfig,
};
pub use compute::{ComputeEndpoint, ExecutionLocation};
pub use config::{
    AnalysisConfig, AudioCppConfig, AudioCppLocalConfig, AudioCppRemoteConfig, ElevenLabsConfig,
    GpuPolicyConfig, GpuPoolEndpoint, KokoroConfig, MergeOptions, MergeStrategy, OrpheusConfig,
    PocketTtsConfig, Qwen3Config, VoiceProvidersConfig, VoiceSynthesisConfig,
};
pub use error::TimelineError;
pub use features::FeatureSet;
pub use fingerprint::Fingerprint;
pub use frame::Frame;
pub use metrics::{BenchmarkResult, StageDurations};
pub use narrator_types::{parse_narrator_style, NarratorParams, NarratorStyle, RenderedPrompt};
pub use provider_registry::{ProviderEntry, ProviderRegistry};
pub use segment::{
    AiVoiceOutput, GapAnalysisOutput, Segment, SegmentKind, TimelineOutput, VisualGap,
};
pub use sfx_types::{
    AiGenerateConfig, FreesoundConfig, LocalSfxConfig, SfxCandidate, SfxLicense,
    SfxProviderCapabilities, SfxQuery, SfxTrigger, SoundEffectsConfig,
};
pub use voice_clone::{VoiceReference, VoiceReferenceParams};
pub use voice_types::{
    AudioOutput, Emotion, ProviderCapabilities, SynthesisRequest, VoiceSynthesizer,
};
