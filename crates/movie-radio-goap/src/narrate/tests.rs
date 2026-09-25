use super::*;
use movie_radio_types::{Segment, SegmentKind, TimelineOutput, VisualGap};

fn make_timeline(segments: Vec<Segment>) -> TimelineOutput {
    TimelineOutput {
        file: "test.wav".to_string(),
        analysis_sample_rate: 16000,
        frame_ms: 20,
        segments,
    }
}

#[test]
fn test_max_words_for_duration() {
    let gen = NarrationGenerator::new(1.0);
    // 3 seconds at 150 wpm = 7.5 words
    assert_eq!(gen.max_words_for_duration(3000), 7);
    // 1 second at 150 wpm = 2.5 words
    assert_eq!(gen.max_words_for_duration(1000), 2);
}

#[test]
fn test_fit_chunks_to_budget_never_truncates_mid_sentence() {
    let gen = NarrationGenerator::default();
    // A budget smaller than the first whole clause still yields the full
    // first clause — over budget but whole — never a fragment like "Ein".
    assert_eq!(
        gen.fit_chunks_to_budget(&["Ein kräftiger Aufprall ertönt."], 1),
        "Ein kräftiger Aufprall ertönt."
    );
    assert_eq!(
        gen.fit_chunks_to_budget(&["Ein kräftiger Aufprall ertönt."], 4),
        "Ein kräftiger Aufprall ertönt."
    );
    // An over-budget second clause is dropped whole, not cut mid-sentence.
    assert_eq!(
        gen.fit_chunks_to_budget(
            &[
                "Ein kräftiger Aufprall ertönt.",
                "Die Passage dauert einige Sekunden."
            ],
            4
        ),
        "Ein kräftiger Aufprall ertönt."
    );
}

#[test]
fn test_generate_always_emits_a_whole_sentence_for_accepted_gaps() {
    let gen = NarrationGenerator::default();
    // Even a 1s gap (single-word budget, far below every whole clause)
    // gets a complete sentence: the assembler absorbs the overrun via
    // time-stretch, while a fragment or silence would describe nothing.
    let timeline = make_timeline(vec![Segment {
        start_ms: 0,
        end_ms: 1_000,
        kind: SegmentKind::NonVoice,
        confidence: 1.0,
        tags: vec!["crowd_like".to_string()],
        prompt: None,
        sfx_trigger: None,
    }]);
    let gaps = vec![VisualGap {
        start_ms: 0,
        end_ms: 1_000,
        confidence: 0.9,
        reason: "Short pause".to_string(),
        priority: 1,
    }];

    let scripts = gen.generate(&timeline, &gaps).unwrap();
    assert_eq!(scripts.len(), 1);
    assert!(
        scripts[0].text.contains("Menschenmenge"),
        "expected a whole grounded sentence, got: {}",
        scripts[0].text
    );
    assert!(
        scripts[0].text.split_whitespace().count() > 1,
        "must never degenerate to a single-word fragment, got: {}",
        scripts[0].text
    );
}

#[test]
fn test_generate_with_gap() {
    let gen = NarrationGenerator::new(0.5);
    let timeline = make_timeline(vec![
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
    ]);

    let gaps = vec![VisualGap {
        start_ms: 1000,
        end_ms: 5000,
        confidence: 0.8,
        reason: "Extended ambience".to_string(),
        priority: 5,
    }];

    let scripts = gen.generate(&timeline, &gaps).unwrap();
    assert_eq!(scripts.len(), 1);
    assert!(!scripts[0].text.is_empty());
    assert!(scripts[0].word_count <= 7);
}

#[test]
fn test_low_confidence_gap_skipped() {
    let gen = NarrationGenerator::default();
    let timeline = make_timeline(vec![]);
    let gaps = vec![VisualGap {
        start_ms: 0,
        end_ms: 5000,
        confidence: 0.2,
        reason: "weak".to_string(),
        priority: 1,
    }];

    let scripts = gen.generate(&timeline, &gaps).unwrap();
    assert!(scripts.is_empty());
}

fn gap_context(tags: &[&str]) -> GapContext {
    GapContext {
        self_tags: Vec::new(),
        before_tags: tags.iter().map(|t| t.to_string()).collect(),
        after_tags: Vec::new(),
        before_kind: None,
        after_kind: None,
        gap_duration_ms: 2_000,
        gap_reason: String::new(),
    }
}

#[test]
fn test_infer_emotion_covers_full_tag_vocabulary() {
    let gen = NarrationGenerator::default();
    for (tags, expected) in [
        (&["impact_heavy"][..], Emotion::Tense),
        (&["crowd_like"][..], Emotion::Excited),
        (&["nature_like"][..], Emotion::Joyful),
        (&["tonal"][..], Emotion::Mysterious),
        (&["music_like"][..], Emotion::Mysterious),
        (&["music_bed"][..], Emotion::Mysterious),
        (&["machinery_like"][..], Emotion::Neutral),
        (&["speech_like"][..], Emotion::Neutral),
        (&["ambience"][..], Emotion::Neutral),
    ] {
        assert_eq!(gen.infer_emotion(&gap_context(tags)), expected, "{tags:?}");
    }
    // Precedence: impact outranks every other tag, and long silences
    // without tags resolve to Mysterious.
    assert_eq!(
        gen.infer_emotion(&gap_context(&["impact_heavy", "crowd_like"])),
        Emotion::Tense
    );
    let mut long_gap = gap_context(&[]);
    long_gap.gap_duration_ms = 9_000;
    assert_eq!(gen.infer_emotion(&long_gap), Emotion::Mysterious);
}

#[test]
fn test_generate_text_is_grounded_not_filler() {
    let gen = NarrationGenerator::default();
    let timeline = make_timeline(vec![Segment {
        start_ms: 0,
        end_ms: 12_000,
        kind: SegmentKind::NonVoice,
        confidence: 1.0,
        tags: vec!["impact_heavy".to_string(), "music_bed".to_string()],
        prompt: None,
        sfx_trigger: None,
    }]);
    let gaps = vec![VisualGap {
        start_ms: 0,
        end_ms: 12_000,
        confidence: 0.9,
        reason: "Duration (12000ms) > 3000ms; Ambiguous SFX needing description".to_string(),
        priority: 5,
    }];

    let scripts = gen.generate(&timeline, &gaps).unwrap();
    assert_eq!(scripts.len(), 1);
    let text = &scripts[0].text;
    // Must never degenerate into content-free filler.
    assert_ne!(text, "Stille.");
    assert_ne!(text, "Pause.");
    // Must reflect the gap's own detected tags, not just neighbours.
    assert!(
        text.contains("Aufprall"),
        "expected the impact_heavy tag to ground the text, got: {text}"
    );
}

#[test]
fn test_generate_text_is_deterministic() {
    let gen = NarrationGenerator::default();
    let timeline = make_timeline(vec![Segment {
        start_ms: 0,
        end_ms: 10_000,
        kind: SegmentKind::NonVoice,
        confidence: 1.0,
        tags: vec!["nature_like".to_string()],
        prompt: None,
        sfx_trigger: None,
    }]);
    let gaps = vec![VisualGap {
        start_ms: 0,
        end_ms: 10_000,
        confidence: 0.9,
        reason: "Extended ambience".to_string(),
        priority: 3,
    }];

    let first = gen.generate(&timeline, &gaps).unwrap();
    let second = gen.generate(&timeline, &gaps).unwrap();
    assert_eq!(first[0].text, second[0].text);
}

#[test]
fn test_self_tags_take_precedence_over_neighbour_tags() {
    let gen = NarrationGenerator::default();
    // The gap itself is tagged machinery_like while the neighbour carries
    // crowd_like, which outranks machinery_like in the global tag
    // priority. Only true source precedence (own tags matched before
    // neighbour tags) selects the machinery clause; a merged tag list
    // would wrongly describe a crowd.
    let timeline = make_timeline(vec![
        Segment {
            start_ms: 0,
            end_ms: 2_000,
            kind: SegmentKind::NonVoice,
            confidence: 1.0,
            tags: vec!["crowd_like".to_string()],
            prompt: None,
            sfx_trigger: None,
        },
        Segment {
            start_ms: 2_000,
            end_ms: 12_000,
            kind: SegmentKind::NonVoice,
            confidence: 1.0,
            tags: vec!["machinery_like".to_string()],
            prompt: None,
            sfx_trigger: None,
        },
    ]);
    let gaps = vec![VisualGap {
        start_ms: 2_000,
        end_ms: 12_000,
        confidence: 0.9,
        reason: "Audio environment change detected".to_string(),
        priority: 4,
    }];

    let scripts = gen.generate(&timeline, &gaps).unwrap();
    assert!(
        scripts[0].text.contains("Maschinengeräusch"),
        "expected the gap's own machinery_like tag to win, got: {}",
        scripts[0].text
    );
}

#[test]
fn test_reject_banned_filler_substitutes_safe_fallback() {
    for filler in BANNED_FILLER_ONLY {
        assert_eq!(reject_banned_filler(filler.to_string()), SAFE_FALLBACK);
    }
    // Padding whitespace must not evade the guardrail.
    assert_eq!(
        reject_banned_filler("  Stille.  ".to_string()),
        SAFE_FALLBACK
    );
}

#[test]
fn test_reject_banned_filler_leaves_real_content_untouched() {
    let real = "Ein kräftiger Aufprall ertönt.".to_string();
    assert_eq!(reject_banned_filler(real.clone()), real);
    // A banned word used inside a longer, real sentence is not a false
    // positive: the guardrail only matches the *entire* trimmed text.
    let compound = "Nach der Stille ertönt ein Aufprall.".to_string();
    assert_eq!(reject_banned_filler(compound.clone()), compound);
}
