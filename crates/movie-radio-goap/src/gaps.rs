use anyhow::Result;
use movie_radio_types::{GapAnalysisOutput, Segment, SegmentKind, TimelineOutput, VisualGap};
use movie_radio_validation::srt;

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

    /// Identifies silent gaps in the timeline that are suitable candidates for audio description.
    ///
    /// It parses an optional subtitles SRT file and analyzes timeline segments using
    /// multiple signal checks (duration, tag context, dialogue proximity, environment changes, and subtitle gaps)
    /// to assign a confidence and priority score to each identified gap.
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

            self.analyze_duration(duration, &mut confidence, &mut reasons);
            self.analyze_tag_context(&seg.tags, duration, &mut confidence, &mut reasons);
            self.analyze_dialogue_proximity(i, &timeline.segments, &mut confidence, &mut reasons);
            self.analyze_audio_environment_change(
                i,
                &timeline.segments,
                &mut confidence,
                &mut reasons,
            );
            self.analyze_subtitle_gap(
                seg.start_ms,
                seg.end_ms,
                &srt_segments,
                &mut confidence,
                &mut reasons,
            );

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

    /// Analyzes the duration signal of a non-voice segment.
    fn analyze_duration(&self, duration: u64, confidence: &mut f32, reasons: &mut Vec<String>) {
        if duration > self.min_silence_duration_ms {
            *confidence += 0.4;
            reasons.push(format!("Duration ({}ms) > 3s", duration));
        } else if duration > 1000 {
            *confidence += 0.1;
        }
    }

    /// Analyzes the semantic tag context of a non-voice segment.
    fn analyze_tag_context(
        &self,
        tags: &[String],
        duration: u64,
        confidence: &mut f32,
        reasons: &mut Vec<String>,
    ) {
        if tags.contains(&"ambience".to_string()) && duration > 2000 {
            *confidence += 0.2;
            reasons.push("Extended ambience".to_string());
        }

        if tags.contains(&"impact_heavy".to_string())
            || tags.contains(&"machinery_like".to_string())
        {
            *confidence += 0.3;
            reasons.push("Ambiguous SFX needing description".to_string());
        }

        if tags.contains(&"music_bed".to_string()) && duration > 5000 {
            // Music interludes often don't need narration unless something visual happens.
            // But long ones might. For now, slight boost.
            *confidence += 0.1;
        }
    }

    /// Analyzes proximity to dialogue blocks.
    fn analyze_dialogue_proximity(
        &self,
        index: usize,
        segments: &[Segment],
        confidence: &mut f32,
        reasons: &mut Vec<String>,
    ) {
        let has_speech_before = index > 0 && segments[index - 1].kind == SegmentKind::Speech;
        let has_speech_after =
            index < segments.len() - 1 && segments[index + 1].kind == SegmentKind::Speech;

        if has_speech_before && has_speech_after {
            *confidence += 0.2;
            reasons.push("Gap between dialogue blocks".to_string());
        }
    }

    /// Detects changes in the audio environment around the non-voice segment as a proxy for scene transitions.
    fn analyze_audio_environment_change(
        &self,
        index: usize,
        segments: &[Segment],
        confidence: &mut f32,
        reasons: &mut Vec<String>,
    ) {
        if index > 0 && index < segments.len() - 1 {
            let prev = &segments[index - 1];
            let next = &segments[index + 1];

            let prev_tags: std::collections::HashSet<_> = prev.tags.iter().collect();
            let next_tags: std::collections::HashSet<_> = next.tags.iter().collect();

            let intersection_count = prev_tags.intersection(&next_tags).count();
            if intersection_count == 0 && !prev.tags.is_empty() && !next.tags.is_empty() {
                *confidence += 0.3;
                reasons.push("Audio environment change detected".to_string());
            }
        }
    }

    /// Confirms gaps using the parsed subtitle timeline.
    fn analyze_subtitle_gap(
        &self,
        seg_start_ms: u64,
        seg_end_ms: u64,
        srt_segments: &Option<Vec<Segment>>,
        confidence: &mut f32,
        reasons: &mut Vec<String>,
    ) {
        if let Some(subs) = srt_segments {
            // If there's a large gap between subtitles that overlaps with this non-voice segment
            // it reinforces that this is a scene without dialogue.
            let mut sub_gap_found = false;
            for j in 0..subs.len().saturating_sub(1) {
                let sub_end = subs[j].end_ms;
                let next_sub_start = subs[j + 1].start_ms;

                if sub_end <= seg_start_ms && next_sub_start >= seg_end_ms {
                    // This non-voice segment is entirely within a subtitle gap
                    sub_gap_found = true;
                    break;
                }
            }
            if sub_gap_found {
                *confidence += 0.2;
                reasons.push("Confirmed by subtitle gap".to_string());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use movie_radio_types::{Segment, SegmentKind};

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
