use anyhow::{Context, Result};
use rodio::source::AutomaticGainControlSettings;
use rodio::Source;
use std::num::{NonZeroU16, NonZeroU32};
use std::time::Duration;

/// Apply rodio reverb to a mono f32 sample buffer.
/// Returns a new Vec<f32> with reverb applied.
pub fn apply_reverb(
    samples: Vec<f32>,
    sample_rate: u32,
    delay_ms: u64,
    amplitude: f32,
) -> Result<Vec<f32>> {
    if delay_ms == 0 || amplitude == 0.0 {
        return Ok(samples);
    }

    let channels = NonZeroU16::new(1).context("1 is non-zero")?;
    let sample_rate_nz =
        NonZeroU32::new(sample_rate).context("sample rate must be greater than zero")?;

    let source = rodio::buffer::SamplesBuffer::new(channels, sample_rate_nz, samples);
    let with_reverb = source.reverb(Duration::from_millis(delay_ms), amplitude);
    Ok(with_reverb.collect())
}

/// Apply Automatic Gain Control to a mono f32 sample buffer using rodio.
pub fn apply_agc(
    samples: Vec<f32>,
    sample_rate: u32,
    attack_time: f32,
    release_time: f32,
    max_gain: f32,
) -> Result<Vec<f32>> {
    let channels = NonZeroU16::new(1).context("1 is non-zero")?;
    let sample_rate_nz =
        NonZeroU32::new(sample_rate).context("sample rate must be greater than zero")?;

    let source = rodio::buffer::SamplesBuffer::new(channels, sample_rate_nz, samples);
    let settings = AutomaticGainControlSettings {
        target_level: 1.0,
        attack_time: Duration::from_secs_f32(attack_time),
        release_time: Duration::from_secs_f32(release_time),
        absolute_max_gain: max_gain,
    };
    let result: Vec<f32> = source.automatic_gain_control(settings).collect();
    Ok(result)
}
