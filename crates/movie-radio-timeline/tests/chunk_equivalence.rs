//! Chunked streaming extraction must agree with whole-file extraction.
//!
//! `pipeline/chunked.rs` streams decode and VAD but runs segmentation, merging
//! and inversion once over the accumulated per-frame state, so the result must
//! not depend on `chunk_duration_sec`. This guards against re-introducing
//! boundary-dependent output (see #418).

use std::path::Path;

use assert_cmd::Command;

/// Stereo 44.1 kHz so the resampling path runs, with alternating vowel-like
/// and near-silent 15 s blocks so the fixture yields several non-voice
/// segments instead of one whole-file span.
const SAMPLE_RATE: u32 = 44_100;
const CHANNELS: u16 = 2;
const BLOCK_SECONDS: usize = 15;
const BLOCKS: usize = 8;

fn write_fixture(path: &Path) {
    let mut samples = Vec::new();
    for block in 0..BLOCKS {
        let speech_like = block % 2 == 0;
        for i in 0..(SAMPLE_RATE as usize * BLOCK_SECONDS) {
            let t = i as f32 / SAMPLE_RATE as f32;
            let value = if speech_like {
                let env = 0.5 + 0.5 * (2.0 * std::f32::consts::PI * 4.0 * t).sin();
                0.12 * env
                    * ((2.0 * std::f32::consts::PI * 150.0 * t).sin() * 0.5
                        + (2.0 * std::f32::consts::PI * 900.0 * t).sin() * 0.35
                        + (2.0 * std::f32::consts::PI * 2400.0 * t).sin() * 0.2)
            } else {
                (2.0 * std::f32::consts::PI * 50.0 * t).sin() * 0.0005
            };
            for _ in 0..CHANNELS {
                samples.push(value);
            }
        }
    }

    let data_len = (samples.len() * 2) as u32;
    let mut bytes = Vec::with_capacity(44 + data_len as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&CHANNELS.to_le_bytes());
    bytes.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    bytes.extend_from_slice(&(SAMPLE_RATE * u32::from(CHANNELS) * 2).to_le_bytes());
    bytes.extend_from_slice(&(CHANNELS * 2).to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        let value = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    std::fs::write(path, bytes).expect("write fixture wav");
}

fn extract(media: &Path, out: &Path, engine: &str, chunk: Option<u32>) -> Vec<(u64, u64)> {
    let mut command = Command::cargo_bin("timeline").expect("timeline binary");
    command.args([
        "extract",
        media.to_str().expect("media path"),
        "--output",
        out.to_str().expect("output path"),
        "--vad-engine",
        engine,
    ]);
    if let Some(seconds) = chunk {
        command.args(["--chunk-duration", &seconds.to_string()]);
    }
    command.assert().success();

    let document: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out).expect("read output"))
            .expect("output is JSON");
    document["segments"]
        .as_array()
        .expect("segments array")
        .iter()
        .map(|segment| {
            (
                segment["start_ms"].as_u64().expect("start_ms"),
                segment["end_ms"].as_u64().expect("end_ms"),
            )
        })
        .collect()
}

#[test]
fn chunked_extraction_matches_whole_file_extraction() {
    let dir = tempfile::tempdir().expect("tempdir");
    let media = dir.path().join("fixture.wav");
    write_fixture(&media);

    // `spectral` additionally adapts its thresholds from every frame, so this
    // also guards the whole-file percentile pass in the streaming path.
    for engine in ["energy", "spectral", "hybrid"] {
        let whole = extract(
            &media,
            &dir.path().join(format!("whole-{engine}.json")),
            engine,
            None,
        );
        assert!(
            whole.len() >= 4,
            "fixture should yield several non-voice segments, got {}",
            whole.len()
        );

        for chunk_seconds in [20, 35, 60] {
            let chunked = extract(
                &media,
                &dir.path()
                    .join(format!("chunk-{engine}-{chunk_seconds}.json")),
                engine,
                Some(chunk_seconds),
            );
            assert_eq!(
                whole, chunked,
                "{engine} with chunk-duration {chunk_seconds}s changed the segmentation; \
                 streaming must not depend on chunk size"
            );
        }
    }
}
