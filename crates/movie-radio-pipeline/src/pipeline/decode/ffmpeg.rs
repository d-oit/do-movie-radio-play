use anyhow::{bail, Context, Result};
use std::{path::Path, process::Command};

use movie_radio_types::TimelineError;

pub fn decode_via_ffmpeg(path: &Path, target_sample_rate: u32) -> Result<(Vec<f32>, u32)> {
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
