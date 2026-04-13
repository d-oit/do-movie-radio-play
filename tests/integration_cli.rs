use assert_cmd::Command;
use predicates::str::contains;
use std::f32::consts::PI;
use tempfile::tempdir;

fn gen_wav(path: &std::path::Path) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec).unwrap_or_else(|_| panic!("create wav"));
    for i in 0..32000 {
        let t = i as f32 / 16000.0;
        let sample = if (0.5..1.0).contains(&t) {
            (2.0 * PI * 220.0 * t).sin() * 0.3
        } else {
            0.0
        };
        let v = (sample * i16::MAX as f32) as i16;
        writer.write_sample(v).unwrap_or_default();
    }
    writer.finalize().unwrap_or_default();
}

#[test]
fn extract_tag_prompt_flow() {
    let d = tempdir().unwrap_or_else(|_| panic!("tmpdir"));
    let wav = d.path().join("in.wav");
    let out = d.path().join("segments.json");
    let tagged = d.path().join("tagged.json");
    let prompted = d.path().join("prompted.json");
    gen_wav(&wav);

    Command::cargo_bin("timeline")
        .unwrap_or_else(|_| panic!("bin"))
        .args([
            "extract",
            wav.to_str().unwrap_or_default(),
            "--output",
            out.to_str().unwrap_or_default(),
        ])
        .assert()
        .success();

    Command::cargo_bin("timeline")
        .unwrap_or_else(|_| panic!("bin"))
        .args([
            "tag",
            wav.to_str().unwrap_or_default(),
            "--input",
            out.to_str().unwrap_or_default(),
            "--output",
            tagged.to_str().unwrap_or_default(),
        ])
        .assert()
        .success();

    Command::cargo_bin("timeline")
        .unwrap_or_else(|_| panic!("bin"))
        .args([
            "prompt",
            tagged.to_str().unwrap_or_default(),
            "--output",
            prompted.to_str().unwrap_or_default(),
        ])
        .assert()
        .success();
}

#[test]
fn missing_file_errors() {
    Command::cargo_bin("timeline")
        .unwrap_or_else(|_| panic!("bin"))
        .args(["extract", "missing.wav", "--output", "x.json"])
        .assert()
        .failure()
        .stderr(contains("input file does not exist"));
}

#[test]
fn malformed_corrections_json_errors() {
    let d = tempdir().unwrap_or_else(|_| panic!("tmpdir"));
    let corr = d.path().join("bad.json");
    std::fs::write(&corr, "not-json").unwrap_or_default();
    Command::cargo_bin("timeline")
        .unwrap_or_else(|_| panic!("bin"))
        .args([
            "calibrate",
            d.path().to_str().unwrap_or_default(),
            "--profile",
            "drama",
        ])
        .assert()
        .failure()
        .stderr(contains("malformed corrections JSON"));
}

#[test]
fn repeated_extract_is_deterministic() {
    let d = tempdir().unwrap_or_else(|_| panic!("tmpdir"));
    let wav = d.path().join("in.wav");
    let out1 = d.path().join("a.json");
    let out2 = d.path().join("b.json");
    gen_wav(&wav);
    for out in [&out1, &out2] {
        Command::cargo_bin("timeline")
            .unwrap_or_else(|_| panic!("bin"))
            .args([
                "extract",
                wav.to_str().unwrap_or_default(),
                "--output",
                out.to_str().unwrap_or_default(),
            ])
            .assert()
            .success();
    }
    let a = std::fs::read(&out1).unwrap_or_default();
    let b = std::fs::read(&out2).unwrap_or_default();
    assert_eq!(a, b);
}

#[test]
fn unsupported_vad_engine_errors_clearly() {
    let d = tempdir().unwrap_or_else(|_| panic!("tmpdir"));
    let wav = d.path().join("in.wav");
    let out = d.path().join("segments.json");
    gen_wav(&wav);

    Command::cargo_bin("timeline")
        .unwrap_or_else(|_| panic!("bin"))
        .args([
            "extract",
            wav.to_str().unwrap_or_default(),
            "--vad-engine",
            "webrtc",
            "--output",
            out.to_str().unwrap_or_default(),
        ])
        .assert()
        .failure()
        .stderr(contains("VAD engine 'webrtc' is not implemented"));
}

#[test]
fn float_wav_falls_back_to_ffmpeg() {
    let ffmpeg_available = std::process::Command::new("ffmpeg")
        .arg("-version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);
    if !ffmpeg_available {
        return;
    }

    let d = tempdir().unwrap_or_else(|_| panic!("tmpdir"));
    let wav = d.path().join("float.wav");
    let out = d.path().join("segments.json");
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16000,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(&wav, spec).unwrap_or_else(|_| panic!("create wav"));
    for i in 0..16000 {
        let t = i as f32 / 16000.0;
        let sample = if (0.25..0.75).contains(&t) {
            (2.0 * PI * 220.0 * t).sin() * 0.25
        } else {
            0.0
        };
        writer.write_sample(sample).unwrap_or_default();
    }
    writer.finalize().unwrap_or_default();

    Command::cargo_bin("timeline")
        .unwrap_or_else(|_| panic!("bin"))
        .args([
            "extract",
            wav.to_str().unwrap_or_default(),
            "--output",
            out.to_str().unwrap_or_default(),
        ])
        .assert()
        .success();

    assert!(out.exists());
}

#[test]
fn invalid_env_override_errors_clearly() {
    let d = tempdir().unwrap_or_else(|_| panic!("tmpdir"));
    let wav = d.path().join("in.wav");
    let out = d.path().join("segments.json");
    gen_wav(&wav);

    Command::cargo_bin("timeline")
        .unwrap_or_else(|_| panic!("bin"))
        .env("TIMELINE_SAMPLE_RATE", "not-a-number")
        .args([
            "extract",
            wav.to_str().unwrap_or_default(),
            "--output",
            out.to_str().unwrap_or_default(),
        ])
        .assert()
        .failure()
        .stderr(contains("invalid env var TIMELINE_SAMPLE_RATE"));
}
