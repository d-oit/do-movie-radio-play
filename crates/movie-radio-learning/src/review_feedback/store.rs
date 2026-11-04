//! Persistence for review decisions and immutable correction events.

use anyhow::{bail, Result};
use libsql::{Connection, Value};

use super::hash::fingerprint;
use super::holdout;
use super::types::*;
use crate::database::LearningDb;

pub(crate) async fn create_review_tables(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS review_decisions (
            decision_id TEXT PRIMARY KEY,
            run_id TEXT NOT NULL,
            source_fingerprint TEXT NOT NULL,
            gap_start_ms INTEGER NOT NULL,
            gap_end_ms INTEGER NOT NULL,
            film TEXT NOT NULL DEFAULT '',
            language TEXT NOT NULL DEFAULT '',
            genre TEXT,
            vad_engine TEXT NOT NULL DEFAULT '',
            cue_source TEXT NOT NULL DEFAULT '',
            acoustic_tags TEXT NOT NULL DEFAULT '[]',
            visual_evidence TEXT,
            confidence REAL NOT NULL DEFAULT 0.0,
            narration TEXT,
            tts_output TEXT,
            assembly_position_ms INTEGER,
            model_versions TEXT NOT NULL DEFAULT '{}',
            label TEXT NOT NULL,
            reviewer TEXT NOT NULL,
            source TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
        (),
    )
    .await?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS correction_events (
            event_id INTEGER PRIMARY KEY AUTOINCREMENT,
            decision_id TEXT NOT NULL REFERENCES review_decisions(decision_id),
            label TEXT NOT NULL,
            original_narration TEXT,
            corrected_narration TEXT,
            reviewer TEXT NOT NULL,
            source TEXT NOT NULL,
            model_versions TEXT NOT NULL,
            payload_hash TEXT NOT NULL,
            recorded_at TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(decision_id, payload_hash)
        )",
        (),
    )
    .await?;

    // Reviewer history is append-only: edits are new events, never rewrites.
    conn.execute(
        "CREATE TRIGGER IF NOT EXISTS correction_events_immutable_update
         BEFORE UPDATE ON correction_events
         BEGIN SELECT RAISE(ABORT, 'correction events are immutable'); END",
        (),
    )
    .await?;
    conn.execute(
        "CREATE TRIGGER IF NOT EXISTS correction_events_immutable_delete
         BEFORE DELETE ON correction_events
         BEGIN SELECT RAISE(ABORT, 'correction events are immutable'); END",
        (),
    )
    .await?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_review_decisions_source
         ON review_decisions(source_fingerprint)",
        (),
    )
    .await?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_correction_events_decision
         ON correction_events(decision_id)",
        (),
    )
    .await?;
    Ok(())
}

impl LearningDb {
    /// Imports a review batch. Validation and holdout checks run before any
    /// write, so a corrupted or protected batch cannot partially import.
    pub async fn import_review_feedback(
        &self,
        batch: &ReviewFeedbackBatch,
        options: &ImportOptions,
    ) -> Result<ImportSummary> {
        if batch.schema_version > REVIEW_FEEDBACK_SCHEMA_VERSION {
            bail!(
                "unsupported review feedback schema version {}",
                batch.schema_version
            );
        }
        if batch.decisions.is_empty() {
            bail!("review feedback batch contains no decisions");
        }

        let mut prepared = Vec::with_capacity(batch.decisions.len());
        for decision in &batch.decisions {
            validate_decision(decision, options)?;
            holdout::assert_not_holdout(&self.conn, decision).await?;
            prepared.push(decision.clone());
        }

        let mut summary = ImportSummary {
            decisions_seen: prepared.len(),
            dry_run: options.dry_run,
            ..ImportSummary::default()
        };
        if options.dry_run {
            for decision in &prepared {
                let id = decision.stable_id();
                if decision_exists(&self.conn, &id).await? {
                    summary.decisions_duplicate += 1;
                } else {
                    summary.decisions_imported += 1;
                }
                let reviewer = effective_reviewer(decision, options);
                let payload_hash = event_payload_hash(decision, &reviewer);
                if event_exists(&self.conn, &id, &payload_hash).await? {
                    summary.events_duplicate += 1;
                } else {
                    summary.events_recorded += 1;
                }
            }
            return Ok(summary);
        }

        for decision in &prepared {
            let id = decision.stable_id();
            let reviewer = effective_reviewer(decision, options);
            let inserted = insert_decision(&self.conn, decision, &id, &reviewer).await?;
            if inserted {
                summary.decisions_imported += 1;
            } else {
                summary.decisions_duplicate += 1;
            }

            let payload_hash = event_payload_hash(decision, &reviewer);
            let recorded =
                insert_event(&self.conn, decision, &id, &reviewer, &payload_hash).await?;
            if recorded {
                summary.events_recorded += 1;
            } else {
                summary.events_duplicate += 1;
            }
        }
        Ok(summary)
    }
}

fn validate_decision(decision: &ReviewDecision, options: &ImportOptions) -> Result<()> {
    if decision.run_id.trim().is_empty() {
        bail!("review decision is missing run_id");
    }
    if decision.source_fingerprint.trim().is_empty() {
        bail!("review decision is missing source_fingerprint");
    }
    if decision.gap_start_ms >= decision.gap_end_ms {
        bail!(
            "review decision gap interval is empty or inverted: {}..{}",
            decision.gap_start_ms,
            decision.gap_end_ms
        );
    }
    if decision.reviewer.trim().is_empty() && options.reviewer_override.is_none() {
        bail!("review decision is missing reviewer");
    }
    if decision.label == ReviewLabel::EditNarration
        && decision
            .edited_narration
            .as_deref()
            .unwrap_or("")
            .trim()
            .is_empty()
    {
        bail!("edit_narration review decision is missing edited_narration");
    }
    let derived = decision_id(
        &decision.source_fingerprint,
        &decision.run_id,
        decision.gap_start_ms,
        decision.gap_end_ms,
    );
    if !decision.decision_id.is_empty() && decision.decision_id != derived {
        bail!(
            "review decision id {} does not match its source/run/interval ({derived})",
            decision.decision_id
        );
    }
    Ok(())
}

fn effective_reviewer(decision: &ReviewDecision, options: &ImportOptions) -> String {
    if decision.reviewer.trim().is_empty() {
        options.reviewer_override.clone().unwrap_or_default()
    } else {
        decision.reviewer.clone()
    }
}

fn event_payload_hash(decision: &ReviewDecision, reviewer: &str) -> String {
    fingerprint(&[
        ("decision", decision.stable_id()),
        ("label", decision.label.as_str().to_string()),
        (
            "edited",
            decision.edited_narration.clone().unwrap_or_default(),
        ),
        ("reviewer", reviewer.to_string()),
        ("source", decision.source.clone()),
        (
            "models",
            serde_json::to_string(&decision.model_versions).unwrap_or_default(),
        ),
    ])
}

async fn insert_decision(
    conn: &Connection,
    decision: &ReviewDecision,
    id: &str,
    reviewer: &str,
) -> Result<bool> {
    let params = vec![
        Value::Text(id.to_string()),
        Value::Text(decision.run_id.clone()),
        Value::Text(decision.source_fingerprint.clone()),
        Value::Integer(decision.gap_start_ms as i64),
        Value::Integer(decision.gap_end_ms as i64),
        Value::Text(decision.film.clone()),
        Value::Text(decision.language.clone()),
        decision.genre.clone().map_or(Value::Null, Value::Text),
        Value::Text(decision.model_versions.vad_engine.clone()),
        Value::Text(decision.cue_source.clone()),
        Value::Text(serde_json::to_string(&decision.evidence.acoustic_tags)?),
        decision
            .evidence
            .visual_evidence
            .clone()
            .map_or(Value::Null, Value::Text),
        Value::Real(decision.evidence.confidence),
        decision.narration.clone().map_or(Value::Null, Value::Text),
        decision.tts_output.clone().map_or(Value::Null, Value::Text),
        decision
            .assembly_position_ms
            .map_or(Value::Null, |value| Value::Integer(value as i64)),
        Value::Text(serde_json::to_string(&decision.model_versions)?),
        Value::Text(decision.label.as_str().to_string()),
        Value::Text(reviewer.to_string()),
        Value::Text(decision.source.clone()),
    ];
    let inserted = conn
        .execute(
            "INSERT OR IGNORE INTO review_decisions (
                decision_id, run_id, source_fingerprint, gap_start_ms, gap_end_ms,
                film, language, genre, vad_engine, cue_source, acoustic_tags,
                visual_evidence, confidence, narration, tts_output,
                assembly_position_ms, model_versions, label, reviewer, source
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
            params,
        )
        .await?;
    Ok(inserted > 0)
}

async fn decision_exists(conn: &Connection, id: &str) -> Result<bool> {
    let mut rows = conn
        .query(
            "SELECT 1 FROM review_decisions WHERE decision_id = ?1",
            [Value::Text(id.to_string())],
        )
        .await?;
    Ok(rows.next().await?.is_some())
}

async fn event_exists(conn: &Connection, id: &str, payload_hash: &str) -> Result<bool> {
    let mut rows = conn
        .query(
            "SELECT 1 FROM correction_events WHERE decision_id = ?1 AND payload_hash = ?2",
            [
                Value::Text(id.to_string()),
                Value::Text(payload_hash.to_string()),
            ],
        )
        .await?;
    Ok(rows.next().await?.is_some())
}

async fn insert_event(
    conn: &Connection,
    decision: &ReviewDecision,
    id: &str,
    reviewer: &str,
    payload_hash: &str,
) -> Result<bool> {
    let inserted = conn
        .execute(
            "INSERT OR IGNORE INTO correction_events (
                decision_id, label, original_narration, corrected_narration,
                reviewer, source, model_versions, payload_hash
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            [
                Value::Text(id.to_string()),
                Value::Text(decision.label.as_str().to_string()),
                decision.narration.clone().map_or(Value::Null, Value::Text),
                decision
                    .edited_narration
                    .clone()
                    .map_or(Value::Null, Value::Text),
                Value::Text(reviewer.to_string()),
                Value::Text(decision.source.clone()),
                Value::Text(serde_json::to_string(&decision.model_versions)?),
                Value::Text(payload_hash.to_string()),
            ],
        )
        .await?;
    Ok(inserted > 0)
}
