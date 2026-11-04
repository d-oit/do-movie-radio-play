use anyhow::Result;
use std::path::Path;
use std::time::Instant;
use tracing::info;

use crate::pipeline::decode::decode_audio_chunks_cb;
use crate::pipeline::filters::should_apply_speech_evidence_filter;
use crate::pipeline::framing;
use crate::pipeline::tri_state::{self, FrameState};
use crate::pipeline::vad::{classify_with_engine, create_engine};
use movie_radio_types::{AnalysisConfig, Frame, StageDurations, TimelineOutput};

/// Streams decode and VAD in bounded chunks, then runs the same post-VAD
/// stages as whole-file extraction over the accumulated per-frame state.
///
/// Only per-frame state is retained (one [`FrameState`] and one likelihood per
/// frame, plus frames when the speech-evidence filter needs them), so peak
/// memory no longer scales with the decoded `f32` sample buffer. Because
/// segmentation, merging and inversion run once over the full timeline, the
/// result is independent of `chunk_duration_sec`.
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
    let frame_len = frame_len(cfg.sample_rate_hz, frame_ms);
    let trust_likelihood = tri_state::engine_trusts_likelihood(&cfg.vad_engine);
    let keep_frames = should_apply_speech_evidence_filter(cfg);

    let mut states: Vec<FrameState> = Vec::new();
    let mut likelihoods: Vec<f32> = Vec::new();
    let mut frames_all: Vec<Frame> = Vec::new();
    let mut carry: Vec<f32> = Vec::new();
    let mut total_samples: u64 = 0;

    // One engine for the whole stream so stateful engines (Silero, WebRTC)
    // keep their context across chunk boundaries.
    let mut engine = create_engine(
        &cfg.vad_engine,
        effective_threshold,
        cfg.spectral_flatness_max,
        cfg.spectral_entropy_min,
        cfg.spectral_centroid_min,
        cfg.spectral_centroid_max,
        cfg.sample_rate_hz,
        frame_ms,
    )?;

    decode_audio_chunks_cb(
        input,
        cfg.sample_rate_hz,
        chunk_duration_sec,
        |chunk_samples, chunk_idx| {
            total_samples += chunk_samples.len() as u64;

            // Carry the samples that did not fill a frame so frame boundaries
            // stay aligned with whole-file framing regardless of chunk size.
            let mut buffer = std::mem::take(&mut carry);
            buffer.extend_from_slice(chunk_samples);
            let usable = buffer.len() - buffer.len() % frame_len;
            let frames = framing::build_frames(
                &buffer[..usable],
                cfg.sample_rate_hz,
                frame_ms,
                cfg.parallel_features,
            );

            let vad_output = classify_with_engine(
                engine.as_mut(),
                &frames,
                &buffer[..usable],
                cfg.sample_rate_hz,
                frame_ms,
            )?;

            states.extend(tri_state::classify_frame_states(
                &vad_output.decisions,
                &frames,
                &vad_output.likelihoods,
                trust_likelihood,
            ));
            likelihoods.extend_from_slice(&vad_output.likelihoods);
            if keep_frames {
                frames_all.extend(frames);
            }

            carry = buffer[usable..].to_vec();

            if chunk_idx % 10 == 0 {
                info!(
                    chunk = chunk_idx,
                    samples = total_samples,
                    frames = states.len(),
                    "chunk processed"
                );
            }

            Ok(())
        },
    )?;

    let total_audio_ms = total_samples * 1000 / cfg.sample_rate_hz as u64;
    let smoothed = tri_state::smooth_states(&states, frame_ms, cfg.speech_hangover_ms);

    let mut stage_ms = StageDurations::default();
    let (segments, _speech_segment_count) = super::segments_from_smoothed(
        input,
        &smoothed,
        &likelihoods,
        &frames_all,
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
        non_voice_segments = timeline.segments.len(),
        "extract done (chunked)"
    );

    Ok(timeline)
}

fn frame_len(sample_rate_hz: u32, frame_ms: u32) -> usize {
    ((sample_rate_hz as usize * frame_ms as usize) / 1000).max(1)
}
