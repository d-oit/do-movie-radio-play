use anyhow::{bail, Context, Result};
use std::io::Cursor;

pub struct SfxProcessor;

impl SfxProcessor {
    pub fn decode_bytes(bytes: &[u8], target_sample_rate: u32) -> Result<Vec<f32>> {
        if bytes.is_empty() {
            bail!("empty audio bytes");
        }
        if let Ok(samples) = Self::try_hound(bytes, target_sample_rate) {
            return Ok(samples);
        }
        Self::try_symphonia(bytes, target_sample_rate)
    }

    fn try_hound(bytes: &[u8], target_sample_rate: u32) -> Result<Vec<f32>> {
        let cursor = Cursor::new(bytes);
        let mut reader = hound::WavReader::new(cursor).context("hound probe")?;
        let spec = reader.spec();
        if spec.channels == 0 || spec.sample_rate == 0 {
            bail!("invalid wav spec");
        }
        let samples: Vec<f32> = match spec.sample_format {
            hound::SampleFormat::Int => reader
                .samples::<i32>()
                .map(|s| {
                    let v = s.context("sample")?;
                    let max = (1i64 << (spec.bits_per_sample - 1)) as f32;
                    Ok(v as f32 / max)
                })
                .collect::<Result<Vec<f32>>>()?,
            hound::SampleFormat::Float => reader
                .samples::<f32>()
                .map(|s| s.context("sample"))
                .collect::<Result<Vec<f32>>>()?,
        };
        let mono = if spec.channels == 1 {
            samples
        } else {
            samples
                .chunks(spec.channels as usize)
                .map(|chunk| chunk.iter().sum::<f32>() / chunk.len() as f32)
                .collect()
        };
        if spec.sample_rate == target_sample_rate {
            Ok(mono)
        } else {
            Ok(Self::resample_linear(
                &mono,
                spec.sample_rate,
                target_sample_rate,
            ))
        }
    }

    fn try_symphonia(bytes: &[u8], target_sample_rate: u32) -> Result<Vec<f32>> {
        use symphonia::core::codecs::audio::AudioDecoderOptions;
        use symphonia::core::formats::probe::Hint;
        use symphonia::core::formats::{FormatOptions, TrackType};
        use symphonia::core::io::MediaSourceStream;
        use symphonia::core::meta::MetadataOptions;

        let mss = MediaSourceStream::new(Box::new(Cursor::new(bytes.to_vec())), Default::default());
        let hint = Hint::new();
        let mut probed = symphonia::default::get_probe()
            .probe(
                &hint,
                mss,
                FormatOptions::default(),
                MetadataOptions::default(),
            )
            .context("symphonia probe")?;
        let track = probed
            .default_track(TrackType::Audio)
            .context("no audio track")?
            .clone();
        let codec_params = track
            .codec_params
            .as_ref()
            .context("no codec params")?
            .audio()
            .context("not audio")?;
        let mut decoder = symphonia::default::get_codecs()
            .make_audio_decoder(codec_params, &AudioDecoderOptions::default())
            .context("make decoder")?;
        let track_id = track.id;
        let decoded_rate = codec_params.sample_rate.unwrap_or(target_sample_rate);
        let mut all_samples = Vec::new();
        let mut decode_buf: Vec<f32> = Vec::new();
        loop {
            let packet = match probed.next_packet() {
                Ok(Some(packet)) => packet,
                Ok(None) => break,
                Err(e) => bail!("next packet: {e}"),
            };
            if packet.track_id != track_id {
                continue;
            }
            let decoded = decoder.decode(&packet).context("decode")?;
            let frames = decoded.frames();
            let chans = decoded.spec().channels().count();
            decode_buf.resize(frames * chans, 0.0);
            decoded.copy_to_slice_interleaved(&mut decode_buf);
            for chunk in decode_buf.chunks_exact(chans) {
                let mono = chunk.iter().sum::<f32>() / chunk.len() as f32;
                all_samples.push(mono);
            }
        }
        if decoded_rate == target_sample_rate {
            Ok(all_samples)
        } else {
            Ok(Self::resample_linear(
                &all_samples,
                decoded_rate,
                target_sample_rate,
            ))
        }
    }

    fn resample_linear(input: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
        if from_rate == to_rate || input.is_empty() {
            return input.to_vec();
        }
        let ratio = to_rate as f64 / from_rate as f64;
        let out_len = ((input.len() as f64) * ratio).round() as usize;
        let mut out = Vec::with_capacity(out_len);
        for i in 0..out_len {
            let src_pos = i as f64 / ratio;
            let lo = src_pos.floor() as usize;
            let hi = (lo + 1).min(input.len() - 1);
            let frac = (src_pos - lo as f64) as f32;
            let s = input[lo] * (1.0 - frac) + input[hi] * frac;
            out.push(s);
        }
        out
    }

    pub fn normalize_peak(samples: &mut [f32], max_peak: f32) {
        if samples.is_empty() {
            return;
        }
        let peak = samples.iter().map(|s| s.abs()).fold(0.0_f32, f32::max);
        if peak > max_peak && peak > 0.0 {
            let scale = max_peak / peak;
            for s in samples.iter_mut() {
                *s *= scale;
            }
        }
    }

    pub fn apply_fade(samples: &mut [f32], sample_rate: u32, fade_ms: u32) {
        if samples.is_empty() || fade_ms == 0 {
            return;
        }
        let fade_len = (sample_rate as u64 * fade_ms as u64 / 1000) as usize;
        let fade_len = fade_len.min(samples.len() / 2);
        if fade_len == 0 {
            return;
        }
        for i in 0..fade_len {
            let gain = i as f32 / fade_len as f32;
            samples[i] *= gain;
            let j = samples.len() - 1 - i;
            samples[j] *= gain;
        }
    }

    pub fn trim_or_pad(samples: Vec<f32>, target_len: usize) -> Vec<f32> {
        if samples.len() == target_len {
            samples
        } else if samples.len() > target_len {
            samples.into_iter().take(target_len).collect()
        } else {
            let mut out = samples;
            out.resize(target_len, 0.0);
            out
        }
    }

    pub fn process(
        bytes: &[u8],
        target_sample_rate: u32,
        target_duration_secs: Option<f32>,
        max_peak: f32,
        fade_ms: u32,
    ) -> Result<Vec<f32>> {
        let mut samples = Self::decode_bytes(bytes, target_sample_rate)?;
        Self::normalize_peak(&mut samples, max_peak);
        Self::apply_fade(&mut samples, target_sample_rate, fade_ms);
        if let Some(dur) = target_duration_secs {
            let target_len = (target_sample_rate as f32 * dur).round() as usize;
            samples = Self::trim_or_pad(samples, target_len);
        }
        Ok(samples)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav_with_samples(vals: &[i16]) -> Vec<u8> {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut buf = Vec::new();
        let mut cursor = std::io::Cursor::new(&mut buf);
        let mut w = hound::WavWriter::new(&mut cursor, spec).expect("writer");
        for &v in vals {
            w.write_sample(v).expect("write");
        }
        w.finalize().expect("finalize");
        buf
    }

    #[test]
    fn test_normalize_peak() {
        let mut s = vec![0.5, -2.0, 1.0];
        SfxProcessor::normalize_peak(&mut s, 1.0);
        let peak = s.iter().map(|x| x.abs()).fold(0.0_f32, f32::max);
        assert!((peak - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_fade() {
        let mut s = vec![1.0; 100];
        SfxProcessor::apply_fade(&mut s, 1000, 10);
        assert!(s[0] < 0.2);
        assert!(s[99] < 0.2);
        assert!(s[50] > 0.9);
    }

    #[test]
    fn test_trim_or_pad() {
        assert_eq!(
            SfxProcessor::trim_or_pad(vec![1.0, 2.0, 3.0], 2),
            vec![1.0, 2.0]
        );
        assert_eq!(SfxProcessor::trim_or_pad(vec![1.0], 3), vec![1.0, 0.0, 0.0]);
    }

    #[test]
    fn test_decode_and_process() -> Result<()> {
        let bytes = wav_with_samples(&[1000, -1000, 2000, -2000]);
        let out = SfxProcessor::process(&bytes, 16000, Some(0.001), 0.9, 5)?;
        assert!(!out.is_empty());
        Ok(())
    }

    #[test]
    fn test_malformed_returns_error() {
        let bad = vec![0u8, 1, 2, 3];
        assert!(SfxProcessor::decode_bytes(&bad, 16000).is_err());
    }

    #[test]
    fn test_resample() {
        let input: Vec<f32> = (0..100).map(|i| i as f32 / 100.0).collect();
        let out = SfxProcessor::resample_linear(&input, 16000, 8000);
        assert!(out.len() < input.len());
        assert!(out.len() == 50);
    }
}
