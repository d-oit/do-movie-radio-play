use anyhow::Result;
use std::path::Path;
use std::time::Instant;
use tracing::info;

use crate::pipeline::decode::decode_audio_chunks_cb;
use crate::pipeline::filters::{ambiguous_expand_max_ms, residual_bridge_gap_ms};
use crate::pipeline::framing;
use crate::pipeline::nonvoice_expand;
use crate::pipeline::segmenter;
use crate::pipeline::tail_recovery;
use crate::pipeline::tri_state;
use crate::pipeline::vad::{classify_with_engine, create_engine};
use movie_radio_types::{AnalysisConfig, Frame, Segment, TimelineOutput};

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
    let hangover_ms = cfg.speech_hangover_ms;
    let warmup_frames = ((hangover_ms + 500) / frame_ms) as usize;

    let (vad_threshold, vad_flatness_max, vad_entropy_min, vad_centroid_min, vad_centroid_max) = (
        effective_threshold,
        cfg.spectral_flatness_max,
        cfg.spectral_entropy_min,
        cfg.spectral_centroid_min,
        cfg.spectral_centroid_max,
    );

    let mut all_segments = Vec::new();
    let mut all_likelihoods = Vec::new();
    let mut chunk_offset_ms: u64 = 0;
    let mut prev_frames: Vec<Frame> = Vec::new();
    let mut prev_likelihoods: Vec<f32> = Vec::new();
    let mut prev_samples: Vec<f32> = Vec::new();
    let mut total_samples_processed: u64 = 0;

    decode_audio_chunks_cb(
        input,
        cfg.sample_rate_hz,
        chunk_duration_sec,
        |chunk_samples, chunk_idx| {
            let chunk_len = chunk_samples.len();
            total_samples_processed += chunk_len as u64;

            let frames = framing::build_frames(chunk_samples, cfg.sample_rate_hz, frame_ms, false);
            let chunk_ms = chunk_len as u64 * 1000 / cfg.sample_rate_hz as u64;

            let mut combined_frames = prev_frames.clone();
            combined_frames.extend_from_slice(&frames);

            let mut combined_likelihoods = prev_likelihoods.clone();

            let mut vad_engine = create_engine(
                &cfg.vad_engine,
                vad_threshold,
                vad_flatness_max,
                vad_entropy_min,
                vad_centroid_min,
                vad_centroid_max,
                cfg.sample_rate_hz,
                frame_ms,
            )?;

            let mut combined_samples = prev_samples.clone();
            combined_samples.extend_from_slice(chunk_samples);
            let vad_output = classify_with_engine(
                vad_engine.as_mut(),
                &combined_frames,
                &combined_samples,
                cfg.sample_rate_hz,
                frame_ms,
            )?;
            let speech = vad_output.decisions;
            combined_likelihoods.extend_from_slice(&vad_output.likelihoods);

            let smoothed = tri_state::resolve_speech_with_ambiguity(
                &speech,
                &combined_frames,
                &combined_likelihoods,
                frame_ms,
                hangover_ms,
            );

            let warmup_count = prev_frames.len();
            let chunk_smoothed = &smoothed[warmup_count..];
            let chunk_likelihoods = &combined_likelihoods[warmup_count..];
            let chunk_frame_start_ms = chunk_offset_ms;

            all_likelihoods.extend_from_slice(chunk_likelihoods);

            let speech_segments = segmenter::speech_segments(
                chunk_smoothed,
                frame_ms,
                cfg.min_speech_ms,
                chunk_likelihoods,
            );

            let merged_speech = segmenter::merge_close_segments(&speech_segments, cfg.merge_gap_ms);
            let prune_floor_ms = cfg
                .merge_options
                .as_ref()
                .map(|opts| opts.min_speech_duration)
                .unwrap_or(cfg.min_speech_ms);
            let pruned_speech =
                segmenter::prune_short_speech_segments(&merged_speech, prune_floor_ms);

            let non_voice = segmenter::invert_to_non_voice(
                &pruned_speech,
                chunk_ms,
                cfg.min_non_voice_ms,
                frame_ms,
                chunk_likelihoods,
            );

            let bridge_speech_ms = cfg
                .merge_options
                .as_ref()
                .map(|opts| opts.min_speech_duration)
                .unwrap_or(0);
            let non_voice = segmenter::bridge_non_voice_segments(&non_voice, bridge_speech_ms);
            let non_voice = if let Some(merge_options) = cfg.merge_options.as_ref() {
                segmenter::apply_non_voice_merge_policy(&non_voice, merge_options)
            } else {
                non_voice
            };
            let non_voice = nonvoice_expand::expand_non_voice_segments_into_ambiguous(
                &non_voice,
                chunk_likelihoods,
                frame_ms,
                ambiguous_expand_max_ms(cfg),
            );

            for seg in non_voice {
                let adjusted = Segment {
                    start_ms: seg.start_ms + chunk_frame_start_ms,
                    end_ms: seg.end_ms + chunk_frame_start_ms,
                    kind: seg.kind,
                    confidence: seg.confidence,
                    tags: seg.tags,
                    prompt: seg.prompt,
                    sfx_trigger: seg.sfx_trigger,
                };
                all_segments.push(adjusted);
            }

            if warmup_count > 0 && frames.len() > warmup_frames {
                prev_frames = frames[frames.len() - warmup_frames..].to_vec();
                prev_likelihoods =
                    chunk_likelihoods[chunk_likelihoods.len() - warmup_frames..].to_vec();
                let flen = frame_len(cfg.sample_rate_hz, frame_ms);
                let tail = chunk_samples.len() % flen;
                let mut sample_count = warmup_frames * flen;
                if tail != 0 {
                    sample_count -= flen - tail;
                }
                prev_samples = tail_samples(chunk_samples, sample_count);
            } else {
                prev_frames = frames;
                prev_likelihoods = chunk_likelihoods.to_vec();
                prev_samples = chunk_samples.to_vec();
            }

            chunk_offset_ms += chunk_ms;

            if chunk_idx % 10 == 0 {
                info!(
                    chunk = chunk_idx,
                    offset_ms = chunk_offset_ms,
                    segments_so_far = all_segments.len(),
                    "chunk processed"
                );
            }

            Ok(())
        },
    )?;

    let total_audio_ms = total_samples_processed * 1000 / cfg.sample_rate_hz as u64;

    all_segments.sort_by_key(|s| s.start_ms);
    let all_segments = merge_boundary_segments(all_segments);
    let all_segments =
        segmenter::bridge_residual_non_voice_gaps(&all_segments, residual_bridge_gap_ms(cfg));
    let all_segments = tail_recovery::extend_terminal_non_voice_segment(
        &all_segments,
        &all_likelihoods,
        cfg.frame_ms,
        total_audio_ms,
        cfg.min_non_voice_ms,
    );

    let timeline = TimelineOutput {
        file: input.file_name().map_or_else(
            || "unknown".to_string(),
            |s| s.to_string_lossy().to_string(),
        ),
        analysis_sample_rate: cfg.sample_rate_hz,
        frame_ms: cfg.frame_ms,
        segments: all_segments,
    };

    info!(
        total_ms = total_start.elapsed().as_millis() as u64,
        total_audio_ms,
        non_voice_segments = timeline.segments.len(),
        "extract done (chunked)"
    );

    Ok(timeline)
}

fn merge_boundary_segments(segments: Vec<Segment>) -> Vec<Segment> {
    if segments.is_empty() {
        return Vec::new();
    }
    let mut merged: Vec<Segment> = Vec::with_capacity(segments.len());
    for seg in segments {
        if let Some(last) = merged.last_mut() {
            if last.kind == seg.kind && last.end_ms >= seg.start_ms {
                last.end_ms = last.end_ms.max(seg.end_ms);
                last.confidence = last.confidence.max(seg.confidence);
                continue;
            }
        }
        merged.push(seg);
    }
    merged
}

fn frame_len(sample_rate_hz: u32, frame_ms: u32) -> usize {
    ((sample_rate_hz as usize * frame_ms as usize) / 1000).max(1)
}

fn tail_samples(samples: &[f32], n: usize) -> Vec<f32> {
    if n >= samples.len() {
        samples.to_vec()
    } else {
        samples[samples.len() - n..].to_vec()
    }
}
