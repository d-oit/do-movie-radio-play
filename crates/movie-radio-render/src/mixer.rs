use crate::agc::{apply_agc, apply_reverb};
use crate::spatial::{ReverbConfig, StereoPosition};
use anyhow::Result;
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

/// Context for audio mixing that reuses internal buffers to avoid redundant allocations.
#[derive(Debug, Clone, Default)]
pub struct Mixer {
    left_channel: Vec<f32>,
    right_channel: Vec<f32>,
    interleaved_output: Vec<f32>,
}

impl Mixer {
    /// Creates a new `Mixer` instance.
    pub fn new() -> Self {
        Self {
            left_channel: Vec::new(),
            right_channel: Vec::new(),
            interleaved_output: Vec::new(),
        }
    }

    /// Renders a mix of tracks into an interleaved stereo output buffer.
    /// Resizes and reuses internal buffers to minimize memory allocations.
    pub fn render_mix(&mut self, tracks: Vec<TrackInput>) -> Result<&[f32]> {
        let max_len = tracks.iter().map(|t| t.samples.len()).max().unwrap_or(0);
        if max_len == 0 {
            self.interleaved_output.clear();
            return Ok(&[]);
        }

        // Clean and prepare internal contiguous per-channel buffers
        self.left_channel.clear();
        self.left_channel.resize(max_len, 0.0);
        self.right_channel.clear();
        self.right_channel.resize(max_len, 0.0);

        for track in tracks {
            let sample_rate = track.sample_rate;

            let agc = apply_agc(
                track.samples,
                sample_rate,
                track.agc_attack,
                track.agc_release,
                track.agc_max_gain,
            )?;

            let reverb = if let Some(ref rev) = track.reverb {
                apply_reverb(agc, sample_rate, rev.delay_ms, rev.amplitude)?
            } else {
                agc
            };

            let (left_gain, right_gain) = track.position.gains();
            for (i, &s) in reverb.iter().enumerate() {
                if i < max_len {
                    self.left_channel[i] += s * left_gain;
                    self.right_channel[i] += s * right_gain;
                }
            }
        }

        // Interleave the separate contiguous channel buffers into the contiguous interleaved output buffer
        self.interleaved_output.clear();
        self.interleaved_output.resize(max_len * 2, 0.0);

        for i in 0..max_len {
            self.interleaved_output[i * 2] = self.left_channel[i];
            self.interleaved_output[i * 2 + 1] = self.right_channel[i];
        }

        // Peak normalisation — prevent clipping
        let peak = self
            .interleaved_output
            .iter()
            .map(|s| s.abs())
            .fold(0.0_f32, f32::max);
        if peak > 1.0 {
            let scale = 1.0 / peak;
            for s in &mut self.interleaved_output {
                *s *= scale;
            }
        }

        Ok(&self.interleaved_output)
    }
}

/// Renders a mix of tracks into a stereo output.
pub fn render_mix(tracks: Vec<TrackInput>) -> Result<Vec<f32>> {
    let mut mixer = Mixer::new();
    let res = mixer.render_mix(tracks)?;
    Ok(res.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
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

    #[test]
    fn test_render_mix_empty() -> Result<()> {
        let result = render_mix(Vec::new())?;
        assert!(result.is_empty());
        Ok(())
    }

    #[test]
    fn test_render_mix_single_track() -> Result<()> {
        let track = TrackInput {
            samples: vec![0.5, -0.5, 0.5, -0.5],
            sample_rate: 44100,
            position: StereoPosition::CENTRE,
            reverb: None,
            agc_attack: 0.01,
            agc_release: 0.1,
            agc_max_gain: 1.0, // Prevent gain scaling
        };

        let result = render_mix(vec![track])?;
        // Length must be 4 * 2 = 8 samples (stereo interleaved)
        assert_eq!(result.len(), 8);
        Ok(())
    }

    #[test]
    fn test_render_mix_mismatched_lengths() -> Result<()> {
        let track1 = TrackInput {
            samples: vec![0.2, -0.2],
            sample_rate: 44100,
            position: StereoPosition::HARD_LEFT,
            reverb: None,
            agc_attack: 0.01,
            agc_release: 0.1,
            agc_max_gain: 1.0,
        };

        let track2 = TrackInput {
            samples: vec![0.1, -0.1, 0.1, -0.1, 0.1, -0.1],
            sample_rate: 44100,
            position: StereoPosition::HARD_RIGHT,
            reverb: None,
            agc_attack: 0.01,
            agc_release: 0.1,
            agc_max_gain: 1.0,
        };

        let result = render_mix(vec![track1, track2])?;
        // Length must be equal to max length (6) * 2 = 12 samples
        assert_eq!(result.len(), 12);
        Ok(())
    }

    #[test]
    fn test_mixer_reusability() -> Result<()> {
        let track = TrackInput {
            samples: vec![0.1, 0.2, 0.3],
            sample_rate: 44100,
            position: StereoPosition::CENTRE,
            reverb: None,
            agc_attack: 0.01,
            agc_release: 0.1,
            agc_max_gain: 1.0,
        };

        let mut mixer = Mixer::new();
        let res1 = mixer.render_mix(vec![track.clone()])?.to_vec();
        let res2 = mixer.render_mix(vec![track])?.to_vec();

        // Sequential mixes of the same input must be completely identical (no state pollution)
        assert_eq!(res1, res2);
        Ok(())
    }
}
