use super::build::{build_manifest, write_run_metadata, RunMetadata};
use super::*;
use crate::narrate::{NarrationEvidence, NarrationScript};
use crate::production::checkpoint::{Checkpoint, InputIdentity, StageRecord};
use crate::production::stage_by_name;
use movie_radio_verification::{
    ContentProvenance, GainDuckingOperation, TrackMap, TrackMapEntry, TrackType,
};
use movie_radio_voice::Emotion;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

fn touch(dir: &Path, name: &str, bytes: &[u8]) -> String {
    let path = dir.join(name);
    fs::write(&path, bytes).expect("write artifact");
    path.to_string_lossy().to_string()
}

fn input(dir: &Path) -> InputIdentity {
    let path = dir.join("movie.mkv");
    fs::write(&path, b"movie").expect("write input");
    InputIdentity::capture(&path).expect("capture input")
}

fn scripts() -> Vec<NarrationScript> {
    vec![NarrationScript {
        gap_start_ms: 1_000,
        gap_end_ms: 3_000,
        text: "A train crosses the empty platform.".to_string(),
        emotion: Emotion::Neutral,
        word_count: 7,
        duration_ms: 2_000,
        evidence: NarrationEvidence::default(),
    }]
}

fn track_map() -> TrackMap {
    TrackMap {
        entries: vec![
            TrackMapEntry {
                id: "narration_seg_0".to_string(),
                track_type: TrackType::Narration,
                source_start_ms: 1_050,
                source_end_ms: 2_900,
                output_start_ms: 1_050,
                output_end_ms: 2_900,
                operation: GainDuckingOperation::None,
                provenance: ContentProvenance::default(),
            },
            TrackMapEntry {
                id: "sfx_seg_0".to_string(),
                track_type: TrackType::Sfx,
                source_start_ms: 4_000,
                source_end_ms: 5_000,
                output_start_ms: 4_100,
                output_end_ms: 5_100,
                operation: GainDuckingOperation::None,
                provenance: ContentProvenance {
                    asset_id: "train_horn".to_string(),
                    source: "local".to_string(),
                    license: movie_radio_types::SfxLicense::Cc0,
                    prompt: None,
                },
            },
        ],
    }
}

fn complete_run(dir: &Path) -> RunMetadata {
    let input = input(dir);
    let mut checkpoint = Checkpoint::fresh(input.clone(), BTreeMap::new());
    let decoded = touch(dir, "decoded.wav", b"RIFF");
    let timeline = touch(dir, "timeline.json", br#"{"file":"movie.mkv"}"#);
    let gaps = touch(dir, "gaps.json", br#"{"file":"movie.mkv","gaps":[]}"#);
    let scripts_path = touch(
        dir,
        "scripts.json",
        &serde_json::to_vec(&scripts()).expect("scripts json"),
    );
    let narration = touch(dir, "narration_0000.wav", b"RIFF");
    let index = touch(
        dir,
        "narration.json",
        br#"{"entries":[{"index":0,"file":"narration_0000.wav","provider":"kokoro","sample_rate_hz":24000}]}"#,
    );
    let deliverable = touch(dir, "movie.radio-play.wav", b"RIFF");
    let mix = touch(dir, "assembled-mix.wav", b"RIFF");
    touch(
        dir,
        "track-map.json",
        &serde_json::to_vec(&track_map()).expect("track map json"),
    );

    let records: Vec<(&str, Vec<(&str, String)>)> = vec![
        ("decode_audio", vec![("decoded_audio", decoded)]),
        ("analyze_speech", vec![("timeline", timeline)]),
        ("identify_gaps", vec![("gaps", gaps)]),
        ("plan_narration", vec![("scripts", scripts_path)]),
        (
            "synthesize_narration",
            vec![("narration_0000", narration), ("narration_index", index)],
        ),
        (
            "assemble_radio_play",
            vec![("deliverable", deliverable), ("assembled_mix", mix)],
        ),
    ];
    for (name, artifacts) in records {
        let stage = stage_by_name(name).expect("known stage");
        let artifacts = artifacts
            .into_iter()
            .map(|(key, path)| (key.to_string(), path))
            .collect();
        checkpoint.record(stage, artifacts, None);
    }
    checkpoint
        .save(&dir.join("checkpoint.json"))
        .expect("save checkpoint");

    let metadata = RunMetadata {
        schema_version: MANIFEST_SCHEMA_VERSION,
        run_id: "run-416".to_string(),
        created_unix_ms: 1,
        input,
        config_hash: "c0ffee".to_string(),
        vad_engine: "spectral".to_string(),
        vad_model: String::new(),
        cue_source: "subtitles".to_string(),
        language: "de".to_string(),
        narrator_provider: "kokoro".to_string(),
        tts_fallback_chain: vec!["kokoro".to_string(), "openai".to_string()],
        tts_models: BTreeMap::from([("kokoro".to_string(), "martin".to_string())]),
        sfx_providers: vec!["local".to_string()],
        sfx_assets: vec!["train_horn".to_string()],
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
        max_cost_per_run_usd: 25.0,
    };
    write_run_metadata(dir, &metadata).expect("write metadata");
    metadata
}

#[test]
fn complete_run_reports_stages_providers_and_links() {
    let dir = tempfile::tempdir().expect("tempdir");
    complete_run(dir.path());

    let manifest = build_manifest(dir.path()).expect("build manifest");
    manifest.validate().expect("valid manifest");
    assert_eq!(manifest.status, RunStatus::Complete);
    assert_eq!(manifest.providers.vad_engine.as_deref(), Some("spectral"));
    assert_eq!(
        manifest.providers.narrator_provider.as_deref(),
        Some("kokoro")
    );
    assert_eq!(manifest.narration.len(), 1);
    assert_eq!(manifest.narration[0].output_start_ms, Some(1_050));
    assert_eq!(manifest.narration[0].provider.as_deref(), Some("kokoro"));
    assert_eq!(manifest.sfx.len(), 1);
    assert_eq!(manifest.sfx[0].output_start_ms, 4_100);
    assert_eq!(manifest.usage.narration_requests, 1);
    assert_eq!(manifest.usage.narration_succeeded, 1);
    assert!(manifest.artifacts.iter().all(|artifact| artifact.present));
    assert!(manifest
        .artifacts
        .iter()
        .all(|artifact| artifact.checksum.is_some()));
    assert!(manifest
        .quality_gates
        .iter()
        .any(|gate| gate.name == "artifacts" && gate.status == GateStatus::Pass));
}

#[test]
fn a_missing_artifact_makes_the_run_incomplete() {
    let dir = tempfile::tempdir().expect("tempdir");
    complete_run(dir.path());
    fs::remove_file(dir.path().join("gaps.json")).expect("remove gaps");

    let manifest = build_manifest(dir.path()).expect("build manifest");
    assert_eq!(manifest.status, RunStatus::Incomplete);
    assert!(manifest
        .quality_gates
        .iter()
        .any(|gate| gate.name == "artifacts" && gate.status == GateStatus::Fail));
    assert!(manifest
        .warnings
        .iter()
        .any(|warning| warning.contains("incomplete")));
}

#[test]
fn manifest_and_html_agree_on_status_and_gates() {
    let dir = tempfile::tempdir().expect("tempdir");
    complete_run(dir.path());
    let manifest = build_manifest(dir.path()).expect("build manifest");
    let (json_path, html_path) =
        write_manifest_and_report(dir.path(), &manifest).expect("write report");

    let raw = fs::read_to_string(&json_path).expect("read manifest");
    let parsed: RunManifest = serde_json::from_str(&raw).expect("parse manifest");
    assert_eq!(parsed.status, manifest.status);
    let html = fs::read_to_string(&html_path).expect("read html");
    assert!(html.contains("Complete"));
    for gate in &manifest.quality_gates {
        assert!(html.contains(&gate.name), "{} missing from HTML", gate.name);
    }
}

#[test]
fn validation_rejects_a_complete_manifest_with_an_unfinished_stage() {
    let dir = tempfile::tempdir().expect("tempdir");
    complete_run(dir.path());
    let mut manifest = build_manifest(dir.path()).expect("build manifest");
    manifest.status = RunStatus::Complete;
    if let Some(stage) = manifest.stages.iter_mut().find(|stage| !stage.optional) {
        stage.status = StageRunStatus::NotRun;
    }
    let error = manifest.validate().expect_err("must reject");
    assert!(error.to_string().contains("unfinished required stage"));
}

#[test]
fn errors_and_credentials_are_redacted() {
    std::env::set_var("OPENAI_API_KEY", "sk-test-secret-value");
    let text = "call failed Authorization: Bearer abc\nurl https://user:pass@example.test/x";
    let redacted = redact(text);
    assert!(!redacted.contains("sk-test-secret-value"));
    assert!(!redacted.contains("Bearer abc"));
    assert!(!redacted.contains("user:pass"));
    assert!(redacted.contains("[REDACTED]"));
    std::env::remove_var("OPENAI_API_KEY");
}

#[test]
fn checksum_detects_content_changes() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("artifact.bin");
    fs::write(&path, b"first").expect("write");
    let first = checksum_file(&path).expect("checksum");
    fs::write(&path, b"second").expect("rewrite");
    let second = checksum_file(&path).expect("checksum");
    assert_ne!(first, second);
}

#[test]
fn unreadable_stage_artifact_is_not_present() {
    let dir = tempfile::tempdir().expect("tempdir");
    complete_run(dir.path());
    let mut checkpoint =
        Checkpoint::load(&dir.path().join("checkpoint.json")).expect("load checkpoint");
    checkpoint.stages.insert(
        "decode_audio".to_string(),
        StageRecord {
            stage: "decode_audio".to_string(),
            stage_version: 1,
            completed_unix_ms: 1,
            artifacts: BTreeMap::from([(
                "decoded_audio".to_string(),
                "/nonexistent/decoded.wav".to_string(),
            )]),
            provider: None,
        },
    );
    checkpoint
        .save(&dir.path().join("checkpoint.json"))
        .expect("save checkpoint");

    let manifest = build_manifest(dir.path()).expect("build manifest");
    assert_eq!(manifest.status, RunStatus::Incomplete);
    let stage = manifest
        .stages
        .iter()
        .find(|stage| stage.name == "decode_audio")
        .expect("decode stage");
    assert!(stage.artifacts.iter().any(|artifact| !artifact.present));
    assert!(stage
        .artifacts
        .iter()
        .any(|artifact| artifact.checksum.is_none()));
}

#[test]
fn versioned_schema_covers_every_manifest_field() {
    let schema: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../schema/run-manifest.schema.json"
    ))
    .expect("parse schema");
    let properties = schema["properties"].as_object().expect("properties");
    let required: Vec<&str> = schema["required"]
        .as_array()
        .expect("required")
        .iter()
        .filter_map(|value| value.as_str())
        .collect();
    assert_eq!(schema["properties"]["schema_version"]["const"], 1);

    let dir = tempfile::tempdir().expect("tempdir");
    complete_run(dir.path());
    let manifest = build_manifest(dir.path()).expect("build manifest");
    let value = serde_json::to_value(&manifest).expect("manifest json");
    let emitted = value.as_object().expect("manifest object");
    for key in emitted.keys() {
        assert!(properties.contains_key(key), "schema lacks '{key}'");
        assert!(required.contains(&key.as_str()), "'{key}' is not required");
    }
}
