use anyhow::{bail, Result};
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;

use crate::review_template::{escape_json_for_script, render_review_html};
use crate::types::{SegmentKind, TimelineOutput};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReviewSegment {
    index: usize,
    start_ms: u64,
    end_ms: u64,
    duration_ms: u64,
    confidence: f32,
    tags: Vec<String>,
    prompt: Option<String>,
    #[serde(default)]
    verification_status: Option<String>,
}

#[derive(Debug, Deserialize)]
struct VerifiedOutput {
    segment_results: Vec<SegmentResult>,
}

#[derive(Debug, Deserialize)]
struct SegmentResult {
    start_ms: u64,
    end_ms: u64,
    verification_status: String,
}

#[allow(dead_code)]
pub fn write_review_html(
    input_media: &Path,
    timeline: &TimelineOutput,
    output: &Path,
    pre_roll_s: f32,
    post_roll_s: f32,
    verified: Option<&Path>,
) -> Result<usize> {
    write_review_html_with_options(
        input_media,
        timeline,
        output,
        pre_roll_s,
        post_roll_s,
        verified,
        false,
    )
}

pub fn write_review_html_with_options(
    input_media: &Path,
    timeline: &TimelineOutput,
    output: &Path,
    pre_roll_s: f32,
    post_roll_s: f32,
    verified: Option<&Path>,
    merged: bool,
) -> Result<usize> {
    if !input_media.exists() {
        bail!("input media does not exist: {}", input_media.display());
    }

    let media_path = std::fs::canonicalize(input_media)
        .unwrap_or_else(|_| input_media.to_path_buf())
        .to_string_lossy()
        .to_string();
    let media_json = escape_json_for_script(serde_json::to_string(&media_path)?);
    let pre_roll_json = escape_json_for_script(serde_json::to_string(&pre_roll_s)?);
    let post_roll_json = escape_json_for_script(serde_json::to_string(&post_roll_s)?);

    let verification_map: HashMap<String, String> = if let Some(verified_path) = verified {
        if verified_path.exists() {
            let content = std::fs::read_to_string(verified_path)?;
            let verified_data: VerifiedOutput = serde_json::from_str(&content)?;
            verified_data
                .segment_results
                .into_iter()
                .map(|r| {
                    let key = format!("{}-{}", r.start_ms, r.end_ms);
                    (key, r.verification_status)
                })
                .collect()
        } else {
            HashMap::new()
        }
    } else {
        HashMap::new()
    };

    let segments: Vec<ReviewSegment> = if merged {
        let non_voice_segments: Vec<_> = timeline
            .segments
            .iter()
            .filter(|segment| segment.kind == SegmentKind::NonVoice)
            .collect();

        if non_voice_segments.is_empty() {
            vec![]
        } else {
            let first_start = non_voice_segments.first().map_or(0, |s| s.start_ms);
            let last_end = non_voice_segments.last().map_or(0, |s| s.end_ms);
            let duration_ms = last_end.saturating_sub(first_start);
            let avg_confidence: f32 = non_voice_segments.iter().map(|s| s.confidence).sum::<f32>()
                / non_voice_segments.len() as f32;
            let all_tags: Vec<String> = non_voice_segments
                .iter()
                .flat_map(|s| s.tags.clone())
                .collect();

            vec![ReviewSegment {
                index: 1,
                start_ms: first_start,
                end_ms: last_end,
                duration_ms,
                confidence: avg_confidence,
                tags: all_tags,
                prompt: None,
                verification_status: None,
            }]
        }
    } else {
        timeline
            .segments
            .iter()
            .filter(|segment| segment.kind == SegmentKind::NonVoice)
            .enumerate()
            .map(|(i, segment)| {
                let key = format!("{}-{}", segment.start_ms, segment.end_ms);
                let verification_status = verification_map.get(&key).cloned();
                ReviewSegment {
                    index: i + 1,
                    start_ms: segment.start_ms,
                    end_ms: segment.end_ms,
                    duration_ms: segment.end_ms.saturating_sub(segment.start_ms),
                    confidence: segment.confidence,
                    tags: segment.tags.clone(),
                    prompt: segment.prompt.clone(),
                    verification_status,
                }
            })
            .collect()
    };

    let segments_json = escape_json_for_script(serde_json::to_string(&segments)?);
    let merged_json = escape_json_for_script(serde_json::to_string(&merged)?);
    let html = render_review_html(
        &segments_json,
        &media_json,
        &pre_roll_json,
        &post_roll_json,
        &merged_json,
    );

    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output, html)?;
    Ok(segments.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Segment, SegmentKind, TimelineOutput};
    use tempfile::tempdir;

    #[test]
    fn test_xss_escaping_in_review_html() {
        let dir = tempdir().unwrap();
        // Use a path that is more likely to be accepted by the filesystem
        let malicious_part = "xss_</script><script>alert(1)</script>.wav";
        let media_file = dir.path().join(malicious_part);

        // Try to create the file, but don't fail if the OS rejects the name
        // (the test can still pass by checking how write_review_html_with_options handles the string)
        if std::fs::write(&media_file, "dummy").is_err() {
            // Fallback to a simpler name if the OS rejects the complex one,
            // and manually craft the input to write_review_html_with_options if needed
            // but let's see if this works first.
        }

        let timeline = TimelineOutput {
            file: "test.wav".to_string(),
            analysis_sample_rate: 16000,
            frame_ms: 20,
            segments: vec![Segment {
                start_ms: 0,
                end_ms: 1000,
                kind: SegmentKind::NonVoice,
                confidence: 1.0,
                tags: vec!["<script>alert(2)</script>".to_string()],
                prompt: Some("</SCRIPT><script>alert(3)</script>".to_string()),
            }],
        };

        let output_html = dir.path().join("review.html");

        // Ensure we have a file that exists for the call to write_review_html_with_options
        let safe_input = if media_file.exists() {
            media_file.clone()
        } else {
            let f = dir.path().join("safe.wav");
            std::fs::write(&f, "dummy").unwrap();
            f
        };

        write_review_html_with_options(&safe_input, &timeline, &output_html, 1.0, 1.0, None, false)
            .unwrap();

        let html = std::fs::read_to_string(output_html).unwrap();
        // Check that the malicious strings are escaped
        assert!(
            html.contains(r"\u003cscript\u003ealert(2)\u003c/script\u003e")
                || html.contains(r"\u003cscript\u003ealert(2)\u003c\/script\u003e")
        );
        assert!(
            html.contains(r"\u003c/SCRIPT\u003e\u003cscript\u003ealert(3)\u003c/script\u003e")
                || html.contains(
                    r"\u003c\/SCRIPT\u003e\u003cscript\u003ealert(3)\u003c\/script\u003e"
                )
        );

        // Check for ampersand escaping
        let timeline_with_amp = TimelineOutput {
            file: "test.wav".to_string(),
            analysis_sample_rate: 16000,
            frame_ms: 20,
            segments: vec![Segment {
                start_ms: 0,
                end_ms: 1000,
                kind: SegmentKind::NonVoice,
                confidence: 1.0,
                tags: vec!["rock & roll".to_string()],
                prompt: None,
            }],
        };
        let output_amp = dir.path().join("review_amp.html");
        write_review_html_with_options(
            &safe_input,
            &timeline_with_amp,
            &output_amp,
            1.0,
            1.0,
            None,
            false,
        )
        .unwrap();
        let html_amp = std::fs::read_to_string(output_amp).unwrap();
        assert!(html_amp.contains(r"rock \u0026 roll"));

        // Check for new escape characters
        let timeline_new = TimelineOutput {
            file: "test.wav".to_string(),
            analysis_sample_rate: 16000,
            frame_ms: 20,
            segments: vec![Segment {
                start_ms: 0,
                end_ms: 1000,
                kind: SegmentKind::NonVoice,
                confidence: 1.0,
                tags: vec!["line\u{2028}para\u{2029}back`tick".to_string()],
                prompt: None,
            }],
        };
        let output_new = dir.path().join("review_new.html");
        write_review_html_with_options(
            &safe_input,
            &timeline_new,
            &output_new,
            1.0,
            1.0,
            None,
            false,
        )
        .unwrap();
        let html_new = std::fs::read_to_string(output_new).unwrap();
        assert!(html_new.contains(r"line\u2028para\u2029back\u0060tick"));

        // Check for CSP meta tag
        assert!(html_new.contains("<meta http-equiv=\"Content-Security-Policy\""));

        // Check that it does NOT contain unescaped '<', '>', or '&' in the script variables
        // Finding where the script variables start
        let script_start = html.find("const mediaSrc =").unwrap();
        let script_end = html.find("allSegments = JSON.parse").unwrap();
        let script_vars = &html[script_start..script_end];
        assert!(!script_vars.contains('<'));
        assert!(!script_vars.contains('>'));
        assert!(!script_vars.contains('&'));
    }
}
