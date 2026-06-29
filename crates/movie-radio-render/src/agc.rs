use rodio::Source;
use std::num::NonZeroU16;
use std::num::NonZeroU32;
use std::time::Duration;

/// Wrapper that applies rodio's AutomaticGainControl to a `Vec<f32>` sample buffer.
/// Returns normalized f32 samples.
pub fn apply_agc(
    samples: Vec<f32>,
    sample_rate: u32,
    attack_time: f32,
    release_time: f32,
    absolute_max_gain: f32,
) -> Vec<f32> {
    let source = rodio::buffer::SamplesBuffer::new(
        NonZeroU16::new(1).unwrap(),
        NonZeroU32::new(sample_rate).unwrap(),
        samples,
    );
    let settings = rodio::source::AutomaticGainControlSettings {
        target_level: 1.0,
        attack_time: Duration::from_secs_f32(attack_time),
        release_time: Duration::from_secs_f32(release_time),
        absolute_max_gain,
    };
    let agc = source.automatic_gain_control(settings);
    agc.collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agc_silence() {
        let samples = vec![0.0f32; 1000];
        let normalized = apply_agc(samples, 44100, 0.1, 0.15, 10.0);
        assert_eq!(normalized.len(), 1000);
        for &s in &normalized {
            assert!(!s.is_nan());
            assert!(!s.is_infinite());
            assert_eq!(s, 0.0);
        }
    }
}
