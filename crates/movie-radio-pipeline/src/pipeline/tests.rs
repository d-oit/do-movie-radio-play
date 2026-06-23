use super::*;
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
