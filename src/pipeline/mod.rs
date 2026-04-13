pub mod decode;
pub mod features;
pub mod framing;
pub mod prompts;
pub mod resample;
pub mod segmenter;
pub mod tags;
pub mod vad;

use anyhow::Result;
use std::{path::Path, time::Instant};
use tracing::info;

use crate::config::AnalysisConfig;
use crate::types::{BenchmarkResult, StageDurations, TimelineOutput};

struct PipelineArtifacts {
    timeline: TimelineOutput,
    frame_count: usize,
    speech_segment_count: usize,
    stage_ms: StageDurations,
}

pub fn extract_timeline(input: &Path, cfg: &AnalysisConfig) -> Result<TimelineOutput> {
    info!(input = %input.display(), sample_rate = cfg.sample_rate_hz, frame_ms = cfg.frame_ms, "extract start");
    let total_start = Instant::now();
    let PipelineArtifacts {
        timeline,
        frame_count,
        speech_segment_count,
        stage_ms,
    } = run_pipeline(input, cfg)?;
    info!(
        total_ms = total_start.elapsed().as_millis(),
        frames = frame_count,
        speech_segments = speech_segment_count,
        non_voice_segments = timeline.segments.len(),
        decode_ms = stage_ms.decode_ms,
        vad_ms = stage_ms.vad_ms,
        "extract done"
    );
    Ok(timeline)
}

fn run_pipeline(input: &Path, cfg: &AnalysisConfig) -> Result<PipelineArtifacts> {
    let mut stage_ms = StageDurations::default();

    let decode_start = Instant::now();
    let (samples, source_rate) = decode::decode_audio(input)?;
    stage_ms.decode_ms = decode_start.elapsed().as_millis();
    info!(
        stage = "decode",
        ms = stage_ms.decode_ms,
        source_rate,
        samples = samples.len(),
        "stage complete"
    );

    let resample_start = Instant::now();
    let mono = resample::resample_linear(&samples, source_rate, cfg.sample_rate_hz);
    stage_ms.resample_ms = resample_start.elapsed().as_millis();
    info!(
        stage = "resample",
        ms = stage_ms.resample_ms,
        target_rate = cfg.sample_rate_hz,
        samples = mono.len(),
        "stage complete"
    );

    let frame_start = Instant::now();
    let frames = framing::build_frames(&mono, cfg.sample_rate_hz, cfg.frame_ms);
    stage_ms.frame_ms = frame_start.elapsed().as_millis();
    info!(
        stage = "frame",
        ms = stage_ms.frame_ms,
        frames = frames.len(),
        frame_ms = cfg.frame_ms,
        "stage complete"
    );
    let frame_count = frames.len();

    let vad_start = Instant::now();
    let speech = vad::classify_frames(&frames, cfg.energy_threshold);
    stage_ms.vad_ms = vad_start.elapsed().as_millis();
    let speech_frames = speech.iter().filter(|&&v| v).count();
    info!(
        stage = "vad",
        ms = stage_ms.vad_ms,
        speech_frames,
        threshold = cfg.energy_threshold,
        "stage complete"
    );

    let smooth_start = Instant::now();
    let smoothed = segmenter::smooth_speech(&speech, cfg.frame_ms, cfg.speech_hangover_ms);
    stage_ms.smooth_ms = smooth_start.elapsed().as_millis();
    let smoothed_frames = smoothed.iter().filter(|&&v| v).count();
    info!(
        stage = "smooth",
        ms = stage_ms.smooth_ms,
        speech_frames = smoothed_frames,
        hangover_ms = cfg.speech_hangover_ms,
        "stage complete"
    );

    let speech_stage = Instant::now();
    let speech_segments = segmenter::speech_segments(&smoothed, cfg.frame_ms, cfg.min_speech_ms);
    stage_ms.speech_ms = speech_stage.elapsed().as_millis();
    info!(
        stage = "speech_segments",
        ms = stage_ms.speech_ms,
        segments = speech_segments.len(),
        min_speech_ms = cfg.min_speech_ms,
        "stage complete"
    );

    let merge_start = Instant::now();
    let merged_speech = segmenter::merge_close_segments(&speech_segments, cfg.merge_gap_ms);
    stage_ms.merge_ms = merge_start.elapsed().as_millis();
    info!(
        stage = "merge_segments",
        ms = stage_ms.merge_ms,
        segments = merged_speech.len(),
        merge_gap_ms = cfg.merge_gap_ms,
        "stage complete"
    );
    let speech_segment_count = merged_speech.len();

    let total_audio_ms = mono.len() as u64 * 1000 / cfg.sample_rate_hz as u64;
    let invert_start = Instant::now();
    let non_voice =
        segmenter::invert_to_non_voice(&merged_speech, total_audio_ms, cfg.min_non_voice_ms);
    stage_ms.invert_ms = invert_start.elapsed().as_millis();
    info!(
        stage = "invert",
        ms = stage_ms.invert_ms,
        segments = non_voice.len(),
        total_ms = total_audio_ms,
        min_non_voice_ms = cfg.min_non_voice_ms,
        "stage complete"
    );

    let timeline = TimelineOutput {
        file: input
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        analysis_sample_rate: cfg.sample_rate_hz,
        frame_ms: cfg.frame_ms,
        segments: non_voice,
    };

    Ok(PipelineArtifacts {
        timeline,
        frame_count,
        speech_segment_count,
        stage_ms,
    })
}

pub fn benchmark_file(input: &Path) -> Result<BenchmarkResult> {
    let cfg = AnalysisConfig::default();
    let total_start = Instant::now();
    let PipelineArtifacts {
        timeline,
        frame_count,
        speech_segment_count: _,
        stage_ms,
    } = run_pipeline(input, &cfg)?;
    let segment_count = timeline.segments.len();
    Ok(BenchmarkResult {
        input_file: input.display().to_string(),
        total_ms: total_start.elapsed().as_millis(),
        decode_ms: stage_ms.decode_ms,
        frame_count,
        segment_count,
        stage_ms,
    })
}
