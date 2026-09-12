pub mod ffmpeg;
pub mod streaming;
pub mod symphonia;
#[cfg(test)]
mod tests;

use anyhow::{bail, Result};
use std::path::Path;
use tracing::info;

use movie_radio_types::TimelineError;

pub fn decode_audio(path: &Path, target_sample_rate: u32) -> Result<(Vec<f32>, u32)> {
    decode_audio_with_fallback(path, target_sample_rate, true)
}

pub fn decode_audio_with_fallback(
    path: &Path,
    target_sample_rate: u32,
    allow_ffmpeg_fallback: bool,
) -> Result<(Vec<f32>, u32)> {
    if !path.exists() {
        return Err(TimelineError::MissingInput(path.display().to_string()).into());
    }

    let ext = path.extension().and_then(|e| e.to_str());
    let ext_lower = ext.map(|s| s.to_lowercase());

    if let Some(ext_str) = ext_lower.as_deref() {
        if matches!(ext_str, "mp3" | "wav" | "flac" | "ogg") {
            match symphonia::decode_via_symphonia(path, ext, target_sample_rate) {
                Ok((samples, sr)) => return Ok((samples, sr)),
                Err(err) => {
                    if !allow_ffmpeg_fallback {
                        return Err(err);
                    }
                    info!(input = %path.display(), error = %err, "symphonia decode failed, falling back to ffmpeg");
                }
            }
        }
    }

    if !allow_ffmpeg_fallback {
        bail!(TimelineError::Decode(
            "symphonia decode failed and ffmpeg fallback is disabled".to_string()
        ));
    }
    ffmpeg::decode_via_ffmpeg(path, target_sample_rate)
}

pub fn decode_audio_chunks_cb<F>(
    path: &Path,
    target_sample_rate: u32,
    chunk_duration_sec: u64,
    mut callback: F,
) -> Result<()>
where
    F: FnMut(&[f32], usize) -> Result<()>,
{
    if !path.exists() {
        return Err(TimelineError::MissingInput(path.display().to_string()).into());
    }

    let ext = path.extension().and_then(|e| e.to_str());
    let ext_lower = ext.map(|s| s.to_lowercase());

    if let Some(ext_str) = ext_lower.as_deref() {
        if matches!(ext_str, "mp3" | "wav" | "flac" | "ogg") {
            if let Ok((samples, _sr)) =
                symphonia::decode_via_symphonia(path, ext, target_sample_rate)
            {
                let target_samples_per_chunk =
                    (chunk_duration_sec * target_sample_rate as u64) as usize;
                if target_samples_per_chunk == 0 {
                    bail!(TimelineError::Decode(
                        "Chunk duration must be > 0".to_string()
                    ));
                }
                for (chunk_idx, chunk) in samples.chunks(target_samples_per_chunk).enumerate() {
                    callback(chunk, chunk_idx)?;
                }
                return Ok(());
            }
        }
    }

    streaming::stream_ffmpeg_chunks(path, target_sample_rate, chunk_duration_sec, callback)
}

pub fn decode_audio_chunked(
    path: &Path,
    target_sample_rate: u32,
    chunk_duration_sec: u64,
) -> Result<Vec<Vec<f32>>> {
    let mut all_chunks = Vec::new();
    decode_audio_chunks_cb(
        path,
        target_sample_rate,
        chunk_duration_sec,
        |samples, _| {
            all_chunks.push(samples.to_vec());
            Ok(())
        },
    )?;
    Ok(all_chunks)
}
