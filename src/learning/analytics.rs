use anyhow::{Context, Result};
use std::path::Path;
use serde_json::json;

#[cfg(feature = "analytics")]
use duckdb::Connection;

use crate::learning::calibrator::CalibrationReport;

#[cfg(feature = "analytics")]
pub fn run_calibration_analytics(db_path: &Path) -> Result<CalibrationReport> {
    let conn = Connection::open_in_memory().context("failed to open in-memory duckdb")?;

    conn.execute_batch("INSTALL sqlite; LOAD sqlite;")?;

    let db_path_str = db_path.to_string_lossy();
    conn.execute(&format!("ATTACH '{}' AS learning (TYPE sqlite);", db_path_str), [])?;

    // Note: spectral_features is stored as JSON in the SQLite database.
    // DuckDB's SQLite extension can access it, but we might need to extract fields.
    // For this implementation, we follow the requested design pattern.
    let stats: serde_json::Value = conn.query_row(
        "SELECT
            PERCENTILE_CONT(0.95) WITHIN GROUP (ORDER BY (spectral_features->>'$.spectral_flatness')::DOUBLE) as flatness_p95,
            PERCENTILE_CONT(0.05) WITHIN GROUP (ORDER BY (spectral_features->>'$.spectral_entropy')::DOUBLE) as entropy_p05,
            PERCENTILE_CONT(0.95) WITHIN GROUP (ORDER BY (spectral_features->>'$.spectral_entropy')::DOUBLE) as entropy_p95,
            MIN((spectral_features->>'$.centroid_hz')::DOUBLE) as centroid_min,
            MAX((spectral_features->>'$.centroid_hz')::DOUBLE) as centroid_max,
            COUNT(*) as sample_size
         FROM learning.verified_segments
         WHERE was_false_positive = 1",
        [],
        |row| {
            Ok(json!({
                "flatness_p95": row.get::<_, f64>(0)?,
                "entropy_p05": row.get::<_, f64>(1)?,
                "entropy_p95": row.get::<_, f64>(2)?,
                "centroid_min": row.get::<_, f64>(3)?,
                "centroid_max": row.get::<_, f64>(4)?,
                "sample_size": row.get::<_, i64>(5)?,
            }))
        }
    )?;

    let sample_size = stats["sample_size"].as_i64().unwrap_or(0) as usize;

    Ok(CalibrationReport {
        version: 1,
        profile: "duckdb-analytics".to_string(),
        records_seen: sample_size,
        speech_to_non_voice: 0,
        non_voice_to_speech: 0,
        recommended_energy_threshold_delta: 0.0,
        duckdb_stats: Some(stats),
    })
}

#[cfg(feature = "analytics")]
pub fn get_learning_stats_analytics(db_path: &Path) -> Result<serde_json::Value> {
    let conn = Connection::open_in_memory().context("failed to open in-memory duckdb")?;
    conn.execute_batch("INSTALL sqlite; LOAD sqlite;")?;
    let db_path_str = db_path.to_string_lossy();
    conn.execute(&format!("ATTACH '{}' AS learning (TYPE sqlite);", db_path_str), [])?;

    let stats = conn.query_row(
        "SELECT
            AVG((spectral_features->>'$.spectral_flatness')::DOUBLE),
            AVG((spectral_features->>'$.spectral_entropy')::DOUBLE),
            PERCENTILE_CONT(0.5) WITHIN GROUP (ORDER BY (spectral_features->>'$.spectral_flatness')::DOUBLE),
            PERCENTILE_CONT(0.5) WITHIN GROUP (ORDER BY (spectral_features->>'$.spectral_entropy')::DOUBLE),
            COUNT(*)
         FROM learning.verified_segments",
        [],
        |row| {
            Ok(json!({
                "avg_flatness": row.get::<_, Option<f64>>(0)?,
                "avg_entropy": row.get::<_, Option<f64>>(1)?,
                "median_flatness": row.get::<_, Option<f64>>(2)?,
                "median_entropy": row.get::<_, Option<f64>>(3)?,
                "total_count": row.get::<_, i64>(4)?,
            }))
        }
    )?;

    Ok(stats)
}
