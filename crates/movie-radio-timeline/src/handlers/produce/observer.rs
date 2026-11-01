//! Checkpoint observer and completion reporting for `produce`.

use anyhow::{bail, Result};
use std::fs;
use std::path::{Path, PathBuf};

use movie_radio_goap::production::{self, artifacts, Checkpoint, StageObserver};
use movie_radio_goap::{PipelineContext, WorldState};

pub(super) fn ensure_complete(checkpoint: &Checkpoint, goal: &WorldState) -> Result<()> {
    let missing: Vec<&str> = production::required_stages(goal)
        .iter()
        .filter(|stage| !checkpoint.stage_artifacts_present(stage))
        .map(|stage| stage.name)
        .collect();
    if !missing.is_empty() {
        bail!(
            "produce finished without committing every required stage: {}",
            missing.join(", ")
        );
    }
    Ok(())
}

pub(super) fn print_summary(checkpoint: &Checkpoint, goal: &WorldState, deliverable: &Path) {
    let required: Vec<&str> = production::required_stages(goal)
        .iter()
        .map(|stage| stage.name)
        .collect();
    println!("produce stages:");
    for stage in production::STAGES {
        let outcome = checkpoint
            .outcomes
            .get(stage.name)
            .cloned()
            .unwrap_or_else(|| {
                if required.contains(&stage.name) {
                    "not run".to_string()
                } else {
                    "skipped: not required".to_string()
                }
            });
        let provider = checkpoint
            .stages
            .get(stage.name)
            .and_then(|record| record.provider.as_deref())
            .map_or_else(String::new, |provider| format!(" provider={provider}"));
        println!("  {:<20} {}{}", stage.name, outcome, provider);
    }
    println!("  deliverable: {}", deliverable.display());
}

fn provider_identity(ctx: &PipelineContext) -> Option<String> {
    let mut ids: Vec<&str> = ctx
        .narration_provider
        .iter()
        .flatten()
        .map(String::as_str)
        .collect();
    ids.sort_unstable();
    ids.dedup();
    if ids.is_empty() {
        None
    } else {
        Some(ids.join(","))
    }
}

pub(super) struct CheckpointObserver {
    checkpoint: Checkpoint,
    path: PathBuf,
    dir: PathBuf,
}

impl CheckpointObserver {
    pub(super) fn new(checkpoint: Checkpoint, path: PathBuf, dir: PathBuf) -> Self {
        Self {
            checkpoint,
            path,
            dir,
        }
    }
}

impl StageObserver for CheckpointObserver {
    fn action_completed(&mut self, action: &str, ctx: &mut PipelineContext) -> Result<()> {
        let Some(stage) = production::stage_by_action(action) else {
            return Ok(());
        };
        let artifacts = artifacts::persist_stage_artifacts(ctx, stage, &self.dir)?;
        if let Some(empty) = artifacts
            .iter()
            .find(|(_, path)| !fs::metadata(path).is_ok_and(|meta| meta.len() > 0))
        {
            bail!(
                "stage '{}' committed an empty artifact: {}",
                stage.name,
                empty.1
            );
        }
        let provider = if stage.name == "synthesize_narration" {
            provider_identity(ctx)
        } else {
            None
        };
        self.checkpoint.record(stage, artifacts, provider);
        self.checkpoint.save(&self.path)
    }

    fn action_failed(
        &mut self,
        action: &str,
        error: &anyhow::Error,
        _ctx: &mut PipelineContext,
    ) -> Result<()> {
        if let Some(stage) = production::stage_by_action(action) {
            self.checkpoint
                .record_outcome(stage.name, format!("failed: {error:#}"));
            self.checkpoint.save(&self.path)?;
        }
        Ok(())
    }
}
