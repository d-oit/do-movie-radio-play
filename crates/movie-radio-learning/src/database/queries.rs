use anyhow::{Context, Result};
use libsql::Value;

use super::{FalsePositive, LearningDb, SpectralFeatures, VerifiedSegment};
use crate::{gap_store, threshold_store};

impl LearningDb {
    pub async fn record_gap_decision(&self, decision: gap_store::GapDecision) -> Result<i64> {
        gap_store::record_gap_decision(&self.conn, decision).await
    }

    pub async fn get_gap_decisions(&self, movie_hash: &str) -> Result<Vec<gap_store::GapDecision>> {
        gap_store::get_gap_decisions(&self.conn, movie_hash).await
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

    pub async fn get_statistics(&self) -> Result<super::LearningStatistics> {
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

        Ok(super::LearningStatistics {
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
        fingerprints: &[movie_radio_types::Fingerprint],
    ) -> Result<()> {
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

    pub async fn get_latest_threshold(
        &self,
    ) -> Result<Option<threshold_store::ThresholdHistoryEntry>> {
        threshold_store::get_latest_threshold(&self.conn).await
    }
}
