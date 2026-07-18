pub mod config;
pub mod error;
pub mod features;
pub mod fingerprint;
pub mod frame;
pub mod metrics;
pub mod segment;
pub mod voice_types;

pub use config::{
    AnalysisConfig, ElevenLabsConfig, KokoroConfig, MergeOptions, MergeStrategy, OrpheusConfig,
    PocketTtsConfig, Qwen3Config, VoiceProvidersConfig, VoiceSynthesisConfig,
};
pub use error::TimelineError;
pub use features::FeatureSet;
pub use fingerprint::Fingerprint;
pub use frame::Frame;
pub use metrics::{BenchmarkResult, StageDurations};
pub use segment::{
    AiVoiceOutput, GapAnalysisOutput, Segment, SegmentEvent, SegmentKind, TimelineOutput, VisualGap,
};
pub use voice_types::{
    AudioOutput, Emotion, ProviderCapabilities, SynthesisRequest, VoiceSynthesizer,
};
