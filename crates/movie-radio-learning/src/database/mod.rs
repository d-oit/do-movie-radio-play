use anyhow::{Context, Result};
use libsql::{Builder, Connection};
use std::path::Path;

use crate::{gap_store, threshold_store};

pub mod migration;
pub mod queries;
pub mod types;

pub use types::{FalsePositive, LearningStatistics, SpectralFeatures, VerifiedSegment};

#[cfg(test)]
mod tests;

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

        Ok(())
    }
}
