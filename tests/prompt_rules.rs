use movie_nonvoice_timeline::config::AnalysisConfig;
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
    let cfg = AnalysisConfig::default();
    add_prompts(&mut out, &cfg);
    assert!(out.segments[0].prompt.is_some());
}

#[test]
fn crowd_and_machinery_have_unique_prompts() {
    let cfg = AnalysisConfig::default();
    let mut timeline = TimelineOutput {
        file: "b".into(),
        analysis_sample_rate: 16000,
        frame_ms: 20,
        segments: vec![
            Segment {
                start_ms: 0,
                end_ms: 4000,
                kind: SegmentKind::NonVoice,
                confidence: 0.9,
                tags: vec!["crowd_like".into()],
                prompt: None,
            },
            Segment {
                start_ms: 5000,
                end_ms: 9000,
                kind: SegmentKind::NonVoice,
                confidence: 0.95,
                tags: vec!["machinery_like".into()],
                prompt: None,
            },
        ],
    };
    add_prompts(&mut timeline, &cfg);
    let prompts: Vec<_> = timeline
        .segments
        .iter()
        .filter_map(|s| s.prompt.clone())
        .collect();
    assert!(prompts.iter().any(|p| p.contains("crowd")));
    assert!(prompts.iter().any(|p| p.contains("Mechanical")));
}
