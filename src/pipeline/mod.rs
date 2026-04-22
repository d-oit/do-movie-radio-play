pub mod decode;
pub mod features;
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

use crate::config::AnalysisConfig;
use crate::pipeline::vad::{adapt_spectral_thresholds, create_engine, VadEngine};
use crate::types::{BenchmarkResult, StageDurations, TimelineOutput};
use crate::verification::{
    default_filter_segment_confidence_ceiling, filter_low_confidence_non_voice_segments,
};

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
    let effective_threshold = cfg.energy_threshold + cfg.vad_threshold_delta;
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
    stage_ms.vad_ms = vad_start.elapsed().as_millis();
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
    let speech_segments = segmenter::speech_segments(
        &smoothed,
        cfg.frame_ms,
        cfg.min_speech_ms,
        &frame_likelihoods,
    );
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
    let prune_floor_ms = cfg
        .merge_options
        .as_ref()
        .map(|opts| opts.min_speech_duration)
        .unwrap_or(cfg.min_speech_ms);
    let pruned_speech = segmenter::prune_short_speech_segments(&merged_speech, prune_floor_ms);
    let filtered_speech =
        speech_evidence::filter_implausible_speech_segments(&pruned_speech, &frames, cfg.frame_ms);
    stage_ms.merge_ms = merge_start.elapsed().as_millis();
    info!(
        stage = "merge_segments",
        ms = stage_ms.merge_ms,
        segments_before_prune = merged_speech.len(),
        segments_after_prune = pruned_speech.len(),
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
    );
    stage_ms.invert_ms = invert_start.elapsed().as_millis();
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
        stage_ms.invert_ms += split_start.elapsed().as_millis();
        info!(
            stage = "split",
            ms = split_start.elapsed().as_millis(),
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
    let segments = filter_low_confidence_non_voice_segments(
        input,
        &segments,
        default_filter_segment_confidence_ceiling(),
    );
    let segments = segmenter::bridge_residual_non_voice_gaps(&segments);
    let segments = tail_recovery::extend_terminal_non_voice_segment(
        &segments,
        &frame_likelihoods,
        cfg.frame_ms,
        total_audio_ms,
    );
    stage_ms.invert_ms += verification_filter_start.elapsed().as_millis();
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

pub fn benchmark_file(input: &Path, cfg: &AnalysisConfig) -> Result<BenchmarkResult> {
    let total_start = Instant::now();
    let PipelineArtifacts {
        timeline,
        frame_count,
        stage_ms,
        ..
    } = run_pipeline(input, cfg)?;
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
