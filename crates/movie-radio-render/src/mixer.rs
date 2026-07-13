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
    pub agc_attack: f32,
    /// AGC release time in seconds
    pub agc_release: f32,
    /// AGC maximum gain multiplier
    pub agc_max_gain: f32,
}

/// Renders a mix of tracks into a stereo output.
pub fn render_mix(tracks: Vec<TrackInput>) -> Result<Vec<f32>> {
    let max_len = tracks.iter().map(|t| t.samples.len()).max().unwrap_or(0);
    if max_len == 0 {
        return Ok(Vec::new());
    }

    let mut mix = vec![0.0_f32; max_len * 2];

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

        let stereo = apply_spatial(reverb, track.position);

        for (i, s) in stereo.iter().enumerate() {
            mix[i] += s;
        }
    }

    // Peak normalisation — prevent clipping
    if let Some(&peak) = mix
        .iter()
        .max_by(|a, b| a.abs().partial_cmp(&b.abs()).unwrap())
    {
        let scale = 1.0 / peak.abs();
        if scale < 1.0 {
            for s in &mut mix {
                *s *= scale;
            }
        }
    }

    Ok(mix)
}

/// Apply constant-power spatial panning to mono samples, returning stereo interleaved.
fn apply_spatial(samples: Vec<f32>, position: StereoPosition) -> Vec<f32> {
    let (left_gain, right_gain) = position.gains();
    let mut stereo = Vec::with_capacity(samples.len() * 2);
    for s in samples {
        stereo.push(s * left_gain);
        stereo.push(s * right_gain);
    }
    stereo
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
