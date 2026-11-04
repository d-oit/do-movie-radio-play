//! CLI handlers for the human-in-the-loop review feedback workflow.

use anyhow::{bail, Context, Result};
use movie_radio_learning::database;
use movie_radio_learning::review_feedback::{
    checksum_file, HoldoutFixture, ImportOptions, PrivacyPolicy, ReviewFeedbackBatch,
};
use std::path::{Path, PathBuf};

fn with_db<T>(
    learning_db: &Path,
    f: impl FnOnce(&tokio::runtime::Runtime, &database::LearningDb) -> Result<T>,
) -> Result<T> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to create async runtime for learning db")?;
    if let Some(parent) = learning_db.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).context("failed to create learning db directory")?;
        }
    }
    let db = rt.block_on(database::LearningDb::new(learning_db))?;
    f(&rt, &db)
}

fn write_report(report: &serde_json::Value, output: Option<PathBuf>) -> Result<()> {
    if let Some(output_path) = output {
        if let Some(parent) = output_path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        std::fs::write(&output_path, serde_json::to_vec_pretty(report)?)?;
        tracing::info!(output = %output_path.display(), "review feedback report written");
    } else {
        println!("{}", serde_json::to_string_pretty(report)?);
    }
    Ok(())
}

pub fn handle_import_review_feedback(
    input: PathBuf,
    learning_db: PathBuf,
    reviewer: Option<String>,
    dry_run: bool,
    output: Option<PathBuf>,
) -> Result<()> {
    let raw = std::fs::read_to_string(&input)
        .with_context(|| format!("cannot read review feedback {}", input.display()))?;
    let batch: ReviewFeedbackBatch = serde_json::from_str(&raw)
        .with_context(|| format!("corrupted review feedback {}", input.display()))?;
    let summary = with_db(&learning_db, |rt, db| {
        rt.block_on(db.import_review_feedback(
            &batch,
            &ImportOptions {
                reviewer_override: reviewer,
                dry_run,
            },
        ))
    })?;
    let report = serde_json::json!({
        "learning_db": learning_db,
        "input": input,
        "summary": summary,
    });
    write_report(&report, output)
}

pub fn handle_review_proposals(learning_db: PathBuf, output: Option<PathBuf>) -> Result<()> {
    let report = with_db(&learning_db, |rt, db| {
        rt.block_on(db.derive_learning_proposals())?;
        let proposals = rt.block_on(db.get_learning_proposals())?;
        let parameters = rt.block_on(db.list_learning_parameters())?;
        let versions = rt.block_on(db.get_learning_versions(None))?;
        let holdout_fixtures = rt.block_on(db.list_holdout_fixtures())?;
        Ok(serde_json::json!({
            "learning_db": learning_db,
            "proposals": proposals,
            "parameters": parameters,
            "versions": versions,
            "holdout_fixtures": holdout_fixtures,
        }))
    })?;
    write_report(&report, output)
}

pub fn handle_apply_review_proposal(
    proposal_id: String,
    confirm: bool,
    learning_db: PathBuf,
) -> Result<()> {
    if !confirm {
        bail!("apply-review-proposal requires --confirm to change learned parameters");
    }
    let version = with_db(&learning_db, |rt, db| {
        rt.block_on(db.apply_learning_proposal(&proposal_id))
    })?;
    let report = serde_json::json!({
        "learning_db": learning_db,
        "applied": version,
    });
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

pub fn handle_rollback_review_proposal(
    proposal_id: String,
    confirm: bool,
    learning_db: PathBuf,
) -> Result<()> {
    if !confirm {
        bail!("rollback-review-proposal requires --confirm to restore the prior version");
    }
    let version = with_db(&learning_db, |rt, db| {
        rt.block_on(db.rollback_learning_proposal(&proposal_id))
    })?;
    let report = serde_json::json!({
        "learning_db": learning_db,
        "rolled_back": version,
    });
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

pub fn handle_review_metrics(learning_db: PathBuf, output: Option<PathBuf>) -> Result<()> {
    let metrics = with_db(&learning_db, |rt, db| {
        rt.block_on(db.review_outcome_metrics())
    })?;
    let report = serde_json::json!({
        "learning_db": learning_db,
        "metrics": metrics,
    });
    write_report(&report, output)
}

pub fn handle_register_holdout(
    fixture_id: String,
    path: PathBuf,
    learning_db: PathBuf,
) -> Result<()> {
    let checksum = checksum_file(&path)?;
    let fixture = HoldoutFixture {
        fixture_id,
        path: path.to_string_lossy().to_string(),
        checksum,
        registered_at: String::new(),
    };
    with_db(&learning_db, |rt, db| {
        rt.block_on(db.register_holdout_fixture(&fixture))
    })?;
    println!(
        "Registered holdout fixture {} ({})",
        fixture.fixture_id, fixture.checksum
    );
    Ok(())
}

pub fn handle_export_review_feedback(
    output: PathBuf,
    learning_db: PathBuf,
    include_artifact_paths: bool,
) -> Result<()> {
    let policy = PrivacyPolicy {
        allow_review_artifact_upload: include_artifact_paths,
        ..PrivacyPolicy::default()
    };
    let export = with_db(&learning_db, |rt, db| {
        rt.block_on(db.export_review_feedback(&policy))
    })?;
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(&output, serde_json::to_vec_pretty(&export)?)?;
    tracing::info!(output = %output.display(), "review feedback exported");
    println!("Review feedback exported to {}", output.display());
    Ok(())
}
