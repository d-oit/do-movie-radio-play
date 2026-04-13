use assert_cmd::Command;
use std::fs;
use tempfile::tempdir;

fn write_invalid_config(path: &std::path::Path) {
    fs::write(
        path,
        r#"{
  "sample_rate_hz": 0,
  "frame_ms": 20,
  "speech_hangover_ms": 300,
  "merge_gap_ms": 250,
  "min_speech_ms": 120,
  "min_non_voice_ms": 1000,
  "energy_threshold": 0.015,
  "vad_threshold_delta": 0.0,
  "prompt_min_duration_ms": 2500,
  "prompt_min_confidence": 0.65,
  "vad_engine": "energy"
}"#,
    )
    .unwrap_or_else(|_| panic!("config"));
}

#[test]
fn generate_and_validate_synthetic_fixture() {
    let d = tempdir().unwrap_or_else(|_| panic!("tmpdir"));
    let fixtures = d.path().join("fixtures");
    Command::cargo_bin("timeline")
        .unwrap_or_else(|_| panic!("bin"))
        .args([
            "gen-fixtures",
            "--output-dir",
            fixtures.to_str().unwrap_or_default(),
        ])
        .assert()
        .success();

    let wav = fixtures.join("alternating.wav");
    let truth = fixtures.join("alternating.truth.json");
    let report = d.path().join("report.json");

    Command::cargo_bin("timeline")
        .unwrap_or_else(|_| panic!("bin"))
        .args([
            "validate",
            wav.to_str().unwrap_or_default(),
            "--truth-json",
            truth.to_str().unwrap_or_default(),
            "--profile",
            "synthetic",
            "--output",
            report.to_str().unwrap_or_default(),
        ])
        .assert()
        .success();

    assert!(report.exists());
}

#[test]
fn validate_with_subtitles() {
    let d = tempdir().unwrap_or_else(|_| panic!("tmpdir"));
    let wav = d.path().join("in.wav");
    let srt = d.path().join("a.srt");
    let report = d.path().join("report.json");

    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(&wav, spec).unwrap_or_else(|_| panic!("wav"));
    for _ in 0..16000 {
        writer.write_sample(0i16).unwrap_or_default();
    }
    writer.finalize().unwrap_or_default();

    std::fs::write(
        &srt,
        "1\n00:00:00,100 --> 00:00:00,300\nhello\n\n2\n00:00:00,400 --> 00:00:00,600\nworld\n",
    )
    .unwrap_or_default();

    Command::cargo_bin("timeline")
        .unwrap_or_else(|_| panic!("bin"))
        .args([
            "validate",
            wav.to_str().unwrap_or_default(),
            "--subtitles",
            srt.to_str().unwrap_or_default(),
            "--total-ms",
            "1000",
            "--output",
            report.to_str().unwrap_or_default(),
        ])
        .assert()
        .success();

    assert!(report.exists());
}

#[test]
fn validate_command_honors_config_validation() {
    let d = tempdir().unwrap_or_else(|_| panic!("tmpdir"));
    let fixtures = d.path().join("fixtures");
    let config = d.path().join("bad-config.json");
    let report = d.path().join("report.json");

    Command::cargo_bin("timeline")
        .unwrap_or_else(|_| panic!("bin"))
        .args([
            "gen-fixtures",
            "--output-dir",
            fixtures.to_str().unwrap_or_default(),
        ])
        .assert()
        .success();

    write_invalid_config(&config);

    let wav = fixtures.join("alternating.wav");
    let truth = fixtures.join("alternating.truth.json");

    Command::cargo_bin("timeline")
        .unwrap_or_else(|_| panic!("bin"))
        .args([
            "validate",
            wav.to_str().unwrap_or_default(),
            "--config",
            config.to_str().unwrap_or_default(),
            "--truth-json",
            truth.to_str().unwrap_or_default(),
            "--profile",
            "synthetic",
            "--output",
            report.to_str().unwrap_or_default(),
        ])
        .assert()
        .failure();
}
