use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::path::Path;

#[cfg(feature = "analytics")]
use duckdb::Connection;

#[cfg(feature = "analytics")]
pub fn run_calibration_analytics(db_path: &Path) -> Result<Value> {
    let conn = Connection::open_in_memory().context("failed to open in-memory duckdb")?;

    conn.execute_batch("INSTALL sqlite; LOAD sqlite;")?;

    let db_path_str = db_path.to_string_lossy();
    conn.execute(
        &format!("ATTACH '{}' AS learning (TYPE sqlite);", db_path_str),
        [],
    )?;

    let stats: Value = conn.query_row(
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
                "flatness_p95": row.get::<_, Option<f64>>(0)?,
                "entropy_p05": row.get::<_, Option<f64>>(1)?,
                "entropy_p95": row.get::<_, Option<f64>>(2)?,
                "centroid_min": row.get::<_, Option<f64>>(3)?,
                "centroid_max": row.get::<_, Option<f64>>(4)?,
                "sample_size": row.get::<_, i64>(5)?,
            }))
        }
    )?;

    Ok(stats)
}

#[cfg(feature = "analytics")]
pub fn get_learning_stats_analytics(db_path: &Path) -> Result<Value> {
    let conn = Connection::open_in_memory().context("failed to open in-memory duckdb")?;
    conn.execute_batch("INSTALL sqlite; LOAD sqlite;")?;
    let db_path_str = db_path.to_string_lossy();
    conn.execute(
        &format!("ATTACH '{}' AS learning (TYPE sqlite);", db_path_str),
        [],
    )?;

    let base_stats = conn.query_row(
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

    let entropy_histogram = get_histogram(
        &conn,
        "(spectral_features->>'$.spectral_entropy')::DOUBLE",
        0.0,
        10.0,
        10,
    )?;
    let flatness_histogram = get_histogram(
        &conn,
        "(spectral_features->>'$.spectral_flatness')::DOUBLE",
        0.0,
        1.0,
        10,
    )?;

    Ok(json!({
        "summary": base_stats,
        "histograms": {
            "spectral_entropy": entropy_histogram,
            "spectral_flatness": flatness_histogram,
        }
    }))
}

#[cfg(feature = "analytics")]
fn get_histogram(
    conn: &Connection,
    column_expr: &str,
    min: f64,
    max: f64,
    buckets: usize,
) -> Result<Value> {
    let bucket_width = (max - min) / buckets as f64;
    let query = format!(
        "SELECT
            floor(({} - {}) / {}) as bucket,
            COUNT(*) as count
         FROM learning.verified_segments
         WHERE {} BETWEEN {} AND {}
         GROUP BY bucket
         ORDER BY bucket",
        column_expr, min, bucket_width, column_expr, min, max
    );

    let mut stmt = conn.prepare(&query)?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, f64>(0)? as usize, row.get::<_, i64>(1)?))
    })?;

    let mut histogram = vec![0i64; buckets];
    for res in rows {
        let (bucket, count) = res?;
        if bucket < buckets {
            histogram[bucket] = count;
        }
    }

    let labels: Vec<String> = (0..buckets)
        .map(|i| {
            format!(
                "{:.2}-{:.2}",
                min + i as f64 * bucket_width,
                min + (i + 1) as f64 * bucket_width
            )
        })
        .collect();

    Ok(json!({
        "buckets": labels,
        "counts": histogram,
    }))
}
