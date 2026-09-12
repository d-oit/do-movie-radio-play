use anyhow::{bail, Context, Result};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};

use movie_radio_types::TimelineError;

pub fn stream_ffmpeg_chunks<F>(
    path: &Path,
    target_sample_rate: u32,
    chunk_duration_sec: u64,
    mut callback: F,
) -> Result<()>
where
    F: FnMut(&[f32], usize) -> Result<()>,
{
    let target_samples_per_chunk = (chunk_duration_sec * target_sample_rate as u64) as usize;
    if target_samples_per_chunk == 0 {
        bail!(TimelineError::Decode(
            "Chunk duration must be > 0".to_string()
        ));
    }

    let mut child = Command::new("ffmpeg")
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
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to spawn ffmpeg for streaming")?;

    let mut stdout = child
        .stdout
        .take()
        .context("failed to open ffmpeg stdout")?;

    let bytes_per_sample = 2;
    let target_bytes = target_samples_per_chunk * bytes_per_sample;
    let mut byte_buf = vec![0u8; target_bytes];
    let mut chunk_idx = 0;
    let mut total_samples_emitted = 0usize;

    loop {
        let mut read_bytes = 0;
        while read_bytes < target_bytes {
            match stdout.read(&mut byte_buf[read_bytes..]) {
                Ok(0) => break, // EOF
                Ok(n) => read_bytes += n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e.into()),
            }
        }

        if read_bytes == 0 {
            break;
        }

        let num_samples = read_bytes / 2;
        let mut samples = Vec::with_capacity(num_samples);
        for chunk in byte_buf[..read_bytes].as_chunks::<2>().0 {
            let s = i16::from_le_bytes([chunk[0], chunk[1]]) as f32 / i16::MAX as f32;
            samples.push(s);
        }

        if !samples.is_empty() {
            total_samples_emitted += samples.len();
            callback(&samples, chunk_idx)?;
            chunk_idx += 1;
        }

        if read_bytes < target_bytes {
            break; // EOF reached
        }
    }

    let status = child.wait().context("failed to wait on ffmpeg process")?;
    if !status.success() && total_samples_emitted == 0 {
        let mut stderr = String::new();
        if let Some(mut err_pipe) = child.stderr.take() {
            let _ = err_pipe.read_to_string(&mut stderr);
        }
        if stderr.contains("Stream map") || stderr.contains("could not find") {
            bail!(TimelineError::Decode(stderr));
        }
        return Err(TimelineError::Decode(stderr).into());
    }

    Ok(())
}
