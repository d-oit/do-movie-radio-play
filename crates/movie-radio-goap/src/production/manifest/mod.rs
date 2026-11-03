//! Versioned production run manifest and human-readable report (#416).
//!
//! The manifest is built from the run directory alone (`checkpoint.json`,
//! `run-metadata.json` and the committed artifacts), so an existing run can be
//! re-reported without re-executing production.

use anyhow::{bail, Context, Result};
use movie_radio_types::SfxAssetProvenance;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

pub mod build;
mod html;
mod quality;
#[cfg(test)]
mod tests;

pub use build::{build_manifest, write_run_metadata, RunMetadata};

/// Bumped when the manifest layout changes in a way older readers cannot honour.
pub const MANIFEST_SCHEMA_VERSION: u32 = 1;
pub const MANIFEST_FILE: &str = "manifest.json";
pub const HTML_REPORT_FILE: &str = "report.html";
pub const CREDITS_FILE: &str = "credits.md";
pub const RUN_METADATA_FILE: &str = "run-metadata.json";
pub const SFX_ASSETS_FILE: &str = "sfx-assets.json";
pub const TOOL_NAME: &str = "movie-radio-timeline";
pub const TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Complete,
    Incomplete,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageRunStatus {
    Completed,
    Reused,
    Skipped,
    NotRun,
    Failed,
}

impl StageRunStatus {
    pub fn is_finished(self) -> bool {
        matches!(self, Self::Completed | Self::Reused)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateStatus {
    Pass,
    Warn,
    Fail,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolInfo {
    pub name: String,
    pub version: String,
    pub schema_version: u32,
}

impl Default for ToolInfo {
    fn default() -> Self {
        Self {
            name: TOOL_NAME.to_string(),
            version: TOOL_VERSION.to_string(),
            schema_version: MANIFEST_SCHEMA_VERSION,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageGraphNode {
    pub name: String,
    pub action: String,
    pub version: u32,
    pub inputs: Vec<String>,
    pub produces: String,
    pub optional_for: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactChecksum {
    pub key: String,
    pub path: String,
    pub size_bytes: u64,
    /// FNV-1a digest of the file contents; `None` when the file is missing.
    pub checksum: Option<String>,
    pub present: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageStatus {
    pub name: String,
    pub action: String,
    pub version: u32,
    pub status: StageRunStatus,
    pub optional: bool,
    pub completed_unix_ms: Option<u64>,
    pub provider: Option<String>,
    pub error: Option<String>,
    pub artifacts: Vec<ArtifactChecksum>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderSelection {
    pub vad_engine: Option<String>,
    pub vad_model: Option<String>,
    pub cue_source: String,
    pub language: Option<String>,
    pub narrator_provider: Option<String>,
    pub narrator_model: Option<String>,
    pub tts_providers: Vec<String>,
    pub tts_models: BTreeMap<String, String>,
    pub fallback_chain: Vec<String>,
    pub sfx_providers: Vec<String>,
    pub sfx_assets: Vec<String>,
    pub mastering_profile: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QualityGate {
    pub name: String,
    pub status: GateStatus,
    pub detail: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UsageEstimate {
    pub narration_requests: u64,
    pub narration_succeeded: u64,
    pub narration_failed: u64,
    pub sfx_assets_used: u64,
    /// Provider-reported spend; `None` when no provider reported a cost.
    pub total_cost_usd: Option<f64>,
    /// Configured per-run budget, when the run recorded one.
    pub budget_usd: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NarrationLink {
    pub index: usize,
    pub source_start_ms: u64,
    pub source_end_ms: u64,
    pub output_start_ms: Option<u64>,
    pub output_end_ms: Option<u64>,
    pub provider: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SfxLink {
    pub id: String,
    pub source_start_ms: u64,
    pub source_end_ms: u64,
    pub output_start_ms: u64,
    pub output_end_ms: u64,
    pub asset_id: String,
    pub source: String,
    pub license: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunManifest {
    pub schema_version: u32,
    pub tool: ToolInfo,
    pub generated_unix_ms: u64,
    pub run_id: String,
    pub status: RunStatus,
    pub input: crate::production::checkpoint::InputIdentity,
    pub input_fingerprint: String,
    pub config_hash: String,
    pub stage_graph: Vec<StageGraphNode>,
    pub stages: Vec<StageStatus>,
    pub providers: ProviderSelection,
    pub quality_gates: Vec<QualityGate>,
    pub warnings: Vec<String>,
    pub usage: UsageEstimate,
    pub omitted_stages: Vec<String>,
    pub narration: Vec<NarrationLink>,
    pub sfx: Vec<SfxLink>,
    /// Full provenance for every SFX asset mixed into the run.
    #[serde(default)]
    pub sfx_assets: Vec<SfxAssetProvenance>,
    pub artifacts: Vec<ArtifactChecksum>,
}

impl RunManifest {
    /// Structural and cross-field checks that do not need the run directory.
    pub fn validate(&self) -> Result<()> {
        validate_manifest(self)
    }
}

/// Rejects a manifest that claims more than its stages and artifacts support.
pub fn validate_manifest(manifest: &RunManifest) -> Result<()> {
    if manifest.schema_version != MANIFEST_SCHEMA_VERSION {
        bail!(
            "unsupported manifest schema version {} (expected {})",
            manifest.schema_version,
            MANIFEST_SCHEMA_VERSION
        );
    }
    if manifest.tool.schema_version != MANIFEST_SCHEMA_VERSION {
        bail!(
            "tool schema version {} disagrees with manifest schema version {}",
            manifest.tool.schema_version,
            manifest.schema_version
        );
    }
    if manifest.stages.is_empty() {
        bail!("manifest has no stages");
    }
    let mut seen = BTreeSet::new();
    for stage in &manifest.stages {
        if !seen.insert(stage.name.as_str()) {
            bail!("manifest lists stage '{}' more than once", stage.name);
        }
        if stage.status.is_finished() {
            if let Some(missing) = stage.artifacts.iter().find(|a| !a.present) {
                bail!(
                    "stage '{}' is {:?} but artifact '{}' is missing",
                    stage.name,
                    stage.status,
                    missing.path
                );
            }
        }
    }
    if manifest.status == RunStatus::Complete {
        if let Some(unfinished) = manifest
            .stages
            .iter()
            .find(|s| !s.optional && !s.status.is_finished())
        {
            bail!(
                "complete manifest has unfinished required stage '{}' ({:?})",
                unfinished.name,
                unfinished.status
            );
        }
    }
    Ok(())
}

/// Writes `manifest.json` and `report.html` atomically and returns their paths.
pub fn write_manifest_and_report(
    run_dir: &Path,
    manifest: &RunManifest,
) -> Result<(PathBuf, PathBuf)> {
    manifest.validate()?;
    let manifest_path = run_dir.join(MANIFEST_FILE);
    write_json_atomic(&manifest_path, manifest)?;
    let report_path = run_dir.join(HTML_REPORT_FILE);
    write_atomic(&report_path, html::render(manifest).as_bytes())?;
    Ok((manifest_path, report_path))
}

/// Writes a credits file covering every asset whose licence requires
/// attribution. Returns its path when the file was written.
pub fn write_credits(run_dir: &Path, manifest: &RunManifest) -> Result<Option<PathBuf>> {
    let lines: Vec<String> = manifest
        .sfx_assets
        .iter()
        .filter_map(SfxAssetProvenance::credits_line)
        .collect();
    if lines.is_empty() {
        return Ok(None);
    }
    let mut body = String::from("# SFX credits\n\n");
    for line in lines {
        body.push_str(&format!("- {line}\n"));
    }
    let path = run_dir.join(CREDITS_FILE);
    write_atomic(&path, body.as_bytes())?;
    Ok(Some(path))
}

pub(crate) fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)
        .with_context(|| format!("cannot serialize {}", path.display()))?;
    write_atomic(path, &bytes)
}

pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("cannot create {}", parent.display()))?;
        }
    }
    let tmp = crate::production::checkpoint::part_sibling(path);
    fs::write(&tmp, bytes).with_context(|| format!("cannot write {}", tmp.display()))?;
    fs::rename(&tmp, path)
        .with_context(|| format!("cannot commit {} -> {}", tmp.display(), path.display()))?;
    Ok(())
}

pub(crate) fn read_json_optional<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    let bytes = fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// FNV-1a digest of a file, streamed so large audio artifacts stay cheap.
pub fn checksum_file(path: &Path) -> Result<String> {
    let mut file =
        fs::File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .with_context(|| format!("cannot read {}", path.display()))?;
        if read == 0 {
            break;
        }
        for byte in &buffer[..read] {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100_0000_01b3);
        }
    }
    Ok(format!("{hash:016x}"))
}

/// Environment variables whose values must never reach a manifest or report.
const SECRET_ENV_VARS: &[&str] = &[
    "ELEVENLABS_API_KEY",
    "OPENAI_API_KEY",
    "FREESOUND_API_KEY",
    "MODAL_TOKEN_ID",
    "MODAL_TOKEN_SECRET",
    "HF_TOKEN",
    "HUGGINGFACE_TOKEN",
];

const SECRET_MARKERS: &[&str] = &[
    "authorization",
    "bearer ",
    "api_key",
    "apikey",
    "api-key",
    "access_token",
    "token=",
    "password",
    "secret",
];

/// Removes credentials from free-form text (error messages, provider labels).
pub fn redact(text: &str) -> String {
    let mut out = text.to_string();
    for name in SECRET_ENV_VARS {
        if let Ok(value) = std::env::var(name) {
            if value.len() >= 4 {
                out = out.replace(&value, "[REDACTED]");
            }
        }
    }
    out.lines().map(redact_line).collect::<Vec<_>>().join("\n")
}

fn redact_line(line: &str) -> String {
    let lower = line.to_ascii_lowercase();
    if SECRET_MARKERS.iter().any(|marker| lower.contains(marker)) {
        return "[REDACTED]".to_string();
    }
    redact_url_userinfo(line)
}

/// Drops `user:password@` from any URL embedded in a line.
fn redact_url_userinfo(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(scheme) = rest.find("://") {
        let (head, tail) = rest.split_at(scheme + 3);
        out.push_str(head);
        let authority_end = tail.find(['/', ' ', '"', '\'']).unwrap_or(tail.len());
        let authority = &tail[..authority_end];
        match authority.rfind('@') {
            Some(at) => {
                out.push_str("[REDACTED]@");
                out.push_str(&authority[at + 1..]);
            }
            None => out.push_str(authority),
        }
        rest = &tail[authority_end..];
    }
    out.push_str(rest);
    out
}
