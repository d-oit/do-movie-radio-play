use anyhow::{Context, Result};
use libsql::{Connection, Value};
use serde::{Deserialize, Serialize};

use crate::adaptive_thresholds::RecommendationConfidence;

const DEFAULT_FLATNESS_MAX: f64 = 0.45;
const DEFAULT_ENTROPY_MIN: f64 = 3.5;
const DEFAULT_ENTROPY_MAX: f64 = 7.0;
const DEFAULT_CENTROID_MIN: f64 = 100.0;
const DEFAULT_CENTROID_MAX: f64 = 6000.0;

const RECOMMENDATION_FLATNESS_MIN_AVG: f64 = 0.3;
const RECOMMENDATION_FLATNESS_MULTIPLIER: f64 = 1.2;
const RECOMMENDATION_FLATNESS_CLAMP_MIN: f64 = 0.45;
const RECOMMENDATION_FLATNESS_CLAMP_MAX: f64 = 0.95;

const RECOMMENDATION_ENTROPY_MULTIPLIER: f64 = 0.8;
const RECOMMENDATION_ENTROPY_CLAMP_MIN: f64 = 1.0;
const RECOMMENDATION_ENTROPY_CLAMP_MAX: f64 = 6.0;

const RECOMMENDATION_CENTROID_MIN_MULTIPLIER: f64 = 0.5;
const RECOMMENDATION_CENTROID_MIN_LIMIT: f64 = 50.0;
const RECOMMENDATION_CENTROID_MAX_MULTIPLIER: f64 = 1.5;
const RECOMMENDATION_CENTROID_MAX_LIMIT: f64 = 8000.0;

const RECOMMENDATION_HIGH_CONFIDENCE_SIZE: usize = 20;
const RECOMMENDATION_MEDIUM_CONFIDENCE_SIZE: usize = 5;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThresholdRecommendation {
    pub suggested_flatness_max: f64,
    pub suggested_entropy_min: f64,
    pub suggested_entropy_max: f64,
    pub suggested_centroid_min: f64,
    pub suggested_centroid_max: f64,
    pub confidence: RecommendationConfidence,
    pub sample_size: usize,
}

impl Default for ThresholdRecommendation {
    fn default() -> Self {
        Self {
            suggested_flatness_max: DEFAULT_FLATNESS_MAX,
            suggested_entropy_min: DEFAULT_ENTROPY_MIN,
            suggested_entropy_max: DEFAULT_ENTROPY_MAX,
            suggested_centroid_min: DEFAULT_CENTROID_MIN,
            suggested_centroid_max: DEFAULT_CENTROID_MAX,
            confidence: RecommendationConfidence::Low,
            sample_size: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThresholdHistoryEntry {
    pub id: i64,
    pub flatness_max: f64,
    pub entropy_min: f64,
    pub centroid_min: f64,
    pub centroid_max: f64,
    pub created_at: String,
}

pub(crate) async fn create_threshold_tables(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS threshold_history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            flatness_max REAL NOT NULL,
            entropy_min REAL NOT NULL,
            centroid_min REAL NOT NULL,
            centroid_max REAL NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
        (),
    )
    .await?;
    Ok(())
}

pub(crate) async fn get_threshold_recommendations(
    conn: &Connection,
) -> Result<ThresholdRecommendation> {
    let mut rows = conn
        .query(
            "SELECT
                AVG(spectral_flatness), AVG(spectral_entropy),
                MIN(centroid_hz), MAX(centroid_hz), COUNT(*)
             FROM verified_segments
             WHERE was_false_positive = 1",
            (),
        )
        .await?;

    let Some(row) = rows.next().await? else {
        return Ok(ThresholdRecommendation::default());
    };

    let avg_flatness: Option<f64> = row.get(0)?;
    let avg_entropy: Option<f64> = row.get(1)?;
    let min_centroid: Option<f64> = row.get(2)?;
    let max_centroid: Option<f64> = row.get(3)?;
    let sample_size: i64 = row.get(4)?;

    if sample_size == 0 || avg_flatness.is_none() {
        return Ok(ThresholdRecommendation::default());
    }

    let avg_flatness = avg_flatness.context("missing average flatness")?;
    let avg_entropy = avg_entropy.context("missing average entropy")?;
    let min_centroid = min_centroid.context("missing minimum centroid")?;
    let max_centroid = max_centroid.context("missing maximum centroid")?;

    let suggested_flatness_max = (avg_flatness.max(RECOMMENDATION_FLATNESS_MIN_AVG)
        * RECOMMENDATION_FLATNESS_MULTIPLIER)
        .clamp(
            RECOMMENDATION_FLATNESS_CLAMP_MIN,
            RECOMMENDATION_FLATNESS_CLAMP_MAX,
        );
    let suggested_entropy_min = (avg_entropy * RECOMMENDATION_ENTROPY_MULTIPLIER).clamp(
        RECOMMENDATION_ENTROPY_CLAMP_MIN,
        RECOMMENDATION_ENTROPY_CLAMP_MAX,
    );

    let suggested_centroid_min = (min_centroid * RECOMMENDATION_CENTROID_MIN_MULTIPLIER)
        .max(RECOMMENDATION_CENTROID_MIN_LIMIT);
    let suggested_centroid_max = (max_centroid * RECOMMENDATION_CENTROID_MAX_MULTIPLIER)
        .min(RECOMMENDATION_CENTROID_MAX_LIMIT);

    let confidence = if sample_size >= RECOMMENDATION_HIGH_CONFIDENCE_SIZE as i64 {
        RecommendationConfidence::High
    } else if sample_size >= RECOMMENDATION_MEDIUM_CONFIDENCE_SIZE as i64 {
        RecommendationConfidence::Medium
    } else {
        RecommendationConfidence::Low
    };

    Ok(ThresholdRecommendation {
        suggested_flatness_max,
        suggested_entropy_min,
        suggested_entropy_max: DEFAULT_ENTROPY_MAX,
        suggested_centroid_min,
        suggested_centroid_max,
        confidence,
        sample_size: sample_size as usize,
    })
}

pub(crate) async fn record_threshold(
    conn: &Connection,
    flatness_max: f64,
    entropy_min: f64,
    centroid_min: f64,
    centroid_max: f64,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO threshold_history (flatness_max, entropy_min, centroid_min, centroid_max)
         VALUES (?1, ?2, ?3, ?4)",
        [
            Value::Real(flatness_max),
            Value::Real(entropy_min),
            Value::Real(centroid_min),
            Value::Real(centroid_max),
        ],
    )
    .await?;

    let mut rows = conn.query("SELECT last_insert_rowid()", ()).await?;
    let row = rows
        .next()
        .await?
        .context("failed to get last insert rowid")?;
    let last_id: i64 = row.get(0)?;
    Ok(last_id)
}

pub(crate) async fn get_latest_threshold(
    conn: &Connection,
) -> Result<Option<ThresholdHistoryEntry>> {
    let mut rows = conn
        .query(
            "SELECT id, flatness_max, entropy_min, centroid_min, centroid_max, created_at
             FROM threshold_history
             ORDER BY id DESC
             LIMIT 1",
            (),
        )
        .await?;

    let Some(row) = rows.next().await? else {
        return Ok(None);
    };

    Ok(Some(ThresholdHistoryEntry {
        id: row.get(0)?,
        flatness_max: row.get(1)?,
        entropy_min: row.get(2)?,
        centroid_min: row.get(3)?,
        centroid_max: row.get(4)?,
        created_at: row.get(5)?,
    }))
}
