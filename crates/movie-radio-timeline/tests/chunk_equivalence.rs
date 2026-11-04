//! Chunked streaming extraction must agree with whole-file extraction.
//!
//! `pipeline/chunked.rs` streams decode and VAD but runs segmentation, merging
//! and inversion once over the accumulated per-frame state, so the result must
//! not depend on `chunk_duration_sec`. This guards against re-introducing
//! boundary-dependent output (see #418).

use std::path::Path;

use assert_cmd::Command;

const SAMPLE_RATE: u32 = 16_000;

/// Alternating tone/noise blocks, long enough that at least one boundary lands
/// inside a run rather than between blocks.
fn write_fixture(path: &Path) {
    let mut samples = Vec::new();
    let mut state: u32 = 7;
    for block in 0..12 {
        let voiced = block % 2 == 0;
        for i in 0..(SAMPLE_RATE as usize * 10) {
            let value = if voiced {
                let t = i as f32 / SAMPLE_RATE as f32;
                (2.0 * std::f32::consts::PI * 220.0 * t).sin() * 0.3
            } else {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                ((state >> 8) as f32 / u32::MAX as f32 - 0.5) * 0.004
            };
            samples.push(value);
        }
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
            !whole.is_empty(),
            "fixture should produce non-voice segments"
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
