use anyhow::{bail, Context, Result};
use std::{path::Path, process::Command};
use tracing::info;

use crate::error::TimelineError;
use crate::io::wav::read_wav_to_f32;

pub fn decode_audio(path: &Path) -> Result<(Vec<f32>, u32)> {
    if !path.exists() {
        return Err(TimelineError::MissingInput(path.display().to_string()).into());
    }
    if path.extension().and_then(|e| e.to_str()) == Some("wav") {
        match read_wav_to_f32(path) {
            Ok((samples, sr)) => {
                if samples.is_empty() {
                    return Err(TimelineError::EmptyAudio.into());
                }
                return Ok((samples, sr));
            }
            Err(err) => {
                info!(input = %path.display(), error = %err, "direct wav decode failed, falling back to ffmpeg");
            }
        }
    }

    decode_with_ffmpeg(path)
}

fn decode_with_ffmpeg(path: &Path) -> Result<(Vec<f32>, u32)> {
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
            "16000",
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
    Ok((samples, 16000))
}
