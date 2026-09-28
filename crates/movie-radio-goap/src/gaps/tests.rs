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

    // > min_silence_duration_ms: strong boost plus reason naming the
    // effective cutoff (profile-tunable, not always 3s).
    id.analyze_duration(4000, &mut c, &mut r);
    assert!((c - 0.5).abs() < 1e-6);
    assert_eq!(r, vec!["Duration (4000ms) > 3000ms".to_string()]);

    // Profile-tuned cutoff stays in sync: documentary accepts 2800ms
    // with a 2700ms reason, not a stale "> 3s" claim.
    let doc = GapIdentifier::with_profile(movie_radio_learning::profiles::profile("documentary"));
    let mut c = 0.0;
    let mut r = Vec::new();
    doc.analyze_duration(2800, &mut c, &mut r);
    assert!((c - 0.4).abs() < 1e-6);
    assert_eq!(r, vec!["Duration (2800ms) > 2700ms".to_string()]);
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
