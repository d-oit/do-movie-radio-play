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

/// Decodes in chunks, preferring incremental symphonia decoding for the
/// formats it handles so the whole decoded track never has to be resident.
///
/// Returns `true` when the stream was delivered incrementally. When it returns
/// `false` any chunks already passed to `callback` must be discarded: the file
/// could not be streamed and was decoded in full instead.
// `callback` is only mutated by the incremental branch, which the
// `high-quality-resample` build does not compile.
#[allow(unused_mut)]
pub fn decode_audio_chunks_prefer_streaming<F>(
    path: &Path,
    target_sample_rate: u32,
    chunk_duration_sec: u64,
    mut callback: F,
) -> Result<bool>
where
    F: FnMut(&[f32], usize) -> Result<()>,
{
    if !path.exists() {
        return Err(TimelineError::MissingInput(path.display().to_string()).into());
    }

    #[cfg(not(feature = "high-quality-resample"))]
    {
        let ext = path.extension().and_then(|e| e.to_str());
        let ext_lower = ext.map(|s| s.to_lowercase());
        if let Some(ext_str) = ext_lower.as_deref() {
            if matches!(ext_str, "mp3" | "wav" | "flac" | "ogg") {
                match symphonia::stream_via_symphonia_chunks(
                    path,
                    ext,
                    target_sample_rate,
                    chunk_duration_sec,
                    &mut callback,
                ) {
                    Ok(()) => return Ok(true),
                    Err(err) => info!(
                        input = %path.display(),
                        error = %err,
                        "incremental symphonia decode failed, falling back to full decode"
                    ),
                }
            }
        }
    }

    decode_audio_chunks_cb(path, target_sample_rate, chunk_duration_sec, callback)?;
    Ok(false)
}

/// Seconds of audio decoded per step when spooling to a file.
const SPOOL_CHUNK_SEC: u64 = 60;

/// Decodes `path` to a mono little-endian `f32` file at `target_sample_rate`
/// without holding the decoded track in memory. Returns the sample count.
pub fn decode_audio_to_raw_f32(path: &Path, target_sample_rate: u32, out: &Path) -> Result<usize> {
    use std::io::{Seek, SeekFrom, Write};

    let mut file = std::fs::File::create(out)?;
    let mut samples = 0usize;
    let mut bytes = Vec::new();
    // Chunk 0 restarts the output, which discards what a failed incremental
    // attempt delivered before the full-decode retry.
    let mut spool = |chunk: &[f32], idx: usize| -> Result<()> {
        if idx == 0 {
            file.set_len(0)?;
            file.seek(SeekFrom::Start(0))?;
            samples = 0;
        }
        bytes.clear();
        bytes.reserve(chunk.len() * 4);
        for sample in chunk {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        file.write_all(&bytes)?;
        samples += chunk.len();
        Ok(())
    };
    let streamed = decode_audio_chunks_prefer_streaming(
        path,
        target_sample_rate,
        SPOOL_CHUNK_SEC,
        &mut spool,
    )?;
    if !streamed {
        decode_audio_chunks_cb(path, target_sample_rate, SPOOL_CHUNK_SEC, &mut spool)?;
    }
    file.flush()?;
    Ok(samples)
}

/// Reads a mono little-endian `f32` file whole.
pub fn read_raw_f32(path: &Path) -> Result<Vec<f32>> {
    let bytes = std::fs::read(path)?;
    Ok(bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b))
        .collect())
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
