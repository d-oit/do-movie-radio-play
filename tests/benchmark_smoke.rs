use assert_cmd::Command;
use movie_nonvoice_timeline::types::BenchmarkResult;
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
fn benchmark_command_writes_output() {
    let d = tempdir().unwrap_or_else(|_| panic!("tmpdir"));
    let wav = d.path().join("bench.wav");
    let out = d.path().join("bench.json");
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

    Command::cargo_bin("timeline")
        .unwrap_or_else(|_| panic!("bin"))
        .args([
            "bench",
            wav.to_str().unwrap_or_default(),
            "--output",
            out.to_str().unwrap_or_default(),
        ])
        .assert()
        .success();

    assert!(out.exists());

    let bytes = fs::read(&out).unwrap_or_else(|_| panic!("read"));
    let result: BenchmarkResult = serde_json::from_slice(&bytes).unwrap_or_else(|_| panic!("json"));
    assert_eq!(result.stage_ms.decode_ms, result.decode_ms);
    let stage_total = result.stage_ms.decode_ms
        + result.stage_ms.resample_ms
        + result.stage_ms.frame_ms
        + result.stage_ms.vad_ms
        + result.stage_ms.smooth_ms
        + result.stage_ms.speech_ms
        + result.stage_ms.merge_ms
        + result.stage_ms.invert_ms;
    assert!(stage_total <= result.total_ms + 1);
}

#[test]
fn benchmark_command_honors_config_validation() {
    let d = tempdir().unwrap_or_else(|_| panic!("tmpdir"));
    let wav = d.path().join("bench.wav");
    let out = d.path().join("bench.json");
    let config = d.path().join("bad-config.json");
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
    write_invalid_config(&config);

    Command::cargo_bin("timeline")
        .unwrap_or_else(|_| panic!("bin"))
        .args([
            "bench",
            wav.to_str().unwrap_or_default(),
            "--config",
            config.to_str().unwrap_or_default(),
            "--output",
            out.to_str().unwrap_or_default(),
        ])
        .assert()
        .failure();
}
