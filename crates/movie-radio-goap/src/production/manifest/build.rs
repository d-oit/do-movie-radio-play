//! Builds a manifest from a production run directory.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use super::quality::{quality_gates, usage, warnings, NarrationIndex};
use super::{
    checksum_file, read_json_optional, redact, write_json_atomic, ArtifactChecksum, NarrationLink,
    ProviderSelection, RunManifest, RunStatus, SfxLink, StageGraphNode, StageRunStatus,
    StageStatus, ToolInfo, MANIFEST_SCHEMA_VERSION, RUN_METADATA_FILE,
};
use crate::narrate::NarrationScript;
use crate::production::checkpoint::{fingerprint, now_unix_ms, Checkpoint, InputIdentity};
use crate::production::{input_class, STAGES};
use movie_radio_verification::{TrackMap, TrackType};

/// Effective run configuration recorded by `produce` before the first stage.
/// It contains provider identities only, never credentials.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunMetadata {
    pub schema_version: u32,
    pub run_id: String,
    pub created_unix_ms: u64,
    pub input: InputIdentity,
    pub config_hash: String,
    #[serde(default)]
    pub vad_engine: String,
    #[serde(default)]
    pub vad_model: String,
    #[serde(default)]
    pub cue_source: String,
    #[serde(default)]
    pub language: String,
    #[serde(default)]
    pub narrator_provider: String,
    #[serde(default)]
    pub tts_fallback_chain: Vec<String>,
    #[serde(default)]
    pub tts_models: BTreeMap<String, String>,
    #[serde(default)]
    pub sfx_providers: Vec<String>,
    #[serde(default)]
    pub sfx_assets: Vec<String>,
    #[serde(default)]
    pub mastering_profile: String,
    #[serde(default)]
    pub goal_stages: Vec<String>,
    #[serde(default)]
    pub max_cost_per_run_usd: f64,
}

impl RunMetadata {
    fn from_checkpoint(checkpoint: &Checkpoint) -> Self {
        Self {
            schema_version: MANIFEST_SCHEMA_VERSION,
            run_id: checkpoint.input.path.clone(),
            created_unix_ms: now_unix_ms(),
            input: checkpoint.input.clone(),
            config_hash: config_hash(&checkpoint.fingerprints),
            vad_engine: String::new(),
            vad_model: String::new(),
            cue_source: "unknown".to_string(),
            language: String::new(),
            narrator_provider: String::new(),
            tts_fallback_chain: Vec::new(),
            tts_models: BTreeMap::new(),
            sfx_providers: Vec::new(),
            sfx_assets: Vec::new(),
            mastering_profile: String::new(),
            goal_stages: Vec::new(),
            max_cost_per_run_usd: 0.0,
        }
    }
}

pub fn write_run_metadata(run_dir: &Path, metadata: &RunMetadata) -> Result<()> {
    write_json_atomic(&run_dir.join(RUN_METADATA_FILE), metadata)
}

fn read_metadata(run_dir: &Path) -> Option<RunMetadata> {
    read_json_optional(&run_dir.join(RUN_METADATA_FILE))
}

fn config_hash(fingerprints: &BTreeMap<String, String>) -> String {
    let parts: Vec<(&str, String)> = fingerprints
        .iter()
        .map(|(key, value)| (key.as_str(), value.clone()))
        .collect();
    fingerprint(&parts)
}

/// Builds the manifest for an existing run directory.
pub fn build_manifest(run_dir: &Path) -> Result<RunManifest> {
    let checkpoint_path = run_dir.join("checkpoint.json");
    let checkpoint = Checkpoint::load(&checkpoint_path).with_context(|| {
        format!(
            "cannot read production checkpoint: {}",
            checkpoint_path.display()
        )
    })?;
    let metadata =
        read_metadata(run_dir).unwrap_or_else(|| RunMetadata::from_checkpoint(&checkpoint));
    let required = required_stages(&metadata);
    let stages = stage_statuses(&checkpoint);
    let artifacts = stages
        .iter()
        .flat_map(|stage| stage.artifacts.iter().cloned())
        .collect::<Vec<_>>();
    let status = run_status(&stages, &required);
    let track_map = read_json_optional::<TrackMap>(&run_dir.join("track-map.json"));
    let narration = narration_links(run_dir, track_map.as_ref());
    let sfx = sfx_links(track_map.as_ref());
    let usage = usage(run_dir, &metadata, sfx.len());
    let quality_gates = quality_gates(run_dir, &artifacts, status);
    let warnings = warnings(&stages, &narration, status);
    let omitted_stages = stages
        .iter()
        .filter(|stage| stage.optional && !stage.status.is_finished())
        .map(|stage| stage.name.clone())
        .collect();

    Ok(RunManifest {
        schema_version: MANIFEST_SCHEMA_VERSION,
        tool: ToolInfo::default(),
        generated_unix_ms: now_unix_ms(),
        run_id: metadata.run_id.clone(),
        status,
        input: checkpoint.input.clone(),
        input_fingerprint: checkpoint
            .fingerprints
            .get(input_class::SOURCE)
            .cloned()
            .unwrap_or_default(),
        config_hash: if metadata.config_hash.is_empty() {
            config_hash(&checkpoint.fingerprints)
        } else {
            metadata.config_hash.clone()
        },
        stage_graph: stage_graph(),
        stages,
        providers: providers(&metadata, &checkpoint, &sfx),
        quality_gates,
        warnings,
        usage,
        omitted_stages,
        narration,
        sfx,
        artifacts,
    })
}

fn required_stages(metadata: &RunMetadata) -> BTreeSet<String> {
    if metadata.goal_stages.is_empty() {
        STAGES
            .iter()
            .filter(|stage| stage.optional_for.is_none())
            .map(|stage| stage.name.to_string())
            .collect()
    } else {
        metadata.goal_stages.iter().cloned().collect()
    }
}

fn stage_graph() -> Vec<StageGraphNode> {
    STAGES
        .iter()
        .map(|stage| StageGraphNode {
            name: stage.name.to_string(),
            action: stage.action.to_string(),
            version: stage.version,
            inputs: stage.inputs.iter().map(|s| (*s).to_string()).collect(),
            produces: stage.produces.to_string(),
            optional_for: stage.optional_for.map(|flag| format!("{flag:?}")),
        })
        .collect()
}

fn stage_statuses(checkpoint: &Checkpoint) -> Vec<StageStatus> {
    STAGES
        .iter()
        .map(|stage| {
            let record = checkpoint.stages.get(stage.name);
            let outcome = checkpoint.outcomes.get(stage.name).map(String::as_str);
            let status = match (record, outcome) {
                (Some(_), Some("reused")) => StageRunStatus::Reused,
                (Some(_), _) => StageRunStatus::Completed,
                (None, Some(value)) if value.starts_with("failed") => StageRunStatus::Failed,
                (None, Some(value)) if value.starts_with("skipped") => StageRunStatus::Skipped,
                _ => StageRunStatus::NotRun,
            };
            let artifacts = record
                .map(|record| {
                    record
                        .artifacts
                        .iter()
                        .map(|(key, path)| artifact_checksum(key, path))
                        .collect()
                })
                .unwrap_or_default();
            StageStatus {
                name: stage.name.to_string(),
                action: stage.action.to_string(),
                version: stage.version,
                status,
                optional: stage.optional_for.is_some(),
                completed_unix_ms: record.map(|record| record.completed_unix_ms),
                provider: record.and_then(|record| record.provider.clone()),
                error: if status == StageRunStatus::Failed {
                    outcome.map(redact)
                } else {
                    None
                },
                artifacts,
            }
        })
        .collect()
}

fn artifact_checksum(key: &str, path: &str) -> ArtifactChecksum {
    let file = Path::new(path);
    let size_bytes = fs::metadata(file).map_or(0, |meta| meta.len());
    let present = size_bytes > 0;
    ArtifactChecksum {
        key: key.to_string(),
        path: path.to_string(),
        size_bytes,
        checksum: if present {
            checksum_file(file).ok()
        } else {
            None
        },
        present,
    }
}

fn run_status(stages: &[StageStatus], required: &BTreeSet<String>) -> RunStatus {
    if stages
        .iter()
        .any(|stage| stage.status == StageRunStatus::Failed)
    {
        return RunStatus::Failed;
    }
    let unfinished = stages
        .iter()
        .any(|stage| required.contains(&stage.name) && !stage.status.is_finished());
    let missing = stages.iter().any(|stage| {
        stage.status.is_finished() && stage.artifacts.iter().any(|artifact| !artifact.present)
    });
    if unfinished || missing {
        RunStatus::Incomplete
    } else {
        RunStatus::Complete
    }
}

fn providers(
    metadata: &RunMetadata,
    checkpoint: &Checkpoint,
    sfx: &[SfxLink],
) -> ProviderSelection {
    let narrator_provider = if metadata.narrator_provider.is_empty() {
        checkpoint
            .stages
            .get("synthesize_narration")
            .and_then(|record| record.provider.clone())
    } else {
        Some(metadata.narrator_provider.clone())
    };
    let narrator_model = narrator_provider
        .as_ref()
        .and_then(|provider| metadata.tts_models.get(provider).cloned());
    let sfx_assets = if metadata.sfx_assets.is_empty() {
        let mut assets: Vec<String> = sfx.iter().map(|link| link.asset_id.clone()).collect();
        assets.sort();
        assets.dedup();
        assets
    } else {
        metadata.sfx_assets.clone()
    };
    ProviderSelection {
        vad_engine: non_empty(&metadata.vad_engine),
        vad_model: non_empty(&metadata.vad_model),
        cue_source: if metadata.cue_source.is_empty() {
            "unknown".to_string()
        } else {
            metadata.cue_source.clone()
        },
        language: non_empty(&metadata.language),
        narrator_provider,
        narrator_model,
        tts_providers: metadata.tts_fallback_chain.clone(),
        tts_models: metadata.tts_models.clone(),
        fallback_chain: metadata.tts_fallback_chain.clone(),
        sfx_providers: metadata.sfx_providers.clone(),
        sfx_assets,
        mastering_profile: non_empty(&metadata.mastering_profile),
    }
}

fn non_empty(value: &str) -> Option<String> {
    if value.trim().is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn narration_links(run_dir: &Path, track_map: Option<&TrackMap>) -> Vec<NarrationLink> {
    let scripts = read_json_optional::<Vec<NarrationScript>>(&run_dir.join("scripts.json"))
        .unwrap_or_default();
    let index = read_json_optional::<NarrationIndex>(&run_dir.join("narration.json"))
        .map(|index| index.entries)
        .unwrap_or_default();
    let mut narration_entries: Vec<_> = track_map
        .map(|map| {
            map.entries
                .iter()
                .filter(|entry| entry.track_type == TrackType::Narration)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    narration_entries.sort_by_key(|entry| entry.source_start_ms);
    let mut used = vec![false; narration_entries.len()];

    scripts
        .iter()
        .enumerate()
        .map(|(i, script)| {
            let matched = nearest_narration(&narration_entries, &mut used, script.gap_start_ms);
            NarrationLink {
                index: i,
                source_start_ms: script.gap_start_ms,
                source_end_ms: script.gap_end_ms,
                output_start_ms: matched.map(|entry| entry.output_start_ms),
                output_end_ms: matched.map(|entry| entry.output_end_ms),
                provider: index.get(i).and_then(|entry| entry.provider.clone()),
                text: script.text.clone(),
            }
        })
        .collect()
}

fn nearest_narration<'a>(
    entries: &[&'a movie_radio_verification::TrackMapEntry],
    used: &mut [bool],
    start_ms: u64,
) -> Option<&'a movie_radio_verification::TrackMapEntry> {
    let mut best: Option<(usize, u64)> = None;
    for (i, entry) in entries.iter().enumerate() {
        if used[i] {
            continue;
        }
        let distance = entry.source_start_ms.abs_diff(start_ms);
        if best.is_none_or(|(_, best_distance)| distance < best_distance) {
            best = Some((i, distance));
        }
    }
    let (i, _) = best?;
    used[i] = true;
    Some(entries[i])
}

fn sfx_links(track_map: Option<&TrackMap>) -> Vec<SfxLink> {
    track_map
        .map(|map| {
            map.entries
                .iter()
                .filter(|entry| entry.track_type == TrackType::Sfx)
                .map(|entry| SfxLink {
                    id: entry.id.clone(),
                    source_start_ms: entry.source_start_ms,
                    source_end_ms: entry.source_end_ms,
                    output_start_ms: entry.output_start_ms,
                    output_end_ms: entry.output_end_ms,
                    asset_id: entry.provenance.asset_id.clone(),
                    source: entry.provenance.source.clone(),
                    license: format!("{:?}", entry.provenance.license),
                })
                .collect()
        })
        .unwrap_or_default()
}
