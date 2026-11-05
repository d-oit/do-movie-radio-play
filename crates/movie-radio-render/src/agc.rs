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
    validate_agc_params(attack_time, release_time, max_gain)?;

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

fn validate_agc_params(attack_time: f32, release_time: f32, max_gain: f32) -> Result<()> {
    if !attack_time.is_finite() || attack_time < 0.0 {
        bail!("AGC attack_time must be a finite, non-negative number, got {attack_time}");
    }
    if !release_time.is_finite() || release_time < 0.0 {
        bail!("AGC release_time must be a finite, non-negative number, got {release_time}");
    }
    if !max_gain.is_finite() || max_gain < 0.0 {
        bail!("AGC max_gain must be a finite, non-negative number, got {max_gain}");
    }
    Ok(())
}

/// Streaming Automatic Gain Control that matches rodio's algorithm sample for
/// sample, so a track can be processed in place or in chunks without a second
/// full-length buffer.
///
/// rodio's AGC keeps an 8192-sample RMS window, an instant-attack peak
/// follower and an asymmetric attack/release gain smoother. All of that state
/// is carried here, so [`StreamingAgc::process`] may be called with any
/// chunking.
pub struct StreamingAgc {
    target_level: f32,
    absolute_max_gain: f32,
    attack_coeff: f32,
    release_coeff: f32,
    current_gain: f32,
    peak_level: f32,
    rms_window: Box<[f32; RMS_WINDOW_SIZE]>,
    rms_sum: f32,
    rms_index: usize,
}

/// rodio's RMS window size; the running sum is divided by this constant.
const RMS_WINDOW_SIZE: usize = 8192;

impl StreamingAgc {
    pub fn new(
        sample_rate: u32,
        attack_time: f32,
        release_time: f32,
        max_gain: f32,
    ) -> Result<Self> {
        validate_agc_params(attack_time, release_time, max_gain)?;
        if sample_rate == 0 {
            bail!("sample rate must be greater than zero");
        }
        Ok(Self {
            target_level: 1.0,
            absolute_max_gain: max_gain,
            attack_coeff: duration_to_coefficient(attack_time, sample_rate),
            release_coeff: duration_to_coefficient(release_time, sample_rate),
            current_gain: 1.0,
            peak_level: 0.0,
            rms_window: Box::new([0.0; RMS_WINDOW_SIZE]),
            rms_sum: 0.0,
            rms_index: 0,
        })
    }

    /// Applies the gain to `samples` in place.
    pub fn process(&mut self, samples: &mut [f32]) {
        for sample in samples {
            let level = sample.abs();

            let peak_coeff = if level > self.peak_level {
                0.0
            } else {
                self.release_coeff
            };
            self.peak_level = self.peak_level * peak_coeff + level * (1.0 - peak_coeff);

            let squared = level * level;
            let old = self.rms_window[self.rms_index];
            self.rms_sum = self.rms_sum - old + squared;
            self.rms_window[self.rms_index] = squared;
            self.rms_index = (self.rms_index + 1) & (RMS_WINDOW_SIZE - 1);
            let rms = (self.rms_sum / RMS_WINDOW_SIZE as f32).sqrt();

            let rms_gain = if rms > 0.0 {
                self.target_level / rms
            } else {
                self.absolute_max_gain
            };
            let peak_gain = if self.peak_level > 0.0 {
                (self.target_level / self.peak_level).min(self.absolute_max_gain)
            } else {
                self.absolute_max_gain
            };
            let desired_gain = rms_gain.min(peak_gain);

            let speed = if desired_gain > self.current_gain {
                self.attack_coeff
            } else {
                self.release_coeff
            };
            self.current_gain = self.current_gain * speed + desired_gain * (1.0 - speed);
            self.current_gain = self.current_gain.clamp(0.1, self.absolute_max_gain);

            *sample *= self.current_gain;
        }
    }
}

/// Mirrors rodio's `duration_to_coefficient` for the f32 sample type.
fn duration_to_coefficient(duration_secs: f32, sample_rate: u32) -> f32 {
    if duration_secs <= 0.0 {
        return 0.0;
    }
    (-1.0 / (duration_secs * sample_rate as f32)).exp()
}

/// Applies rodio-equivalent AGC to a mono buffer in place.
pub fn apply_agc_in_place(
    samples: &mut [f32],
    sample_rate: u32,
    attack_time: f32,
    release_time: f32,
    max_gain: f32,
) -> Result<()> {
    let mut agc = StreamingAgc::new(sample_rate, attack_time, release_time, max_gain)?;
    agc.process(samples);
    Ok(())
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

    fn signal(len: usize) -> Vec<f32> {
        (0..len)
            .map(|i| {
                let t = i as f32;
                (t * 0.013).sin() * 0.3 + (t * 0.0007).cos() * 0.2 + (t * 0.11).sin() * 0.05
            })
            .collect()
    }

    #[test]
    fn streaming_agc_matches_rodio() {
        for (attack, release, max_gain) in [
            (0.01_f32, 0.1_f32, 1.0_f32),
            (0.01, 0.1, 20.0),
            (0.5, 0.0, 7.0),
            (0.0, 0.0, 2.0),
        ] {
            let input = signal(40_000);
            let expected =
                apply_agc(input.clone(), 16_000, attack, release, max_gain).expect("rodio agc");
            let mut actual = input;
            apply_agc_in_place(&mut actual, 16_000, attack, release, max_gain)
                .expect("streaming agc");
            assert_eq!(
                expected, actual,
                "mismatch for attack={attack} release={release} max_gain={max_gain}"
            );
        }
    }

    #[test]
    fn streaming_agc_is_chunking_independent() {
        let input = signal(20_000);
        let mut whole = input.clone();
        apply_agc_in_place(&mut whole, 16_000, 0.01, 0.1, 20.0).expect("whole");

        for chunk in [1, 7, 1000, 8192] {
            let mut agc = StreamingAgc::new(16_000, 0.01, 0.1, 20.0).expect("agc");
            let mut chunked = input.clone();
            for part in chunked.chunks_mut(chunk) {
                agc.process(part);
            }
            assert_eq!(whole, chunked, "chunk size {chunk} changed the output");
        }
    }
}
