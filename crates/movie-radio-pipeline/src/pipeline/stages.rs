use std::path::Path;
use std::time::Instant;

use anyhow::Result;
use tracing::info;

use crate::pipeline::filters::{
    ambiguous_expand_max_ms, residual_bridge_gap_ms, should_apply_speech_evidence_filter,
    should_apply_verification_filter,
};
use crate::pipeline::vad::{adapt_spectral_thresholds, create_engine, VadEngine};
use crate::pipeline::{
    decode, framing, nonvoice_expand, segmenter, speech_evidence, tail_recovery, tri_state,
};
use movie_radio_types::{AnalysisConfig, Frame, Segment, StageDurations};
use movie_radio_verification::{
    default_filter_segment_confidence_ceiling, filter_low_confidence_non_voice_segments,
};

/// 1. Decode Stage
pub fn decode_stage(
    input: &Path,
    cfg: &AnalysisConfig,
    stage_ms: &mut StageDurations,
) -> Result<(Vec<f32>, u32)> {
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
    Ok((mono, source_rate))
}

/// 2. Framing Stage
pub fn framing_stage(
    mono: &[f32],
    cfg: &AnalysisConfig,
    stage_ms: &mut StageDurations,
) -> Vec<Frame> {
    let frame_start = Instant::now();
    let frames = framing::build_frames(
        mono,
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
    frames
}

/// 3. VAD Stage
pub fn vad_stage(
    frames: &[Frame],
    cfg: &AnalysisConfig,
    effective_threshold: f32,
    stage_ms: &mut StageDurations,
) -> Result<(Vec<bool>, Vec<f32>)> {
    let (vad_threshold, vad_flatness_max, vad_entropy_min, vad_centroid_min, vad_centroid_max) =
        if cfg.vad_engine == "spectral" {
            let adapted = adapt_spectral_thresholds(
                frames,
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
    let vad_output = vad_engine.classify(frames);
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

    Ok((speech, frame_likelihoods))
}

/// 4. Smoothing Stage
pub fn smoothing_stage(
    speech: &[bool],
    frames: &[Frame],
    frame_likelihoods: &[f32],
    cfg: &AnalysisConfig,
    stage_ms: &mut StageDurations,
) -> Vec<bool> {
    let smooth_start = Instant::now();
    let smoothed = tri_state::resolve_speech_with_ambiguity(
        speech,
        frames,
        frame_likelihoods,
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
    smoothed
}

/// 5. Speech Segments Stage
pub fn speech_segments_stage(
    smoothed: &[bool],
    frame_likelihoods: &[f32],
    cfg: &AnalysisConfig,
    stage_ms: &mut StageDurations,
) -> Vec<Segment> {
    let speech_stage = Instant::now();
    let speech_segments =
        segmenter::speech_segments(smoothed, cfg.frame_ms, cfg.min_speech_ms, frame_likelihoods);
    stage_ms.speech_ms = speech_stage.elapsed().as_millis() as u64;
    info!(
        stage = "speech_segments",
        ms = stage_ms.speech_ms,
        segments = speech_segments.len(),
        min_speech_ms = cfg.min_speech_ms,
        "stage complete"
    );
    speech_segments
}

/// 6. Merging Stage
pub fn merging_stage(
    speech_segments: &[Segment],
    frames: &[Frame],
    cfg: &AnalysisConfig,
    stage_ms: &mut StageDurations,
) -> Vec<Segment> {
    let merge_start = Instant::now();
    let merged_speech = segmenter::merge_close_segments(speech_segments, cfg.merge_gap_ms);
    let prune_floor_ms = cfg
        .merge_options
        .as_ref()
        .map(|opts| opts.min_speech_duration)
        .unwrap_or(cfg.min_speech_ms);
    let pruned_speech = segmenter::prune_short_speech_segments(&merged_speech, prune_floor_ms);
    let segments_after_prune = pruned_speech.len();
    let filtered_speech = if should_apply_speech_evidence_filter(cfg) {
        speech_evidence::filter_implausible_speech_segments(&pruned_speech, frames, cfg.frame_ms)
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
    filtered_speech
}

/// 7. Non-Voice Inversion Stage
pub fn non_voice_inversion_stage(
    input: &Path,
    filtered_speech: &[Segment],
    frame_likelihoods: &[f32],
    total_audio_ms: u64,
    cfg: &AnalysisConfig,
    stage_ms: &mut StageDurations,
) -> Result<Vec<Segment>> {
    let invert_start = Instant::now();
    let non_voice = segmenter::invert_to_non_voice(
        filtered_speech,
        total_audio_ms,
        cfg.min_non_voice_ms,
        cfg.frame_ms,
        frame_likelihoods,
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
        frame_likelihoods,
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
            frame_likelihoods,
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
        frame_likelihoods,
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
    Ok(segments)
}
