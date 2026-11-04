//! Read paths, outcome metrics and privacy-aware export for review feedback.

use anyhow::{Context, Result};
use libsql::{Connection, Value};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use super::holdout;
use super::types::*;
use crate::database::LearningDb;

const DECISION_COLUMNS: &str = "decision_id, run_id, source_fingerprint, gap_start_ms, gap_end_ms, \
     film, language, genre, vad_engine, cue_source, acoustic_tags, visual_evidence, confidence, \
     narration, tts_output, assembly_position_ms, model_versions, label, reviewer, source, created_at";

const EVENT_COLUMNS: &str =
    "event_id, decision_id, label, original_narration, corrected_narration, \
     reviewer, source, model_versions, payload_hash, recorded_at";

impl LearningDb {
    pub async fn get_review_decisions(&self, limit: usize) -> Result<Vec<ReviewDecision>> {
        let sql = format!(
            "SELECT {DECISION_COLUMNS} FROM review_decisions
             ORDER BY created_at DESC, decision_id ASC LIMIT ?1"
        );
        read_decisions(&self.conn, &sql, vec![Value::Integer(limit as i64)]).await
    }

    pub async fn get_correction_events(&self, limit: usize) -> Result<Vec<CorrectionEvent>> {
        let sql = format!(
            "SELECT {EVENT_COLUMNS} FROM correction_events
             ORDER BY event_id DESC LIMIT ?1"
        );
        read_events(&self.conn, &sql, vec![Value::Integer(limit as i64)]).await
    }

    pub async fn review_outcome_metrics(&self) -> Result<Vec<OutcomeMetricRow>> {
        let decisions = read_all_decisions(&self.conn).await?;
        let events = read_all_events(&self.conn).await?;
        let mut events_by_decision: HashMap<&str, Vec<&CorrectionEvent>> = HashMap::new();
        for event in &events {
            events_by_decision
                .entry(event.decision_id.as_str())
                .or_default()
                .push(event);
        }

        let mut groups: BTreeMap<(String, String, String, String, String), OutcomeMetricRow> =
            BTreeMap::new();
        for decision in &decisions {
            let key = (
                decision.film.clone(),
                decision.language.clone(),
                decision.genre.clone().unwrap_or_default(),
                decision.model_versions.vad_engine.clone(),
                decision.cue_source.clone(),
            );
            let row = groups.entry(key).or_insert_with(|| OutcomeMetricRow {
                film: decision.film.clone(),
                language: decision.language.clone(),
                genre: decision.genre.clone(),
                vad_engine: decision.model_versions.vad_engine.clone(),
                cue_source: decision.cue_source.clone(),
                decisions: 0,
                corrections: 0,
                accepted: 0,
                rejected: 0,
                label_counts: BTreeMap::new(),
            });
            row.decisions += 1;
            for event in events_by_decision
                .get(decision.decision_id.as_str())
                .map_or(&[][..], Vec::as_slice)
            {
                row.corrections += 1;
                *row.label_counts
                    .entry(event.label.as_str().to_string())
                    .or_insert(0) += 1;
                if event.label == ReviewLabel::Accept {
                    row.accepted += 1;
                } else {
                    row.rejected += 1;
                }
            }
        }
        Ok(groups.into_values().collect())
    }

    pub async fn register_holdout_fixture(&self, fixture: &HoldoutFixture) -> Result<()> {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO holdout_fixtures (fixture_id, path, checksum)
                 VALUES (?1, ?2, ?3)",
                [
                    Value::Text(fixture.fixture_id.clone()),
                    Value::Text(fixture.path.clone()),
                    Value::Text(fixture.checksum.clone()),
                ],
            )
            .await?;
        Ok(())
    }

    pub async fn list_holdout_fixtures(&self) -> Result<Vec<HoldoutFixture>> {
        holdout::list_fixtures(&self.conn).await
    }

    /// Exports decisions and events without media bytes. Artifact paths are
    /// reduced to file names unless the policy explicitly allows them.
    pub async fn export_review_feedback(
        &self,
        policy: &PrivacyPolicy,
    ) -> Result<ReviewFeedbackExport> {
        let mut decisions = read_all_decisions(&self.conn).await?;
        if !policy.allow_review_artifact_upload {
            for decision in &mut decisions {
                decision.tts_output = decision.tts_output.as_deref().map(redact_path);
            }
        }
        Ok(ReviewFeedbackExport {
            schema_version: REVIEW_FEEDBACK_SCHEMA_VERSION,
            decisions,
            correction_events: read_all_events(&self.conn).await?,
            privacy: policy.clone(),
        })
    }
}

pub(crate) async fn read_all_decisions(conn: &Connection) -> Result<Vec<ReviewDecision>> {
    let sql = format!(
        "SELECT {DECISION_COLUMNS} FROM review_decisions
         ORDER BY created_at ASC, decision_id ASC"
    );
    read_decisions(conn, &sql, Vec::new()).await
}

pub(crate) async fn read_all_events(conn: &Connection) -> Result<Vec<CorrectionEvent>> {
    let sql = format!("SELECT {EVENT_COLUMNS} FROM correction_events ORDER BY event_id ASC");
    read_events(conn, &sql, Vec::new()).await
}

async fn read_decisions(
    conn: &Connection,
    sql: &str,
    params: Vec<Value>,
) -> Result<Vec<ReviewDecision>> {
    let mut rows = conn.query(sql, params).await?;
    let mut decisions = Vec::new();
    while let Some(row) = rows.next().await? {
        let tags_json: String = row.get(10)?;
        let models_json: String = row.get(16)?;
        let label: String = row.get(17)?;
        decisions.push(ReviewDecision {
            decision_id: row.get(0)?,
            run_id: row.get(1)?,
            source_fingerprint: row.get(2)?,
            gap_start_ms: row.get::<i64>(3)? as u64,
            gap_end_ms: row.get::<i64>(4)? as u64,
            film: row.get(5)?,
            language: row.get(6)?,
            genre: row.get(7)?,
            cue_source: row.get(9)?,
            evidence: ReviewEvidence {
                acoustic_tags: serde_json::from_str(&tags_json)
                    .context("corrupted acoustic_tags JSON")?,
                visual_evidence: row.get(11)?,
                confidence: row.get(12)?,
            },
            narration: row.get(13)?,
            tts_output: row.get(14)?,
            assembly_position_ms: row.get::<Option<i64>>(15)?.map(|value| value as u64),
            model_versions: serde_json::from_str(&models_json)
                .context("corrupted model_versions JSON")?,
            label: ReviewLabel::parse(&label)?,
            edited_narration: None,
            reviewer: row.get(18)?,
            source: row.get(19)?,
        });
    }
    Ok(decisions)
}

async fn read_events(
    conn: &Connection,
    sql: &str,
    params: Vec<Value>,
) -> Result<Vec<CorrectionEvent>> {
    let mut rows = conn.query(sql, params).await?;
    let mut events = Vec::new();
    while let Some(row) = rows.next().await? {
        let label: String = row.get(2)?;
        let models_json: String = row.get(7)?;
        events.push(CorrectionEvent {
            event_id: row.get(0)?,
            decision_id: row.get(1)?,
            label: ReviewLabel::parse(&label)?,
            original_narration: row.get(3)?,
            corrected_narration: row.get(4)?,
            reviewer: row.get(5)?,
            source: row.get(6)?,
            model_versions: serde_json::from_str(&models_json)
                .context("corrupted model_versions JSON")?,
            payload_hash: row.get(8)?,
            recorded_at: row.get(9)?,
        });
    }
    Ok(events)
}

fn redact_path(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "<redacted>".to_string())
}
