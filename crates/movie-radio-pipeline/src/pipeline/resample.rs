use anyhow::Result;

#[cfg(feature = "high-quality-resample")]
pub fn resample(input: &[f32], src_rate: u32, dst_rate: u32) -> Result<Vec<f32>> {
    use anyhow::Context;
    use rubato::audioadapter::Adapter;
    use rubato::audioadapter_buffers::owned::InterleavedOwned;
    use rubato::{
        Async, FixedAsync, Resampler, SincInterpolationParameters, SincInterpolationType,
        WindowFunction,
    };

    if src_rate == dst_rate || input.is_empty() {
        return Ok(input.to_vec());
    }
    let ratio = dst_rate as f64 / src_rate as f64;
    let params = SincInterpolationParameters {
        sinc_len: 256,
        f_cutoff: Some(0.95),
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 256,
        window: WindowFunction::Blackman2,
    };
    let mut resampler =
        Async::<f32>::new_sinc(ratio, 1.0, &params, input.len(), 1, FixedAsync::Input)
            .context("failed to initialize resampler with provided parameters")?;
    let buffer_in = InterleavedOwned::<f32>::new_from(input.to_vec(), 1, input.len())
        .context("failed to create resampler input buffer")?;
    let result = resampler
        .process(&buffer_in, None)
        .context("resampling process failed")?;
    let frames_out = result.frames();
    let mut output = result.take_data();
    output.truncate(frames_out);
    Ok(output)
}

#[cfg(not(feature = "high-quality-resample"))]
pub fn resample(input: &[f32], src_rate: u32, dst_rate: u32) -> Result<Vec<f32>> {
    Ok(resample_linear(input, src_rate, dst_rate))
}

/// Whether decoded audio can be resampled incrementally. The sinc path needs
/// the whole buffer, so streaming decode is limited to the linear path.
pub const fn streaming_supported() -> bool {
    cfg!(not(feature = "high-quality-resample"))
}

/// Incremental linear resampler. Yields exactly the samples [`resample`]
/// produces for the same input regardless of how it is split across
/// [`StreamingResampler::push`] calls, so streaming and whole-file decode stay
/// sample-identical.
#[cfg(not(feature = "high-quality-resample"))]
pub struct StreamingResampler {
    ratio: f64,
    passthrough: bool,
    next_out: u64,
    base: u64,
    total_in: u64,
    buf: std::collections::VecDeque<f32>,
}

#[cfg(not(feature = "high-quality-resample"))]
impl StreamingResampler {
    pub fn new(src_rate: u32, dst_rate: u32) -> Self {
        Self {
            ratio: dst_rate as f64 / src_rate as f64,
            passthrough: src_rate == dst_rate,
            next_out: 0,
            base: 0,
            total_in: 0,
            buf: std::collections::VecDeque::new(),
        }
    }

    pub fn push(&mut self, input: &[f32]) -> Vec<f32> {
        self.total_in += input.len() as u64;
        if self.passthrough {
            return input.to_vec();
        }
        self.buf.extend(input.iter().copied());
        self.emit(false)
    }

    pub fn finish(&mut self) -> Vec<f32> {
        if self.passthrough {
            return Vec::new();
        }
        self.emit(true)
    }

    fn sample(&self, idx: u64) -> Option<f32> {
        if idx < self.base || idx >= self.total_in {
            return None;
        }
        self.buf.get((idx - self.base) as usize).copied()
    }

    fn emit(&mut self, final_pass: bool) -> Vec<f32> {
        // The final length is `round(total * ratio)`, which is not known until
        // the stream ends. `floor(total * ratio)` is a safe lower bound, so a
        // streaming pass never emits a sample the whole-buffer path would omit.
        let out_len = if final_pass {
            (self.total_in as f64 * self.ratio).round() as u64
        } else {
            (self.total_in as f64 * self.ratio).floor() as u64
        };
        let mut out = Vec::new();
        while self.next_out < out_len {
            let src_pos = self.next_out as f64 / self.ratio;
            let idx = src_pos.floor() as u64;
            // A non-final pass can only emit samples whose right neighbour is
            // already decoded; the tail is produced by `finish`.
            if !final_pass && (self.sample(idx).is_none() || self.sample(idx + 1).is_none()) {
                break;
            }
            let a = self.sample(idx).unwrap_or(0.0);
            let b = self.sample(idx + 1).unwrap_or(a);
            let frac = (src_pos - idx as f64) as f32;
            out.push(a + (b - a) * frac);
            self.next_out += 1;
        }
        let keep_from = (self.next_out as f64 / self.ratio).floor() as u64;
        while self.base < keep_from && !self.buf.is_empty() {
            self.buf.pop_front();
            self.base += 1;
        }
        out
    }
}

#[cfg(not(feature = "high-quality-resample"))]
fn resample_linear(input: &[f32], src_rate: u32, dst_rate: u32) -> Vec<f32> {
    if src_rate == dst_rate || input.is_empty() {
        return input.to_vec();
    }
    let ratio = dst_rate as f64 / src_rate as f64;
    let out_len = (input.len() as f64 * ratio).round() as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src_pos = i as f64 / ratio;
        let idx = src_pos.floor() as usize;
        let frac = (src_pos - idx as f64) as f32;
        let a = *input.get(idx).unwrap_or(&0.0);
        let b = *input.get(idx + 1).unwrap_or(&a);
        out.push(a + (b - a) * frac);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resample_identity_when_rates_match() {
        let input = vec![0.0, 0.5, 1.0, 0.5, 0.0];
        let output = resample(&input, 16000, 16000).unwrap();
        assert_eq!(output, input);
    }

    #[test]
    fn resample_empty_input() {
        let input: Vec<f32> = vec![];
        let output = resample(&input, 48000, 16000).unwrap();
        assert!(output.is_empty());
    }

    #[test]
    fn resample_downsample_48k_to_16k() {
        let input: Vec<f32> = (0..192)
            .map(|i| (i as f32 / 192.0 * std::f32::consts::TAU).sin())
            .collect();
        let output = resample(&input, 48000, 16000).unwrap();
        // rubato async resampler output may vary by 1 from exact ratio
        let expected = (input.len() as f64 * 16000.0 / 48000.0).round() as usize;
        assert!(
            output.len() == expected || output.len() == expected - 1,
            "expected {} or {}, got {}",
            expected,
            expected - 1,
            output.len()
        );
        assert!(output.iter().all(|s| s.is_finite()));
    }

    #[cfg(not(feature = "high-quality-resample"))]
    #[test]
    fn resample_linear_matches_expectation() {
        let input = vec![0.0, 1.0];
        let output = resample_linear(&input, 2, 4);
        assert_eq!(output.len(), 4);
        assert!((output[0] - 0.0).abs() < 1e-6);
        assert!((output[2] - 1.0).abs() < 1e-6);
    }

    #[cfg(not(feature = "high-quality-resample"))]
    fn tone(len: usize, hz: f32, rate: u32) -> Vec<f32> {
        (0..len)
            .map(|i| (i as f32 / rate as f32 * hz * std::f32::consts::TAU).sin() * 0.5)
            .collect()
    }

    #[cfg(not(feature = "high-quality-resample"))]
    #[test]
    fn streaming_resampler_matches_whole_buffer_for_split_input() {
        for (src, dst) in [(48_000u32, 16_000u32), (44_100, 16_000), (16_000, 16_000)] {
            let input = tone(src as usize * 2 + 37, 440.0, src);
            let whole = resample(&input, src, dst).unwrap();

            for split in [1usize, 97, 1_000, 8_191] {
                let mut resampler = StreamingResampler::new(src, dst);
                let mut streamed = Vec::new();
                for chunk in input.chunks(split) {
                    streamed.extend(resampler.push(chunk));
                }
                streamed.extend(resampler.finish());
                assert_eq!(
                    streamed.len(),
                    whole.len(),
                    "split {split} at {src}->{dst} produced a different sample count"
                );
                for (idx, (got, want)) in streamed.iter().zip(&whole).enumerate() {
                    assert_eq!(got, want, "split {split} at {src}->{dst} diverged at {idx}");
                }
            }
        }
    }

    #[cfg(not(feature = "high-quality-resample"))]
    #[test]
    fn streaming_resampler_upsamples_across_boundaries() {
        let input = tone(4_000, 300.0, 16_000);
        let whole = resample(&input, 16_000, 48_000).unwrap();
        let mut resampler = StreamingResampler::new(16_000, 48_000);
        let mut streamed = Vec::new();
        for chunk in input.chunks(333) {
            streamed.extend(resampler.push(chunk));
        }
        streamed.extend(resampler.finish());
        assert_eq!(streamed, whole);
    }

    /// The whole point of the `high-quality-resample` feature: the sinc path
    /// must differ measurably from linear interpolation for an in-band tone
    /// (linear interpolation error is O((2πf/fs)²)). Guards against the
    /// feature silently degrading to the linear fallback.
    #[cfg(feature = "high-quality-resample")]
    #[test]
    fn resample_sinc_differs_from_linear_for_inband_tone() {
        // 3 kHz tone at 48 kHz, downsampled to 16 kHz (3 kHz < 8 kHz Nyquist).
        let input: Vec<f32> = (0..4800)
            .map(|i| (i as f32 / 48_000.0 * 3_000.0 * std::f32::consts::TAU).sin())
            .collect();
        let sinc = resample(&input, 48_000, 16_000).unwrap();

        // Local linear-interpolation reference (the cfg-gated helper is not
        // compiled under this feature).
        let ratio = 16_000_f64 / 48_000_f64;
        let out_len = (input.len() as f64 * ratio).round() as usize;
        let linear: Vec<f32> = (0..out_len)
            .map(|i| {
                let src_pos = i as f64 / ratio;
                let idx = src_pos.floor() as usize;
                let frac = (src_pos - idx as f64) as f32;
                let a = input[idx];
                let b = input.get(idx + 1).copied().unwrap_or(a);
                a + (b - a) * frac
            })
            .collect();

        let n = sinc.len().min(linear.len());
        assert!(n > 1000, "suspiciously short output: {n}");
        let max_diff = sinc[..n]
            .iter()
            .zip(&linear[..n])
            .map(|(a, b)| (a - b).abs())
            .fold(0.0_f32, f32::max);
        assert!(
            max_diff > 1e-4,
            "sinc output identical to linear interpolation (max diff {max_diff})"
        );
        assert!(sinc.iter().all(|s| s.is_finite()));
    }
}
