use assert_cmd::Command;
use predicates::str::contains;
use std::f32::consts::PI;
use tempfile::tempdir;

use movie_nonvoice_timeline::learning::profiles::CalibrationProfile;

fn gen_wav_with_speech(path: &std::path::Path, speech_start_ms: u32, speech_end_ms: u32) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec).unwrap_or_else(|_| panic!("create wav"));
    let total_samples = 32000;
    let speech_start = (speech_start_ms * 16) as usize;
    let speech_end = (speech_end_ms * 16) as usize;
    for i in 0..total_samples {
        let t = i as f32 / 16000.0;
        let sample = if (speech_start..speech_end).contains(&i) {
            (2.0 * PI * 220.0 * t).sin() * 0.3
        } else {
            0.001
        };
        let v = (sample * i16::MAX as f32) as i16;
        writer.write_sample(v).unwrap_or_default();
    }
    writer.finalize().unwrap_or_default();
}

#[test]
fn calibration_profile_changes_output() {
    let d = tempdir().unwrap_or_else(|_| panic!("tmpdir"));
    let wav = d.path().join("test.wav");
    let out_default = d.path().join("default.json");
    let out_calibrated = d.path().join("calibrated.json");
    let profile = d.path().join("profile.json");

    gen_wav_with_speech(&wav, 500, 1000);

    let default_profile = CalibrationProfile {
        name: "test".to_string(),
        energy_threshold_delta: 0.0,
        version: 1,
    };
    std::fs::write(
        &profile,
        serde_json::to_string_pretty(&default_profile).unwrap(),
    )
    .unwrap();

    Command::cargo_bin("timeline")
        .unwrap_or_else(|_| panic!("bin"))
        .args([
            "extract",
            wav.to_str().unwrap_or_default(),
            "--output",
            out_default.to_str().unwrap_or_default(),
        ])
        .assert()
        .success();

    Command::cargo_bin("timeline")
        .unwrap_or_else(|_| panic!("bin"))
        .args([
            "extract",
            wav.to_str().unwrap_or_default(),
            "--output",
            out_calibrated.to_str().unwrap_or_default(),
            "--calibration-profile",
            profile.to_str().unwrap_or_default(),
        ])
        .assert()
        .success();

    let default_out = std::fs::read_to_string(&out_default).unwrap();
    let calibrated_out = std::fs::read_to_string(&out_calibrated).unwrap();
    assert_eq!(
        default_out, calibrated_out,
        "Zero delta should produce same output"
    );
}

#[test]
fn positive_delta_reduces_speech_detection() {
    let d = tempdir().unwrap_or_else(|_| panic!("tmpdir"));
    let wav = d.path().join("test.wav");
    let out_low = d.path().join("low.json");
    let out_high = d.path().join("high.json");
    let profile_low = d.path().join("low.json");
    let profile_high = d.path().join("high.json");

    gen_wav_with_speech(&wav, 500, 1000);

    let low_profile = CalibrationProfile {
        name: "low".to_string(),
        energy_threshold_delta: -0.005,
        version: 1,
    };
    let high_profile = CalibrationProfile {
        name: "high".to_string(),
        energy_threshold_delta: 0.005,
        version: 1,
    };

    std::fs::write(
        &profile_low,
        serde_json::to_string_pretty(&low_profile).unwrap(),
    )
    .unwrap();
    std::fs::write(
        &profile_high,
        serde_json::to_string_pretty(&high_profile).unwrap(),
    )
    .unwrap();

    Command::cargo_bin("timeline")
        .unwrap_or_else(|_| panic!("bin"))
        .args([
            "extract",
            wav.to_str().unwrap_or_default(),
            "--output",
            out_low.to_str().unwrap_or_default(),
            "--calibration-profile",
            profile_low.to_str().unwrap_or_default(),
        ])
        .assert()
        .success();

    Command::cargo_bin("timeline")
        .unwrap_or_else(|_| panic!("bin"))
        .args([
            "extract",
            wav.to_str().unwrap_or_default(),
            "--output",
            out_high.to_str().unwrap_or_default(),
            "--calibration-profile",
            profile_high.to_str().unwrap_or_default(),
        ])
        .assert()
        .success();

    let low_out: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&out_low).unwrap()).unwrap();
    let high_out: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&out_high).unwrap()).unwrap();

    let low_segments = &low_out["segments"];
    let high_segments = &high_out["segments"];

    let low_count = if let Some(arr) = low_segments.as_array() {
        arr.len()
    } else {
        0
    };
    let high_count = if let Some(arr) = high_segments.as_array() {
        arr.len()
    } else {
        0
    };

    assert!(
        low_count >= high_count,
        "Lower threshold (more sensitive) should detect same or more speech => fewer non-voice segments; got low={low_count}, high={high_count}"
    );
}

#[test]
fn invalid_calibration_profile_errors() {
    let d = tempdir().unwrap_or_else(|_| panic!("tmpdir"));
    let wav = d.path().join("test.wav");
    let out = d.path().join("out.json");
    let bad_profile = d.path().join("bad.json");

    gen_wav_with_speech(&wav, 500, 1000);
    std::fs::write(&bad_profile, "not-valid-json").unwrap();

    Command::cargo_bin("timeline")
        .unwrap_or_else(|_| panic!("bin"))
        .args([
            "extract",
            wav.to_str().unwrap_or_default(),
            "--output",
            out.to_str().unwrap_or_default(),
            "--calibration-profile",
            bad_profile.to_str().unwrap_or_default(),
        ])
        .assert()
        .failure()
        .stderr(contains("failed to read calibration profile"));
}

#[test]
fn save_calibration_writes_profile() {
    let d = tempdir().unwrap_or_else(|_| panic!("tmpdir"));
    let wav = d.path().join("test.wav");
    let out = d.path().join("out.json");

    gen_wav_with_speech(&wav, 500, 1000);

    let temp_home = d.path().join("home");
    std::fs::create_dir_all(&temp_home).unwrap();

    Command::cargo_bin("timeline")
        .unwrap_or_else(|_| panic!("bin"))
        .args([
            "extract",
            wav.to_str().unwrap_or_default(),
            "--output",
            out.to_str().unwrap_or_default(),
            "--save-calibration",
        ])
        .env("HOME", temp_home.to_str().unwrap())
        .env_remove("XDG_CONFIG_HOME")
        .assert()
        .success();

    let expected_path = temp_home
        .join(".config")
        .join("do-movie-radio-play")
        .join("profiles")
        .join("latest.json");

    assert!(
        expected_path.exists(),
        "Calibration profile should be saved to {expected_path:?}"
    );

    let content = std::fs::read_to_string(&expected_path).unwrap();
    let saved: CalibrationProfile = serde_json::from_str(&content).unwrap();
    assert_eq!(saved.name, "runtime");
    assert_eq!(saved.version, 1);
}

#[test]
fn apply_calibration_converts_report() {
    let d = tempdir().unwrap_or_else(|_| panic!("tmpdir"));
    let report_path = d.path().join("report.json");
    let output_profile = d.path().join("applied.json");
    let report = serde_json::json!({
        "version": 1,
        "profile": "drama",
        "records_seen": 10,
        "speech_to_non_voice": 2,
        "non_voice_to_speech": 3,
        "recommended_energy_threshold_delta": -0.0025
    });
    std::fs::write(&report_path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();

    Command::cargo_bin("timeline")
        .unwrap_or_else(|_| panic!("bin"))
        .args([
            "apply-calibration",
            "--report",
            report_path.to_str().unwrap_or_default(),
            "--output",
            output_profile.to_str().unwrap_or_default(),
        ])
        .assert()
        .success();

    let applied: CalibrationProfile =
        serde_json::from_slice(&std::fs::read(&output_profile).unwrap()).unwrap();
    assert_eq!(applied.version, 2);
    assert!(applied.name.contains("drama"));
    assert_eq!(applied.energy_threshold_delta, -0.0025);
}
