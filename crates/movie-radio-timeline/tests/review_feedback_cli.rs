//! End-to-end CLI contract for the human-in-the-loop review feedback workflow.

use assert_cmd::Command;
use serde_json::{json, Value};
use std::path::Path;

fn timeline() -> Command {
    let mut command = Command::cargo_bin("timeline").expect("timeline binary");
    // Keep stdout machine-readable: tracing defaults to stdout at INFO.
    command.env("RUST_LOG", "error");
    command
}

fn run_json(args: &[&str]) -> Value {
    let output = timeline()
        .args(args)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&output).expect("json output")
}

fn write_feedback(path: &Path, decisions: &str) {
    std::fs::write(
        path,
        format!(r#"{{"schema_version":1,"decisions":[{decisions}]}}"#),
    )
    .expect("write feedback");
}

fn decision_json(start: u64, end: u64, label: &str, source: &str) -> String {
    format!(
        r#"{{"run_id":"run-1","source_fingerprint":"{source}","gap_start_ms":{start},"gap_end_ms":{end},"film":"film-a","language":"de","genre":"drama","cue_source":"vad","evidence":{{"acoustic_tags":["music"],"confidence":0.7}},"narration":"Original.","tts_output":"/runs/run-1/seg.wav","assembly_position_ms":1000,"model_versions":{{"vad_engine":"energy","narrator_model":"narrator-v2","tts_provider":"kokoro","tts_model":"kokoro-v1"}},"label":"{label}","reviewer":"reviewer-1","source":"review-html"}}"#
    )
}

fn fnv1a(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    format!("{hash:016x}")
}

#[test]
fn review_feedback_workflow_round_trips() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db = dir.path().join("learning.db");
    let feedback = dir.path().join("feedback.json");
    let db_str = db.to_str().expect("db path");
    let feedback_str = feedback.to_str().expect("feedback path");

    write_feedback(
        &feedback,
        &format!(
            "{},{}",
            decision_json(1000, 2000, "reject", "src-abc"),
            decision_json(3000, 4000, "reject", "src-abc")
        ),
    );

    let imported = run_json(&[
        "review-feedback",
        "import",
        feedback_str,
        "--learning-db",
        db_str,
    ]);
    assert_eq!(imported["summary"]["decisions_imported"], json!(2));
    assert_eq!(imported["summary"]["events_recorded"], json!(2));

    let proposals = run_json(&["review-feedback", "proposals", "--learning-db", db_str]);
    let proposal = proposals["proposals"]
        .as_array()
        .expect("proposals array")
        .iter()
        .find(|proposal| proposal["parameter"] == "gap_confidence_threshold")
        .expect("gap proposal");
    let proposal_id = proposal["proposal_id"].as_str().expect("proposal id");
    assert_eq!(proposal["status"], "proposed");
    assert_eq!(proposal["origin"], "learned");

    // Applying a learned change requires explicit confirmation.
    timeline()
        .args([
            "review-feedback",
            "apply",
            "--proposal-id",
            proposal_id,
            "--learning-db",
            db_str,
        ])
        .assert()
        .failure();

    let applied = run_json(&[
        "review-feedback",
        "apply",
        "--proposal-id",
        proposal_id,
        "--confirm",
        "--learning-db",
        db_str,
    ]);
    assert_eq!(applied["applied"]["action"], "apply");
    assert_eq!(applied["applied"]["prior_value"], "0.50");

    let metrics = run_json(&["review-feedback", "metrics", "--learning-db", db_str]);
    let row = &metrics["metrics"][0];
    assert_eq!(row["film"], "film-a");
    assert_eq!(row["rejected"], json!(2));

    let rolled = run_json(&[
        "review-feedback",
        "rollback",
        "--proposal-id",
        proposal_id,
        "--confirm",
        "--learning-db",
        db_str,
    ]);
    assert_eq!(rolled["rolled_back"]["action"], "rollback");
    assert_eq!(rolled["rolled_back"]["new_value"], "0.50");

    let export_path = dir.path().join("export.json");
    timeline()
        .args([
            "review-feedback",
            "export",
            "--output",
            export_path.to_str().expect("export path"),
            "--learning-db",
            db_str,
        ])
        .assert()
        .success();
    let export: Value =
        serde_json::from_str(&std::fs::read_to_string(&export_path).expect("read export"))
            .expect("parse export");
    assert_eq!(export["decisions"][0]["tts_output"], "seg.wav");
}

#[test]
fn holdout_fixture_blocks_import() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db = dir.path().join("learning.db");
    let fixture = dir.path().join("holdout.wav");
    let feedback = dir.path().join("feedback.json");
    std::fs::write(&fixture, b"holdout-bytes").expect("write fixture");
    let checksum = fnv1a(b"holdout-bytes");
    let db_str = db.to_str().expect("db path");

    timeline()
        .args([
            "review-feedback",
            "register-holdout",
            "--fixture-id",
            "holdout-1",
            "--path",
            fixture.to_str().expect("fixture path"),
            "--learning-db",
            db_str,
        ])
        .assert()
        .success();

    write_feedback(&feedback, &decision_json(1000, 2000, "reject", &checksum));
    timeline()
        .args([
            "review-feedback",
            "import",
            feedback.to_str().expect("feedback path"),
            "--learning-db",
            db_str,
        ])
        .assert()
        .failure();
}
