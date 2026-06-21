pub mod frame;
pub mod metrics;
pub mod segment;

pub use metrics::{BenchmarkResult, StageDurations};
pub use segment::{
    AiVoiceOutput, GapAnalysisOutput, Segment, SegmentKind, TimelineOutput, VisualGap,
};
