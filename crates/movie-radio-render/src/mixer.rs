use crate::agc::{apply_agc, apply_reverb};
use crate::spatial::{ReverbConfig, StereoPosition};
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
    /// If None, defaults to ReverbConfig::DRY (no reverb)
    #[serde(default)]
    pub reverb: Option<ReverbConfig>,
}

/// Renders a mix of tracks into a stereo output.
pub fn render_mix(tracks: Vec<TrackInput>) -> Vec<f32> {
    let mut _mixed = Vec::new();

    for track in tracks {
        let sample_rate = track.sample_rate;

        // 1. Apply AGC
        let normalized = apply_agc(track.samples, sample_rate);

        // 2. Apply Reverb
        let with_reverb = if let Some(ref rev) = track.reverb {
            apply_reverb(normalized, sample_rate, rev.delay_ms, rev.amplitude)
        } else {
            normalized
        };

        // 3. Spatial Pan (placeholder: returns mono buffer for now)
        let _spatial = apply_spatial(with_reverb, track.position);

        // Mixing logic would go here
    }

    _mixed
}

/// Placeholder for spatial panning
fn apply_spatial(samples: Vec<f32>, _position: StereoPosition) -> Vec<f32> {
    samples
}

#[cfg(test)]
mod tests {
    use crate::spatial::ReverbConfig;

    #[test]
    fn reverb_does_not_produce_nan() {
        let samples: Vec<f32> = (0..4800).map(|i| (i as f32 * 0.001).sin() * 0.5).collect();
        let result = crate::agc::apply_reverb(
            samples,
            48000,
            ReverbConfig::MEDIUM_ROOM.delay_ms,
            ReverbConfig::MEDIUM_ROOM.amplitude,
        );
        assert!(
            result.iter().all(|s| s.is_finite()),
            "reverb produced NaN/Inf"
        );
    }

    #[test]
    fn dry_reverb_is_passthrough() {
        let samples: Vec<f32> = vec![0.1, 0.2, 0.3, -0.1, -0.2];
        let result = crate::agc::apply_reverb(
            samples.clone(),
            44100,
            ReverbConfig::DRY.delay_ms,
            ReverbConfig::DRY.amplitude,
        );
        assert_eq!(result, samples);
    }
}
