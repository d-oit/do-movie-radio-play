use movie_radio_types::{SegmentKind, SfxTrigger, TimelineOutput};

const SILENT_THRESHOLD_MS: u64 = 3000;

pub fn autofill_silent_scene_sfx(timeline: &mut TimelineOutput) {
    for seg in &mut timeline.segments {
        if seg.kind != SegmentKind::NonVoice {
            continue;
        }
        if seg.sfx_trigger.is_some() {
            continue;
        }
        let dur = seg.end_ms.saturating_sub(seg.start_ms);
        if dur < SILENT_THRESHOLD_MS {
            seg.sfx_trigger = Some(SfxTrigger::None);
            continue;
        }
        if seg.tags.is_empty() {
            seg.sfx_trigger = Some(SfxTrigger::AutoSelect {
                tags: vec!["ambience".to_string()],
                mood: None,
            });
            continue;
        }
        let mood = infer_mood(&seg.tags);
        seg.sfx_trigger = Some(SfxTrigger::AutoSelect {
            tags: seg.tags.clone(),
            mood,
        });
    }
}

fn infer_mood(tags: &[String]) -> Option<String> {
    for tag in tags {
        match tag.as_str() {
            "impact_heavy" => return Some("tense".to_string()),
            "music_bed" => return Some("emotional".to_string()),
            "machinery_like" => return Some("industrial".to_string()),
            "crowd_like" => return Some("lively".to_string()),
            "nature_like" => return Some("calm".to_string()),
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use movie_radio_types::Segment;

    fn seg(kind: SegmentKind, start: u64, end: u64, tags: Vec<&str>) -> Segment {
        Segment {
            start_ms: start,
            end_ms: end,
            kind,
            confidence: 0.9,
            tags: tags.into_iter().map(|s| s.to_string()).collect(),
            prompt: None,
            sfx_trigger: None,
        }
    }

    #[test]
    fn test_autofill_long_silent() {
        let mut tl = TimelineOutput {
            file: "test".to_string(),
            analysis_sample_rate: 16000,
            frame_ms: 20,
            segments: vec![seg(SegmentKind::NonVoice, 0, 5000, vec!["ambience"])],
        };
        autofill_silent_scene_sfx(&mut tl);
        assert!(matches!(
            tl.segments[0].sfx_trigger,
            Some(SfxTrigger::AutoSelect { .. })
        ));
    }

    #[test]
    fn test_short_silent_none() {
        let mut tl = TimelineOutput {
            file: "test".to_string(),
            analysis_sample_rate: 16000,
            frame_ms: 20,
            segments: vec![seg(SegmentKind::NonVoice, 0, 1000, vec!["ambience"])],
        };
        autofill_silent_scene_sfx(&mut tl);
        assert_eq!(tl.segments[0].sfx_trigger, Some(SfxTrigger::None));
    }

    #[test]
    fn test_speech_unchanged() {
        let mut tl = TimelineOutput {
            file: "test".to_string(),
            analysis_sample_rate: 16000,
            frame_ms: 20,
            segments: vec![seg(SegmentKind::Speech, 0, 5000, vec!["speech"])],
        };
        autofill_silent_scene_sfx(&mut tl);
        assert!(tl.segments[0].sfx_trigger.is_none());
    }

    #[test]
    fn test_deterministic() {
        let mk = || TimelineOutput {
            file: "test".to_string(),
            analysis_sample_rate: 16000,
            frame_ms: 20,
            segments: vec![seg(SegmentKind::NonVoice, 0, 4000, vec!["nature_like"])],
        };
        let mut a = mk();
        let mut b = mk();
        autofill_silent_scene_sfx(&mut a);
        autofill_silent_scene_sfx(&mut b);
        assert_eq!(
            serde_json::to_string(&a.segments[0].sfx_trigger).unwrap(),
            serde_json::to_string(&b.segments[0].sfx_trigger).unwrap()
        );
    }

    #[test]
    fn test_existing_trigger_preserved() {
        let mut tl = TimelineOutput {
            file: "test".to_string(),
            analysis_sample_rate: 16000,
            frame_ms: 20,
            segments: vec![Segment {
                start_ms: 0,
                end_ms: 5000,
                kind: SegmentKind::NonVoice,
                confidence: 0.9,
                tags: vec!["ambience".to_string()],
                prompt: None,
                sfx_trigger: Some(SfxTrigger::Specific {
                    sfx_id: "keep".to_string(),
                }),
            }],
        };
        autofill_silent_scene_sfx(&mut tl);
        assert_eq!(
            tl.segments[0].sfx_trigger,
            Some(SfxTrigger::Specific {
                sfx_id: "keep".to_string()
            })
        );
    }
}
