use anyhow::Result;
use std::path::Path;
use std::time::Instant;
use tracing::info;

use crate::pipeline::decode::decode_audio_chunks_cb;
use crate::pipeline::framing::FrameStreamer;
use crate::pipeline::tri_state;
use crate::pipeline::vad::{adapt_spectral_thresholds, classify_with_engine, create_engine};
use movie_radio_types::{AnalysisConfig, Frame, StageDurations, TimelineOutput};

/// Streams decode and framing in bounded chunks, then classifies and runs the
/// same post-VAD stages as whole-file extraction over the retained frames.
///
/// Peak memory no longer scales with the decoded `f32` sample buffer: only
/// per-frame features (32 bytes per frame, ~12 MB for a 2 h film at 20 ms
/// frames), one likelihood and one frame state per frame are kept. Frames are
/// built by a [`FrameStreamer`], so spectral flux and the adaptive spectral
/// thresholds match whole-file extraction exactly and the result does not
/// depend on `chunk_duration_sec`.
pub fn extract_timeline_chunked(
    input: &Path,
    cfg: &AnalysisConfig,
    chunk_duration_sec: u64,
) -> Result<TimelineOutput> {
    info!(
        input = %input.display(),
        chunk_sec = chunk_duration_sec,
        "extract start (chunked streaming)"
    );
    let total_start = Instant::now();
    let effective_threshold = cfg.energy_threshold + cfg.vad_threshold_delta;
    let frame_ms = cfg.frame_ms;

    let mut streamer = FrameStreamer::new(cfg.sample_rate_hz, frame_ms);
    let mut frames: Vec<Frame> = Vec::new();
    let mut total_samples: u64 = 0;

    decode_audio_chunks_cb(
        input,
        cfg.sample_rate_hz,
        chunk_duration_sec,
        |chunk_samples, chunk_idx| {
            total_samples += chunk_samples.len() as u64;
            frames.extend(streamer.push(chunk_samples));

            if chunk_idx % 10 == 0 {
                info!(
                    chunk = chunk_idx,
                    samples = total_samples,
                    frames = frames.len(),
                    "chunk processed"
                );
            }

            Ok(())
        },
    )?;
    frames.extend(streamer.finish());

    // Spectral engines adapt their thresholds from the whole file, so this
    // must run after decode, over every frame.
    let (threshold, flatness_max, entropy_min, centroid_min, centroid_max) =
        if cfg.vad_engine == "spectral" {
            let adapted = adapt_spectral_thresholds(
                &frames,
                effective_threshold,
                cfg.spectral_flatness_max,
                cfg.spectral_entropy_min,
                cfg.spectral_centroid_min,
                cfg.spectral_centroid_max,
            );
            (
                adapted.threshold,
                Some(adapted.flatness_max),
                Some(adapted.entropy_min),
                Some(adapted.centroid_min),
                Some(adapted.centroid_max),
            )
        } else {
            (
                effective_threshold,
                cfg.spectral_flatness_max,
                cfg.spectral_entropy_min,
                cfg.spectral_centroid_min,
                cfg.spectral_centroid_max,
            )
        };

    let mut engine = create_engine(
        &cfg.vad_engine,
        threshold,
        flatness_max,
        entropy_min,
        centroid_min,
        centroid_max,
        cfg.sample_rate_hz,
        frame_ms,
    )?;
    let vad_output =
        classify_with_engine(engine.as_mut(), &frames, &[], cfg.sample_rate_hz, frame_ms)?;
    let states = tri_state::classify_frame_states(
        &vad_output.decisions,
        &frames,
        &vad_output.likelihoods,
        tri_state::engine_trusts_likelihood(&cfg.vad_engine),
    );
    let smoothed = tri_state::smooth_states(&states, frame_ms, cfg.speech_hangover_ms);

    let total_audio_ms = total_samples * 1000 / cfg.sample_rate_hz as u64;
    let mut stage_ms = StageDurations::default();
    let (segments, _speech_segment_count) = super::segments_from_smoothed(
        input,
        &smoothed,
        &vad_output.likelihoods,
        &frames,
        total_audio_ms,
        cfg,
        &mut stage_ms,
    )?;

    let timeline = TimelineOutput {
        file: input.file_name().map_or_else(
            || "unknown".to_string(),
            |s| s.to_string_lossy().to_string(),
        ),
        analysis_sample_rate: cfg.sample_rate_hz,
        frame_ms: cfg.frame_ms,
        segments,
    };

    info!(
        total_ms = total_start.elapsed().as_millis() as u64,
        total_audio_ms,
        frames = frames.len(),
        non_voice_segments = timeline.segments.len(),
        "extract done (chunked)"
    );

    Ok(timeline)
}
