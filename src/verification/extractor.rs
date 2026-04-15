use crate::types::Segment;
use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::Command;
use tracing::info;

pub fn extract_segment_audio(
    media_path: &Path,
    segment: &Segment,
    output_path: &Path,
) -> Result<()> {
    let start_sec = segment.start_ms as f64 / 1000.0;
    let duration_ms = segment.end_ms.saturating_sub(segment.start_ms);
    let duration_sec = duration_ms as f64 / 1000.0;

    let duration_str = if duration_sec > 0.0 {
        format!("{duration_sec}")
    } else {
        bail!("segment duration must be positive");
    };

    info!(
        media = %media_path.display(),
        start = start_sec,
        duration = duration_str,
        output = %output_path.display(),
        "extracting audio segment"
    );

    let output = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-ss",
            &format!("{start_sec}"),
            "-i",
            &media_path.display().to_string(),
            "-t",
            &duration_str,
            "-vn",
            "-ac",
            "1",
            "-ar",
            "16000",
            "-acodec",
            "pcm_s16le",
            "-y",
            &output_path.display().to_string(),
        ])
        .output()
        .context("failed to execute ffmpeg for segment extraction")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        bail!("ffmpeg extraction failed: {stderr}");
    }

    if !output_path.exists() {
        bail!("ffmpeg did not produce output file");
    }

    Ok(())
}

#[allow(dead_code)]
pub fn extract_multiple_segments(
    media_path: &Path,
    segments: &[Segment],
    output_dir: &Path,
) -> Result<Vec<(Segment, std::path::PathBuf)>> {
    std::fs::create_dir_all(output_dir)?;

    let mut results = Vec::new();
    for (i, segment) in segments.iter().enumerate() {
        let output_path = output_dir.join(format!("segment_{i:04}.wav"));

        extract_segment_audio(media_path, segment, &output_path)?;
        results.push((segment.clone(), output_path));
    }

    Ok(results)
}

#[allow(dead_code)]
pub fn extract_audio_chunk(
    media_path: &Path,
    start_ms: u64,
    end_ms: u64,
    output_path: &Path,
) -> Result<()> {
    let segment = Segment {
        start_ms,
        end_ms,
        kind: crate::types::SegmentKind::NonVoice,
        confidence: 1.0,
        tags: vec![],
        prompt: None,
    };
    extract_segment_audio(media_path, &segment, output_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segment_duration_validation() {
        let segment = Segment {
            start_ms: 1000,
            end_ms: 500,
            kind: crate::types::SegmentKind::NonVoice,
            confidence: 1.0,
            tags: vec![],
            prompt: None,
        };

        let temp_path = tempfile::NamedTempFile::new().unwrap().into_temp_path();
        let result = extract_segment_audio(Path::new("nonexistent.mp4"), &segment, &temp_path);
        assert!(result.is_err());
    }
}
