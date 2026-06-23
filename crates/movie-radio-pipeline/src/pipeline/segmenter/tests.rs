use movie_radio_types::{MergeOptions, MergeStrategy, Segment, SegmentKind};

use super::*;

fn fake_speech(start_ms: u64, end_ms: u64) -> Segment {
    Segment {
        start_ms,
        end_ms,
        kind: SegmentKind::Speech,
        confidence: 0.8,
        tags: vec![],
        prompt: None,
    }
}

#[test]
fn merge_gap_works() {
    let speech = vec![fake_speech(0, 200), fake_speech(300, 500)];
    let merged = merge_close_segments(&speech, 120);
    assert_eq!(merged.len(), 1);
}

#[test]
fn prune_short_low_confidence_speech_segments() {
    let mut short_low = fake_speech(100, 260);
    short_low.confidence = 0.55;
    let mut short_high = fake_speech(300, 450);
    short_high.confidence = 0.9;
    let long = fake_speech(600, 1200);

    let pruned = prune_short_speech_segments(&[short_low, short_high, long], 300);
    assert_eq!(pruned.len(), 2);
    assert_eq!(pruned[0].start_ms, 300);
    assert_eq!(pruned[1].start_ms, 600);
}

#[test]
fn bridges_non_voice_across_short_speech_gap() {
    let nv = vec![
        Segment {
            start_ms: 0,
            end_ms: 2000,
            kind: SegmentKind::NonVoice,
            confidence: 0.8,
            tags: vec![],
            prompt: None,
        },
        Segment {
            start_ms: 2150,
            end_ms: 4000,
            kind: SegmentKind::NonVoice,
            confidence: 0.7,
            tags: vec![],
            prompt: None,
        },
    ];

    let bridged = bridge_non_voice_segments(&nv, 200);
    assert_eq!(bridged.len(), 1);
    assert_eq!(bridged[0].start_ms, 0);
    assert_eq!(bridged[0].end_ms, 4000);
}

#[test]
fn sparse_merge_policy_merges_close_non_voice_segments() {
    let opts = MergeOptions {
        min_gap_to_merge: 400,
        merge_strategy: MergeStrategy::Sparse,
        min_speech_duration: 500,
        min_silence_duration: 300,
        silence_threshold_db: -42,
    };
    let segments = vec![
        Segment {
            start_ms: 0,
            end_ms: 1000,
            kind: SegmentKind::NonVoice,
            confidence: 0.8,
            tags: vec![],
            prompt: None,
        },
        Segment {
            start_ms: 1200,
            end_ms: 2000,
            kind: SegmentKind::NonVoice,
            confidence: 0.6,
            tags: vec![],
            prompt: None,
        },
    ];

    let merged = apply_non_voice_merge_policy(&segments, &opts);
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].start_ms, 0);
    assert_eq!(merged[0].end_ms, 2000);
}

#[test]
fn all_merge_policy_respects_gap_threshold() {
    let opts = MergeOptions {
        min_gap_to_merge: 400,
        merge_strategy: MergeStrategy::All,
        min_speech_duration: 500,
        min_silence_duration: 300,
        silence_threshold_db: -42,
    };
    let segments = vec![
        Segment {
            start_ms: 0,
            end_ms: 1000,
            kind: SegmentKind::NonVoice,
            confidence: 0.8,
            tags: vec![],
            prompt: None,
        },
        Segment {
            start_ms: 1700,
            end_ms: 2400,
            kind: SegmentKind::NonVoice,
            confidence: 0.7,
            tags: vec![],
            prompt: None,
        },
    ];

    let merged = apply_non_voice_merge_policy(&segments, &opts);
    assert_eq!(merged.len(), 2);
    assert_eq!(merged[0].start_ms, 0);
    assert_eq!(merged[0].end_ms, 1000);
    assert_eq!(merged[1].start_ms, 1700);
    assert_eq!(merged[1].end_ms, 2400);
}

#[test]
fn residual_gap_bridge_merges_tiny_final_gap() {
    let segments = vec![
        Segment {
            start_ms: 1000,
            end_ms: 2000,
            kind: SegmentKind::NonVoice,
            confidence: 0.8,
            tags: vec![],
            prompt: None,
        },
        Segment {
            start_ms: 3500,
            end_ms: 5000,
            kind: SegmentKind::NonVoice,
            confidence: 0.7,
            tags: vec![],
            prompt: None,
        },
    ];

    let bridged = bridge_residual_non_voice_gaps(&segments, 2_500);
    assert_eq!(bridged.len(), 1);
    assert_eq!(bridged[0].start_ms, 1000);
    assert_eq!(bridged[0].end_ms, 5000);
}
