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
    /// Creates a gap identifier with default signal thresholds.
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

            let duration = seg.end_ms.saturating_sub(seg.start_ms);
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
        if index >= segments.len() {
            return;
        }
        let has_speech_before = index > 0 && segments[index - 1].kind == SegmentKind::Speech;
        let has_speech_after =
            index + 1 < segments.len() && segments[index + 1].kind == SegmentKind::Speech;

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
        if index == 0 || index >= segments.len() {
            return;
        }
        if let Some(next) = segments.get(index + 1) {
            let prev = &segments[index - 1];

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
                    sfx_trigger: None,
                },
                Segment {
                    start_ms: 1000,
                    end_ms: 5000,
                    kind: SegmentKind::NonVoice,
                    confidence: 1.0,
                    tags: vec!["ambience".to_string()],
                    prompt: None,
                    sfx_trigger: None,
                },
                Segment {
                    start_ms: 5000,
                    end_ms: 6000,
                    kind: SegmentKind::Speech,
                    confidence: 1.0,
                    tags: vec![],
                    prompt: None,
                    sfx_trigger: None,
                },
            ],
        };

        let identifier = GapIdentifier::default();
        let output = identifier.identify_gaps(&timeline, None).unwrap();

        assert!(!output.gaps.is_empty());
        assert_eq!(output.gaps[0].start_ms, 1000);
        // 0.4 (duration) + 0.2 (ambience) + 0.2 (surrounding speech) = 0.8
        assert!(output.gaps[0].confidence >= 0.79);
    }

    fn segment(start_ms: u64, end_ms: u64, kind: SegmentKind, tags: &[&str]) -> Segment {
        Segment {
            start_ms,
            end_ms,
            kind,
            confidence: 1.0,
            tags: tags.iter().map(|s| s.to_string()).collect(),
            prompt: None,
            sfx_trigger: None,
        }
    }

    #[test]
    fn test_analyze_duration_signal() {
        let id = GapIdentifier::default();
        let mut c = 0.0;
        let mut r = Vec::new();

        // Below 1s: no contribution.
        id.analyze_duration(800, &mut c, &mut r);
        assert_eq!(c, 0.0);
        assert!(r.is_empty());

        // 1s..=3s: small boost only.
        id.analyze_duration(2500, &mut c, &mut r);
        assert!((c - 0.1).abs() < 1e-6);
        assert!(r.is_empty());

        // > min_silence_duration_ms: strong boost plus reason.
        id.analyze_duration(4000, &mut c, &mut r);
        assert!((c - 0.5).abs() < 1e-6);
        assert_eq!(r, vec!["Duration (4000ms) > 3s".to_string()]);
    }

    #[test]
    fn test_analyze_tag_context() {
        let id = GapIdentifier::default();
        let mut c = 0.0;
        let mut r = Vec::new();

        // Ambience only counts when the segment is longer than 2s.
        id.analyze_tag_context(&["ambience".to_string()], 1500, &mut c, &mut r);
        assert_eq!(c, 0.0);
        id.analyze_tag_context(&["ambience".to_string()], 3000, &mut c, &mut r);
        assert!((c - 0.2).abs() < 1e-6);
        assert_eq!(r, vec!["Extended ambience".to_string()]);

        // impact_heavy / machinery_like always boost.
        id.analyze_tag_context(&["impact_heavy".to_string()], 1000, &mut c, &mut r);
        assert!((c - 0.5).abs() < 1e-6);
        assert_eq!(r[1], "Ambiguous SFX needing description".to_string());

        // music_bed only counts when longer than 5s.
        id.analyze_tag_context(&["music_bed".to_string()], 4000, &mut c, &mut r);
        assert!((c - 0.5).abs() < 1e-6);
        id.analyze_tag_context(&["music_bed".to_string()], 6000, &mut c, &mut r);
        assert!((c - 0.6).abs() < 1e-6);
    }

    #[test]
    fn test_analyze_dialogue_proximity() {
        let id = GapIdentifier::default();
        let segments = vec![
            segment(0, 1000, SegmentKind::Speech, &[]),
            segment(1000, 2000, SegmentKind::NonVoice, &[]),
            segment(2000, 3000, SegmentKind::Speech, &[]),
        ];

        // Between two speech blocks.
        let mut c = 0.0;
        let mut r = Vec::new();
        id.analyze_dialogue_proximity(1, &segments, &mut c, &mut r);
        assert!((c - 0.2).abs() < 1e-6);
        assert_eq!(r, vec!["Gap between dialogue blocks".to_string()]);

        // Speech only on one side: no boost.
        let mut c = 0.0;
        let mut r = Vec::new();
        id.analyze_dialogue_proximity(1, &segments[..2], &mut c, &mut r);
        assert_eq!(c, 0.0);

        // First segment has no predecessor: no boost.
        let mut c = 0.0;
        let mut r = Vec::new();
        id.analyze_dialogue_proximity(0, &segments, &mut c, &mut r);
        assert_eq!(c, 0.0);
    }

    #[test]
    fn test_analyze_audio_environment_change() {
        let id = GapIdentifier::default();
        let segments = vec![
            segment(0, 1000, SegmentKind::Speech, &["indoor"]),
            segment(1000, 2000, SegmentKind::NonVoice, &[]),
            segment(2000, 3000, SegmentKind::Speech, &["outdoor"]),
        ];

        // Disjoint, non-empty tag sets: scene change detected.
        let mut c = 0.0;
        let mut r = Vec::new();
        id.analyze_audio_environment_change(1, &segments, &mut c, &mut r);
        assert!((c - 0.3).abs() < 1e-6);
        assert_eq!(r, vec!["Audio environment change detected".to_string()]);

        // Overlapping tag sets: no scene change.
        let overlapping = vec![
            segment(0, 1000, SegmentKind::Speech, &["indoor"]),
            segment(1000, 2000, SegmentKind::NonVoice, &[]),
            segment(2000, 3000, SegmentKind::Speech, &["indoor", "outdoor"]),
        ];
        let mut c = 0.0;
        let mut r = Vec::new();
        id.analyze_audio_environment_change(1, &overlapping, &mut c, &mut r);
        assert_eq!(c, 0.0);

        // Boundary index: no scene change.
        let mut c = 0.0;
        let mut r = Vec::new();
        id.analyze_audio_environment_change(0, &segments, &mut c, &mut r);
        assert_eq!(c, 0.0);
    }

    #[test]
    fn test_analyze_subtitle_gap() {
        let id = GapIdentifier::default();
        let subs = vec![
            segment(0, 1000, SegmentKind::Speech, &[]),
            segment(2000, 3000, SegmentKind::Speech, &[]),
        ];

        // Segment fully inside the subtitle gap: confirmed.
        let mut c = 0.0;
        let mut r = Vec::new();
        id.analyze_subtitle_gap(1200, 1800, &Some(subs.clone()), &mut c, &mut r);
        assert!((c - 0.2).abs() < 1e-6);
        assert_eq!(r, vec!["Confirmed by subtitle gap".to_string()]);

        // Segment overlapping a subtitle: not confirmed.
        let mut c = 0.0;
        let mut r = Vec::new();
        id.analyze_subtitle_gap(900, 1500, &Some(subs.clone()), &mut c, &mut r);
        assert_eq!(c, 0.0);

        // No subtitles available: no contribution.
        let mut c = 0.0;
        let mut r = Vec::new();
        id.analyze_subtitle_gap(1200, 1800, &None, &mut c, &mut r);
        assert_eq!(c, 0.0);
    }

    #[test]
    fn test_proximity_and_environment_helpers_reject_invalid_slices() {
        let id = GapIdentifier::default();
        let empty: Vec<Segment> = Vec::new();

        let mut c = 0.0;
        let mut r = Vec::new();
        id.analyze_dialogue_proximity(0, &empty, &mut c, &mut r);
        assert_eq!(c, 0.0);

        let mut c = 0.0;
        let mut r = Vec::new();
        id.analyze_dialogue_proximity(3, &empty, &mut c, &mut r);
        assert_eq!(c, 0.0);

        let mut c = 0.0;
        let mut r = Vec::new();
        id.analyze_audio_environment_change(0, &empty, &mut c, &mut r);
        assert_eq!(c, 0.0);

        let mut c = 0.0;
        let mut r = Vec::new();
        id.analyze_audio_environment_change(3, &empty, &mut c, &mut r);
        assert_eq!(c, 0.0);

        let segments = vec![
            segment(0, 1000, SegmentKind::Speech, &["indoor"]),
            segment(1000, 2000, SegmentKind::NonVoice, &[]),
        ];
        let mut c = 0.0;
        let mut r = Vec::new();
        id.analyze_audio_environment_change(segments.len(), &segments, &mut c, &mut r);
        assert_eq!(c, 0.0);
    }

    #[test]
    fn test_inverted_segment_yields_no_gap() {
        let timeline = TimelineOutput {
            file: "test.mp3".to_string(),
            analysis_sample_rate: 16000,
            frame_ms: 20,
            segments: vec![segment(5000, 1000, SegmentKind::NonVoice, &["ambience"])],
        };
        let identifier = GapIdentifier::default();
        let output = identifier.identify_gaps(&timeline, None).unwrap();
        assert!(output.gaps.is_empty());
    }
}
