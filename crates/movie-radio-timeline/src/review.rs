use anyhow::{bail, Result};
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;

use movie_radio_types::{SegmentKind, TimelineOutput};

use crate::review_template::{escape_json_for_script, render_review_html};

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
    #[serde(default)]
    priority_review: bool,
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

    let (verification_map, maybe_thresholds): (
        HashMap<
            String,
            (
                String,
                movie_radio_verification::verification::SpectralFeatures,
            ),
        >,
        Option<movie_radio_verification::verification::AppliedThresholds>,
    ) = if let Some(verified_path) = verified {
        if verified_path.exists() {
            let content = std::fs::read_to_string(verified_path)?;
            let verified_data: movie_radio_verification::verification::VerificationReport =
                serde_json::from_str(&content)?;
            let thresholds = verified_data.summary.thresholds_applied.clone();
            let map: HashMap<
                String,
                (
                    String,
                    movie_radio_verification::verification::SpectralFeatures,
                ),
            > = verified_data
                .segment_results
                .into_iter()
                .map(|r| {
                    let key = format!("{}-{}", r.start_ms, r.end_ms);
                    let status_str = match r.verification_status {
                        movie_radio_verification::verification::VerificationStatus::Verified => {
                            "verified"
                        }
                        movie_radio_verification::verification::VerificationStatus::Suspicious => {
                            "suspicious"
                        }
                        movie_radio_verification::verification::VerificationStatus::Rejected => {
                            "rejected"
                        }
                        movie_radio_verification::verification::VerificationStatus::Inconclusive => {
                            "inconclusive"
                        }
                    };
                    (key, (status_str.to_string(), r.spectral_features))
                })
                .collect();
            (map, Some(thresholds))
        } else {
            (HashMap::new(), None)
        }
    } else {
        (HashMap::new(), None)
    };

    let selector = movie_radio_learning::active_learning::ActiveLearningSelector::default();

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
                priority_review: false,
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
                let (verification_status, maybe_features): (
                    Option<String>,
                    Option<movie_radio_verification::verification::SpectralFeatures>,
                ) = if let Some((status, features)) = verification_map.get(&key) {
                    (Some(status.clone()), Some(features.clone()))
                } else {
                    (None, None)
                };

                let priority_review = if let (Some(features), Some(ref thresholds)) =
                    (&maybe_features, &maybe_thresholds)
                {
                    selector.select_segment(
                        i + 1,
                        segment.confidence as f64,
                        features.rms as f64,
                        features.spectral_flatness as f64,
                        features.spectral_entropy as f64,
                        features.centroid_hz as f64,
                        thresholds.flatness_max as f64,
                        thresholds.entropy_min as f64,
                        thresholds.centroid_min as f64,
                        thresholds.centroid_max as f64,
                        thresholds.energy_min as f64,
                    )
                } else {
                    // Confidence-only and random sample checks
                    (segment.confidence >= selector.config.confidence_min as f32
                        && segment.confidence <= selector.config.confidence_max as f32)
                        || (((i + 1) * 2654435761) % 1000
                            < (selector.config.random_sample_rate * 1000.0) as usize)
                };

                ReviewSegment {
                    index: i + 1,
                    start_ms: segment.start_ms,
                    end_ms: segment.end_ms,
                    duration_ms: segment.end_ms.saturating_sub(segment.start_ms),
                    confidence: segment.confidence,
                    tags: segment.tags.clone(),
                    prompt: segment.prompt.clone(),
                    verification_status,
                    priority_review,
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
    use movie_radio_types::{Segment, SegmentKind, TimelineOutput};
    use tempfile::tempdir;

    #[test]
    fn test_xss_escaping_in_review_html() {
        let dir = tempdir().unwrap();
        let malicious_part = "xss_</script><script>alert(1)</script>.wav";
        let media_file = dir.path().join(malicious_part);

        if std::fs::write(&media_file, "dummy").is_err() {}

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

        assert!(html_new.contains("<meta http-equiv=\"Content-Security-Policy\""));

        let script_start = html.find("const mediaSrc =").unwrap();
        let script_end = html.find("allSegments = JSON.parse").unwrap();
        let script_vars = &html[script_start..script_end];
        assert!(!script_vars.contains('<'));
        assert!(!script_vars.contains('>'));
        assert!(!script_vars.contains('&'));
    }

    #[test]
    fn test_priority_review_generation() {
        let dir = tempdir().unwrap();
        let media_file = dir.path().join("test.wav");
        std::fs::write(&media_file, "dummy").unwrap();

        let timeline = TimelineOutput {
            file: "test.wav".to_string(),
            analysis_sample_rate: 16000,
            frame_ms: 20,
            segments: vec![
                Segment {
                    start_ms: 0,
                    end_ms: 1000,
                    kind: SegmentKind::NonVoice,
                    confidence: 0.5,
                    tags: vec![],
                    prompt: None,
                },
                Segment {
                    start_ms: 2000,
                    end_ms: 3000,
                    kind: SegmentKind::NonVoice,
                    confidence: 0.95,
                    tags: vec![],
                    prompt: None,
                },
            ],
        };

        let output_html = dir.path().join("review.html");
        write_review_html_with_options(&media_file, &timeline, &output_html, 1.0, 1.0, None, false)
            .unwrap();

        let html = std::fs::read_to_string(output_html).unwrap();
        println!("HTML OUTPUT:\n{}", html);
        assert!(html.contains("priority_review"));
        assert!(html.contains("Priority Review Candidates"));
    }
}
