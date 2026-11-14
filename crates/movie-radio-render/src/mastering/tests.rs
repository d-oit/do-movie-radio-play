use super::*;
use std::f32::consts::PI;

const SR: u32 = 48_000;

fn tone(freq: f32, seconds: f32, amplitude: f32) -> Vec<f32> {
    let len = (SR as f32 * seconds) as usize;
    (0..len)
        .map(|i| (2.0 * PI * freq * i as f32 / SR as f32).sin() * amplitude)
        .collect()
}

fn interleave(left: &[f32], right: &[f32]) -> Vec<f32> {
    let len = left.len().min(right.len());
    let mut out = Vec::with_capacity(len * 2);
    for i in 0..len {
        out.push(left[i]);
        out.push(right[i]);
    }
    out
}

#[test]
fn profile_lookup_covers_builtins() {
    assert_eq!(
        MasteringProfile::from_name("ebu_r128").map(|p| p.target_lufs),
        Some(-23.0)
    );
    assert_eq!(
        MasteringProfile::from_name("podcast").map(|p| p.target_lufs),
        Some(-16.0)
    );
    assert_eq!(
        MasteringProfile::from_name("atsc").map(|p| p.max_true_peak_dbtp),
        Some(-2.0)
    );
    assert!(MasteringProfile::from_name("unknown").is_none());
    assert_eq!(MasteringProfile::default().name, DEFAULT_PROFILE);
}

#[test]
fn mono_and_stereo_measurements_are_finite() {
    let mono = tone(1000.0, 2.0, 0.1);
    let mono_report = validate(&mono, SR, 1, &MasteringProfile::ebu_r128());
    assert!(mono_report.measurements.integrated_lufs.is_finite());
    assert!(mono_report.measurements.true_peak_dbtp.is_finite());
    assert_eq!(mono_report.measurements.channel_imbalance_db, 0.0);

    let stereo = interleave(&mono, &mono);
    let stereo_report = validate(&stereo, SR, 2, &MasteringProfile::ebu_r128());
    assert!(stereo_report.measurements.integrated_lufs.is_finite());
    assert!(stereo_report.measurements.channel_imbalance_db < 0.1);
}

#[test]
fn quiet_signal_normalizes_to_target() {
    let quiet = tone(1000.0, 3.0, 0.01);
    let profile = MasteringProfile::ebu_r128();
    let outcome = master(&quiet, SR, 1, &profile);
    let measured = outcome.report.measurements.integrated_lufs;
    assert!(
        (measured - profile.target_lufs).abs() <= profile.tolerance_lu,
        "measured {measured} LUFS, target {}",
        profile.target_lufs
    );
    assert!(outcome.applied_gain_db > 0.0);
}

#[test]
fn clipping_is_detected_and_fails() {
    let clipped = tone(440.0, 1.0, 1.5);
    let report = validate(&clipped, SR, 1, &MasteringProfile::ebu_r128());
    assert!(report.measurements.clipped_samples > 0);
    assert!(report.overall.is_fail());
    assert!(report.failures().any(|c| c.id == "clipping"));
}

#[test]
fn silence_fails_validation() {
    let silence = vec![0.0f32; SR as usize];
    let report = validate(&silence, SR, 1, &MasteringProfile::ebu_r128());
    assert!(report.overall.is_fail());
    assert!(report.failures().any(|c| c.id == "integrated_loudness"));
}

#[test]
fn true_peak_ceiling_is_respected() {
    let loud = tone(1000.0, 2.0, 0.9);
    let profile = MasteringProfile::custom("loud", -10.0, -1.0);
    let outcome = master(&loud, SR, 1, &profile);
    assert!(
        outcome.report.measurements.true_peak_dbtp <= profile.max_true_peak_dbtp + 0.1,
        "true peak {} dBTP exceeds ceiling {}",
        outcome.report.measurements.true_peak_dbtp,
        profile.max_true_peak_dbtp
    );
}

#[test]
fn mastering_is_deterministic() {
    let input = interleave(&tone(440.0, 1.0, 0.2), &tone(660.0, 1.0, 0.1));
    let profile = MasteringProfile::podcast();
    let first = master(&input, SR, 2, &profile);
    let second = master(&input, SR, 2, &profile);
    assert_eq!(first.samples, second.samples);
    assert_eq!(first.report.measurements, second.report.measurements);
}

#[test]
fn report_round_trips_through_json() {
    let input = tone(1000.0, 1.0, 0.1);
    let report = validate(&input, SR, 1, &MasteringProfile::ebu_r128());
    let json = serde_json::to_string(&report).expect("serialize report");
    let decoded: MasteringReport = serde_json::from_str(&json).expect("deserialize report");
    assert_eq!(decoded.overall, report.overall);
    assert_eq!(decoded.measurements, report.measurements);
}

#[test]
fn fixtures_generate_and_validate() {
    let dir = tempfile::tempdir().expect("tempdir");
    generate_fixtures(dir.path()).expect("generate fixtures");
    let clipping = dir.path().join("mastering_clipping.wav");
    let report = validate_file(&clipping, &MasteringProfile::ebu_r128()).expect("validate");
    assert!(report.measurements.clipped_samples > 0);
    assert!(dir.path().join("mastering_mono.wav").exists());
    assert!(dir.path().join("mastering_stereo.wav").exists());
}

#[test]
fn loudness_meter_matches_measure_across_chunkings() {
    let input = interleave(&tone(440.0, 2.0, 0.2), &tone(660.0, 2.0, 0.1));
    let config = MeasurementConfig::default();
    let expected = measure(&input, SR, 2, &config);
    for chunk_frames in [1, 7, 1000, 48_000] {
        let mut meter = LoudnessMeter::new(SR, 2, &config);
        for chunk in input.chunks(chunk_frames * 2) {
            meter.push(chunk);
        }
        let actual = meter.finish();
        assert_eq!(actual.integrated_lufs, expected.integrated_lufs);
        assert_eq!(actual.loudness_range_lu, expected.loudness_range_lu);
        assert_eq!(actual.true_peak_dbtp, expected.true_peak_dbtp);
        assert_eq!(actual.duration_s, expected.duration_s);
    }
}

#[test]
fn streaming_master_matches_reference_within_tolerance() {
    let input = interleave(&tone(440.0, 2.0, 0.2), &tone(660.0, 2.0, 0.1));
    let profile = MasteringProfile::podcast();
    let reference = master(&input, SR, 2, &profile);

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("streamed.wav");
    let streamed = master_to_wav(&input, &path, SR, 2, &profile).expect("streaming master");

    assert!(
        (streamed.report.measurements.integrated_lufs
            - reference.report.measurements.integrated_lufs)
            .abs()
            < 0.5,
        "streamed {} LUFS vs reference {} LUFS",
        streamed.report.measurements.integrated_lufs,
        reference.report.measurements.integrated_lufs
    );
    assert!(
        (streamed.report.measurements.true_peak_dbtp
            - reference.report.measurements.true_peak_dbtp)
            .abs()
            < 0.5
    );

    let (samples, sample_rate, channels) = read_wav(&path).expect("read streamed wav");
    assert_eq!(sample_rate, SR);
    assert_eq!(channels, 2);
    assert_eq!(samples.len(), reference.samples.len());
    let max_diff = samples
        .iter()
        .zip(&reference.samples)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0f32, f32::max);
    assert!(max_diff < 0.02, "max sample difference {max_diff}");
}

#[test]
fn streaming_master_respects_true_peak_ceiling() {
    let loud = tone(1000.0, 2.0, 0.9);
    let profile = MasteringProfile::custom("loud", -10.0, -1.0);
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("streamed.wav");
    let outcome = master_to_wav(&loud, &path, SR, 1, &profile).expect("streaming master");
    assert!(
        outcome.report.measurements.true_peak_dbtp <= profile.max_true_peak_dbtp + 0.1,
        "true peak {} dBTP exceeds ceiling {}",
        outcome.report.measurements.true_peak_dbtp,
        profile.max_true_peak_dbtp
    );
}

#[test]
fn streaming_master_is_deterministic() {
    let input = interleave(&tone(440.0, 1.0, 0.2), &tone(660.0, 1.0, 0.1));
    let profile = MasteringProfile::podcast();
    let dir = tempfile::tempdir().expect("tempdir");
    let first_path = dir.path().join("first.wav");
    let second_path = dir.path().join("second.wav");
    let first = master_to_wav(&input, &first_path, SR, 2, &profile).expect("first");
    let second = master_to_wav(&input, &second_path, SR, 2, &profile).expect("second");
    assert_eq!(first.report.measurements, second.report.measurements);
    assert_eq!(
        std::fs::read(&first_path).expect("read first"),
        std::fs::read(&second_path).expect("read second")
    );
}

#[test]
fn streaming_master_handles_dynamic_material() {
    let mut input = Vec::new();
    for block in 0..4 {
        let amplitude = if block % 2 == 0 { 0.05 } else { 0.4 };
        input.extend(tone(220.0 + block as f32 * 110.0, 1.0, amplitude));
    }
    let profile = MasteringProfile::ebu_r128();
    let reference = master(&input, SR, 1, &profile);
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("dynamic.wav");
    let streamed = master_to_wav(&input, &path, SR, 1, &profile).expect("streaming master");
    assert!(
        (streamed.report.measurements.integrated_lufs
            - reference.report.measurements.integrated_lufs)
            .abs()
            < 0.5,
        "streamed {} LUFS vs reference {} LUFS",
        streamed.report.measurements.integrated_lufs,
        reference.report.measurements.integrated_lufs
    );
    assert!(
        (streamed.report.measurements.true_peak_dbtp
            - reference.report.measurements.true_peak_dbtp)
            .abs()
            < 0.5
    );
}

fn write_f32_file(path: &std::path::Path, samples: &[f32]) {
    let mut bytes = Vec::with_capacity(samples.len() * 4);
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    std::fs::write(path, bytes).expect("write f32 file");
}

#[test]
fn file_backed_master_matches_slice_master() {
    let input = interleave(&tone(440.0, 2.0, 0.2), &tone(660.0, 2.0, 0.1));
    let profile = MasteringProfile::podcast();
    let dir = tempfile::tempdir().expect("tempdir");

    let slice_path = dir.path().join("slice.wav");
    let slice = master_to_wav(&input, &slice_path, SR, 2, &profile).expect("slice master");

    let source_path = dir.path().join("source.f32");
    write_f32_file(&source_path, &input);
    let file_path = dir.path().join("file.wav");
    let file = master_file_to_wav(&source_path, &file_path, SR, 2, &profile).expect("file master");

    assert_eq!(
        slice.report.measurements, file.report.measurements,
        "file-backed mastering must measure identically"
    );
    assert_eq!(slice.applied_gain_db, file.applied_gain_db);
    assert_eq!(slice.limiting_reduction_db, file.limiting_reduction_db);
    assert_eq!(
        std::fs::read(&slice_path).expect("read slice"),
        std::fs::read(&file_path).expect("read file"),
        "file-backed mastering must produce identical bytes"
    );
}

#[test]
fn file_backed_master_handles_empty_source() {
    let dir = tempfile::tempdir().expect("tempdir");
    let source_path = dir.path().join("empty.f32");
    write_f32_file(&source_path, &[]);
    let output_path = dir.path().join("out.wav");
    let outcome = master_file_to_wav(
        &source_path,
        &output_path,
        SR,
        1,
        &MasteringProfile::ebu_r128(),
    )
    .expect("empty master");
    assert_eq!(outcome.report.measurements.duration_s, 0.0);
    assert!(output_path.exists());
}

#[test]
fn ebu_r128_warns_outside_half_lu_and_fails_outside_one_lu() {
    let signal = tone(1000.0, 3.0, 0.05);
    let measured = validate(&signal, SR, 1, &MasteringProfile::ebu_r128())
        .measurements
        .integrated_lufs;
    let status_at = |offset: f32| {
        let mut profile = MasteringProfile::ebu_r128();
        profile.target_lufs = measured + offset;
        profile.min_lufs = profile.target_lufs - 1.0;
        profile.max_lufs = profile.target_lufs + 1.0;
        validate(&signal, SR, 1, &profile)
            .checks
            .iter()
            .find(|c| c.id == "integrated_loudness")
            .map(|c| c.status)
    };
    assert_eq!(status_at(0.2), Some(CheckStatus::Pass));
    assert_eq!(status_at(0.7), Some(CheckStatus::Warn));
    assert_eq!(status_at(1.5), Some(CheckStatus::Fail));
}
