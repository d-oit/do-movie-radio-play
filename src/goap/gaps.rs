use crate::types::{GapAnalysisOutput, SegmentKind, TimelineOutput, VisualGap};
use crate::validation::srt;
use anyhow::Result;

pub struct GapIdentifier {
    pub min_silence_duration_ms: u64,
    pub high_confidence_threshold: f32,
}

impl Default for GapIdentifier {
    fn default() -> Self {
        Self {
            min_silence_duration_ms: 3000,
            high_confidence_threshold: 0.8,
        }
    }
}

impl GapIdentifier {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn identify_gaps(
        &self,
        timeline: &TimelineOutput,
        subtitles_srt: Option<&str>,
    ) -> Result<GapAnalysisOutput> {
        let mut gaps = Vec::new();

        let srt_segments = if let Some(srt_content) = subtitles_srt {
            Some(srt::parse_srt_segments(srt_content)?)
        } else {
            None
        };

        for (i, seg) in timeline.segments.iter().enumerate() {
            if seg.kind != SegmentKind::NonVoice {
                continue;
            }

            let duration = seg.end_ms - seg.start_ms;
            let mut confidence = 0.0;
            let mut reasons = Vec::new();

            // 1. Duration Signal
            if duration > self.min_silence_duration_ms {
                confidence += 0.4;
                reasons.push(format!("Duration ({}ms) > 3s", duration));
            } else if duration > 1000 {
                confidence += 0.1;
            }

            // 2. Tag-based Context
            if seg.tags.contains(&"ambience".to_string()) && duration > 2000 {
                confidence += 0.2;
                reasons.push("Extended ambience".to_string());
            }

            if seg.tags.contains(&"impact_heavy".to_string())
                || seg.tags.contains(&"machinery_like".to_string())
            {
                confidence += 0.3;
                reasons.push("Ambiguous SFX needing description".to_string());
            }

            if seg.tags.contains(&"music_bed".to_string()) && duration > 5000 {
                // Music interludes often don't need narration unless something visual happens.
                // But long ones might. For now, slight boost.
                confidence += 0.1;
            }

            // 3. Proximity to dialogue (Context Analyzer)
            let has_speech_before = i > 0 && timeline.segments[i - 1].kind == SegmentKind::Speech;
            let has_speech_after = i < timeline.segments.len() - 1
                && timeline.segments[i + 1].kind == SegmentKind::Speech;

            if has_speech_before && has_speech_after {
                confidence += 0.2;
                reasons.push("Gap between dialogue blocks".to_string());
            }

            // 4. Audio environment change (Proxy for scene transition)
            if i > 0 && i < timeline.segments.len() - 1 {
                let prev = &timeline.segments[i - 1];
                let next = &timeline.segments[i + 1];

                let prev_tags: std::collections::HashSet<_> = prev.tags.iter().collect();
                let next_tags: std::collections::HashSet<_> = next.tags.iter().collect();

                let intersection_count = prev_tags.intersection(&next_tags).count();
                if intersection_count == 0 && !prev.tags.is_empty() && !next.tags.is_empty() {
                    confidence += 0.3;
                    reasons.push("Audio environment change detected".to_string());
                }
            }

            // 5. Subtitle Signal
            if let Some(subs) = &srt_segments {
                // If there's a large gap between subtitles that overlaps with this non-voice segment
                // it reinforces that this is a scene without dialogue.
                let mut sub_gap_found = false;
                for j in 0..subs.len().saturating_sub(1) {
                    let sub_end = subs[j].end_ms;
                    let next_sub_start = subs[j + 1].start_ms;

                    if sub_end <= seg.start_ms && next_sub_start >= seg.end_ms {
                        // This non-voice segment is entirely within a subtitle gap
                        sub_gap_found = true;
                        break;
                    }
                }
                if sub_gap_found {
                    confidence += 0.2;
                    reasons.push("Confirmed by subtitle gap".to_string());
                }
            }

            // Final normalization and thresholding
            if duration < 500 {
                confidence = 0.0;
            }

            if confidence > 0.3 {
                // Priority is influenced by confidence and duration.
                // Longer gaps with high confidence are most important.
                let priority = ((confidence * 10.0) + (duration as f32 / 5000.0)).min(15.0) as u32;

                gaps.push(VisualGap {
                    start_ms: seg.start_ms,
                    end_ms: seg.end_ms,
                    confidence: confidence.min(1.0),
                    reason: reasons.join("; "),
                    priority,
                });
            }
        }

        // Sort by priority descending, then by start time
        gaps.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then_with(|| a.start_ms.cmp(&b.start_ms))
        });

        Ok(GapAnalysisOutput {
            file: timeline.file.clone(),
            gaps,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Segment, SegmentKind};

    #[test]
    fn test_identify_long_gap() {
        let timeline = TimelineOutput {
            file: "test.mp3".to_string(),
            analysis_sample_rate: 16000,
            frame_ms: 20,
            segments: vec![
                Segment {
                    start_ms: 0,
                    end_ms: 1000,
                    kind: SegmentKind::Speech,
                    confidence: 1.0,
                    tags: vec![],
                    prompt: None,
                },
                Segment {
                    start_ms: 1000,
                    end_ms: 5000,
                    kind: SegmentKind::NonVoice,
                    confidence: 1.0,
                    tags: vec!["ambience".to_string()],
                    prompt: None,
                },
                Segment {
                    start_ms: 5000,
                    end_ms: 6000,
                    kind: SegmentKind::Speech,
                    confidence: 1.0,
                    tags: vec![],
                    prompt: None,
                },
            ],
        };

        let identifier = GapIdentifier::new();
        let output = identifier.identify_gaps(&timeline, None).unwrap();

        assert!(!output.gaps.is_empty());
        assert_eq!(output.gaps[0].start_ms, 1000);
        // 0.4 (duration) + 0.2 (ambience) + 0.2 (surrounding speech) = 0.8
        assert!(output.gaps[0].confidence >= 0.79);
    }
}
