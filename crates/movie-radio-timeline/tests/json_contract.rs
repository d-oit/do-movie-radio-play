//! `extract` output must validate against `schema/timeline.schema.json`.
//!
//! `plans/040-validation/ACCEPTANCE.md` lists this as a must-pass gate, but
//! neither this test nor the schema check existed, so the published contract had
//! drifted from the emitted JSON: `Segment::sfx_trigger` is serialized (as null
//! when absent) while the schema forbade additional properties and did not
//! declare it. Validating here keeps the two in step.
//!
//! The validator is deliberately small and local rather than a JSON Schema
//! dependency (that would add ~23 crates for a 130-line contract). It supports
//! exactly the keywords this schema uses and panics on any keyword it does not
//! understand, so a schema edit cannot slip past unchecked.

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use serde_json::Value;

const SAMPLE_RATE: u32 = 16_000;
const FRAME_MS: u32 = 20;

const ANNOTATION_KEYWORDS: [&str; 5] = ["$schema", "$id", "title", "description", "default"];

fn type_matches(expected: &str, instance: &Value) -> bool {
    match expected {
        "null" => instance.is_null(),
        "boolean" => instance.is_boolean(),
        "integer" => {
            instance.is_i64()
                || instance.is_u64()
                || instance.as_f64().is_some_and(|f| f.fract() == 0.0)
        }
        "number" => instance.is_number(),
        "string" => instance.is_string(),
        "array" => instance.is_array(),
        "object" => instance.is_object(),
        other => panic!("unsupported schema type '{other}'"),
    }
}

/// Returns every violation of `schema` by `instance`.
fn validate(schema: &Value, instance: &Value, path: &str) -> Vec<String> {
    let Some(object) = schema.as_object() else {
        panic!("schema node at {path} is not an object");
    };
    let mut errors = Vec::new();

    for keyword in object.keys() {
        let known = ANNOTATION_KEYWORDS.contains(&keyword.as_str())
            || matches!(
                keyword.as_str(),
                "type"
                    | "enum"
                    | "const"
                    | "minimum"
                    | "maximum"
                    | "required"
                    | "properties"
                    | "additionalProperties"
                    | "items"
                    | "oneOf"
            );
        if !known {
            panic!(
                "schema at {path} uses keyword '{keyword}' that this contract test does not \
                 implement; extend validate() rather than letting it pass unchecked"
            );
        }
    }

    if let Some(expected) = object.get("type") {
        let allowed: Vec<&str> = match expected {
            Value::String(single) => vec![single.as_str()],
            Value::Array(items) => items
                .iter()
                .map(|item| item.as_str().expect("type array holds strings"))
                .collect(),
            other => panic!("unsupported 'type' node at {path}: {other}"),
        };
        if !allowed.iter().any(|name| type_matches(name, instance)) {
            errors.push(format!("{path}: expected type {allowed:?}, got {instance}"));
        }
    }

    if let Some(Value::Array(options)) = object.get("enum") {
        if !options.contains(instance) {
            errors.push(format!("{path}: {instance} not in enum {options:?}"));
        }
    }

    if let Some(expected) = object.get("const") {
        if expected != instance {
            errors.push(format!("{path}: {instance} != const {expected}"));
        }
    }

    if let Some(number) = instance.as_f64() {
        if let Some(minimum) = object.get("minimum").and_then(Value::as_f64) {
            if number < minimum {
                errors.push(format!("{path}: {number} below minimum {minimum}"));
            }
        }
        if let Some(maximum) = object.get("maximum").and_then(Value::as_f64) {
            if number > maximum {
                errors.push(format!("{path}: {number} above maximum {maximum}"));
            }
        }
    }

    if let Some(found) = instance.as_object() {
        if let Some(Value::Array(required)) = object.get("required") {
            for name in required.iter().filter_map(Value::as_str) {
                if !found.contains_key(name) {
                    errors.push(format!("{path}: missing required property '{name}'"));
                }
            }
        }
        let properties = object.get("properties").and_then(Value::as_object);
        if let Some(properties) = properties {
            for (name, value) in found {
                match properties.get(name) {
                    Some(child) => {
                        errors.extend(validate(child, value, &format!("{path}.{name}")));
                    }
                    None => {
                        if object.get("additionalProperties").and_then(Value::as_bool)
                            == Some(false)
                        {
                            errors.push(format!(
                                "{path}: property '{name}' is not allowed by the schema"
                            ));
                        }
                    }
                }
            }
        }
    }

    if let (Some(items), Value::Array(values)) = (object.get("items"), instance) {
        for (index, value) in values.iter().enumerate() {
            errors.extend(validate(items, value, &format!("{path}[{index}]")));
        }
    }

    if let Some(Value::Array(branches)) = object.get("oneOf") {
        let matches = branches
            .iter()
            .filter(|branch| validate(branch, instance, path).is_empty())
            .count();
        if matches != 1 {
            errors.push(format!(
                "{path}: expected exactly one oneOf branch to match, {matches} did"
            ));
        }
    }

    errors
}

/// A tone followed by silence: enough for the energy detector to emit both
/// kinds of segment without depending on downloaded media.
fn write_fixture(path: &Path) {
    // 12 s of low-level noise, matching the repository's own `long_ambience`
    // fixture, which is known to yield a segment. Shorter clips or digital
    // silence produce an empty timeline and would make these assertions vacuous.
    let mut state: u32 = 42;
    let mut samples = Vec::new();
    for _ in 0..(SAMPLE_RATE * 12) {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        samples.push(((state >> 8) as f32 / u32::MAX as f32 - 0.5) * 0.2 * 0.02);
    }

    let data_len = (samples.len() * 2) as u32;
    let mut bytes = Vec::with_capacity(44 + data_len as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
    bytes.extend_from_slice(&1u16.to_le_bytes()); // mono
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

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root")
        .to_path_buf()
}

fn extract(dir: &Path) -> serde_json::Value {
    let media = dir.join("fixture.wav");
    write_fixture(&media);
    let out = dir.join("timeline.json");
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
    serde_json::from_str(&std::fs::read_to_string(&out).expect("read output"))
        .expect("output is JSON")
}

#[test]
fn extract_output_validates_against_published_schema() {
    let dir = tempfile::tempdir().expect("tempdir");
    let document = extract(dir.path());

    let schema_path = repo_root().join("schema/timeline.schema.json");
    let schema: Value =
        serde_json::from_str(&std::fs::read_to_string(&schema_path).expect("read schema"))
            .expect("schema is JSON");

    let errors = validate(&schema, &document, "$");
    assert!(
        errors.is_empty(),
        "extract output violates {}:\n  {}",
        schema_path.display(),
        errors.join("\n  ")
    );
}

#[test]
fn validator_rejects_undeclared_property() {
    // Proves the check above is not vacuous: the pre-existing drift (a segment
    // key the schema did not declare) must be reported.
    let schema: Value = serde_json::from_str(
        &std::fs::read_to_string(repo_root().join("schema/timeline.schema.json"))
            .expect("read schema"),
    )
    .expect("schema is JSON");
    let document = serde_json::json!({
        "file": "x.wav",
        "analysis_sample_rate": 16_000,
        "frame_ms": 20,
        "segments": [{
            "start_ms": 0,
            "end_ms": 1_000,
            "kind": "non_voice",
            "confidence": 0.5,
            "tags": [],
            "prompt": null,
            "unexpected_key": 1
        }]
    });
    let errors = validate(&schema, &document, "$");
    assert!(
        errors.iter().any(|error| error.contains("unexpected_key")),
        "validator should reject an undeclared segment property, got: {errors:?}"
    );
}

#[test]
fn contract_fields_are_the_documented_ones() {
    let dir = tempfile::tempdir().expect("tempdir");
    let document = extract(dir.path());

    let mut top: Vec<&str> = document
        .as_object()
        .expect("top-level object")
        .keys()
        .map(String::as_str)
        .collect();
    top.sort_unstable();
    assert_eq!(
        top,
        ["analysis_sample_rate", "file", "frame_ms", "segments"],
        "top-level timeline keys changed; update schema/timeline.schema.json"
    );
    assert_eq!(document["analysis_sample_rate"], SAMPLE_RATE);
    assert_eq!(document["frame_ms"], FRAME_MS);

    let segments = document["segments"].as_array().expect("segments array");
    assert!(
        !segments.is_empty(),
        "fixture should yield at least one segment"
    );
    for segment in segments {
        let mut keys: Vec<&str> = segment
            .as_object()
            .expect("segment object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "confidence",
                "end_ms",
                "kind",
                "prompt",
                "sfx_trigger",
                "start_ms",
                "tags"
            ],
            "segment keys changed; update schema/timeline.schema.json"
        );
        let kind = segment["kind"].as_str().expect("kind");
        assert!(
            matches!(kind, "speech" | "non_voice"),
            "unexpected kind {kind}"
        );
        let confidence = segment["confidence"].as_f64().expect("confidence");
        assert!(
            (0.0..=1.0).contains(&confidence),
            "confidence out of range: {confidence}"
        );
    }
}
