//! `timeline report` must inspect, validate and re-generate a run manifest.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use assert_cmd::Command;
use movie_radio_goap::production::{
    stage_by_name, write_run_metadata, Checkpoint, InputIdentity, RunMetadata,
};

fn touch(dir: &Path, name: &str, bytes: &[u8]) -> String {
    let path = dir.join(name);
    fs::write(&path, bytes).expect("write artifact");
    path.to_string_lossy().to_string()
}

fn synthetic_run(dir: &Path) {
    let input_path = dir.join("movie.mkv");
    fs::write(&input_path, b"movie").expect("write input");
    let input = InputIdentity::capture(&input_path).expect("capture input");
    let mut checkpoint = Checkpoint::fresh(input.clone(), BTreeMap::new());
    let records: Vec<(&str, Vec<(&str, String)>)> = vec![
        ("decode_audio", vec![("decoded_audio", touch(dir, "decoded.wav", b"RIFF"))]),
        ("analyze_speech", vec![("timeline", touch(dir, "timeline.json", b"{}"))]),
        ("identify_gaps", vec![("gaps", touch(dir, "gaps.json", b"{}"))]),
        ("plan_narration", vec![("scripts", touch(dir, "scripts.json", b"[]"))]),
        (
            "synthesize_narration",
            vec![
                ("narration_0000", touch(dir, "narration_0000.wav", b"RIFF")),
                (
                    "narration_index",
                    touch(
                        dir,
                        "narration.json",
                        br#"{"entries":[{"index":0,"file":"narration_0000.wav","provider":"kokoro","sample_rate_hz":24000}]}"#,
                    ),
                ),
            ],
        ),
        (
            "assemble_radio_play",
            vec![
                ("deliverable", touch(dir, "movie.radio-play.wav", b"RIFF")),
                ("assembled_mix", touch(dir, "assembled-mix.wav", b"RIFF")),
            ],
        ),
    ];
    for (name, artifacts) in records {
        let stage = stage_by_name(name).expect("known stage");
        checkpoint.record(
            stage,
            artifacts
                .into_iter()
                .map(|(key, path)| (key.to_string(), path))
                .collect(),
            None,
        );
    }
    checkpoint
        .save(&dir.join("checkpoint.json"))
        .expect("save checkpoint");
    let metadata = RunMetadata {
        schema_version: 1,
        run_id: "cli-run".to_string(),
        created_unix_ms: 1,
        input,
        config_hash: "abc".to_string(),
        vad_engine: "energy".to_string(),
        vad_model: String::new(),
        cue_source: "none".to_string(),
        language: "de".to_string(),
        narrator_provider: "kokoro".to_string(),
        tts_fallback_chain: vec!["kokoro".to_string()],
        tts_models: BTreeMap::new(),
        sfx_providers: Vec::new(),
        sfx_assets: Vec::new(),
        mastering_profile: "radio".to_string(),
        goal_stages: vec![
            "decode_audio",
            "analyze_speech",
            "identify_gaps",
            "plan_narration",
            "synthesize_narration",
            "assemble_radio_play",
        ]
        .into_iter()
        .map(str::to_string)
        .collect(),
        max_cost_per_run_usd: 0.0,
    };
    write_run_metadata(dir, &metadata).expect("write metadata");
}

#[test]
fn report_writes_a_valid_manifest_and_html() {
    let dir = tempfile::tempdir().expect("tempdir");
    synthetic_run(dir.path());

    let output = Command::cargo_bin("timeline")
        .expect("binary")
        .env("RUST_LOG", "error")
        .args(["report", dir.path().to_str().expect("path"), "--json"])
        .output()
        .expect("run report");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let manifest: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("manifest json");
    assert_eq!(manifest["schema_version"], 1);
    assert_eq!(manifest["status"], "complete");
    assert!(dir.path().join("manifest.json").exists());
    assert!(dir.path().join("report.html").exists());
}

#[test]
fn validate_rejects_a_tampered_manifest() {
    let dir = tempfile::tempdir().expect("tempdir");
    synthetic_run(dir.path());
    Command::cargo_bin("timeline")
        .expect("binary")
        .args(["report", dir.path().to_str().expect("path")])
        .assert()
        .success();

    let path = dir.path().join("manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).expect("read")).expect("parse");
    manifest["status"] = serde_json::json!("failed");
    fs::write(&path, serde_json::to_vec_pretty(&manifest).expect("json")).expect("write");

    Command::cargo_bin("timeline")
        .expect("binary")
        .args(["report", dir.path().to_str().expect("path"), "--validate"])
        .assert()
        .failure();
}

#[test]
fn report_rejects_a_directory_without_a_checkpoint() {
    let dir = tempfile::tempdir().expect("tempdir");
    Command::cargo_bin("timeline")
        .expect("binary")
        .args(["report", dir.path().to_str().expect("path")])
        .assert()
        .failure();
}
