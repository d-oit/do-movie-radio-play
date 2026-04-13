use movie_nonvoice_timeline::pipeline::prompts::add_prompts;
use movie_nonvoice_timeline::types::{Segment, SegmentKind, TimelineOutput};

#[test]
fn prompt_rule_applies() {
    let mut out = TimelineOutput {
        file: "a".into(),
        analysis_sample_rate: 16000,
        frame_ms: 20,
        segments: vec![Segment {
            start_ms: 0,
            end_ms: 5000,
            kind: SegmentKind::NonVoice,
            confidence: 0.9,
            tags: vec!["ambience".into()],
            prompt: None,
        }],
    };
    add_prompts(&mut out);
    assert!(out.segments[0].prompt.is_some());
}
