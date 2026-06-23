use std::path::Path;

use anyhow::Result;

use super::{run_pipeline, PipelineArtifacts};
use movie_radio_types::{AnalysisConfig, BenchmarkResult};

pub fn benchmark_file(input: &Path, cfg: &AnalysisConfig) -> Result<BenchmarkResult> {
    let total_start = std::time::Instant::now();
    let PipelineArtifacts {
        timeline,
        frame_count,
        stage_ms,
        ..
    } = run_pipeline(input, cfg)?;
    let segment_count = timeline.segments.len();
    Ok(BenchmarkResult {
        input_file: input.display().to_string(),
        total_ms: total_start.elapsed().as_millis() as u64,
        decode_ms: stage_ms.decode_ms,
        frame_count,
        segment_count,
        stage_ms,
    })
}
