use anyhow::{Context, Result};
use libsql::{Builder, Connection, Value};
use std::path::Path;

use crate::{gap_store, threshold_store};

pub mod queries;
pub mod types;

#[cfg(test)]
mod tests;

pub use queries::{CalibrationReportRow, ExperimentRow, ProfileVersionRow};
pub use types::{FalsePositive, LearningStatistics, SpectralFeatures, VerifiedSegment};

#[derive(Clone)]
#[allow(dead_code)]
pub struct LearningDb {
    pub(crate) conn: Connection,
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

        self.migrate_spectral_features().await?;

        threshold_store::create_threshold_tables(&self.conn).await?;

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

        gap_store::create_gap_tables(&self.conn).await?;

        self.conn
            .execute(
                "CREATE TABLE IF NOT EXISTS experiments (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    experiment_id TEXT NOT NULL UNIQUE,
                    name TEXT NOT NULL,
                    description TEXT,
                    created_at TEXT DEFAULT (datetime('now'))
                )",
                (),
            )
            .await?;

        self.conn
            .execute(
                "CREATE TABLE IF NOT EXISTS calibration_reports (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    profile_id TEXT,
                    version INTEGER,
                    records_seen INTEGER,
                    speech_to_non_voice INTEGER,
                    non_voice_to_speech INTEGER,
                    recommended_energy_threshold_delta REAL,
                    experiment_id TEXT REFERENCES experiments(experiment_id) ON DELETE SET NULL,
                    created_at TEXT DEFAULT (datetime('now'))
                )",
                (),
            )
            .await?;

        self.conn
            .execute(
                "CREATE TABLE IF NOT EXISTS profile_versions (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    profile_id TEXT NOT NULL,
                    version INTEGER NOT NULL,
                    config_json TEXT NOT NULL,
                    calibration_report_id INTEGER REFERENCES calibration_reports(id) ON DELETE SET NULL,
                    created_at TEXT DEFAULT (datetime('now')),
                    UNIQUE(profile_id, version)
                )",
                (),
            )
            .await?;

        Ok(())
    }

    async fn migrate_spectral_features(&self) -> Result<()> {
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

        Ok(())
    }

    pub async fn record_gap_decision(&self, decision: gap_store::GapDecision) -> Result<i64> {
        gap_store::record_gap_decision(&self.conn, decision).await
    }

    pub async fn get_gap_decisions(&self, movie_hash: &str) -> Result<Vec<gap_store::GapDecision>> {
        gap_store::get_gap_decisions(&self.conn, movie_hash).await
    }

    pub async fn get_threshold_recommendations(
        &self,
    ) -> Result<threshold_store::ThresholdRecommendation> {
        threshold_store::get_threshold_recommendations(&self.conn).await
    }

    pub async fn record_threshold(
        &self,
        flatness_max: f64,
        entropy_min: f64,
        centroid_min: f64,
        centroid_max: f64,
    ) -> Result<i64> {
        threshold_store::record_threshold(
            &self.conn,
            flatness_max,
            entropy_min,
            centroid_min,
            centroid_max,
        )
        .await
    }

    pub async fn get_latest_threshold(
        &self,
    ) -> Result<Option<threshold_store::ThresholdHistoryEntry>> {
        threshold_store::get_latest_threshold(&self.conn).await
    }
}
