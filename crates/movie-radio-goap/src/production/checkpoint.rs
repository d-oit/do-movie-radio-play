//! Resumable production checkpoint: input identity, per-input-class
//! fingerprints, and per-stage artifact records.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use super::ProductionStage;

/// Bumped when the on-disk layout changes in a way older records cannot honour.
pub const SCHEMA_VERSION: u32 = 1;

/// Stable identity of the source media: a stage is stale once this changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputIdentity {
    pub path: String,
    pub size_bytes: u64,
    pub modified_unix_ms: u64,
}

impl InputIdentity {
    pub fn capture(path: &Path) -> Result<Self> {
        let meta = fs::metadata(path)
            .with_context(|| format!("cannot stat input media: {}", path.display()))?;
        let modified_unix_ms = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_millis() as u64);
        Ok(Self {
            path: path.to_string_lossy().to_string(),
            size_bytes: meta.len(),
            modified_unix_ms,
        })
    }

    /// True when the file on disk is still the one this identity describes.
    pub fn still_matches(&self, path: &Path) -> bool {
        Self::capture(path).is_ok_and(|now| &now == self)
    }
}

pub fn identify_input(path: &Path) -> Result<InputIdentity> {
    InputIdentity::capture(path)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageRecord {
    pub stage: String,
    pub stage_version: u32,
    pub completed_unix_ms: u64,
    /// Artifact key mapped to the path committed on disk.
    pub artifacts: BTreeMap<String, String>,
    #[serde(default)]
    pub provider: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub schema_version: u32,
    pub input: InputIdentity,
    /// Hash per input class (see `super::input_class`).
    pub fingerprints: BTreeMap<String, String>,
    pub stages: BTreeMap<String, StageRecord>,
    /// Last outcome per stage (`completed`, `reused`, `skipped: …`,
    /// `failed: …`), so a failed run does not look like work that never ran.
    #[serde(default)]
    pub outcomes: BTreeMap<String, String>,
}

impl Checkpoint {
    pub fn fresh(input: InputIdentity, fingerprints: BTreeMap<String, String>) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            input,
            fingerprints,
            stages: BTreeMap::new(),
            outcomes: BTreeMap::new(),
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read checkpoint: {}", path.display()))?;
        let checkpoint: Self = serde_json::from_str(&content)
            .with_context(|| format!("failed to parse checkpoint JSON: {}", path.display()))?;
        Ok(checkpoint)
    }

    /// Writes to a sibling temp file and renames it, so an interrupted write
    /// never leaves a half-written checkpoint behind.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("cannot create {}", parent.display()))?;
            }
        }
        let tmp = part_sibling(path);
        let json = serde_json::to_vec_pretty(self)?;
        fs::write(&tmp, json)
            .with_context(|| format!("failed to write checkpoint: {}", tmp.display()))?;
        fs::rename(&tmp, path).with_context(|| {
            format!(
                "failed to commit checkpoint {} -> {}",
                tmp.display(),
                path.display()
            )
        })?;
        Ok(())
    }

    /// True when the checkpoint still describes this exact input and the
    /// fingerprints are identical for every class both sides recorded.
    pub fn matches_run(
        &self,
        input: &InputIdentity,
        fingerprints: &BTreeMap<String, String>,
    ) -> bool {
        self.input == *input && self.fingerprints == *fingerprints
    }

    /// A stage is reusable only when its version, every input class it reads,
    /// and every artifact it committed still agree with the current run.
    pub fn is_stage_valid(
        &self,
        stage: &ProductionStage,
        fingerprints: &BTreeMap<String, String>,
    ) -> bool {
        if self.schema_version != SCHEMA_VERSION {
            return false;
        }
        let Some(record) = self.stages.get(stage.name) else {
            return false;
        };
        if record.stage_version != stage.version {
            return false;
        }
        let inputs_match = stage
            .inputs
            .iter()
            .all(|class| self.fingerprints.get(*class) == fingerprints.get(*class));
        inputs_match && artifacts_present(record)
    }

    /// Recorded artifacts exist and are non-empty, ignoring input/config drift.
    pub fn stage_artifacts_present(&self, stage: &ProductionStage) -> bool {
        self.stages.get(stage.name).is_some_and(artifacts_present)
    }

    pub fn record(
        &mut self,
        stage: &ProductionStage,
        artifacts: BTreeMap<String, String>,
        provider: Option<String>,
    ) {
        self.outcomes
            .insert(stage.name.to_string(), "completed".to_string());
        self.stages.insert(
            stage.name.to_string(),
            StageRecord {
                stage: stage.name.to_string(),
                stage_version: stage.version,
                completed_unix_ms: now_unix_ms(),
                artifacts,
                provider,
            },
        );
    }

    pub fn record_outcome(&mut self, stage: &str, outcome: impl Into<String>) {
        self.outcomes.insert(stage.to_string(), outcome.into());
    }

    /// Drops `name` and every later stage: their artifacts depend on it.
    pub fn invalidate_from(&mut self, name: &str) {
        let Some(idx) = super::stage_index(name) else {
            return;
        };
        for stage in &super::STAGES[idx..] {
            self.stages.remove(stage.name);
            self.outcomes.remove(stage.name);
        }
    }
}

fn artifacts_present(record: &StageRecord) -> bool {
    !record.artifacts.is_empty()
        && record
            .artifacts
            .values()
            .all(|p| fs::metadata(p).is_ok_and(|m| m.len() > 0))
}

/// Sibling `.part` path used to stage a file before its atomic rename.
pub(crate) fn part_sibling(path: &Path) -> PathBuf {
    let mut name = path.file_name().map_or_else(
        || std::ffi::OsString::from("checkpoint.json"),
        std::ffi::OsStr::to_os_string,
    );
    name.push(".part");
    path.with_file_name(name)
}

pub fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// Stable FNV-1a digest over ordered key/value pairs. Hand-rolled so the
/// fingerprint stays identical across toolchains and needs no extra crate.
pub fn fingerprint(parts: &[(&str, String)]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for (key, value) in parts {
        for bytes in [key.as_bytes(), b"=", value.as_bytes(), b"\n"] {
            for byte in bytes {
                hash ^= u64::from(*byte);
                hash = hash.wrapping_mul(0x100_0000_01b3);
            }
        }
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::production::{input_class, stage_by_name};

    fn fingerprints(voice: &str) -> BTreeMap<String, String> {
        let mut map = BTreeMap::new();
        map.insert(input_class::SOURCE.to_string(), "src".to_string());
        map.insert(input_class::ANALYSIS.to_string(), "an".to_string());
        map.insert(input_class::VOICE.to_string(), voice.to_string());
        map
    }

    fn input(dir: &Path) -> InputIdentity {
        let media = dir.join("movie.wav");
        fs::write(&media, b"RIFF").expect("write media");
        InputIdentity::capture(&media).expect("capture identity")
    }

    #[test]
    fn fingerprint_is_stable_and_input_sensitive() {
        let a = fingerprint(&[("voice", "modal".to_string())]);
        let b = fingerprint(&[("voice", "modal".to_string())]);
        let c = fingerprint(&[("voice", "openai".to_string())]);
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.len(), 16);
    }

    #[test]
    fn stage_validity_tracks_classes_version_and_artifacts() {
        let dir = tempfile::tempdir().expect("tempdir");
        let artifact = dir.path().join("gaps.json");
        fs::write(&artifact, b"{}").expect("write artifact");
        let stage = stage_by_name("identify_gaps").expect("stage");

        let fps = fingerprints("modal");
        let mut checkpoint = Checkpoint::fresh(input(dir.path()), fps.clone());
        let mut artifacts = BTreeMap::new();
        artifacts.insert("gaps".to_string(), artifact.to_string_lossy().to_string());
        checkpoint.record(stage, artifacts, None);
        assert!(checkpoint.is_stage_valid(stage, &fps));

        // A voice-only change must not invalidate gap identification.
        assert!(checkpoint.is_stage_valid(stage, &fingerprints("openai")));

        // But a source change must.
        let mut changed = fingerprints("modal");
        changed.insert(input_class::SOURCE.to_string(), "other".to_string());
        assert!(!checkpoint.is_stage_valid(stage, &changed));

        // A bumped stage version or a vanished artifact also invalidates.
        let mut bumped = *stage;
        bumped.version += 1;
        assert!(!checkpoint.is_stage_valid(&bumped, &fps));
        fs::write(&artifact, b"").expect("truncate artifact");
        assert!(!checkpoint.is_stage_valid(stage, &fps));
    }

    #[test]
    fn save_load_roundtrip_and_invalidate_from() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("cp/checkpoint.json");
        let mut checkpoint = Checkpoint::fresh(input(dir.path()), fingerprints("modal"));
        for name in ["decode_audio", "analyze_speech", "identify_gaps"] {
            let stage = stage_by_name(name).expect("stage");
            let artifact = dir.path().join(format!("{name}.json"));
            fs::write(&artifact, b"{}").expect("write artifact");
            let mut artifacts = BTreeMap::new();
            artifacts.insert(name.to_string(), artifact.to_string_lossy().to_string());
            checkpoint.record(stage, artifacts, None);
        }
        checkpoint.record_outcome("plan_narration", "failed: boom");
        checkpoint.save(&path).expect("save");

        let mut loaded = Checkpoint::load(&path).expect("load");
        assert_eq!(loaded.stages.len(), 3);
        loaded.invalidate_from("analyze_speech");
        assert!(loaded.stages.contains_key("decode_audio"));
        assert!(!loaded.stages.contains_key("analyze_speech"));
        assert!(!loaded.stages.contains_key("identify_gaps"));
        assert_eq!(
            loaded.outcomes.get("decode_audio").map(String::as_str),
            Some("completed")
        );
        assert!(!loaded.outcomes.contains_key("plan_narration"));
        assert!(!dir.path().join("cp/checkpoint.json.part").exists());
    }
}
