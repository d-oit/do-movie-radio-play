use super::*;
use std::path::PathBuf;

fn observation_json() -> String {
    r#"{"frames":[{"frame_ref":"frame-2500","timestamp_ms":2500,
        "description":"Eine Frau öffnet die Tür.",
        "entities":["Frau","Tür"],"actions":["öffnet die Tür"],
        "confidence":0.92,"uncertainty":""}]}"#
        .to_string()
}

#[test]
fn sampling_respects_scene_changes_and_budget() {
    let window = NarrationWindow {
        start_ms: 0,
        end_ms: 10_000,
    };
    let times = sample_timestamps(window, &[2_000, 7_000], 3);
    assert!(times.len() <= 3);
    assert!(times.contains(&2_000) && times.contains(&7_000));
    assert!(times.iter().all(|t| *t < window.end_ms));

    let uniform = sample_timestamps(window, &[], 3);
    assert_eq!(uniform.len(), 3);
    assert!(uniform.windows(2).all(|w| w[0] < w[1]));

    let scene_heavy = sample_timestamps(window, &[1_000, 2_000, 3_000, 4_000], 2);
    assert_eq!(scene_heavy.len(), 2);
}

#[test]
fn cache_key_tracks_provider_model_and_window() {
    let policy = SamplingPolicy::from_config(&VisualConfig::default());
    let window = NarrationWindow {
        start_ms: 1_000,
        end_ms: 5_000,
    };
    let movie = Path::new("missing-movie.mkv");
    let base = cache_key(movie, window, &policy, "openrouter", "model-a", "de");
    assert_eq!(
        base,
        cache_key(movie, window, &policy, "openrouter", "model-a", "de")
    );
    assert_ne!(
        base,
        cache_key(movie, window, &policy, "openrouter", "model-b", "de")
    );
    assert_ne!(
        base,
        cache_key(
            movie,
            NarrationWindow {
                start_ms: 2_000,
                end_ms: 5_000
            },
            &policy,
            "openrouter",
            "model-a",
            "de"
        )
    );
}

#[test]
fn parse_analysis_accepts_fences_and_validates_provenance() {
    let frames = vec![FrameSample {
        frame_ref: "frame-2500".to_string(),
        timestamp_ms: 2_500,
        path: PathBuf::from("/frames/frame-2500.jpg"),
        bytes: b"jpeg".to_vec(),
    }];
    let fenced = format!("```json\n{}\n```", observation_json());
    let analysis = parse_analysis(&fenced, &frames).expect("parse fenced json");
    assert_eq!(analysis.frames.len(), 1);
    assert_eq!(analysis.frames[0].timestamp_ms, 2_500);

    let unknown = r#"{"frames":[{"frame_ref":"frame-9999","timestamp_ms":9999,"confidence":0.5}]}"#;
    let error = parse_analysis(unknown, &frames).expect_err("unknown frame");
    assert_eq!(error.kind, VisionErrorKind::UnknownFrame);

    let empty = r#"{"frames":[]}"#;
    assert!(parse_analysis(empty, &frames).is_err());
}

#[test]
fn prompt_states_language_evidence_and_uncertainty_rules() {
    let frames = vec![FrameSample {
        frame_ref: "frame-2500".to_string(),
        timestamp_ms: 2_500,
        path: PathBuf::from("/frames/frame-2500.jpg"),
        bytes: Vec::new(),
    }];
    let tags = vec!["ambience".to_string()];
    let request = VisionRequest {
        language: "de",
        window_start_ms: 2_000,
        window_end_ms: 8_000,
        max_words: 7,
        acoustic_tags: &tags,
        before_context: "Speech",
        after_context: "Speech",
        known_characters: &[],
        frames: &frames,
    };
    let prompt = build_prompt(&request);
    assert!(prompt.contains("Narration language: de"));
    assert!(prompt.contains("frame-2500"));
    assert!(prompt.contains("Never attribute dialogue"));
    assert!(prompt.contains("state the uncertainty"));
}

#[test]
fn grounding_rejects_unprovable_claims_but_keeps_observations() {
    let policy = grounding::GroundingPolicy::new(vec!["Anna".to_string()], 0.5);
    assert_eq!(
        grounding::classify("Er sagt: Ich komme.", &policy),
        Some(GroundingIssue::DialogueAttribution)
    );
    assert_eq!(
        grounding::classify("Die Frau wirkt wütend.", &policy),
        Some(GroundingIssue::EmotionClaim)
    );
    assert_eq!(
        grounding::classify("Das Fenster zerbricht, weil er es stößt.", &policy),
        Some(GroundingIssue::CausalClaim)
    );
    assert_eq!(
        grounding::classify("Der Mann heißt Karl.", &policy),
        Some(GroundingIssue::UnsupportedIdentity)
    );
    assert_eq!(
        grounding::classify("Eine Frau öffnet die Tür.", &policy),
        None
    );
    assert_eq!(grounding::classify("Anna öffnet die Tür.", &policy), None);
    assert_eq!(
        grounding::classify("We see a door.", &policy),
        Some(GroundingIssue::VisualOnlyLanguage)
    );
    assert_eq!(
        grounding::split_sentences("Eine Tür öffnet sich. Ein Mann tritt ein!"),
        vec![
            "Eine Tür öffnet sich.".to_string(),
            "Ein Mann tritt ein!".to_string()
        ]
    );
}

#[tokio::test]
async fn ffmpeg_extracts_bounded_frames_from_fixture_video() {
    if std::process::Command::new("ffmpeg")
        .arg("-version")
        .output()
        .is_err()
    {
        return;
    }
    let dir = tempfile::tempdir().expect("tempdir");
    let video = dir.path().join("fixture.avi");
    let generated = std::process::Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=duration=2:size=160x120:rate=10",
            "-c:v",
            "mpeg4",
        ])
        .arg(&video)
        .status();
    if !matches!(generated, Ok(status) if status.success()) {
        return;
    }
    let policy = SamplingPolicy {
        max_frames_per_window: 2,
        max_total_frames: 4,
        scene_change_threshold: 0.35,
        frame_width: 64,
    };
    let extractor = FfmpegFrameExtractor::default();
    let window = NarrationWindow {
        start_ms: 0,
        end_ms: 1_500,
    };
    let frames = extractor
        .extract_window(&video, window, &policy, 2, &dir.path().join("frames"))
        .await
        .expect("extract frames");
    assert!(!frames.is_empty());
    assert!(frames.len() <= 2);
    for frame in frames {
        assert!(frame.timestamp_ms < window.end_ms);
        assert!(!frame.bytes.is_empty());
    }
}
