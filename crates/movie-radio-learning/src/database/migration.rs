use anyhow::{Context, Result};
use libsql::Value;

use super::{LearningDb, SpectralFeatures};

impl LearningDb {
    pub(crate) async fn migrate_spectral_features(&self) -> Result<()> {
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
}
