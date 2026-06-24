pub use benchmark::benchmark_file;
pub mod benchmark;
pub mod decode;
pub mod features;
pub mod filters;
pub mod framing;
pub mod nonvoice_expand;
pub mod prompts;
pub mod resample;
pub mod segmenter;
pub mod speech_evidence;
pub mod tags;
pub mod tail_recovery;
pub mod tri_state;
pub mod vad;

use std::{path::Path, time::Instant};

use anyhow::Result;
use tracing::info;

use crate::pipeline::vad::{adapt_spectral_thresholds, create_engine, VadEngine};
use filters::{
    ambiguous_expand_max_ms, residual_bridge_gap_ms, should_apply_speech_evidence_filter,
    should_apply_verification_filter,
};
use movie_radio_types::{AnalysisConfig, MergeStrategy};
use movie_radio_types::{StageDurations, TimelineOutput};
use movie_radio_verification::{
    default_filter_segment_confidence_ceiling, filter_low_confidence_non_voice_segments,
};

const MAX_FILTER_MIN_NON_VOICE_MS: u32 = 1_000;
const FILTER_MERGE_STRATEGY: MergeStrategy = MergeStrategy::Sparse;
const MAX_RESIDUAL_BRIDGE_GAP_MS: u64 = 2_500;
const NON_SPARSE_AMBIGUOUS_EXPAND_MAX_MS: u64 = 400;

struct PipelineArtifacts {
    timeline: TimelineOutput,
    frame_count: usize,
    speech_segment_count: usize,
    stage_ms: StageDurations,
}

pub fn extract_timeline(input: &Path, cfg: &AnalysisConfig) -> Result<TimelineOutput> {
    if let Some(chunk_sec) = cfg.chunk_duration_sec.filter(|&s| s > 0) {
        return extract_timeline_chunked(input, cfg, chunk_sec);
    }
    info!(input = %input.display(), sample_rate = cfg.sample_rate_hz, frame_ms = cfg.frame_ms, "extract start");
    let total_start = Instant::now();
    let PipelineArtifacts {
        timeline,
        frame_count,
        speech_segment_count,
        stage_ms,
    } = run_pipeline(input, cfg)?;
    info!(
        total_ms = total_start.elapsed().as_millis() as u64,
        frames = frame_count,
        speech_segments = speech_segment_count,
        non_voice_segments = timeline.segments.len(),
        decode_ms = stage_ms.decode_ms,
        vad_ms = stage_ms.vad_ms,
        "extract done"
    );
    Ok(timeline)
}

fn extract_timeline_chunked(
    input: &Path,
    cfg: &AnalysisConfig,
    chunk_duration_sec: u64,
) -> Result<TimelineOutput> {
    info!(
        input = %input.display(),
        chunk_sec = chunk_duration_sec,
        "extract start (chunked)"
    );
    let total_start = Instant::now();
    let effective_threshold = cfg.energy_threshold + cfg.vad_threshold_delta;

    let frame_ms = cfg.frame_ms;
    let hangover_ms = cfg.speech_hangover_ms;
    let warmup_frames = ((hangover_ms + 500) / frame_ms) as usize;

    let chunks = decode::decode_audio_chunked(input, cfg.sample_rate_hz, chunk_duration_sec)?;

    let (vad_threshold, vad_flatness_max, vad_entropy_min, vad_centroid_min, vad_centroid_max) = (
        effective_threshold,
        cfg.spectral_flatness_max,
        cfg.spectral_entropy_min,
        cfg.spectral_centroid_min,
        cfg.spectral_centroid_max,
    );

    let mut all_segments = Vec::new();
    let mut chunk_offset_ms: u64 = 0;
    let mut prev_frames: Vec<movie_radio_types::Frame> = Vec::new();
    let mut prev_likelihoods: Vec<f32> = Vec::new();

    for (chunk_idx, chunk_samples) in chunks.iter().enumerate() {
        let chunk_len = chunk_samples.len();

        let frames = framing::build_frames(chunk_samples, cfg.sample_rate_hz, frame_ms, false);
        let chunk_ms = chunk_len as u64 * 1000 / cfg.sample_rate_hz as u64;

        let mut combined_frames = prev_frames.clone();
        combined_frames.extend_from_slice(&frames);

        let mut combined_likelihoods = prev_likelihoods.clone();

        let vad_engine = create_engine(
            &cfg.vad_engine,
            vad_threshold,
            vad_flatness_max,
            vad_entropy_min,
            vad_centroid_min,
            vad_centroid_max,
        )?;

        let vad_output = vad_engine.classify(&combined_frames);
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
        let pruned_speech = segmenter::prune_short_speech_segments(&merged_speech, prune_floor_ms);

        let chunk_total_ms = chunk_ms;
        let non_voice = segmenter::invert_to_non_voice(
            &pruned_speech,
            chunk_total_ms,
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
            let adjusted = movie_radio_types::Segment {
                start_ms: seg.start_ms + chunk_frame_start_ms,
                end_ms: seg.end_ms + chunk_frame_start_ms,
                kind: seg.kind,
                confidence: seg.confidence,
                tags: seg.tags,
                prompt: seg.prompt,
            };
            all_segments.push(adjusted);
        }

        if warmup_count > 0 && frames.len() > warmup_frames {
            prev_frames = frames[frames.len() - warmup_frames..].to_vec();
            prev_likelihoods =
                chunk_likelihoods[chunk_likelihoods.len() - warmup_frames..].to_vec();
        } else {
            prev_frames = frames;
            prev_likelihoods = chunk_likelihoods.to_vec();
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
    }

    let total_samples: u64 = chunks.iter().map(|c| c.len() as u64).sum();
    let total_audio_ms = total_samples * 1000 / cfg.sample_rate_hz as u64;

    all_segments.sort_by_key(|s| s.start_ms);

    let timeline = TimelineOutput {
        file: input
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string()),
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

fn run_pipeline(input: &Path, cfg: &AnalysisConfig) -> Result<PipelineArtifacts> {
    let effective_threshold = cfg.energy_threshold + cfg.vad_threshold_delta;
    let mut stage_ms = StageDurations::default();

    let decode_start = Instant::now();
    let (mono, source_rate) = decode::decode_audio(input, cfg.sample_rate_hz)?;
    stage_ms.decode_ms = decode_start.elapsed().as_millis() as u64;
    info!(
        stage = "decode",
        ms = stage_ms.decode_ms,
        source_rate,
        samples = mono.len(),
        "stage complete"
    );

    stage_ms.resample_ms = 0; // Resampling is now integrated into decode

    let frame_start = Instant::now();
    let frames = framing::build_frames(
        &mono,
        cfg.sample_rate_hz,
        cfg.frame_ms,
        cfg.parallel_features,
    );
    stage_ms.frame_ms = frame_start.elapsed().as_millis() as u64;
    info!(
        stage = "frame",
        ms = stage_ms.frame_ms,
        frames = frames.len(),
        frame_ms = cfg.frame_ms,
        "stage complete"
    );
    let frame_count = frames.len();

    let (vad_threshold, vad_flatness_max, vad_entropy_min, vad_centroid_min, vad_centroid_max) =
        if cfg.vad_engine == "spectral" {
            let adapted = adapt_spectral_thresholds(
                &frames,
                effective_threshold,
                cfg.spectral_flatness_max,
                cfg.spectral_entropy_min,
                cfg.spectral_centroid_min,
                cfg.spectral_centroid_max,
            );
            info!(
                stage = "vad_adapt",
                threshold = adapted.threshold,
                flatness_max = adapted.flatness_max,
                entropy_min = adapted.entropy_min,
                centroid_min = adapted.centroid_min,
                centroid_max = adapted.centroid_max,
                "adaptive spectral thresholds computed"
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

    let vad_engine: Box<dyn VadEngine> = create_engine(
        &cfg.vad_engine,
        vad_threshold,
        vad_flatness_max,
        vad_entropy_min,
        vad_centroid_min,
        vad_centroid_max,
    )?;

    let vad_name = vad_engine.name();

    let vad_start = Instant::now();
    let vad_output = vad_engine.classify(&frames);
    let speech = vad_output.decisions;
    let frame_likelihoods = vad_output.likelihoods;
    stage_ms.vad_ms = vad_start.elapsed().as_millis() as u64;
    let speech_frames = speech.iter().filter(|&&v| v).count();
    info!(
        stage = "vad",
        ms = stage_ms.vad_ms,
        engine = vad_name,
        speech_frames,
        threshold = vad_threshold,
        base_threshold = cfg.energy_threshold,
        delta = cfg.vad_threshold_delta,
        "stage complete"
    );

    let smooth_start = Instant::now();
    let smoothed = tri_state::resolve_speech_with_ambiguity(
        &speech,
        &frames,
        &frame_likelihoods,
        cfg.frame_ms,
        cfg.speech_hangover_ms,
    );
    stage_ms.smooth_ms = smooth_start.elapsed().as_millis() as u64;
    let smoothed_frames = smoothed.iter().filter(|&&v| v).count();
    info!(
        stage = "smooth",
        ms = stage_ms.smooth_ms,
        speech_frames = smoothed_frames,
        hangover_ms = cfg.speech_hangover_ms,
        "stage complete"
    );

    let speech_stage = Instant::now();
    let speech_segments = segmenter::speech_segments(
        &smoothed,
        cfg.frame_ms,
        cfg.min_speech_ms,
        &frame_likelihoods,
    );
    stage_ms.speech_ms = speech_stage.elapsed().as_millis() as u64;
    info!(
        stage = "speech_segments",
        ms = stage_ms.speech_ms,
        segments = speech_segments.len(),
        min_speech_ms = cfg.min_speech_ms,
        "stage complete"
    );

    let merge_start = Instant::now();
    let merged_speech = segmenter::merge_close_segments(&speech_segments, cfg.merge_gap_ms);
    let prune_floor_ms = cfg
        .merge_options
        .as_ref()
        .map(|opts| opts.min_speech_duration)
        .unwrap_or(cfg.min_speech_ms);
    let pruned_speech = segmenter::prune_short_speech_segments(&merged_speech, prune_floor_ms);
    let segments_after_prune = pruned_speech.len();
    let filtered_speech = if should_apply_speech_evidence_filter(cfg) {
        speech_evidence::filter_implausible_speech_segments(&pruned_speech, &frames, cfg.frame_ms)
    } else {
        pruned_speech
    };
    stage_ms.merge_ms = merge_start.elapsed().as_millis() as u64;
    info!(
        stage = "merge_segments",
        ms = stage_ms.merge_ms,
        segments_before_prune = merged_speech.len(),
        segments_after_prune,
        segments_after_evidence = filtered_speech.len(),
        prune_floor_ms,
        merge_gap_ms = cfg.merge_gap_ms,
        "stage complete"
    );
    let speech_segment_count = filtered_speech.len();

    let total_audio_ms = mono.len() as u64 * 1000 / cfg.sample_rate_hz as u64;
    let invert_start = Instant::now();
    let non_voice = segmenter::invert_to_non_voice(
        &filtered_speech,
        total_audio_ms,
        cfg.min_non_voice_ms,
        cfg.frame_ms,
        &frame_likelihoods,
    );
    let segments_before_bridge = non_voice.len();
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
        &frame_likelihoods,
        cfg.frame_ms,
        ambiguous_expand_max_ms(cfg),
    );
    stage_ms.invert_ms = invert_start.elapsed().as_millis() as u64;
    let segments_before_split = non_voice.len();
    let segments = if let Some(max_ms) = cfg.max_non_voice_ms {
        let split_start = Instant::now();
        let split = segmenter::split_long_segments(
            non_voice,
            max_ms,
            cfg.min_non_voice_ms,
            cfg.frame_ms,
            &frame_likelihoods,
        );
        stage_ms.invert_ms += split_start.elapsed().as_millis() as u64;
        info!(
            stage = "split",
            ms = split_start.elapsed().as_millis() as u64,
            segments_before = segments_before_split,
            segments_after = split.len(),
            max_non_voice_ms = max_ms,
            "stage complete"
        );
        split
    } else {
        non_voice
    };
    let verification_filter_start = Instant::now();
    let segments_before_filter = segments.len();
    let segments = if should_apply_verification_filter(cfg) {
        filter_low_confidence_non_voice_segments(
            input,
            &segments,
            default_filter_segment_confidence_ceiling(),
        )
    } else {
        segments
    };
    let segments =
        segmenter::bridge_residual_non_voice_gaps(&segments, residual_bridge_gap_ms(cfg));
    let segments = tail_recovery::extend_terminal_non_voice_segment(
        &segments,
        &frame_likelihoods,
        cfg.frame_ms,
        total_audio_ms,
        cfg.min_non_voice_ms,
    );
    stage_ms.invert_ms += verification_filter_start.elapsed().as_millis() as u64;
    info!(
        stage = "invert",
        ms = stage_ms.invert_ms,
        segments = segments.len(),
        segments_before_bridge,
        segments_before_filter,
        bridge_speech_ms,
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
        segments,
    };

    Ok(PipelineArtifacts {
        timeline,
        frame_count,
        speech_segment_count,
        stage_ms,
    })
}

#[cfg(test)]
mod tests;
