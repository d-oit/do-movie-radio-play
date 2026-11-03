//! End-to-end CLI contract: repeated runs on identical input are byte-identical.
//!
//! `plans/040-validation/ACCEPTANCE.md` names this exact test, and the repository
//! rule requires deterministic output for identical inputs, but the test did not
//! exist. Timing is reported on stderr, never in the JSON, so a stage that starts
//! leaking wall-clock values into the document shows up here as a failure.

use std::path::Path;

use assert_cmd::Command;

const SAMPLE_RATE: u32 = 16_000;

fn write_fixture(path: &Path) {
    // 12 s of low-level noise, matching the repository's own `long_ambience`
    // fixture. That shape is known to yield a segment: the segmenter drops runs
    // shorter than its minimum duration, and digital silence is handled
    // differently from quiet ambience.
    let total_s = 12;
    let mut state: u32 = 42;
    let mut samples = Vec::new();
    for _ in 0..(SAMPLE_RATE * total_s) {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        samples.push(((state >> 8) as f32 / u32::MAX as f32 - 0.5) * 0.2 * 0.02);
    }

    let data_len = (samples.len() * 2) as u32;
    let mut bytes = Vec::with_capacity(44 + data_len as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    bytes.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        let value = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    std::fs::write(path, bytes).expect("write fixture wav");
}

fn extract_to(media: &Path, out: &Path) {
    Command::cargo_bin("timeline")
        .expect("timeline binary")
        .args([
            "extract",
            media.to_str().expect("media path"),
            "--output",
            out.to_str().expect("output path"),
            "--vad-engine",
            "energy",
        ])
        .assert()
        .success();
}

#[test]
fn repeated_extract_is_deterministic() {
    let dir = tempfile::tempdir().expect("tempdir");
    let media = dir.path().join("fixture.wav");
    write_fixture(&media);

    let first = dir.path().join("first.json");
    let second = dir.path().join("second.json");
    extract_to(&media, &first);
    extract_to(&media, &second);

    let a = std::fs::read(&first).expect("read first output");
    let b = std::fs::read(&second).expect("read second output");
    assert_eq!(
        a, b,
        "repeated extraction produced different bytes; timing or iteration order leaked into the output"
    );
    assert!(!a.is_empty(), "extract wrote an empty document");
}

#[test]
fn fixture_actually_produces_segments() {
    // Keeps the determinism test from passing vacuously: two identical empty
    // documents would satisfy it while hiding a pipeline that outputs nothing.
    let dir = tempfile::tempdir().expect("tempdir");
    let media = dir.path().join("fixture.wav");
    write_fixture(&media);

    let out = dir.path().join("out.json");
    extract_to(&media, &out);
    let document = std::fs::read_to_string(&out).expect("read output");
    assert!(
        !document.contains("\"segments\": []"),
        "fixture should yield at least one segment, got: {document}"
    );
}
