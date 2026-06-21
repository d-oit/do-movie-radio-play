use anyhow::{Context, Result};
use libsql::{Builder, Connection, Value};
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::learning::adaptive_thresholds::RecommendationConfidence;
use crate::pipeline::features::FeatureSet;

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

#[derive(Clone)]
#[allow(dead_code)]
pub struct LearningDb {
    conn: Connection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifiedSegment {
    pub start_ms: i64,
    pub end_ms: i64,
    pub confidence: f64,
    pub spectral_features: SpectralFeatures,
    pub was_false_positive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SpectralFeatures {
    pub rms: f64,
    pub zcr: f64,
    pub spectral_flux: f64,
    pub spectral_flatness: f64,
    pub spectral_entropy: f64,
    pub centroid_hz: f64,
    pub low_band_ratio: f64,
    pub high_band_ratio: f64,
}

impl From<FeatureSet> for SpectralFeatures {
    fn from(fs: FeatureSet) -> Self {
        Self {
            rms: fs.rms as f64,
            zcr: fs.zcr as f64,
            spectral_flux: fs.spectral_flux as f64,
            spectral_flatness: fs.spectral_flatness as f64,
            spectral_entropy: fs.spectral_entropy as f64,
            centroid_hz: fs.centroid_hz as f64,
            low_band_ratio: fs.low_band_ratio as f64,
            high_band_ratio: fs.high_band_ratio as f64,
        }
    }
}

impl From<SpectralFeatures> for FeatureSet {
    fn from(sf: SpectralFeatures) -> Self {
        Self {
            rms: sf.rms as f32,
            zcr: sf.zcr as f32,
            spectral_flux: sf.spectral_flux as f32,
            spectral_flatness: sf.spectral_flatness as f32,
            spectral_entropy: sf.spectral_entropy as f32,
            centroid_hz: sf.centroid_hz as f32,
            low_band_ratio: sf.low_band_ratio as f32,
            high_band_ratio: sf.high_band_ratio as f32,
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct FalsePositive {
    pub id: i64,
    pub start_ms: i64,
    pub end_ms: i64,
    pub confidence: f64,
    pub spectral_features: SpectralFeatures,
    pub timestamp: String,
}

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

#[allow(dead_code)]
impl LearningDb {
    pub async fn new(path: &Path) -> Result<Self> {
        let db_path = path.to_string_lossy().to_string();
        let db = Builder::new_local(&db_path)
            .build()
            .await
            .context("failed to open database")?;
        let conn = db.connect().context("failed to create connection")?;

        let learning_db = Self { conn };
        learning_db.initialize().await?;
        Ok(learning_db)
    }

    async fn initialize(&self) -> Result<()> {
        self.conn.execute("PRAGMA foreign_keys = ON", ()).await?;
        self.conn
            .execute(
                "CREATE TABLE IF NOT EXISTS verified_segments (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    start_ms INTEGER NOT NULL,
                    end_ms INTEGER NOT NULL,
                    confidence REAL NOT NULL,
                    rms REAL NOT NULL DEFAULT 0.0,
                    zcr REAL NOT NULL DEFAULT 0.0,
                    spectral_flux REAL NOT NULL DEFAULT 0.0,
                    spectral_flatness REAL NOT NULL DEFAULT 0.0,
                    spectral_entropy REAL NOT NULL DEFAULT 0.0,
                    centroid_hz REAL NOT NULL DEFAULT 0.0,
                    low_band_ratio REAL NOT NULL DEFAULT 0.0,
                    high_band_ratio REAL NOT NULL DEFAULT 0.0,
                    was_false_positive INTEGER NOT NULL DEFAULT 0,
                    timestamp TEXT NOT NULL DEFAULT (datetime('now'))
                )",
                (),
            )
            .await?;

        // Migration logic
        let mut rows = self
            .conn
            .query("PRAGMA table_info(verified_segments)", ())
            .await?;
        let mut existing_columns = std::collections::HashSet::new();
        while let Some(row) = rows.next().await? {
            let name: String = row.get(1)?;
            existing_columns.insert(name);
        }

        let has_spectral_features = existing_columns.contains("spectral_features");

        let feature_cols = [
            "rms",
            "zcr",
            "spectral_flux",
            "spectral_flatness",
            "spectral_entropy",
            "centroid_hz",
            "low_band_ratio",
            "high_band_ratio",
        ];

        for col in feature_cols {
            if !existing_columns.contains(col) {
                self.conn
                    .execute(
                        &format!(
                            "ALTER TABLE verified_segments ADD COLUMN {} REAL NOT NULL DEFAULT 0.0",
                            col
                        ),
                        (),
                    )
                    .await?;
            }
        }

        if has_spectral_features {
            let tx = self.conn.transaction().await?;
            {
                let mut rows = tx
                    .query(
                        "SELECT id, spectral_features FROM verified_segments WHERE spectral_features IS NOT NULL AND spectral_features != ''",
                        (),
                    )
                    .await?;
                while let Some(row) = rows.next().await? {
                    let id: i64 = row.get(0)?;
                    let features_json: String = row.get(1)?;
                    let features: SpectralFeatures = serde_json::from_str(&features_json)
                        .context("failed to parse spectral features during migration")?;

                    tx.execute(
                        "UPDATE verified_segments SET
                            rms = ?1, zcr = ?2, spectral_flux = ?3, spectral_flatness = ?4,
                            spectral_entropy = ?5, centroid_hz = ?6, low_band_ratio = ?7, high_band_ratio = ?8
                         WHERE id = ?9",
                        [
                            Value::Real(features.rms),
                            Value::Real(features.zcr),
                            Value::Real(features.spectral_flux),
                            Value::Real(features.spectral_flatness),
                            Value::Real(features.spectral_entropy),
                            Value::Real(features.centroid_hz),
                            Value::Real(features.low_band_ratio),
                            Value::Real(features.high_band_ratio),
                            Value::Integer(id),
                        ],
                    )
                    .await?;
                }
            }
            tx.commit().await?;

            self.conn
                .execute(
                    "ALTER TABLE verified_segments DROP COLUMN spectral_features",
                    (),
                )
                .await?;
        }

        self.conn
            .execute(
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

        self.conn
            .execute(
                "CREATE INDEX IF NOT EXISTS idx_fp ON verified_segments(was_false_positive) WHERE was_false_positive = 1",
                (),
            )
            .await?;

        self.conn
            .execute(
                "CREATE TABLE IF NOT EXISTS segment_fingerprints (
                    id          INTEGER PRIMARY KEY AUTOINCREMENT,
                    hash        INTEGER NOT NULL,
                    offset_ms   INTEGER NOT NULL,
                    segment_id  INTEGER REFERENCES verified_segments(id) ON DELETE CASCADE,
                    created_at  TEXT DEFAULT (datetime('now'))
                )",
                (),
            )
            .await?;

        self.conn
            .execute(
                "CREATE INDEX IF NOT EXISTS idx_fp_hash ON segment_fingerprints(hash)",
                (),
            )
            .await?;

        self.conn
            .execute(
                "CREATE TABLE IF NOT EXISTS gap_decisions (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    movie_hash TEXT NOT NULL,
                    start_ms INTEGER NOT NULL,
                    end_ms INTEGER NOT NULL,
                    confidence REAL NOT NULL,
                    reason TEXT,
                    priority INTEGER NOT NULL,
                    user_approved INTEGER, -- NULL = undecided, 1 = approved, 0 = rejected
                    created_at TEXT DEFAULT (datetime('now'))
                )",
                (),
            )
            .await?;

        self.conn
            .execute(
                "CREATE INDEX IF NOT EXISTS idx_gap_movie ON gap_decisions(movie_hash)",
                (),
            )
            .await?;

        Ok(())
    }

    pub async fn record_gap_decision(&self, decision: GapDecision) -> Result<i64> {
        let approved: Option<i64> = decision.user_approved.map(|b| if b { 1 } else { 0 });

        self.conn
            .execute(
                "INSERT INTO gap_decisions (
                    movie_hash, start_ms, end_ms, confidence, reason, priority, user_approved
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                [
                    Value::Text(decision.movie_hash),
                    Value::Integer(decision.start_ms),
                    Value::Integer(decision.end_ms),
                    Value::Real(decision.confidence),
                    Value::Text(decision.reason),
                    Value::Integer(decision.priority as i64),
                    approved.map(Value::Integer).unwrap_or(Value::Null),
                ],
            )
            .await?;

        let mut rows = self.conn.query("SELECT last_insert_rowid()", ()).await?;
        let row = rows
            .next()
            .await?
            .context("failed to get last insert rowid")?;
        let last_id: i64 = row.get(0)?;
        Ok(last_id)
    }

    pub async fn get_gap_decisions(&self, movie_hash: &str) -> Result<Vec<GapDecision>> {
        let mut results = Vec::new();
        let mut rows = self
            .conn
            .query(
                "SELECT movie_hash, start_ms, end_ms, confidence, reason, priority, user_approved
                 FROM gap_decisions
                 WHERE movie_hash = ?1
                 ORDER BY start_ms",
                [Value::Text(movie_hash.to_string())],
            )
            .await?;

        while let Some(row) = rows.next().await? {
            let approved: Option<i64> = row.get(6)?;
            results.push(GapDecision {
                movie_hash: row.get(0)?,
                start_ms: row.get(1)?,
                end_ms: row.get(2)?,
                confidence: row.get(3)?,
                reason: row.get(4)?,
                priority: row.get(5).map(|p: i64| p as u32)?,
                user_approved: approved.map(|a| a == 1),
            });
        }
        Ok(results)
    }

    pub async fn record_verification(&self, segment: VerifiedSegment) -> Result<i64> {
        let was_fp: i64 = if segment.was_false_positive { 1 } else { 0 };

        self.conn
            .execute(
                "INSERT INTO verified_segments (
                    start_ms, end_ms, confidence, was_false_positive,
                    rms, zcr, spectral_flux, spectral_flatness,
                    spectral_entropy, centroid_hz, low_band_ratio, high_band_ratio
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                [
                    Value::Integer(segment.start_ms),
                    Value::Integer(segment.end_ms),
                    Value::Real(segment.confidence),
                    Value::Integer(was_fp),
                    Value::Real(segment.spectral_features.rms),
                    Value::Real(segment.spectral_features.zcr),
                    Value::Real(segment.spectral_features.spectral_flux),
                    Value::Real(segment.spectral_features.spectral_flatness),
                    Value::Real(segment.spectral_features.spectral_entropy),
                    Value::Real(segment.spectral_features.centroid_hz),
                    Value::Real(segment.spectral_features.low_band_ratio),
                    Value::Real(segment.spectral_features.high_band_ratio),
                ],
            )
            .await?;

        let mut rows = self.conn.query("SELECT last_insert_rowid()", ()).await?;
        let row = rows
            .next()
            .await?
            .context("failed to get last insert rowid")?;
        let last_id: i64 = row.get(0)?;
        Ok(last_id)
    }

    pub async fn get_false_positives(&self) -> Result<Vec<FalsePositive>> {
        let mut results: Vec<FalsePositive> = Vec::new();

        let mut rows = self
            .conn
            .query(
                "SELECT id, start_ms, end_ms, confidence, timestamp,
                        rms, zcr, spectral_flux, spectral_flatness,
                        spectral_entropy, centroid_hz, low_band_ratio, high_band_ratio
                 FROM verified_segments
                 WHERE was_false_positive = 1
                 ORDER BY timestamp DESC",
                (),
            )
            .await?;

        while let Some(row) = rows.next().await? {
            let spectral_features = SpectralFeatures {
                rms: row.get(5)?,
                zcr: row.get(6)?,
                spectral_flux: row.get(7)?,
                spectral_flatness: row.get(8)?,
                spectral_entropy: row.get(9)?,
                centroid_hz: row.get(10)?,
                low_band_ratio: row.get(11)?,
                high_band_ratio: row.get(12)?,
            };

            results.push(FalsePositive {
                id: row.get(0)?,
                start_ms: row.get(1)?,
                end_ms: row.get(2)?,
                confidence: row.get(3)?,
                spectral_features,
                timestamp: row.get(4)?,
            });
        }

        Ok(results)
    }

    pub async fn get_threshold_recommendations(&self) -> Result<ThresholdRecommendation> {
        let mut rows = self
            .conn
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

    pub async fn record_threshold(
        &self,
        flatness_max: f64,
        entropy_min: f64,
        centroid_min: f64,
        centroid_max: f64,
    ) -> Result<i64> {
        self.conn
            .execute(
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

        let mut rows = self.conn.query("SELECT last_insert_rowid()", ()).await?;
        let row = rows
            .next()
            .await?
            .context("failed to get last insert rowid")?;
        let last_id: i64 = row.get(0)?;
        Ok(last_id)
    }

    pub async fn get_total_verifications(&self) -> Result<usize> {
        let mut rows = self
            .conn
            .query("SELECT COUNT(*) FROM verified_segments", ())
            .await?;
        let row = rows.next().await?.context("failed to get count")?;
        let count: i64 = row.get(0)?;
        Ok(count as usize)
    }

    pub async fn get_false_positive_count(&self) -> Result<usize> {
        let mut rows = self
            .conn
            .query(
                "SELECT COUNT(*) FROM verified_segments WHERE was_false_positive = 1",
                (),
            )
            .await?;
        let row = rows.next().await?.context("failed to get count")?;
        let count: i64 = row.get(0)?;
        Ok(count as usize)
    }

    pub async fn get_false_positive_rate(&self) -> Result<f64> {
        let total = self.get_total_verifications().await?;
        if total == 0 {
            return Ok(0.0);
        }
        let fps = self.get_false_positive_count().await?;
        Ok(fps as f64 / total as f64)
    }

    pub async fn get_statistics(&self) -> Result<LearningStatistics> {
        let total = self.get_total_verifications().await?;
        let fps = self.get_false_positive_count().await?;
        let fp_rate = if total > 0 {
            fps as f64 / total as f64
        } else {
            0.0
        };

        let mut rows = self
            .conn
            .query("SELECT COUNT(*) FROM segment_fingerprints", ())
            .await?;
        let row = rows.next().await?.context("failed to get count")?;
        let fingerprint_count: i64 = row.get(0)?;

        let avg_fingerprints_per_segment = if total > 0 {
            fingerprint_count as f64 / total as f64
        } else {
            0.0
        };

        Ok(LearningStatistics {
            total_verifications: total,
            total_false_positives: fps,
            false_positive_rate: fp_rate,
            total_fingerprints: fingerprint_count as usize,
            avg_fingerprints_per_segment,
        })
    }

    pub async fn record_fingerprints(
        &self,
        segment_id: i64,
        fingerprints: &[crate::verification::fingerprint::Fingerprint],
    ) -> Result<()> {
        // Use RAII transaction for batch insertion
        let tx = self.conn.transaction().await?;

        for fp in fingerprints {
            tx.execute(
                "INSERT INTO segment_fingerprints (hash, offset_ms, segment_id) VALUES (?1, ?2, ?3)",
                [
                    Value::Integer(fp.hash as i64),
                    Value::Integer(fp.offset_ms as i64),
                    Value::Integer(segment_id),
                ],
            )
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    pub async fn find_fingerprint_matches(&self, hashes: &[u32]) -> Result<Vec<(i64, u32, u32)>> {
        if hashes.is_empty() {
            return Ok(Vec::new());
        }

        let mut results = Vec::new();

        // Chunk hashes to avoid exceeding SQLITE_MAX_VARIABLE_NUMBER
        for chunk in hashes.chunks(900) {
            let placeholders = chunk
                .iter()
                .enumerate()
                .map(|(i, _)| format!("?{}", i + 1))
                .collect::<Vec<_>>()
                .join(",");
            let sql = format!(
                "SELECT segment_id, hash, offset_ms FROM segment_fingerprints WHERE hash IN ({})",
                placeholders
            );

            let params: Vec<Value> = chunk.iter().map(|&h| Value::Integer(h as i64)).collect();
            let mut rows = self.conn.query(&sql, params).await?;

            while let Some(row) = rows.next().await? {
                results.push((row.get(0)?, row.get(1)?, row.get(2)?));
            }
        }

        Ok(results)
    }

    pub async fn get_latest_threshold(&self) -> Result<Option<ThresholdHistoryEntry>> {
        let mut rows = self
            .conn
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningStatistics {
    pub total_verifications: usize,
    pub total_false_positives: usize,
    pub false_positive_rate: f64,
    pub total_fingerprints: usize,
    pub avg_fingerprints_per_segment: f64,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GapDecision {
    pub movie_hash: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub confidence: f64,
    pub reason: String,
    pub priority: u32,
    pub user_approved: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn setup_test_db_path() -> NamedTempFile {
        NamedTempFile::new().unwrap()
    }

    #[tokio::test]
    async fn test_initialize_creates_tables() {
        let temp_file = setup_test_db_path();
        let db = LearningDb::new(temp_file.path()).await.unwrap();
        let stats = db.get_statistics().await.unwrap();
        assert_eq!(stats.total_verifications, 0);
    }

    #[tokio::test]
    async fn test_record_verification() {
        let temp_file = setup_test_db_path();
        let db = LearningDb::new(temp_file.path()).await.unwrap();

        let segment = VerifiedSegment {
            start_ms: 1000,
            end_ms: 2000,
            confidence: 0.85,
            spectral_features: SpectralFeatures {
                rms: 0.01,
                zcr: 0.1,
                spectral_flux: 0.5,
                spectral_flatness: 0.3,
                spectral_entropy: 4.5,
                centroid_hz: 1500.0,
                low_band_ratio: 0.6,
                high_band_ratio: 0.1,
            },
            was_false_positive: false,
        };

        let id = db.record_verification(segment).await.unwrap();
        assert!(id > 0);

        let stats = db.get_statistics().await.unwrap();
        assert_eq!(stats.total_verifications, 1);
        assert_eq!(stats.total_false_positives, 0);
    }

    #[tokio::test]
    async fn test_record_false_positive() {
        let temp_file = setup_test_db_path();
        let db = LearningDb::new(temp_file.path()).await.unwrap();

        let segment = VerifiedSegment {
            start_ms: 3000,
            end_ms: 4000,
            confidence: 0.6,
            spectral_features: SpectralFeatures {
                rms: 0.005,
                zcr: 0.2,
                spectral_flux: 0.3,
                spectral_flatness: 0.7,
                spectral_entropy: 2.5,
                centroid_hz: 800.0,
                low_band_ratio: 0.8,
                high_band_ratio: 0.05,
            },
            was_false_positive: true,
        };

        db.record_verification(segment).await.unwrap();

        let fps = db.get_false_positives().await.unwrap();
        assert_eq!(fps.len(), 1);
        assert_eq!(fps[0].start_ms, 3000);
    }

    #[tokio::test]
    async fn test_threshold_recommendations_low_samples() {
        let temp_file = setup_test_db_path();
        let db = LearningDb::new(temp_file.path()).await.unwrap();

        let rec = db.get_threshold_recommendations().await.unwrap();
        assert_eq!(rec.sample_size, 0);
        assert!(rec.confidence == RecommendationConfidence::Low);
    }

    #[tokio::test]
    async fn test_threshold_recommendations_from_fps() {
        let temp_file = setup_test_db_path();
        let db = LearningDb::new(temp_file.path()).await.unwrap();

        for i in 0..10 {
            let segment = VerifiedSegment {
                start_ms: i as i64 * 1000,
                end_ms: (i + 1) as i64 * 1000,
                confidence: 0.7,
                spectral_features: SpectralFeatures {
                    rms: 0.008 + i as f64 * 0.001,
                    zcr: 0.15,
                    spectral_flux: 0.4,
                    spectral_flatness: 0.55 + i as f64 * 0.01,
                    spectral_entropy: 3.0 + i as f64 * 0.1,
                    centroid_hz: 1200.0 + i as f64 * 50.0,
                    low_band_ratio: 0.65,
                    high_band_ratio: 0.08,
                },
                was_false_positive: true,
            };
            db.record_verification(segment).await.unwrap();
        }

        let rec = db.get_threshold_recommendations().await.unwrap();
        assert_eq!(rec.sample_size, 10);
        assert!(rec.suggested_flatness_max > 0.55);
        assert!(rec.suggested_entropy_min < 4.0);
        assert!(rec.confidence == RecommendationConfidence::Medium);
    }

    #[tokio::test]
    async fn test_false_positive_rate() {
        let temp_file = setup_test_db_path();
        let db = LearningDb::new(temp_file.path()).await.unwrap();

        for i in 0..5 {
            db.record_verification(VerifiedSegment {
                start_ms: i as i64 * 1000,
                end_ms: (i as i64 + 1) * 1000,
                confidence: 0.8,
                spectral_features: SpectralFeatures::default(),
                was_false_positive: i % 2 == 0,
            })
            .await
            .unwrap();
        }

        let rate = db.get_false_positive_rate().await.unwrap();
        assert_eq!(rate, 0.6);
    }

    #[tokio::test]
    async fn test_migration_from_json_to_columns() {
        let temp_file = setup_test_db_path();
        let db_path = temp_file.path().to_string_lossy().to_string();

        // 1. Manually create an old-style database
        let db = Builder::new_local(&db_path).build().await.unwrap();
        let conn = db.connect().unwrap();
        conn.execute(
            "CREATE TABLE verified_segments (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                start_ms INTEGER NOT NULL,
                end_ms INTEGER NOT NULL,
                confidence REAL NOT NULL,
                spectral_features TEXT NOT NULL,
                was_false_positive INTEGER NOT NULL DEFAULT 0,
                timestamp TEXT NOT NULL DEFAULT (datetime('now'))
            )",
            (),
        )
        .await
        .unwrap();

        let features = SpectralFeatures {
            rms: 0.123,
            zcr: 0.456,
            spectral_flux: 0.789,
            spectral_flatness: 0.111,
            spectral_entropy: 0.222,
            centroid_hz: 333.3,
            low_band_ratio: 0.444,
            high_band_ratio: 0.555,
        };
        let features_json = serde_json::to_string(&features).unwrap();

        conn.execute(
            "INSERT INTO verified_segments (start_ms, end_ms, confidence, spectral_features, was_false_positive)
             VALUES (100, 200, 0.9, ?1, 1)",
            [Value::Text(features_json)],
        )
        .await
        .unwrap();

        // 2. Open it with LearningDb, which should trigger migration
        let learning_db = LearningDb::new(temp_file.path()).await.unwrap();

        // 3. Verify columns exist and data is migrated
        let fps = learning_db.get_false_positives().await.unwrap();
        assert_eq!(fps.len(), 1);
        let fp = &fps[0];
        assert_eq!(fp.spectral_features.rms, 0.123);
        assert_eq!(fp.spectral_features.zcr, 0.456);
        assert_eq!(fp.spectral_features.centroid_hz, 333.3);

        // 4. Verify original column is dropped
        let mut rows = learning_db
            .conn
            .query("PRAGMA table_info(verified_segments)", ())
            .await
            .unwrap();
        let mut has_spectral_features = false;
        while let Some(row) = rows.next().await.unwrap() {
            let name: String = row.get(1).unwrap();
            if name == "spectral_features" {
                has_spectral_features = true;
            }
        }
        assert!(!has_spectral_features);
    }

    #[tokio::test]
    async fn test_foreign_key_enforcement() {
        let temp_file = setup_test_db_path();
        let db = LearningDb::new(temp_file.path()).await.unwrap();

        // Try to insert a fingerprint with a non-existent segment_id (9999)
        let result = db
            .conn
            .execute(
                "INSERT INTO segment_fingerprints (hash, offset_ms, segment_id) VALUES (1, 100, 9999)",
                (),
            )
            .await;

        assert!(
            result.is_err(),
            "Foreign key constraint should have prevented insertion"
        );
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("FOREIGN KEY constraint failed"));
    }

    #[tokio::test]
    async fn test_record_fingerprints_atomicity() {
        let temp_file = setup_test_db_path();
        let db = LearningDb::new(temp_file.path()).await.unwrap();

        let segment = VerifiedSegment {
            start_ms: 1000,
            end_ms: 2000,
            confidence: 0.85,
            spectral_features: SpectralFeatures::default(),
            was_false_positive: false,
        };
        let segment_id = db.record_verification(segment).await.unwrap();

        let fingerprints = vec![
            crate::verification::fingerprint::Fingerprint {
                hash: 1,
                offset_ms: 100,
            },
            crate::verification::fingerprint::Fingerprint {
                hash: 2,
                offset_ms: 200,
            },
        ];

        // 1. Success case
        db.record_fingerprints(segment_id, &fingerprints)
            .await
            .unwrap();

        let mut rows = db
            .conn
            .query(
                "SELECT COUNT(*) FROM segment_fingerprints WHERE segment_id = ?1",
                [Value::Integer(segment_id)],
            )
            .await
            .unwrap();
        let row = rows.next().await.unwrap().unwrap();
        let count: i64 = row.get(0).unwrap();
        assert_eq!(count, 2);

        // 2. Failure case (atomicity)
        let invalid_segment_id = 9999;
        let mixed_fingerprints = vec![
            crate::verification::fingerprint::Fingerprint {
                hash: 3,
                offset_ms: 300,
            },
            crate::verification::fingerprint::Fingerprint {
                hash: 4,
                offset_ms: 400,
            },
        ];

        // This should fail because segment_id 9999 doesn't exist
        let result = db
            .record_fingerprints(invalid_segment_id, &mixed_fingerprints)
            .await;
        assert!(result.is_err());

        // Verify that NO fingerprints with hash 3 or 4 were inserted
        let mut rows = db
            .conn
            .query(
                "SELECT COUNT(*) FROM segment_fingerprints WHERE hash IN (3, 4)",
                (),
            )
            .await
            .unwrap();
        let row = rows.next().await.unwrap().unwrap();
        let count: i64 = row.get(0).unwrap();
        assert_eq!(count, 0, "Batch should have rolled back completely");
    }

    #[tokio::test]
    async fn test_pragma_foreign_keys_is_on() {
        let temp_file = setup_test_db_path();
        let db = LearningDb::new(temp_file.path()).await.unwrap();

        let mut rows = db.conn.query("PRAGMA foreign_keys", ()).await.unwrap();
        let row = rows.next().await.unwrap().unwrap();
        let is_on: i64 = row.get(0).unwrap();
        assert_eq!(is_on, 1);
    }

    #[tokio::test]
    async fn test_gap_decisions_storage() {
        let temp_file = setup_test_db_path();
        let db = LearningDb::new(temp_file.path()).await.unwrap();

        let decision = GapDecision {
            movie_hash: "movie123".to_string(),
            start_ms: 5000,
            end_ms: 8000,
            confidence: 0.9,
            reason: "long silence".to_string(),
            priority: 5,
            user_approved: Some(true),
        };

        let id = db.record_gap_decision(decision.clone()).await.unwrap();
        assert!(id > 0);

        let results = db.get_gap_decisions("movie123").await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].movie_hash, "movie123");
        assert_eq!(results[0].start_ms, 5000);
        assert_eq!(results[0].user_approved, Some(true));
    }
}
