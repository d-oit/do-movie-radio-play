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
) -> Vec<f32> {
    if delay_ms == 0 || amplitude == 0.0 {
        return samples; // skip processing for dry signal
    }

    let channels = NonZeroU16::new(1).expect("1 is non-zero");
    let sample_rate_nz = NonZeroU32::new(sample_rate).expect("sample rate is non-zero");

    let source = rodio::buffer::SamplesBuffer::new(channels, sample_rate_nz, samples);
    let with_reverb = source.reverb(Duration::from_millis(delay_ms), amplitude);
    with_reverb.collect()
}

/// Placeholder for Automatic Gain Control.
pub fn apply_agc(samples: Vec<f32>, _sample_rate: u32) -> Vec<f32> {
    // Current placeholder logic: pass-through
    samples
}
