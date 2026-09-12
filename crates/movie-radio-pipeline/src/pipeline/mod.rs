pub use benchmark::benchmark_file;
pub mod benchmark;
pub mod chunked;
pub mod decode;
pub mod features;
pub mod filters;
pub mod framing;
pub mod nonvoice_expand;
pub mod prompts;
pub mod resample;
pub mod segmenter;
pub mod sfx_autofill;
pub mod speech_evidence;
pub mod tags;
pub mod tail_recovery;
pub mod tri_state;
pub mod vad;

use std::{path::Path, time::Instant};

use anyhow::Result;
use tracing::info;

use crate::pipeline::vad::{
    adapt_spectral_thresholds, classify_with_engine, create_engine, VadEngine,
};
use filters::{
    ambiguous_expand_max_ms, residual_bridge_gap_ms, should_apply_speech_evidence_filter,
    should_apply_verification_filter,
};
use movie_radio_types::{AnalysisConfig, MergeStrategy};
use movie_radio_types::{Frame, Segment, StageDurations, TimelineOutput};
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

macro_rules! timed_stage {
    ($stage_ms:expr, $field:ident, $body:block) => {{
        let __stage_start = Instant::now();
        let __stage_out = $body;
        $stage_ms.$field = __stage_start.elapsed().as_millis() as u64;
        __stage_out
    }};
}

pub fn extract_timeline(input: &Path, cfg: &AnalysisConfig) -> Result<TimelineOutput> {
    if let Some(chunk_sec) = cfg.chunk_duration_sec.filter(|&s| s > 0) {
        return chunked::extract_timeline_chunked(input, cfg, chunk_sec);
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

pub fn extract_timeline_from_samples(
    samples: &[f32],
    cfg: &AnalysisConfig,
) -> Result<TimelineOutput> {
    let dummy_path = Path::new("in_memory.wav");
    // skipcq: RS-E1015 — DeepSource false positive: dummy_path is &Path, not ()
    extract_timeline_from_samples_with_path(samples, dummy_path, cfg) // skipcq: RS-E1015
}

pub fn extract_timeline_from_samples_with_path(
    samples: &[f32],
    file_path: &Path,
    cfg: &AnalysisConfig,
) -> Result<TimelineOutput> {
    info!(
        sample_count = samples.len(),
        sample_rate = cfg.sample_rate_hz,
        frame_ms = cfg.frame_ms,
        "extract_from_samples start"
    );
    let total_start = Instant::now();
    // skipcq: RS-E1015 — DeepSource false positive on ? with Result<PipelineArtifacts>
    let PipelineArtifacts {
        timeline,
        frame_count,
        speech_segment_count,
        stage_ms,
    } = run_pipeline_from_samples(samples, file_path, cfg)?; // skipcq: RS-E1015
    info!(
        total_ms = total_start.elapsed().as_millis() as u64,
        frames = frame_count,
        speech_segments = speech_segment_count,
        non_voice_segments = timeline.segments.len(),
        decode_ms = stage_ms.decode_ms,
        vad_ms = stage_ms.vad_ms,
        "extract_from_samples done"
    );
    Ok(timeline)
}

fn run_pipeline(input: &Path, cfg: &AnalysisConfig) -> Result<PipelineArtifacts> {
    let mut stage_ms = StageDurations::default();
    let (mono, _source_rate) = decode_stage(input, cfg, &mut stage_ms)?;
    let mut artifacts = run_pipeline_from_samples(&mono, input, cfg)?;
    artifacts.stage_ms.decode_ms = stage_ms.decode_ms;
    Ok(artifacts)
}

fn run_pipeline_from_samples(
    mono: &[f32],
    file_path: &Path,
    cfg: &AnalysisConfig,
) -> Result<PipelineArtifacts> {
    let effective_threshold = cfg.energy_threshold + cfg.vad_threshold_delta;
    let mut stage_ms = StageDurations::default();

    let frames = framing_stage(mono, cfg, &mut stage_ms);
    let frame_count = frames.len();

    let (speech, frame_likelihoods) =
        vad_stage(mono, &frames, cfg, effective_threshold, &mut stage_ms)?;
    let smoothed = smoothing_stage(&speech, &frames, &frame_likelihoods, cfg, &mut stage_ms);
    let speech_segments = speech_segments_stage(&smoothed, &frame_likelihoods, cfg, &mut stage_ms);
    let filtered_speech = merging_stage(&speech_segments, &frames, cfg, &mut stage_ms);
    let speech_segment_count = filtered_speech.len();

    let total_audio_ms = mono.len() as u64 * 1000 / cfg.sample_rate_hz as u64;
    let segments = non_voice_inversion_stage(
        file_path,
        &filtered_speech,
        &frame_likelihoods,
        total_audio_ms,
        cfg,
        &mut stage_ms,
    )?;

    let file_name = file_path.file_name().map_or_else(
        || "unknown".to_string(),
        |s| s.to_string_lossy().to_string(),
    );

    let timeline = TimelineOutput {
        file: file_name,
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

#[rustfmt::skip]
fn decode_stage(input: &Path, cfg: &AnalysisConfig, stage_ms: &mut StageDurations) -> Result<(Vec<f32>, u32)> {
    let (mono, source_rate) = timed_stage!(stage_ms, decode_ms, {
        decode::decode_audio(input, cfg.sample_rate_hz)?
    });
    stage_ms.resample_ms = 0;
    info!(stage = "decode", ms = stage_ms.decode_ms, source_rate, samples = mono.len(), "stage complete");
    Ok((mono, source_rate))
}

#[rustfmt::skip]
fn framing_stage(mono: &[f32], cfg: &AnalysisConfig, stage_ms: &mut StageDurations) -> Vec<Frame> {
    let frames = timed_stage!(stage_ms, frame_ms, {
        framing::build_frames(mono, cfg.sample_rate_hz, cfg.frame_ms, cfg.parallel_features)
    });
    info!(stage = "frame", ms = stage_ms.frame_ms, frames = frames.len(), "stage complete");
    frames
}

#[rustfmt::skip]
fn vad_stage(mono: &[f32], frames: &[Frame], cfg: &AnalysisConfig, eff_thresh: f32, stage_ms: &mut StageDurations) -> Result<(Vec<bool>, Vec<f32>)> {
    let (thresh, flat, ent, cent_min, cent_max) = if cfg.vad_engine == "spectral" {
        let ad = adapt_spectral_thresholds(
            frames, eff_thresh, cfg.spectral_flatness_max,
            cfg.spectral_entropy_min, cfg.spectral_centroid_min, cfg.spectral_centroid_max,
        );
        info!(stage = "vad_adapt", threshold = ad.threshold, "adaptive spectral thresholds computed");
        (ad.threshold, Some(ad.flatness_max), Some(ad.entropy_min), Some(ad.centroid_min), Some(ad.centroid_max))
    } else {
        (eff_thresh, cfg.spectral_flatness_max, cfg.spectral_entropy_min, cfg.spectral_centroid_min, cfg.spectral_centroid_max)
    };
    let mut engine: Box<dyn VadEngine> = create_engine(&cfg.vad_engine, thresh, flat, ent, cent_min, cent_max, cfg.sample_rate_hz, cfg.frame_ms)?;
    let name = engine.name();
    let output = timed_stage!(stage_ms, vad_ms, {
        classify_with_engine(engine.as_mut(), frames, mono, cfg.sample_rate_hz, cfg.frame_ms)?
    });
    info!(stage = "vad", ms = stage_ms.vad_ms, engine = name, "stage complete");
    Ok((output.decisions, output.likelihoods))
}

#[rustfmt::skip]
fn smoothing_stage(speech: &[bool], frames: &[Frame], frame_likelihoods: &[f32], cfg: &AnalysisConfig, stage_ms: &mut StageDurations) -> Vec<bool> {
    let smoothed = timed_stage!(stage_ms, smooth_ms, {
        tri_state::resolve_speech_with_ambiguity(
            speech, frames, frame_likelihoods, cfg.frame_ms, cfg.speech_hangover_ms,
        )
    });
    info!(stage = "smooth", ms = stage_ms.smooth_ms, "stage complete");
    smoothed
}

#[rustfmt::skip]
fn speech_segments_stage(smoothed: &[bool], frame_likelihoods: &[f32], cfg: &AnalysisConfig, stage_ms: &mut StageDurations) -> Vec<Segment> {
    let segs = timed_stage!(stage_ms, speech_ms, {
        segmenter::speech_segments(smoothed, cfg.frame_ms, cfg.min_speech_ms, frame_likelihoods)
    });
    info!(stage = "speech_segments", ms = stage_ms.speech_ms, segments = segs.len(), "stage complete");
    segs
}

#[rustfmt::skip]
fn merging_stage(speech_segments: &[Segment], frames: &[Frame], cfg: &AnalysisConfig, stage_ms: &mut StageDurations) -> Vec<Segment> {
    let filtered = timed_stage!(stage_ms, merge_ms, {
        let merged = segmenter::merge_close_segments(speech_segments, cfg.merge_gap_ms);
        let floor_ms = cfg.merge_options.as_ref().map(|o| o.min_speech_duration).unwrap_or(cfg.min_speech_ms);
        let pruned = segmenter::prune_short_speech_segments(&merged, floor_ms);
        if should_apply_speech_evidence_filter(cfg) {
            speech_evidence::filter_implausible_speech_segments(&pruned, frames, cfg.frame_ms)
        } else {
            pruned
        }
    });
    info!(stage = "merge_segments", ms = stage_ms.merge_ms, segments = filtered.len(), "stage complete");
    filtered
}

#[rustfmt::skip]
fn non_voice_inversion_stage(input: &Path, filtered_speech: &[Segment], frame_likelihoods: &[f32], total_audio_ms: u64, cfg: &AnalysisConfig, stage_ms: &mut StageDurations) -> Result<Vec<Segment>> {
    let non_voice = timed_stage!(stage_ms, invert_ms, {
        let non_voice = segmenter::invert_to_non_voice(
            filtered_speech, total_audio_ms, cfg.min_non_voice_ms, cfg.frame_ms, frame_likelihoods,
        );
        let bridge_ms = cfg.merge_options.as_ref().map(|o| o.min_speech_duration).unwrap_or(0);
        let non_voice = segmenter::bridge_non_voice_segments(&non_voice, bridge_ms);
        let non_voice = if let Some(opts) = cfg.merge_options.as_ref() {
            segmenter::apply_non_voice_merge_policy(&non_voice, opts)
        } else {
            non_voice
        };
        nonvoice_expand::expand_non_voice_segments_into_ambiguous(
            &non_voice, frame_likelihoods, cfg.frame_ms, ambiguous_expand_max_ms(cfg),
        )
    });
    let segments = if let Some(max_ms) = cfg.max_non_voice_ms {
        let s_start = Instant::now();
        let split = segmenter::split_long_segments(
            non_voice, max_ms, cfg.min_non_voice_ms, cfg.frame_ms, frame_likelihoods,
        );
        stage_ms.invert_ms += s_start.elapsed().as_millis() as u64;
        split
    } else {
        non_voice
    };
    let v_start = Instant::now();
    let segments = if should_apply_verification_filter(cfg) {
        filter_low_confidence_non_voice_segments(input, &segments, default_filter_segment_confidence_ceiling())
    } else {
        segments
    };
    let segments = segmenter::bridge_residual_non_voice_gaps(&segments, residual_bridge_gap_ms(cfg));
    let segments = tail_recovery::extend_terminal_non_voice_segment(
        &segments, frame_likelihoods, cfg.frame_ms, total_audio_ms, cfg.min_non_voice_ms,
    );
    stage_ms.invert_ms += v_start.elapsed().as_millis() as u64;
    info!(stage = "invert", ms = stage_ms.invert_ms, segments = segments.len(), "stage complete");
    Ok(segments)
}

#[cfg(test)]
mod tests;
