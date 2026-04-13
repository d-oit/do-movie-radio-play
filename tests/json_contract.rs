use assert_cmd::Command;
use jsonschema::JSONSchema;
use serde_json::Value;
use std::f32::consts::PI;
use tempfile::tempdir;

fn gen_wav(path: &std::path::Path) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec).unwrap();
    for i in 0..32000 {
        let t = i as f32 / 16000.0;
        let sample = if (0.2..0.5).contains(&t) {
            (2.0 * PI * 180.0 * t).sin() * 0.4
        } else {
            0.0
        };
        let v = (sample * i16::MAX as f32) as i16;
        writer.write_sample(v).unwrap();
    }
    writer.finalize().unwrap();
}

#[test]
fn extract_output_matches_schema() {
    let d = tempdir().unwrap();
    let wav = d.path().join("input.wav");
    let output = d.path().join("timeline.json");
    gen_wav(&wav);

    Command::cargo_bin("timeline")
        .unwrap()
        .args([
            "extract",
            wav.to_str().unwrap_or_default(),
            "--output",
            output.to_str().unwrap_or_default(),
        ])
        .assert()
        .success();

    let schema_value: Value =
        serde_json::from_str(include_str!("../schema/timeline.schema.json")).unwrap();
    let compiled = JSONSchema::compile(&schema_value).expect("schema compiles");
    let data: Value = serde_json::from_slice(&std::fs::read(&output).unwrap()).unwrap();
    if let Err(errors) = compiled.validate(&data) {
        let messages: Vec<String> = errors.map(|e| e.to_string()).collect();
        panic!("schema validation failed: {messages:?}");
    };
}
