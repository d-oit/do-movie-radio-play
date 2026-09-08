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

#[allow(dead_code)]
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

    #[test]
    fn resample_linear_matches_expectation() {
        let input = vec![0.0, 1.0];
        let output = resample_linear(&input, 2, 4);
        assert_eq!(output.len(), 4);
        assert!((output[0] - 0.0).abs() < 1e-6);
        assert!((output[2] - 1.0).abs() < 1e-6);
    }

    #[cfg(feature = "high-quality-resample")]
    #[test]
    fn resample_sinc_vs_linear_quality_comparison() {
        // Downsample a 1 kHz sine wave from 48 kHz to 16 kHz.
        // The sinc resampler should reconstruct the bandlimited signal with higher fidelity
        // (lower RMS error against the true analytical sine wave at 16 kHz) than linear interpolation.
        let src_rate = 48000;
        let dst_rate = 16000;
        let freq = 1000.0f32;
        let num_src_samples = 4800; // 100 ms of audio

        let src_samples: Vec<f32> = (0..num_src_samples)
            .map(|i| (2.0 * std::f32::consts::PI * freq * (i as f32) / (src_rate as f32)).sin())
            .collect();

        let sinc_out = resample(&src_samples, src_rate, dst_rate).unwrap();
        let linear_out = resample_linear(&src_samples, src_rate, dst_rate);

        // Sinc resampler introduces filter latency / group delay.
        // Measure spectral energy retention around the target frequency (1 kHz) vs out-of-band energy to demonstrate sinc's higher anti-aliasing / bandlimited quality compared to linear interpolation.
        let compute_sine_correlation = |samples: &[f32]| -> f32 {
            let n = samples.len();
            let dot: f32 = samples
                .iter()
                .enumerate()
                .map(|(i, &s)| {
                    let t = i as f32 / (dst_rate as f32);
                    s * (2.0 * std::f32::consts::PI * freq * t).sin()
                })
                .sum();
            (2.0 * dot / (n as f32)).abs()
        };

        let sinc_corr = compute_sine_correlation(&sinc_out[100..sinc_out.len() - 100]);
        let linear_corr = compute_sine_correlation(&linear_out[100..linear_out.len() - 100]);

        // Sinc interpolation preserves passband amplitude closer to 1.0 (near unity gain for 1 kHz)
        let sinc_amplitude_error = (sinc_corr - 1.0).abs();
        let linear_amplitude_error = (linear_corr - 1.0).abs();

        assert!(
            sinc_amplitude_error < linear_amplitude_error,
            "Sinc resampler passband amplitude error ({}) should be lower than linear resampler amplitude error ({})",
            sinc_amplitude_error,
            linear_amplitude_error
        );
    }
}
