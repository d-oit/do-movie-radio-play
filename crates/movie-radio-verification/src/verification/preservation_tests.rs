use super::*;
use movie_radio_types::SfxLicense;

pub struct DeterministicE2eFixture {
    pub source_samples: Vec<f32>,
    pub output_samples: Vec<f32>,
    pub sample_rate: u32,
    pub dialogue_regions: Vec<TimeRange>,
    pub track_map: TrackMap,
    pub config: PreservationConfig,
}

#[allow(clippy::needless_range_loop)]
pub fn generate_deterministic_e2e_fixture() -> DeterministicE2eFixture {
    let sample_rate = 16_000u32;
    let total_duration_sec = 10;
    let total_samples = (total_duration_sec * sample_rate) as usize;

    let mut source_samples = vec![0.01f32; total_samples]; // Low noise baseline

    // Speech 1: 1000ms .. 4000ms (440 Hz sine)
    let s1_start = (1.0 * sample_rate as f32) as usize;
    let s1_end = (4.0 * sample_rate as f32) as usize;
    for i in s1_start..s1_end {
        let t = (i - s1_start) as f32 / sample_rate as f32;
        source_samples[i] = (2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.5;
    }

    // Speech 2: 7000ms .. 10000ms (880 Hz sine)
    let s2_start = (7.0 * sample_rate as f32) as usize;
    let s2_end = (10.0 * sample_rate as f32) as usize;
    for i in s2_start..s2_end {
        let t = (i - s2_start) as f32 / sample_rate as f32;
        source_samples[i] = (2.0 * std::f32::consts::PI * 880.0 * t).sin() * 0.5;
    }

    let dialogue_regions = vec![TimeRange::new(1000, 4000), TimeRange::new(7000, 10000)];

    let mut output_samples = source_samples.clone();

    // Narration in Gap 2: 4100ms .. 6900ms (523 Hz sine)
    let n_start = (4.1 * sample_rate as f32) as usize;
    let n_end = (6.9 * sample_rate as f32) as usize;
    for i in n_start..n_end {
        let t = (i - n_start) as f32 / sample_rate as f32;
        output_samples[i] += (2.0 * std::f32::consts::PI * 523.25 * t).sin() * 0.3;
    }

    // SFX in Gap 1: 100ms .. 800ms
    let sfx_start = (0.1 * sample_rate as f32) as usize;
    let sfx_end = (0.8 * sample_rate as f32) as usize;
    for i in sfx_start..sfx_end {
        let t = (i - sfx_start) as f32 / sample_rate as f32;
        output_samples[i] += (2.0 * std::f32::consts::PI * 1000.0 * t).sin() * 0.1;
    }

    let track_map = TrackMap {
        entries: vec![
            TrackMapEntry {
                id: "source_film_untouched_1".to_string(),
                track_type: TrackType::SourceFilm,
                source_start_ms: 1000,
                source_end_ms: 4000,
                output_start_ms: 1000,
                output_end_ms: 4000,
                operation: GainDuckingOperation::None,
                provenance: ContentProvenance::default(),
            },
            TrackMapEntry {
                id: "source_film_untouched_2".to_string(),
                track_type: TrackType::SourceFilm,
                source_start_ms: 7000,
                source_end_ms: 10000,
                output_start_ms: 7000,
                output_end_ms: 10000,
                operation: GainDuckingOperation::None,
                provenance: ContentProvenance::default(),
            },
            TrackMapEntry {
                id: "narration_gap_2".to_string(),
                track_type: TrackType::Narration,
                source_start_ms: 4100,
                source_end_ms: 6900,
                output_start_ms: 4100,
                output_end_ms: 6900,
                operation: GainDuckingOperation::Gain { db: 0.0 },
                provenance: ContentProvenance {
                    asset_id: "narr_01".to_string(),
                    source: "synthesized".to_string(),
                    license: SfxLicense::Cc0,
                    prompt: Some("The character looks around the room.".to_string()),
                },
            },
            TrackMapEntry {
                id: "sfx_gap_1".to_string(),
                track_type: TrackType::Sfx,
                source_start_ms: 100,
                source_end_ms: 800,
                output_start_ms: 100,
                output_end_ms: 800,
                operation: GainDuckingOperation::Gain { db: 0.0 },
                provenance: ContentProvenance {
                    asset_id: "sfx_whoosh".to_string(),
                    source: "local".to_string(),
                    license: SfxLicense::Cc0,
                    prompt: None,
                },
            },
        ],
    };

    let config = PreservationConfig::default();

    DeterministicE2eFixture {
        source_samples,
        output_samples,
        sample_rate,
        dialogue_regions,
        track_map,
        config,
    }
}

#[test]
fn test_e2e_known_good_fixture_passes() {
    let fix = generate_deterministic_e2e_fixture();
    let report = verify_audio_preservation_samples(
        &fix.source_samples,
        &fix.output_samples,
        fix.sample_rate,
        1,
        &fix.dialogue_regions,
        &fix.track_map,
        &fix.config,
    );

    assert!(
        report.overall_passed,
        "Known-good complete radio-play fixture should pass verification. Failures: {:#?}",
        report.window_verdicts
    );
    assert_eq!(report.summary.failed_windows, 0);
    assert_eq!(report.summary.failed_licenses, 0);
    assert!(report.file_integrity.codec_valid);
}

#[test]
fn test_mutation_narration_shifted_into_dialogue_fails() {
    let mut fix = generate_deterministic_e2e_fixture();

    // Shift narration segment output window into dialogue 1 (e.g. 3500ms..5500ms)
    for entry in &mut fix.track_map.entries {
        if entry.id == "narration_gap_2" {
            entry.output_start_ms = 3500;
            entry.output_end_ms = 5500;
        }
    }

    let report = verify_audio_preservation_samples(
        &fix.source_samples,
        &fix.output_samples,
        fix.sample_rate,
        1,
        &fix.dialogue_regions,
        &fix.track_map,
        &fix.config,
    );

    assert!(
        !report.overall_passed,
        "Shifted narration encroaching dialogue should fail verification"
    );
    assert!(report.summary.failed_windows > 0);

    let dialogue_verdict = report
        .window_verdicts
        .iter()
        .find(|w| w.window_id == "protected_dialogue_0")
        .expect("dialogue 0 verdict");

    assert!(
        !dialogue_verdict.passed,
        "Protected dialogue window must fail"
    );
    assert!(
        dialogue_verdict
            .failure_reasons
            .iter()
            .any(|r| r.contains("encroaches on protected dialogue cue")),
        "Failure reason must identify exact dialogue window encroachment: {:?}",
        dialogue_verdict.failure_reasons
    );
}

#[test]
fn test_mutation_missing_source_segment_fails() {
    let fix = generate_deterministic_e2e_fixture();
    let mut output_samples = fix.output_samples.clone();

    // Mute source dialogue in output during dialogue 1 (1000ms..4000ms)
    let s_start = (1.0 * fix.sample_rate as f32) as usize;
    let s_end = (4.0 * fix.sample_rate as f32) as usize;
    for sample in &mut output_samples[s_start..s_end] {
        *sample = 0.0;
    }

    let report = verify_audio_preservation_samples(
        &fix.source_samples,
        &output_samples,
        fix.sample_rate,
        1,
        &fix.dialogue_regions,
        &fix.track_map,
        &fix.config,
    );

    assert!(
        !report.overall_passed,
        "Muted/missing source audio should fail verification"
    );
    assert!(report.summary.failed_windows > 0);

    let untouched_verdict = report
        .window_verdicts
        .iter()
        .find(|w| w.window_id == "source_film_untouched_1")
        .expect("untouched 1 verdict");

    assert!(
        !untouched_verdict.passed,
        "Untouched source window must fail"
    );
    assert!(
        untouched_verdict
            .failure_reasons
            .iter()
            .any(|r| r.contains("Source film audio modified")),
        "Failure reason must report RMS difference in modified region: {:?}",
        untouched_verdict.failure_reasons
    );
}

#[test]
fn test_mutation_exceed_gain_limits_clipping_fails() {
    let fix = generate_deterministic_e2e_fixture();
    let mut output_samples = fix.output_samples.clone();

    // Amplify narration window (4100ms..6900ms) to 10.0x causing sample clipping
    let n_start = (4.1 * fix.sample_rate as f32) as usize;
    let n_end = (6.9 * fix.sample_rate as f32) as usize;
    for sample in &mut output_samples[n_start..n_end] {
        *sample *= 10.0;
    }

    let report = verify_audio_preservation_samples(
        &fix.source_samples,
        &output_samples,
        fix.sample_rate,
        1,
        &fix.dialogue_regions,
        &fix.track_map,
        &fix.config,
    );

    assert!(
        !report.overall_passed,
        "Audio clipping / excessive gain should fail verification"
    );
    assert!(report.summary.failed_windows > 0);

    let narration_verdict = report
        .window_verdicts
        .iter()
        .find(|w| w.window_id == "narration_gap_2")
        .expect("narration verdict");

    assert!(
        !narration_verdict.passed,
        "Narration window with clipping must fail"
    );
    assert!(
        narration_verdict
            .failure_reasons
            .iter()
            .any(|r| r.contains("Audio clipping")),
        "Failure reason must identify audio clipping: {:?}",
        narration_verdict.failure_reasons
    );
}

#[test]
fn test_mutation_disallowed_sfx_license_fails() {
    let mut fix = generate_deterministic_e2e_fixture();

    // Change SFX license to non-commercial CC-BY-NC
    for entry in &mut fix.track_map.entries {
        if entry.id == "sfx_gap_1" {
            entry.provenance.license = SfxLicense::CcByNc;
        }
    }

    // Restrict allowed licenses to CC0 only
    let mut config = fix.config.clone();
    config.allowed_licenses = vec!["cc0".to_string()];

    let report = verify_audio_preservation_samples(
        &fix.source_samples,
        &fix.output_samples,
        fix.sample_rate,
        1,
        &fix.dialogue_regions,
        &fix.track_map,
        &config,
    );

    assert!(
        !report.overall_passed,
        "Disallowed SFX license provenance should fail verification"
    );
    assert_eq!(report.summary.failed_licenses, 1);

    let license_verdict = report
        .license_provenance_summary
        .iter()
        .find(|l| l.asset_id == "sfx_whoosh")
        .expect("sfx_whoosh license verdict");

    assert!(!license_verdict.allowed);
    assert!(
        license_verdict
            .reason
            .as_ref()
            .unwrap()
            .contains("not permitted"),
        "Reason must state license is not permitted: {:?}",
        license_verdict.reason
    );
}
