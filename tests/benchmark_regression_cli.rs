use assert_cmd::prelude::*;
use predicates::str::contains;
use std::fs;
use std::process::Command;
use tempfile::tempdir;

fn python3_available() -> bool {
    Command::new("python3")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn write_benchmark_json(path: &std::path::Path, total_ms: u64, decode_ms: u64, frame_ms: u64) {
    fs::write(
        path,
        format!(
            concat!(
                "{{\n",
                "  \"input_file\": \"testdata/raw/eggs_1970.mp4\",\n",
                "  \"total_ms\": {total_ms},\n",
                "  \"decode_ms\": {decode_ms},\n",
                "  \"frame_count\": 30480,\n",
                "  \"segment_count\": 0,\n",
                "  \"stage_ms\": {{\n",
                "    \"decode_ms\": {decode_ms},\n",
                "    \"resample_ms\": 24,\n",
                "    \"frame_ms\": {frame_ms},\n",
                "    \"vad_ms\": 3,\n",
                "    \"smooth_ms\": 2,\n",
                "    \"speech_ms\": 0,\n",
                "    \"merge_ms\": 0,\n",
                "    \"invert_ms\": 0\n",
                "  }}\n",
                "}}\n"
            ),
            total_ms = total_ms,
            decode_ms = decode_ms,
            frame_ms = frame_ms,
        ),
    )
    .unwrap_or_else(|_| panic!("benchmark json"));
}

#[test]
fn benchmark_regression_check_passes_for_similar_candidate() {
    if !python3_available() {
        return;
    }

    let d = tempdir().unwrap_or_else(|_| panic!("tmpdir"));
    let baseline = d.path().join("baseline.json");
    let candidate = d.path().join("candidate.json");
    write_benchmark_json(&baseline, 5412, 2522, 2846);
    write_benchmark_json(&candidate, 6951, 3789, 3120);

    Command::new("python3")
        .args([
            "scripts/check_benchmark_regression.py",
            "--baseline",
            baseline.to_str().unwrap_or_default(),
            "--candidate",
            candidate.to_str().unwrap_or_default(),
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .assert()
        .success()
        .stdout(contains("Benchmark regression check passed"));
}

#[test]
fn benchmark_regression_check_fails_for_large_regression() {
    if !python3_available() {
        return;
    }

    let d = tempdir().unwrap_or_else(|_| panic!("tmpdir"));
    let baseline = d.path().join("baseline.json");
    let candidate = d.path().join("candidate.json");
    write_benchmark_json(&baseline, 5412, 2522, 2846);
    write_benchmark_json(&candidate, 9000, 5000, 3800);

    Command::new("python3")
        .args([
            "scripts/check_benchmark_regression.py",
            "--baseline",
            baseline.to_str().unwrap_or_default(),
            "--candidate",
            candidate.to_str().unwrap_or_default(),
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .assert()
        .failure()
        .stderr(contains("Benchmark regression check failed"));
}

#[test]
fn benchmark_regression_check_rejects_schema_mismatch() {
    if !python3_available() {
        return;
    }

    let d = tempdir().unwrap_or_else(|_| panic!("tmpdir"));
    let baseline = d.path().join("baseline.json");
    let candidate = d.path().join("candidate.json");
    write_benchmark_json(&baseline, 5412, 2522, 2846);
    fs::write(
        &candidate,
        "{\n  \"input_file\": \"testdata/raw/eggs_1970.mp4\",\n  \"total_ms\": 5412\n}\n",
    )
    .unwrap_or_else(|_| panic!("candidate json"));

    Command::new("python3")
        .args([
            "scripts/check_benchmark_regression.py",
            "--baseline",
            baseline.to_str().unwrap_or_default(),
            "--candidate",
            candidate.to_str().unwrap_or_default(),
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .assert()
        .failure()
        .stderr(contains("benchmark artifact error"));
}
