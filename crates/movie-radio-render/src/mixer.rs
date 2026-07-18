use crate::agc::{apply_agc, apply_reverb};
use crate::spatial::{ReverbConfig, StereoPosition};
use anyhow::Result;
use movie_radio_types::{Segment, SegmentEvent, SegmentKind};
use serde::{Deserialize, Serialize};

/// Input track for the mixer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackInput {
    /// Mono audio samples
    pub samples: Vec<f32>,
    /// Sample rate in Hz
    pub sample_rate: u32,
    /// Spatial position of the track
    pub position: StereoPosition,
    /// Optional reverb configuration for this track
    #[serde(default)]
    pub reverb: Option<ReverbConfig>,
    /// AGC attack time in seconds
    #[serde(default = "default_agc_attack")]
    pub agc_attack: f32,
    /// AGC release time in seconds
    #[serde(default = "default_agc_release")]
    pub agc_release: f32,
    /// AGC maximum gain multiplier
    #[serde(default = "default_agc_max_gain")]
    pub agc_max_gain: f32,
}

fn default_agc_attack() -> f32 {
    0.01
}

fn default_agc_release() -> f32 {
    0.1
}

fn default_agc_max_gain() -> f32 {
    20.0
}

/// Renders a mix of tracks into a stereo output.
pub fn render_mix(tracks: Vec<TrackInput>) -> Result<Vec<f32>> {
    let mut no_op_callback: Option<fn(SegmentEvent) -> Result<()>> = None;
    render_mix_streaming(tracks, 4096, no_op_callback.as_mut())
}

/// Renders a mix of tracks into a stereo output in a block-by-block streaming manner.
/// Calls the optional `on_event` callback when segment events are detected.
#[allow(clippy::needless_range_loop)]
pub fn render_mix_streaming<F>(
    tracks: Vec<TrackInput>,
    block_size: usize,
    mut on_event: Option<&mut F>,
) -> Result<Vec<f32>>
where
    F: FnMut(SegmentEvent) -> Result<()>,
{
    let max_len = tracks.iter().map(|t| t.samples.len()).max().unwrap_or(0);
    if max_len == 0 {
        return Ok(Vec::new());
    }

    let sample_rate = tracks.first().map(|t| t.sample_rate).unwrap_or(48000);

    let mut processed_tracks = Vec::new();
    for track in tracks {
        let agc = apply_agc(
            track.samples,
            track.sample_rate,
            track.agc_attack,
            track.agc_release,
            track.agc_max_gain,
        )?;

        let reverb = if let Some(ref rev) = track.reverb {
            apply_reverb(agc, track.sample_rate, rev.delay_ms, rev.amplitude)?
        } else {
            agc
        };

        processed_tracks.push((reverb, track.position.gains()));
    }

    let mut mix = Vec::with_capacity(max_len * 2);
    let mut current_segment: Option<(usize, SegmentKind, Vec<f32>)> = None;
    let mut total_sample_idx = 0;

    for chunk_start in (0..max_len).step_by(block_size) {
        let chunk_end = (chunk_start + block_size).min(max_len);
        let chunk_len = chunk_end - chunk_start;

        let mut block_mix = vec![0.0_f32; chunk_len * 2];

        for (processed_samples, (left_gain, right_gain)) in &processed_tracks {
            let track_len = processed_samples.len();
            if chunk_start >= track_len {
                continue;
            }
            let end = chunk_end.min(track_len);
            for i in chunk_start..end {
                let s = processed_samples[i];
                let block_idx = i - chunk_start;
                block_mix[block_idx * 2] += s * left_gain;
                block_mix[block_idx * 2 + 1] += s * right_gain;
            }
        }

        if on_event.is_some() {
            let block_mono: Vec<f32> = block_mix
                .chunks_exact(2)
                .map(|ch| (ch[0] + ch[1]) * 0.5)
                .collect();

            let frame_len = (sample_rate as f32 * 0.02) as usize; // 20ms frame
            let frame_len = frame_len.max(1);

            for (frame_idx, frame) in block_mono.chunks(frame_len).enumerate() {
                let frame_samples_len = frame.len();
                if frame_samples_len == 0 {
                    continue;
                }
                let mut sum_sq = 0.0;
                for &s in frame {
                    sum_sq += s * s;
                }
                let rms = (sum_sq / frame_samples_len as f32).sqrt();
                let kind = if rms > 0.015 {
                    SegmentKind::Speech
                } else {
                    SegmentKind::NonVoice
                };

                let frame_sample_start = total_sample_idx + frame_idx * frame_len;

                match &mut current_segment {
                    Some((start_idx, curr_kind, confs)) => {
                        if *curr_kind == kind {
                            confs.push(rms);
                        } else {
                            let end_idx = frame_sample_start;
                            let start_ms = (*start_idx as f64 * 1000.0 / sample_rate as f64) as u64;
                            let end_ms = (end_idx as f64 * 1000.0 / sample_rate as f64) as u64;
                            let avg_conf = confs.iter().sum::<f32>() / confs.len() as f32;
                            if let Some(ref mut cb) = on_event {
                                cb(SegmentEvent::SegmentDetected {
                                    segment: Segment {
                                        start_ms,
                                        end_ms,
                                        kind: curr_kind.clone(),
                                        confidence: avg_conf,
                                        tags: vec![],
                                        prompt: None,
                                    },
                                })?;
                            }
                            *start_idx = frame_sample_start;
                            *curr_kind = kind;
                            confs.clear();
                            confs.push(rms);
                        }
                    }
                    None => {
                        current_segment = Some((frame_sample_start, kind, vec![rms]));
                    }
                }
            }
            total_sample_idx += block_mono.len();
        }

        mix.extend_from_slice(&block_mix);
    }

    if let Some((start_idx, curr_kind, confs)) = current_segment {
        let end_idx = max_len;
        let start_ms = (start_idx as f64 * 1000.0 / sample_rate as f64) as u64;
        let end_ms = (end_idx as f64 * 1000.0 / sample_rate as f64) as u64;
        let avg_conf = if confs.is_empty() {
            0.0
        } else {
            confs.iter().sum::<f32>() / confs.len() as f32
        };
        if let Some(ref mut cb) = on_event {
            cb(SegmentEvent::SegmentDetected {
                segment: Segment {
                    start_ms,
                    end_ms,
                    kind: curr_kind,
                    confidence: avg_conf,
                    tags: vec![],
                    prompt: None,
                },
            })?;
        }
    }

    // Peak normalisation — prevent clipping
    let peak = mix.iter().map(|s| s.abs()).fold(0.0_f32, f32::max);
    if peak > 1.0 {
        let scale = 1.0 / peak;
        for s in &mut mix {
            *s *= scale;
        }
    }

    Ok(mix)
}

#[cfg(test)]
mod tests {
    use crate::spatial::ReverbConfig;
    use anyhow::Result;

    #[test]
    fn reverb_does_not_produce_nan() -> Result<()> {
        let samples: Vec<f32> = (0..4800).map(|i| (i as f32 * 0.001).sin() * 0.5).collect();
        let result = crate::agc::apply_reverb(
            samples,
            48000,
            ReverbConfig::MEDIUM_ROOM.delay_ms,
            ReverbConfig::MEDIUM_ROOM.amplitude,
        )?;
        assert!(
            result.iter().all(|s| s.is_finite()),
            "reverb produced NaN/Inf"
        );
        Ok(())
    }

    #[test]
    fn dry_reverb_is_passthrough() -> Result<()> {
        let samples: Vec<f32> = vec![0.1, 0.2, 0.3, -0.1, -0.2];
        let result = crate::agc::apply_reverb(
            samples.clone(),
            44100,
            ReverbConfig::DRY.delay_ms,
            ReverbConfig::DRY.amplitude,
        )?;
        assert_eq!(result, samples);
        Ok(())
    }
}
