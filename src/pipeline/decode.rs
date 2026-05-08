use anyhow::{bail, Context, Result};
use std::{path::Path, process::Command};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::errors::Error;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use tracing::info;

use crate::error::TimelineError;

pub fn decode_audio(path: &Path, target_sample_rate: u32) -> Result<(Vec<f32>, u32)> {
    if !path.exists() {
        return Err(TimelineError::MissingInput(path.display().to_string()).into());
    }

    let ext = path.extension().and_then(|e| e.to_str());
    let ext_lower = ext.map(|s| s.to_lowercase());

    if let Some(ext_str) = ext_lower.as_deref() {
        if matches!(ext_str, "mp3" | "wav" | "flac" | "ogg") {
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

    let probed = symphonia::default::get_probe().format(
        &hint,
        mss,
        &FormatOptions::default(),
        &MetadataOptions::default(),
    )?;

    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != symphonia::core::codecs::CODEC_TYPE_NULL)
        .context("no audio track found")?;

    let mut decoder =
        symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default())?;

    let track_id = track.id;
    let source_sample_rate = track
        .codec_params
        .sample_rate
        .context("unknown sample rate")?;

    let mut samples = Vec::new();
    let mut sample_buf = None;

    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(Error::IoError(ref e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e.into()),
        };

        if packet.track_id() != track_id {
            continue;
        }

        let decoded = decoder.decode(&packet)?;

        if sample_buf.is_none() {
            let spec = *decoded.spec();
            let duration = decoded.capacity() as u64;
            sample_buf = Some(SampleBuffer::<f32>::new(duration, spec));
        }

        let frames = decoded.frames();
        let channels = decoded.spec().channels.count();

        let buf = sample_buf.as_mut().unwrap();
        buf.copy_interleaved_ref(decoded);

        for frame in buf.samples()[..frames * channels].chunks_exact(channels) {
            let mono_sample: f32 = frame.iter().sum::<f32>() / channels as f32;
            samples.push(mono_sample);
        }
    }

    if samples.is_empty() {
        return Err(TimelineError::EmptyAudio.into());
    }

    let resampled = crate::pipeline::resample::resample_linear(
        &samples,
        source_sample_rate,
        target_sample_rate,
    );
    Ok((resampled, target_sample_rate))
}

fn decode_via_ffmpeg(path: &Path, target_sample_rate: u32) -> Result<(Vec<f32>, u32)> {
    let output = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-i",
            &path.display().to_string(),
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
    for chunk in bytes.chunks_exact(2) {
        let s = i16::from_le_bytes([chunk[0], chunk[1]]) as f32 / i16::MAX as f32;
        samples.push(s);
    }
    // ffmpeg can return the actual source sample rate if we probed it, but here we assume it worked
    // as requested. To get the TRUE source rate we'd need a separate ffprobe call or parse stderr.
    // For now we'll return target_sample_rate as a best-effort if we can't easily get the original.
    // However, the pipeline might want to know if it was 44100 or 48000.
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

        // Create a 1 second silence WAV file
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
        assert_eq!(sr, 8000); // Now always returns target rate
        assert_eq!(samples.len(), 8000);
    }

    #[test]
    fn test_decode_via_symphonia_stereo() {
        let temp_dir = tempfile::tempdir().unwrap();
        let wav_path = temp_dir.path().join("stereo.wav");

        // Create a 1 second stereo WAV file with 1.0 in left and -1.0 in right
        // Downmix should be (1.0 + -1.0) / 2 = 0.0
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
    fn test_decode_audio_dispatch_ffmpeg() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mp4_path = temp_dir.path().join("test.mp4");
        std::fs::write(&mp4_path, b"dummy content").unwrap();

        // Should NOT call symphonia because of .mp4 extension
        // Should attempt ffmpeg and fail because it's not a real mp4
        let res = decode_audio(&mp4_path, 16000);
        assert!(res.is_err());
        let err = res.unwrap_err().to_string();
        // Should fail because it's not a real mp4.
        // We expect either an execution error or a decode error from ffmpeg.
        assert!(
            err.contains("ffmpeg")
                || err.contains("No such file")
                || err.contains("decode failure"),
            "Error should be related to ffmpeg or decode failure, got: {}",
            err
        );
    }
}
