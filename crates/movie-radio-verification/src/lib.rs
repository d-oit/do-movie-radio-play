pub mod verification;

pub use verification::{
    default_filter_segment_confidence_ceiling, filter_low_confidence_non_voice_segments,
    verify_audio_preservation_files, verify_audio_preservation_samples,
    verify_audio_preservation_sources, verify_timeline, AppliedThresholds, ContentProvenance,
    FileIntegrityResult, GainDuckingOperation, LicenseProvenanceVerdict, PreservationConfig,
    PreservationReport, PreservationSummary, RawF32Source, SampleSource, TimeRange, TrackMap,
    TrackMapEntry, TrackType, VerificationReport, VerificationStatus, WavSampleSource, WindowType,
    WindowVerificationVerdict,
};
