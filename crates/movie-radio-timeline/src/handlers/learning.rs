use anyhow::{bail, Context, Result};
use movie_radio_learning::database;
use std::path::PathBuf;

pub fn handle_learning_stats(
    learning_db: PathBuf,
    radio_play: bool,
    output: Option<PathBuf>,
) -> Result<()> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to create async runtime for learning db")?;
    let db = rt.block_on(database::LearningDb::new(&learning_db))?;
    let stats = rt.block_on(db.get_statistics())?;
    let recommendations = rt.block_on(db.get_threshold_recommendations())?;
    let latest_threshold = rt.block_on(db.get_latest_threshold())?;

    let mut report = serde_json::json!({
        "learning_db": learning_db,
        "statistics": stats,
        "recommendations": recommendations,
        "latest_threshold": latest_threshold,
    });

    if radio_play {
        let run_traces = rt.block_on(db.get_run_traces(20))?;
        let emotion_outcomes = rt.block_on(db.get_emotion_outcomes(None))?;
        let provider_performance = rt.block_on(db.get_provider_performances())?;
        let adaptation_log = rt.block_on(db.get_adaptation_logs(20))?;

        report["radio_play"] = serde_json::json!({
            "run_traces": run_traces,
            "emotion_outcomes": emotion_outcomes,
            "provider_performance": provider_performance,
            "adaptation_log": adaptation_log,
        });
    }

    if let Some(output_path) = output {
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&output_path, serde_json::to_vec_pretty(&report)?)?;
        tracing::info!(output = %output_path.display(), "learning stats written");
    } else {
        println!("{}", serde_json::to_string_pretty(&report)?);
    }
    Ok(())
}

pub fn handle_learning_log(
    last: usize,
    learning_db: PathBuf,
    output: Option<PathBuf>,
) -> Result<()> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to create async runtime for learning db")?;
    let db = rt.block_on(database::LearningDb::new(&learning_db))?;

    let adaptations = rt.block_on(db.get_adaptation_logs(last))?;
    let traces = rt.block_on(db.get_run_traces(last))?;

    let report = serde_json::json!({
        "learning_db": learning_db,
        "adaptation_log": adaptations,
        "run_traces": traces,
    });

    if let Some(output_path) = output {
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&output_path, serde_json::to_vec_pretty(&report)?)?;
        tracing::info!(output = %output_path.display(), "learning log written");
    } else {
        println!("{}", serde_json::to_string_pretty(&report)?);
    }
    Ok(())
}

pub fn handle_reset_learnings(confirm: bool, learning_db: PathBuf) -> Result<()> {
    if !confirm {
        bail!("reset-learnings requires --confirm flag to proceed");
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to create async runtime for learning db")?;
    let db = rt.block_on(database::LearningDb::new(&learning_db))?;

    rt.block_on(db.reset_learnings())?;
    println!("Learnings reset successfully for {}", learning_db.display());
    Ok(())
}

pub fn handle_export_learnings(output: PathBuf, learning_db: PathBuf) -> Result<()> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to create async runtime for learning db")?;
    let db = rt.block_on(database::LearningDb::new(&learning_db))?;

    let export_data = rt.block_on(db.export_learnings())?;

    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&output, serde_json::to_vec_pretty(&export_data)?)?;
    tracing::info!(output = %output.display(), "learnings exported");
    println!("Learnings exported to {}", output.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_learning_handlers() -> Result<()> {
        let dir = tempdir()?;
        let db_path = dir.path().join("learning.db");
        let export_path = dir.path().join("export.json");
        let stats_path = dir.path().join("stats.json");
        let log_path = dir.path().join("log.json");

        handle_export_learnings(export_path.clone(), db_path.clone())?;
        assert!(export_path.exists());

        handle_learning_stats(db_path.clone(), true, Some(stats_path.clone()))?;
        assert!(stats_path.exists());

        handle_learning_log(10, db_path.clone(), Some(log_path.clone()))?;
        assert!(log_path.exists());

        assert!(handle_reset_learnings(false, db_path.clone()).is_err());
        handle_reset_learnings(true, db_path.clone())?;

        Ok(())
    }
}
