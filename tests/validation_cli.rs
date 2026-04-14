use assert_cmd::Command;
use predicates::str::contains;
use std::fs;
use std::path::Path;
use tempfile::tempdir;

fn preferred_validation_media() -> Option<(&'static str, &'static str, &'static str)> {
    if Path::new("testdata/raw/the_hole_1962.mp4").exists()
        && Path::new("testdata/raw/the_hole_1962.srt").exists()
    {
        return Some((
            "testdata/raw/the_hole_1962.mp4",
            "testdata/raw/the_hole_1962.srt",
            "937900",
        ));
    }
    if Path::new("testdata/raw/the_singing_fool_1928.webm").exists()
        && Path::new("testdata/raw/the_singing_fool_1928.srt").exists()
    {
        return Some((
            "testdata/raw/the_singing_fool_1928.webm",
            "testdata/raw/the_singing_fool_1928.srt",
            "6151000",
        ));
    }
    None
}

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
    if let Some((media, subtitles, total_ms)) = preferred_validation_media() {
        let d = tempdir().unwrap_or_else(|_| panic!("tmpdir"));
        let report = d.path().join("report.json");

        Command::cargo_bin("timeline")
            .unwrap_or_else(|_| panic!("bin"))
            .args([
                "validate",
                media,
                "--subtitles",
                subtitles,
                "--total-ms",
                total_ms,
                "--output",
                report.to_str().unwrap_or_default(),
            ])
            .assert()
            .success();

        assert!(report.exists());
        return;
    }

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
    let config = d.path().join("bad-config.json");
    let report = d.path().join("report.json");

    write_invalid_config(&config);
    let (input_media, truth) = if Path::new("testdata/raw/the_hole_1962.mp4").exists()
        && Path::new("testdata/validation/the_hole_1962.json").exists()
    {
        (
            "testdata/raw/the_hole_1962.mp4".to_string(),
            "testdata/validation/the_hole_1962.json".to_string(),
        )
    } else {
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
        (
            fixtures
                .join("alternating.wav")
                .to_string_lossy()
                .to_string(),
            fixtures
                .join("alternating.truth.json")
                .to_string_lossy()
                .to_string(),
        )
    };

    Command::cargo_bin("timeline")
        .unwrap_or_else(|_| panic!("bin"))
        .args([
            "validate",
            &input_media,
            "--config",
            config.to_str().unwrap_or_default(),
            "--truth-json",
            &truth,
            "--profile",
            "synthetic",
            "--output",
            report.to_str().unwrap_or_default(),
        ])
        .assert()
        .failure();
}

#[test]
fn validate_with_dataset_manifest_rejects_bad_rows() {
    let d = tempdir().unwrap_or_else(|_| panic!("tmpdir"));
    let wav = d.path().join("in.wav");
    let manifest = d.path().join("speech.csv");
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

    fs::write(&manifest, "start_ms,end_ms\nabc,400\n").unwrap_or_default();

    Command::cargo_bin("timeline")
        .unwrap_or_else(|_| panic!("bin"))
        .args([
            "validate",
            wav.to_str().unwrap_or_default(),
            "--dataset-manifest",
            manifest.to_str().unwrap_or_default(),
            "--total-ms",
            "1000",
            "--output",
            report.to_str().unwrap_or_default(),
        ])
        .assert()
        .failure()
        .stderr(contains("invalid start_ms at line 2"));
}
