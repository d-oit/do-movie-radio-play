use anyhow::{Context, Result};
use std::path::Path;
use std::time::Instant;
use tracing::info;

use crate::pipeline::decode::{decode_audio_chunks_cb, decode_audio_chunks_prefer_streaming};
use crate::pipeline::framing::FrameStreamer;
use crate::pipeline::tri_state;
use crate::pipeline::vad::{
    adapt_spectral_thresholds, classify_with_engine, create_engine, supports_chunked_extraction,
};
use movie_radio_types::{AnalysisConfig, Frame, StageDurations, TimelineOutput};

/// Frames accumulated while decoding. Reset when a streaming decode attempt is
/// discarded and the file has to be decoded in full.
struct FrameSink {
    streamer: FrameStreamer,
    frames: Vec<Frame>,
    total_samples: u64,
}

impl FrameSink {
    fn new(cfg: &AnalysisConfig) -> Self {
        Self {
            streamer: FrameStreamer::new(cfg.sample_rate_hz, cfg.frame_ms),
            frames: Vec::new(),
            total_samples: 0,
        }
    }

    fn reset(&mut self, cfg: &AnalysisConfig) {
        *self = Self::new(cfg);
    }

    fn push(&mut self, chunk_samples: &[f32]) {
        self.total_samples += chunk_samples.len() as u64;
        self.frames.extend(self.streamer.push(chunk_samples));
    }

    fn finish(&mut self) -> Vec<Frame> {
        self.frames.extend(self.streamer.finish());
        std::mem::take(&mut self.frames)
    }
}

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

    let mut sink = FrameSink::new(cfg);
    let streamed = decode_audio_chunks_prefer_streaming(
        input,
        cfg.sample_rate_hz,
        chunk_duration_sec,
        |chunk_samples, chunk_idx| {
            sink.push(chunk_samples);
            if chunk_idx % 10 == 0 {
                info!(
                    chunk = chunk_idx,
                    samples = sink.total_samples,
                    frames = sink.frames.len(),
                    "chunk processed"
                );
            }
            Ok(())
        },
    )?;
    if !streamed {
        // Chunks from the failed incremental attempt are unusable, so retry
        // with the full-decode path rather than the streaming wrapper.
        sink.reset(cfg);
        decode_audio_chunks_cb(
            input,
            cfg.sample_rate_hz,
            chunk_duration_sec,
            |chunk_samples, _| {
                sink.push(chunk_samples);
                Ok(())
            },
        )?;
    }
    let total_samples = sink.total_samples;
    let frames = sink.finish();
    timeline_from_frames(input, cfg, frames, total_samples, total_start)
}

/// Samples read per step when replaying a raw `f32` file.
const RAW_READ_SAMPLES: usize = 1 << 20;

/// Extracts a timeline from a mono little-endian `f32` file at
/// `cfg.sample_rate_hz`, reading it in bounded steps.
///
/// Gives the same result as [`extract_timeline_chunked`] on the decode that
/// produced the file, without keeping the decoded samples in memory (#418).
pub fn extract_timeline_from_raw_f32(
    raw: &Path,
    label: &Path,
    cfg: &AnalysisConfig,
) -> Result<TimelineOutput> {
    use std::io::Read;

    if !supports_chunked_extraction(&cfg.vad_engine) {
        // Raw-sample engines keep detector state across the whole signal, so
        // they need it in one slice. The copy only lives for this call.
        let samples = crate::pipeline::decode::read_raw_f32(raw)?;
        return super::extract_timeline_from_samples_with_path(&samples, label, cfg);
    }

    let total_start = Instant::now();
    let mut file = std::io::BufReader::new(
        std::fs::File::open(raw).with_context(|| format!("cannot open {}", raw.display()))?,
    );
    let mut sink = FrameSink::new(cfg);
    let mut bytes = vec![0u8; RAW_READ_SAMPLES * 4];
    let mut samples = Vec::with_capacity(RAW_READ_SAMPLES);
    // A read may end mid-sample; carry the remainder into the next step.
    let mut filled = 0usize;
    loop {
        let read = file.read(&mut bytes[filled..])?;
        if read == 0 {
            break;
        }
        filled += read;
        let whole = filled - filled % 4;
        samples.clear();
        samples.extend(
            bytes[..whole]
                .as_chunks::<4>()
                .0
                .iter()
                .map(|b| f32::from_le_bytes(*b)),
        );
        sink.push(&samples);
        bytes.copy_within(whole..filled, 0);
        filled -= whole;
    }
    let total_samples = sink.total_samples;
    let frames = sink.finish();
    timeline_from_frames(label, cfg, frames, total_samples, total_start)
}

fn timeline_from_frames(
    input: &Path,
    cfg: &AnalysisConfig,
    frames: Vec<Frame>,
    total_samples: u64,
    total_start: Instant,
) -> Result<TimelineOutput> {
    let effective_threshold = cfg.energy_threshold + cfg.vad_threshold_delta;
    let frame_ms = cfg.frame_ms;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_file_extraction_matches_in_memory_extraction() {
        let cfg = AnalysisConfig::default();
        let rate = cfg.sample_rate_hz as usize;
        let samples: Vec<f32> = (0..rate * 30)
            .map(|i| {
                let t = i as f32 / rate as f32;
                if (i / (rate * 5)).is_multiple_of(2) {
                    0.2 * (2.0 * std::f32::consts::PI * 220.0 * t).sin()
                } else {
                    0.0003
                }
            })
            .collect();
        let dir = tempfile::tempdir().expect("tempdir");
        let raw = dir.path().join("audio.f32");
        let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        std::fs::write(&raw, bytes).expect("write raw");

        let label = Path::new("film.wav");
        let expected =
            crate::pipeline::extract_timeline_from_samples_with_path(&samples, label, &cfg)
                .expect("in-memory extraction");
        let streamed = extract_timeline_from_raw_f32(&raw, label, &cfg).expect("raw extraction");
        let span = |t: &TimelineOutput| -> Vec<(u64, u64)> {
            t.segments.iter().map(|s| (s.start_ms, s.end_ms)).collect()
        };
        assert!(!span(&expected).is_empty());
        assert_eq!(span(&expected), span(&streamed));
    }
}
