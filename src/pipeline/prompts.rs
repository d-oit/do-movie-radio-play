use crate::config::AnalysisConfig;
use crate::types::{SegmentKind, TimelineOutput};

pub fn add_prompts(timeline: &mut TimelineOutput) {
    let cfg = AnalysisConfig::default();
    for seg in &mut timeline.segments {
        if seg.kind != SegmentKind::NonVoice {
            continue;
        }
        let dur = seg.end_ms.saturating_sub(seg.start_ms);
        if dur < cfg.prompt_min_duration_ms || seg.confidence < cfg.prompt_min_confidence {
            seg.prompt = None;
            continue;
        }
        seg.prompt = Some(prompt_for_tags(&seg.tags));
    }
}

fn prompt_for_tags(tags: &[String]) -> String {
    if tags.iter().any(|t| t == "impact_heavy") {
        return "No dialogue. Energetic impact-forward soundscape.".to_string();
    }
    if tags.iter().any(|t| t == "music_bed") {
        return "No dialogue. Background music bed without spoken words.".to_string();
    }
    if tags.iter().any(|t| t == "nature_like") {
        return "No dialogue. Ambient natural atmosphere.".to_string();
    }
    "No dialogue. Ambient environmental sound.".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Segment, TimelineOutput};

    #[test]
    fn prompts_set_for_eligible_segments() {
        let mut t = TimelineOutput {
            file: "x".into(),
            analysis_sample_rate: 16000,
            frame_ms: 20,
            segments: vec![Segment {
                start_ms: 0,
                end_ms: 4000,
                kind: SegmentKind::NonVoice,
                confidence: 0.9,
                tags: vec!["ambience".into()],
                prompt: None,
            }],
        };
        add_prompts(&mut t);
        assert_eq!(
            t.segments[0].prompt.clone().unwrap_or_default(),
            "No dialogue. Ambient environmental sound."
        );
    }
}
