use anyhow::{Context, Result};
use libsql::{Builder, Connection, Value};
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::learning::adaptive_thresholds::RecommendationConfidence;
use crate::pipeline::features::FeatureSet;

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
        self.conn
            .execute(
                "CREATE TABLE IF NOT EXISTS verified_segments (
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
            .await?;

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

        Ok(())
    }

    pub async fn record_verification(&self, segment: VerifiedSegment) -> Result<i64> {
        let features_json = serde_json::to_string(&segment.spectral_features)
            .context("failed to serialize spectral features")?;

        let was_fp: i64 = if segment.was_false_positive { 1 } else { 0 };

        self.conn
            .execute(
                "INSERT INTO verified_segments (start_ms, end_ms, confidence, spectral_features, was_false_positive)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                [
                    Value::Integer(segment.start_ms),
                    Value::Integer(segment.end_ms),
                    Value::Real(segment.confidence),
                    Value::Text(features_json),
                    Value::Integer(was_fp),
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
                "SELECT id, start_ms, end_ms, confidence, spectral_features, timestamp
                 FROM verified_segments
                 WHERE was_false_positive = 1
                 ORDER BY timestamp DESC",
                (),
            )
            .await?;

        while let Some(row) = rows.next().await? {
            let features_json: String = row.get(4)?;
            let spectral_features: SpectralFeatures = serde_json::from_str(&features_json)
                .context("failed to parse spectral features")?;

            results.push(FalsePositive {
                id: row.get(0)?,
                start_ms: row.get(1)?,
                end_ms: row.get(2)?,
                confidence: row.get(3)?,
                spectral_features,
                timestamp: row.get(5)?,
            });
        }

        Ok(results)
    }

    pub async fn get_threshold_recommendations(&self) -> Result<ThresholdRecommendation> {
        let fps = self.get_false_positives().await?;
        let sample_size = fps.len();

        if sample_size == 0 {
            return Ok(ThresholdRecommendation {
                suggested_flatness_max: 0.45,
                suggested_entropy_min: 3.5,
                suggested_entropy_max: 7.0,
                suggested_centroid_min: 100.0,
                suggested_centroid_max: 6000.0,
                confidence: RecommendationConfidence::Low,
                sample_size: 0,
            });
        }

        let total_flatness: f64 = fps
            .iter()
            .map(|fp| fp.spectral_features.spectral_flatness)
            .sum();
        let total_entropy: f64 = fps
            .iter()
            .map(|fp| fp.spectral_features.spectral_entropy)
            .sum();
        let total_centroid: f64 = fps.iter().map(|fp| fp.spectral_features.centroid_hz).sum();

        let avg_flatness = total_flatness / sample_size as f64;
        let avg_entropy = total_entropy / sample_size as f64;
        let _avg_centroid = total_centroid / sample_size as f64;

        let suggested_flatness_max = (avg_flatness.max(0.3) * 1.2).clamp(0.45, 0.95);
        let suggested_entropy_min = (avg_entropy * 0.8).clamp(1.0, 6.0);

        let min_centroid = fps
            .iter()
            .map(|fp| fp.spectral_features.centroid_hz)
            .fold(f64::MAX, f64::min);
        let max_centroid = fps
            .iter()
            .map(|fp| fp.spectral_features.centroid_hz)
            .fold(0.0f64, f64::max);

        let suggested_centroid_min = (min_centroid * 0.5).max(50.0);
        let suggested_centroid_max = (max_centroid * 1.5).min(8000.0);

        let confidence = if sample_size >= 20 {
            RecommendationConfidence::High
        } else if sample_size >= 5 {
            RecommendationConfidence::Medium
        } else {
            RecommendationConfidence::Low
        };

        Ok(ThresholdRecommendation {
            suggested_flatness_max,
            suggested_entropy_min,
            suggested_entropy_max: 7.0,
            suggested_centroid_min,
            suggested_centroid_max,
            confidence,
            sample_size,
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

        Ok(LearningStatistics {
            total_verifications: total,
            total_false_positives: fps,
            false_positive_rate: fp_rate,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningStatistics {
    pub total_verifications: usize,
    pub total_false_positives: usize,
    pub false_positive_rate: f64,
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
}
