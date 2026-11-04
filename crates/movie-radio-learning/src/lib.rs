pub mod active_learning;
pub mod adaptive_thresholds;
pub mod calibrator;
pub mod corrections;
pub mod database;
pub mod gap_store;
pub mod pattern_similarity;
pub mod profiles;
pub mod review_feedback;
pub mod threshold_store;
pub mod trace_store;

#[allow(unused_imports)]
pub use database::{
    FalsePositive, LearningDb, LearningStatistics, SpectralFeatures, VerifiedSegment,
};
#[allow(unused_imports)]
pub use gap_store::GapDecision;
#[allow(unused_imports)]
pub use pattern_similarity::{PatternMatchResult, SpectralVector};
#[allow(unused_imports)]
pub use review_feedback::{
    ArtifactKind, CorrectionEvent, HoldoutFixture, ImportOptions, ImportSummary, LearningParameter,
    LearningProposal, LearningVersion, ModelVersions, OutcomeMetricRow, PrivacyPolicy,
    ProposalEvidence, ProposalKind, ProposalOrigin, ProposalStatus, ReviewDecision, ReviewEvidence,
    ReviewFeedbackBatch, ReviewFeedbackExport, ReviewLabel, VersionAction,
};
#[allow(unused_imports)]
pub use threshold_store::{ThresholdHistoryEntry, ThresholdRecommendation};
#[allow(unused_imports)]
pub use trace_store::{
    AdaptationLog, EmotionOutcome, LearningsExport, ProviderPerformance, RunTrace,
};
