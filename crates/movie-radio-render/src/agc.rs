use anyhow::{bail, Context, Result};
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
    if !amplitude.is_finite() || !(0.0..=1.0).contains(&amplitude) {
        bail!("reverb amplitude must be finite and in the range [0.0, 1.0], got {amplitude}");
    }

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
    if !attack_time.is_finite() || attack_time < 0.0 {
        bail!("AGC attack_time must be a finite, non-negative number, got {attack_time}");
    }
    if !release_time.is_finite() || release_time < 0.0 {
        bail!("AGC release_time must be a finite, non-negative number, got {release_time}");
    }
    if !max_gain.is_finite() || max_gain < 0.0 {
        bail!("AGC max_gain must be a finite, non-negative number, got {max_gain}");
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_agc_invalid_inputs() {
        let samples = vec![0.0; 100];

        // Negative attack_time
        assert!(apply_agc(samples.clone(), 16000, -0.01, 0.1, 1.0).is_err());
        // NaN attack_time
        assert!(apply_agc(samples.clone(), 16000, f32::NAN, 0.1, 1.0).is_err());
        // Infinite attack_time
        assert!(apply_agc(samples.clone(), 16000, f32::INFINITY, 0.1, 1.0).is_err());

        // Negative release_time
        assert!(apply_agc(samples.clone(), 16000, 0.01, -0.1, 1.0).is_err());
        // NaN release_time
        assert!(apply_agc(samples.clone(), 16000, 0.01, f32::NAN, 1.0).is_err());
        // Infinite release_time
        assert!(apply_agc(samples.clone(), 16000, 0.01, f32::INFINITY, 1.0).is_err());

        // Negative max_gain
        assert!(apply_agc(samples.clone(), 16000, 0.01, 0.1, -1.0).is_err());
        // NaN max_gain
        assert!(apply_agc(samples.clone(), 16000, 0.01, 0.1, f32::NAN).is_err());
        // Infinite max_gain
        assert!(apply_agc(samples.clone(), 16000, 0.01, 0.1, f32::INFINITY).is_err());
    }

    #[test]
    fn test_apply_reverb_invalid_inputs() {
        let samples = vec![0.0; 100];

        // Negative amplitude
        assert!(apply_reverb(samples.clone(), 16000, 30, -0.1).is_err());
        // Amplitude > 1.0
        assert!(apply_reverb(samples.clone(), 16000, 30, 1.1).is_err());
        // NaN amplitude
        assert!(apply_reverb(samples.clone(), 16000, 30, f32::NAN).is_err());
        // Infinite amplitude
        assert!(apply_reverb(samples.clone(), 16000, 30, f32::INFINITY).is_err());
    }

    #[test]
    fn test_apply_agc_valid() {
        let samples = vec![0.0; 100];
        let result = apply_agc(samples, 16000, 0.01, 0.1, 2.0);
        assert!(result.is_ok());
    }

    #[test]
    fn test_apply_reverb_valid() {
        let samples = vec![0.0; 100];
        let result = apply_reverb(samples, 16000, 30, 0.5);
        assert!(result.is_ok());
    }
}
