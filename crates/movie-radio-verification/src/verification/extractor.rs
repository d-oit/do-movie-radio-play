use anyhow::{bail, Context, Result};
use movie_radio_types::Segment;
use std::path::Path;
use std::process::Command;
use tracing::info;

fn reject_traversal(path: &Path, field: &str) -> Result<()> {
    if path
        .components()
        .any(|c| c == std::path::Component::ParentDir)
    {
        bail!("{field} must not contain ..");
    }
    Ok(())
}

pub fn extract_segment_audio(
    media_path: &Path,
    segment: &Segment,
    output_path: &Path,
) -> Result<()> {
    reject_traversal(media_path, "media path")?;
    reject_traversal(output_path, "output path")?;
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
        .arg("-nostdin")
        .arg("-protocol_whitelist")
        .arg("file,pipe,fd")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-ss",
            &format!("{start_sec}"),
        ])
        .arg("-i")
        .arg(media_path)
        .args([
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
        ])
        .arg(output_path)
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
        kind: movie_radio_types::SegmentKind::NonVoice,
        confidence: 1.0,
        tags: vec![],
        prompt: None,
        sfx_trigger: None,
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
            kind: movie_radio_types::SegmentKind::NonVoice,
            confidence: 1.0,
            tags: vec![],
            prompt: None,
            sfx_trigger: None,
        };

        let temp_path = tempfile::NamedTempFile::new().unwrap().into_temp_path();
        let result = extract_segment_audio(Path::new("nonexistent.mp4"), &segment, &temp_path);
        assert!(result.is_err());
    }

    #[test]
    fn path_traversal_validation() {
        let segment = Segment {
            start_ms: 0,
            end_ms: 1000,
            kind: movie_radio_types::SegmentKind::NonVoice,
            confidence: 1.0,
            tags: vec![],
            prompt: None,
            sfx_trigger: None,
        };

        let temp_path = tempfile::NamedTempFile::new().unwrap().into_temp_path();

        let res1 = extract_segment_audio(Path::new("../secret.mp4"), &segment, &temp_path);
        assert!(res1.is_err());
        assert!(res1.unwrap_err().to_string().contains("media path must not contain .."));

        let res2 = extract_segment_audio(Path::new("movie.mp4"), &segment, Path::new("../out.wav"));
        assert!(res2.is_err());
        assert!(res2.unwrap_err().to_string().contains("output path must not contain .."));
    }
}
