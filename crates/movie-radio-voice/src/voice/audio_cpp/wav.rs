use super::AudioOutput;
use anyhow::{Context, Result};

pub(crate) struct WavLayout {
    pub data: std::ops::Range<usize>,
    pub sample_rate: u32,
    pub channels: u16,
    pub bits_per_sample: u16,
}

pub(crate) fn parse_fmt_chunk_info(
    bytes: &[u8],
    data_start: usize,
    size: usize,
) -> Option<(u32, u16, u16)> {
    if size < 16 || data_start + 16 > bytes.len() {
        return None;
    }
    let fmt = &bytes[data_start..data_start + 16];
    let audio_format = u16::from_le_bytes([fmt[0], fmt[1]]);
    if audio_format != 1 {
        return None;
    }
    let channels = u16::from_le_bytes([fmt[2], fmt[3]]);
    let sample_rate = u32::from_le_bytes([fmt[4], fmt[5], fmt[6], fmt[7]]);
    let bits_per_sample = u16::from_le_bytes([fmt[14], fmt[15]]);
    Some((sample_rate, channels, bits_per_sample))
}

pub(crate) fn parse_wav_layout(bytes: &[u8]) -> Result<WavLayout> {
    const HEADER_LEN: usize = 12;
    if bytes.len() < HEADER_LEN || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        anyhow::bail!("Not a valid RIFF/WAVE container");
    }

    let mut fmt_info: Option<(u32, u16, u16)> = None;
    let mut data_range: Option<std::ops::Range<usize>> = None;
    let mut offset = HEADER_LEN;

    while offset + 8 <= bytes.len() {
        let id = &bytes[offset..offset + 4];
        let size = u32::from_le_bytes(
            bytes[offset + 4..offset + 8]
                .try_into()
                .context("Invalid chunk header size")?,
        ) as usize;
        let data_start = offset + 8;
        let data_end = match data_start.checked_add(size) {
            Some(end) if end <= bytes.len() => end,
            _ => break,
        };

        if id == b"fmt " {
            fmt_info = parse_fmt_chunk_info(bytes, data_start, size);
        } else if id == b"data" {
            data_range = Some(data_start..data_end);
        }

        offset = data_end + (size & 1);
    }

    let (sample_rate, channels, bits_per_sample) =
        fmt_info.ok_or_else(|| anyhow::anyhow!("WAVE container missing valid PCM fmt chunk"))?;
    let data = data_range.ok_or_else(|| anyhow::anyhow!("WAVE container missing data chunk"))?;

    Ok(WavLayout {
        data,
        sample_rate,
        channels,
        bits_per_sample,
    })
}

pub(crate) fn decode_and_resample_wav(
    bytes: &[u8],
    target_sample_rate_hz: u32,
) -> Result<AudioOutput> {
    let layout = parse_wav_layout(bytes)?;

    if layout.bits_per_sample != 16 {
        anyhow::bail!(
            "Unsupported bit depth {} (expected 16-bit PCM)",
            layout.bits_per_sample
        );
    }

    let raw_data = &bytes[layout.data];
    let channels = layout.channels as usize;
    if channels == 0 {
        anyhow::bail!("WAVE container has 0 channels");
    }

    let frame_count = raw_data.len() / (2 * channels);
    let mut mono_samples = Vec::with_capacity(frame_count);

    for i in 0..frame_count {
        let frame_offset = i * 2 * channels;
        let mut sum = 0.0f32;
        for c in 0..channels {
            let sample_offset = frame_offset + c * 2;
            let sample_i16 =
                i16::from_le_bytes([raw_data[sample_offset], raw_data[sample_offset + 1]]);
            sum += sample_i16 as f32 / i16::MAX as f32;
        }
        mono_samples.push(sum / (channels as f32));
    }

    let src_rate = layout.sample_rate;
    if src_rate == target_sample_rate_hz || mono_samples.is_empty() {
        return Ok(AudioOutput {
            samples: mono_samples,
            sample_rate_hz: target_sample_rate_hz,
        });
    }

    let ratio = (src_rate as f64) / (target_sample_rate_hz as f64);
    let target_len = ((mono_samples.len() as f64) / ratio).round() as usize;
    let mut resampled = Vec::with_capacity(target_len);

    for i in 0..target_len {
        let src_idx = (i as f64) * ratio;
        let idx0 = src_idx.floor() as usize;
        let idx1 = (idx0 + 1).min(mono_samples.len() - 1);
        let frac = (src_idx - (idx0 as f64)) as f32;

        let s0 = mono_samples.get(idx0).copied().unwrap_or(0.0);
        let s1 = mono_samples.get(idx1).copied().unwrap_or(s0);
        resampled.push(s0 * (1.0 - frac) + s1 * frac);
    }

    Ok(AudioOutput {
        samples: resampled,
        sample_rate_hz: target_sample_rate_hz,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_wav_pcm16_mono(sample_rate: u32, samples: &[i16]) -> Vec<u8> {
        let data_len = (samples.len() * 2) as u32;
        let mut fmt = Vec::with_capacity(16);
        fmt.extend_from_slice(&1u16.to_le_bytes()); // PCM = 1
        fmt.extend_from_slice(&1u16.to_le_bytes()); // Mono = 1
        fmt.extend_from_slice(&sample_rate.to_le_bytes());
        fmt.extend_from_slice(&(sample_rate * 2).to_le_bytes());
        fmt.extend_from_slice(&2u16.to_le_bytes());
        fmt.extend_from_slice(&16u16.to_le_bytes());

        let mut body = b"WAVE".to_vec();
        body.extend_from_slice(b"fmt ");
        body.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
        body.extend_from_slice(&fmt);

        body.extend_from_slice(b"data");
        body.extend_from_slice(&data_len.to_le_bytes());
        for &s in samples {
            body.extend_from_slice(&s.to_le_bytes());
        }

        let mut out = b"RIFF".to_vec();
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&body);
        out
    }

    #[test]
    fn test_wav_decoding_and_resampling() {
        let wav = sample_wav_pcm16_mono(16000, &[0, 16384, -16384, 0]);
        let output = decode_and_resample_wav(&wav, 16000).unwrap();
        assert_eq!(output.sample_rate_hz, 16000);
        assert_eq!(output.samples.len(), 4);
        assert!((output.samples[1] - 0.5).abs() < 0.01);

        let resampled = decode_and_resample_wav(&wav, 8000).unwrap();
        assert_eq!(resampled.sample_rate_hz, 8000);
        assert_eq!(resampled.samples.len(), 2);
    }
}
