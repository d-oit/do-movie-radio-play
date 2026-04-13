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
use crate::types::{BenchmarkResult, TimelineOutput};

pub fn extract_timeline(input: &Path, cfg: &AnalysisConfig) -> Result<TimelineOutput> {
    info!(input = %input.display(), sample_rate = cfg.sample_rate_hz, frame_ms = cfg.frame_ms, "extract start");
    let stage = Instant::now();
    let (samples, source_rate) = decode::decode_audio(input)?;
    info!(
        ms = stage.elapsed().as_millis(),
        source_rate,
        samples = samples.len(),
        "decode stage done"
    );

    let mono16k = resample::resample_linear(&samples, source_rate, cfg.sample_rate_hz);
    let frames = framing::build_frames(&mono16k, cfg.sample_rate_hz, cfg.frame_ms);
    let speech = vad::classify_frames(&frames, cfg.energy_threshold);
    let smoothed = segmenter::smooth_speech(&speech, cfg.frame_ms, cfg.speech_hangover_ms);
    let speech_segments = segmenter::speech_segments(&smoothed, cfg.frame_ms, cfg.min_speech_ms);
    let merged_speech = segmenter::merge_close_segments(&speech_segments, cfg.merge_gap_ms);
    let non_voice = segmenter::invert_to_non_voice(
        &merged_speech,
        mono16k.len() as u64 * 1000 / cfg.sample_rate_hz as u64,
        cfg.min_non_voice_ms,
    );
    info!(
        frames = frames.len(),
        speech_segments = merged_speech.len(),
        non_voice_segments = non_voice.len(),
        "extract done"
    );
    Ok(TimelineOutput {
        file: input
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        analysis_sample_rate: cfg.sample_rate_hz,
        frame_ms: cfg.frame_ms,
        segments: non_voice,
    })
}

pub fn benchmark_file(input: &Path) -> Result<BenchmarkResult> {
    let start = Instant::now();
    let decode_start = Instant::now();
    let (samples, source_rate) = decode::decode_audio(input)?;
    let decode_ms = decode_start.elapsed().as_millis();
    let mono16k = resample::resample_linear(&samples, source_rate, 16000);
    let frames = framing::build_frames(&mono16k, 16000, 20);
    let speech = vad::classify_frames(&frames, 0.015);
    let smoothed = segmenter::smooth_speech(&speech, 20, 300);
    let speech_segments = segmenter::speech_segments(&smoothed, 20, 120);
    let merged = segmenter::merge_close_segments(&speech_segments, 250);
    let non_voice =
        segmenter::invert_to_non_voice(&merged, mono16k.len() as u64 * 1000 / 16000, 1000);
    Ok(BenchmarkResult {
        input_file: input.display().to_string(),
        total_ms: start.elapsed().as_millis(),
        decode_ms,
        frame_count: frames.len(),
        segment_count: non_voice.len(),
    })
}
