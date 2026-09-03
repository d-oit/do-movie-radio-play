use anyhow::{bail, Context, Result};
use std::{path::Path, process::Command};
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use tracing::info;

use movie_radio_types::TimelineError;

pub fn decode_audio(path: &Path, target_sample_rate: u32) -> Result<(Vec<f32>, u32)> {
    if !path.exists() {
        return Err(TimelineError::MissingInput(path.display().to_string()).into());
    }

    let ext = path.extension().and_then(|e| e.to_str());
    let ext_lower = ext.map(|s| s.to_lowercase());

    if let Some(ext_str) = ext_lower.as_deref() {
        if matches!(ext_str, "mp3" | "wav" | "flac" | "ogg") {
            if ext_str == "wav" {
                log_wav_spec(path);
            }
            match decode_via_symphonia(path, ext, target_sample_rate) {
                Ok((samples, sr)) => return Ok((samples, sr)),
                Err(err) => {
                    info!(input = %path.display(), error = %err, "symphonia decode failed, falling back to ffmpeg");
                }
            }
        }
    }

    decode_via_ffmpeg(path, target_sample_rate)
}

/// Log WAV header details (bits/sample, PCM vs float) to aid decode diagnostics.
/// Best-effort only: failures are silently ignored so logging never breaks decoding.
fn log_wav_spec(path: &Path) {
    if let Ok(reader) = hound::WavReader::open(path) {
        let spec = reader.spec();
        info!(
            input = %path.display(),
            channels = spec.channels,
            sample_rate = spec.sample_rate,
            bits_per_sample = spec.bits_per_sample,
            sample_format = ?spec.sample_format,
            "wav header spec",
        );
    }
}

pub fn decode_audio_chunked(
    path: &Path,
    target_sample_rate: u32,
    chunk_duration_sec: u64,
) -> Result<Vec<Vec<f32>>> {
    if !path.exists() {
        return Err(TimelineError::MissingInput(path.display().to_string()).into());
    }

    let chunk_ms = chunk_duration_sec * 1000;
    let mut all_chunks = Vec::new();
    let mut offset_ms: u64 = 0;

    loop {
        let output = Command::new("ffmpeg")
            .arg("-nostdin")
            .arg("-protocol_whitelist")
            .arg("file,pipe,fd")
            .args(["-hide_banner", "-loglevel", "error"])
            .arg("-ss")
            .arg(offset_ms.to_string())
            .arg("-t")
            .arg(chunk_ms.to_string())
            .arg("-i")
            .arg(path)
            .args([
                "-vn",
                "-ac",
                "1",
                "-ar",
                &target_sample_rate.to_string(),
                "-f",
                "s16le",
                "-",
            ])
            .output()
            .context("failed to execute ffmpeg")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            if stderr.contains("Stream map") || stderr.contains("could not find") {
                bail!(TimelineError::Decode(stderr));
            }
            break;
        }

        let bytes = output.stdout;
        if bytes.is_empty() {
            break;
        }

        let mut samples = Vec::with_capacity(bytes.len() / 2);
        for chunk in bytes.as_chunks::<2>().0 {
            let s = i16::from_le_bytes([chunk[0], chunk[1]]) as f32 / i16::MAX as f32;
            samples.push(s);
        }

        if samples.is_empty() {
            break;
        }

        info!(
            chunk = all_chunks.len(),
            offset_ms,
            samples = samples.len(),
            "decoded chunk"
        );

        all_chunks.push(samples);
        offset_ms += chunk_ms;

        if (bytes.len() as u64) < (chunk_ms * target_sample_rate as u64 / 1000 * 2) {
            break;
        }
    }

    Ok(all_chunks)
}

fn decode_via_symphonia(
    path: &Path,
    extension: Option<&str>,
    target_sample_rate: u32,
) -> Result<(Vec<f32>, u32)> {
    let file = std::fs::File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = extension {
        hint.with_extension(ext);
    }

    let mut format_reader = symphonia::default::get_probe().probe(
        &hint,
        mss,
        FormatOptions::default(),
        MetadataOptions::default(),
    )?;

    let track = format_reader
        .default_track(TrackType::Audio)
        .context("no audio track found")?;

    let codec_params = track
        .codec_params
        .as_ref()
        .context("no codec params")?
        .audio()
        .context("not audio")?;

    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(codec_params, &AudioDecoderOptions::default())?;

    let track_id = track.id;
    let source_sample_rate = codec_params.sample_rate.context("unknown sample rate")?;

    let mut samples = Vec::new();
    let mut decode_buf: Vec<f32> = Vec::new();

    loop {
        let packet = match format_reader.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => break,
            Err(e) => return Err(e.into()),
        };

        if packet.track_id != track_id {
            continue;
        }

        let decoded = decoder.decode(&packet)?;
        let frames = decoded.frames();
        let channels = decoded.spec().channels().count();

        decode_buf.resize(frames * channels, 0.0);
        decoded.copy_to_slice_interleaved(&mut decode_buf);

        for frame in decode_buf.chunks_exact(channels) {
            let mono_sample: f32 = frame.iter().sum::<f32>() / channels as f32;
            samples.push(mono_sample);
        }
    }

    if samples.is_empty() {
        return Err(TimelineError::EmptyAudio.into());
    }

    let resampled =
        crate::pipeline::resample::resample(&samples, source_sample_rate, target_sample_rate)?;
    Ok((resampled, target_sample_rate))
}

fn decode_via_ffmpeg(path: &Path, target_sample_rate: u32) -> Result<(Vec<f32>, u32)> {
    let output = Command::new("ffmpeg")
        .arg("-nostdin")
        .arg("-protocol_whitelist")
        .arg("file,pipe,fd")
        .args(["-hide_banner", "-loglevel", "error"])
        .arg("-i")
        .arg(path)
        .args([
            "-vn",
            "-ac",
            "1",
            "-ar",
            &target_sample_rate.to_string(),
            "-f",
            "s16le",
            "-",
        ])
        .output()
        .context("failed to execute ffmpeg")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        if stderr.contains("Stream map") || stderr.contains("could not find") {
            bail!(TimelineError::Decode(stderr));
        }
        return Err(TimelineError::Decode(stderr).into());
    }

    let bytes = output.stdout;
    if bytes.is_empty() {
        return Err(TimelineError::EmptyAudio.into());
    }
    let mut samples = Vec::with_capacity(bytes.len() / 2);
    for chunk in bytes.as_chunks::<2>().0 {
        let s = i16::from_le_bytes([chunk[0], chunk[1]]) as f32 / i16::MAX as f32;
        samples.push(s);
    }
    Ok((samples, target_sample_rate))
}

#[cfg(test)]
mod tests {
    use super::*;
    use hound::{WavSpec, WavWriter};

    #[test]
    fn test_decode_via_symphonia_wav() {
        let temp_dir = tempfile::tempdir().unwrap();
        let wav_path = temp_dir.path().join("test.wav");

        let spec = WavSpec {
            channels: 1,
            sample_rate: 16000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = WavWriter::create(&wav_path, spec).unwrap();
        for _ in 0..16000 {
            writer.write_sample(0i16).unwrap();
        }
        writer.finalize().unwrap();

        let (samples, _) = decode_via_symphonia(&wav_path, Some("wav"), 16000).unwrap();
        assert_eq!(samples.len(), 16000);
        for &s in &samples {
            assert_eq!(s, 0.0);
        }
    }

    #[test]
    fn test_decode_audio_dispatch() {
        let temp_dir = tempfile::tempdir().unwrap();
        let wav_path = temp_dir.path().join("test.wav");
        let spec = WavSpec {
            channels: 1,
            sample_rate: 16000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = WavWriter::create(&wav_path, spec).unwrap();
        for _ in 0..16000 {
            writer.write_sample(0i16).unwrap();
        }
        writer.finalize().unwrap();

        let (samples, sr) = decode_audio(&wav_path, 8000).unwrap();
        assert_eq!(sr, 8000);
        // rubato resampler may produce 7999 or 8000 due to async buffering
        assert!(
            samples.len() == 8000 || samples.len() == 7999,
            "expected ~8000 samples, got {}",
            samples.len()
        );
    }

    #[test]
    fn test_decode_via_symphonia_stereo() {
        let temp_dir = tempfile::tempdir().unwrap();
        let wav_path = temp_dir.path().join("stereo.wav");

        let spec = WavSpec {
            channels: 2,
            sample_rate: 16000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = WavWriter::create(&wav_path, spec).unwrap();
        for _ in 0..16000 {
            writer.write_sample(i16::MAX).unwrap();
            writer.write_sample(i16::MIN + 1).unwrap();
        }
        writer.finalize().unwrap();

        let (samples, _) = decode_via_symphonia(&wav_path, Some("wav"), 16000).unwrap();
        assert_eq!(samples.len(), 16000);
        for &s in &samples {
            assert!(s.abs() < 1e-4);
        }
    }

    #[test]
    fn test_decode_via_symphonia_wav_24bit() {
        let temp_dir = tempfile::tempdir().unwrap();
        let wav_path = temp_dir.path().join("test24.wav");

        let spec = WavSpec {
            channels: 1,
            sample_rate: 16000,
            bits_per_sample: 24,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = WavWriter::create(&wav_path, spec).unwrap();
        // Full-scale positive/negative 24-bit values plus silence.
        let full_scale = (1i32 << 23) - 1;
        writer.write_sample(full_scale).unwrap();
        writer.write_sample(-full_scale).unwrap();
        writer.write_sample(0i32).unwrap();
        writer.finalize().unwrap();

        let (samples, _) = decode_via_symphonia(&wav_path, Some("wav"), 16000).unwrap();
        assert_eq!(samples.len(), 3);
        assert!((samples[0] - 1.0).abs() < 1e-6, "got {}", samples[0]);
        assert!((samples[1] + 1.0).abs() < 1e-6, "got {}", samples[1]);
        assert_eq!(samples[2], 0.0);
    }

    #[test]
    fn test_decode_via_symphonia_wav_32bit_float() {
        let temp_dir = tempfile::tempdir().unwrap();
        let wav_path = temp_dir.path().join("test32f.wav");

        let spec = WavSpec {
            channels: 1,
            sample_rate: 16000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut writer = WavWriter::create(&wav_path, spec).unwrap();
        writer.write_sample(1.0f32).unwrap();
        writer.write_sample(-1.0f32).unwrap();
        writer.write_sample(0.25f32).unwrap();
        writer.finalize().unwrap();

        let (samples, _) = decode_via_symphonia(&wav_path, Some("wav"), 16000).unwrap();
        assert_eq!(samples.len(), 3);
        assert!((samples[0] - 1.0).abs() < 1e-6, "got {}", samples[0]);
        assert!((samples[1] + 1.0).abs() < 1e-6, "got {}", samples[1]);
        assert!((samples[2] - 0.25).abs() < 1e-6, "got {}", samples[2]);
    }

    #[test]
    fn test_decode_via_symphonia_stereo_24bit_mono_mix() {
        let temp_dir = tempfile::tempdir().unwrap();
        let wav_path = temp_dir.path().join("stereo24.wav");

        let spec = WavSpec {
            channels: 2,
            sample_rate: 16000,
            bits_per_sample: 24,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = WavWriter::create(&wav_path, spec).unwrap();
        let full_scale = (1i32 << 23) - 1;
        for _ in 0..100 {
            writer.write_sample(full_scale).unwrap();
            writer.write_sample(-full_scale).unwrap();
        }
        writer.finalize().unwrap();

        let (samples, _) = decode_via_symphonia(&wav_path, Some("wav"), 16000).unwrap();
        assert_eq!(samples.len(), 100);
        for &s in &samples {
            assert!(s.abs() < 1e-4, "got {s}");
        }
    }

    #[test]
    fn test_decode_audio_dispatch_ffmpeg() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mp4_path = temp_dir.path().join("test.mp4");
        std::fs::write(&mp4_path, b"dummy content").unwrap();

        let res = decode_audio(&mp4_path, 16000);
        assert!(res.is_err());
        let err = res.unwrap_err().to_string();
        assert!(
            err.contains("ffmpeg")
                || err.contains("No such file")
                || err.contains("decode failure"),
            "Error should be related to ffmpeg or decode failure, got: {}",
            err
        );
    }
}
