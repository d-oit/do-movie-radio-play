use super::*;
use filters::{
    residual_bridge_gap_ms, should_apply_speech_evidence_filter, should_apply_verification_filter,
};
use hound::{WavSpec, WavWriter};
use movie_radio_types::AnalysisConfig;
use movie_radio_types::MergeOptions;

#[test]
fn verification_filter_applies_to_sparse_low_min_non_voice_profiles() {
    let cfg = AnalysisConfig {
        min_non_voice_ms: 800,
        merge_options: Some(MergeOptions {
            merge_strategy: MergeStrategy::Sparse,
            ..MergeOptions::default()
        }),
        ..AnalysisConfig::default()
    };

    assert!(should_apply_verification_filter(&cfg));
}

#[test]
fn verification_filter_skips_non_sparse_profiles() {
    let cfg = AnalysisConfig {
        min_non_voice_ms: 500,
        merge_options: Some(MergeOptions {
            merge_strategy: MergeStrategy::All,
            ..MergeOptions::default()
        }),
        ..AnalysisConfig::default()
    };

    assert!(!should_apply_verification_filter(&cfg));
}

#[test]
fn residual_bridge_gap_stays_wide_for_sparse_profiles() {
    let cfg = AnalysisConfig {
        merge_options: Some(MergeOptions {
            merge_strategy: MergeStrategy::Sparse,
            min_gap_to_merge: 600,
            min_silence_duration: 500,
            ..MergeOptions::default()
        }),
        ..AnalysisConfig::default()
    };

    assert_eq!(residual_bridge_gap_ms(&cfg), MAX_RESIDUAL_BRIDGE_GAP_MS);
}

#[test]
fn residual_bridge_gap_is_bounded_for_non_sparse_profiles() {
    let cfg = AnalysisConfig {
        merge_options: Some(MergeOptions {
            merge_strategy: MergeStrategy::All,
            min_gap_to_merge: 400,
            min_silence_duration: 300,
            ..MergeOptions::default()
        }),
        ..AnalysisConfig::default()
    };

    assert_eq!(residual_bridge_gap_ms(&cfg), 400);
}

#[test]
fn ambiguous_expand_unbounded_for_sparse_profiles() {
    let cfg = AnalysisConfig {
        merge_options: Some(MergeOptions {
            merge_strategy: MergeStrategy::Sparse,
            ..MergeOptions::default()
        }),
        ..AnalysisConfig::default()
    };

    assert_eq!(ambiguous_expand_max_ms(&cfg), None);
}

#[test]
fn ambiguous_expand_bounded_for_non_sparse_profiles() {
    let cfg = AnalysisConfig {
        merge_options: Some(MergeOptions {
            merge_strategy: MergeStrategy::All,
            ..MergeOptions::default()
        }),
        ..AnalysisConfig::default()
    };

    assert_eq!(
        ambiguous_expand_max_ms(&cfg),
        Some(NON_SPARSE_AMBIGUOUS_EXPAND_MAX_MS)
    );
}

#[test]
fn speech_evidence_filter_enabled_for_sparse_profiles() {
    let cfg = AnalysisConfig {
        merge_options: Some(MergeOptions {
            merge_strategy: MergeStrategy::Sparse,
            ..MergeOptions::default()
        }),
        ..AnalysisConfig::default()
    };

    assert!(should_apply_speech_evidence_filter(&cfg));
}

#[test]
fn speech_evidence_filter_disabled_for_non_sparse_profiles() {
    let cfg = AnalysisConfig {
        merge_options: Some(MergeOptions {
            merge_strategy: MergeStrategy::All,
            ..MergeOptions::default()
        }),
        ..AnalysisConfig::default()
    };

    assert!(!should_apply_speech_evidence_filter(&cfg));
}

#[test]
fn test_run_pipeline_smoke() {
    let temp_dir = tempfile::tempdir().unwrap();
    let wav_path = temp_dir.path().join("test.wav");
    let spec = WavSpec {
        channels: 1,
        sample_rate: 16000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = WavWriter::create(&wav_path, spec).unwrap();
    for _ in 0..16000 {
        writer.write_sample(0i16).unwrap();
    }
    writer.finalize().unwrap();

    let cfg = AnalysisConfig {
        min_non_voice_ms: 100,
        ..AnalysisConfig::default()
    };
    let result = run_pipeline(&wav_path, &cfg).unwrap();
    assert!(!result.timeline.segments.is_empty());
    assert_eq!(result.timeline.analysis_sample_rate, 16000);
}

#[test]
fn test_chunked_streaming_extraction_deterministic() {
    let temp_dir = tempfile::tempdir().unwrap();
    let wav_path = temp_dir.path().join("test_chunked_det.wav");
    let spec = WavSpec {
        channels: 1,
        sample_rate: 16000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = WavWriter::create(&wav_path, spec).unwrap();
    for i in 0..80000 {
        let t = i as f32 / 16000.0;
        let val = if (1.0..2.5).contains(&t) || (3.5..4.5).contains(&t) {
            ((2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.5 * i16::MAX as f32) as i16
        } else {
            0i16
        };
        writer.write_sample(val).unwrap();
    }
    writer.finalize().unwrap();

    let cfg_standard = AnalysisConfig {
        min_non_voice_ms: 200,
        chunk_duration_sec: None,
        ..AnalysisConfig::default()
    };
    let timeline_standard = extract_timeline(&wav_path, &cfg_standard).unwrap();

    let cfg_chunked = AnalysisConfig {
        min_non_voice_ms: 200,
        chunk_duration_sec: Some(60),
        ..AnalysisConfig::default()
    };
    let timeline_chunked = extract_timeline(&wav_path, &cfg_chunked).unwrap();

    println!("STANDARD SEGMENTS: {:?}", timeline_standard.segments);
    println!("CHUNKED SEGMENTS:  {:?}", timeline_chunked.segments);

    assert_eq!(
        timeline_standard.segments.len(),
        timeline_chunked.segments.len(),
        "Segment count mismatch between standard and chunked streaming"
    );

    for (s1, s2) in timeline_standard
        .segments
        .iter()
        .zip(timeline_chunked.segments.iter())
    {
        assert_eq!(s1.start_ms, s2.start_ms);
        assert_eq!(s1.end_ms, s2.end_ms);
        assert_eq!(s1.kind, s2.kind);
    }
}
