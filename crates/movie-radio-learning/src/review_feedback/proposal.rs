//! Proposed learning changes, explicit application and versioned rollback.

use anyhow::{bail, Context, Result};
use libsql::{Connection, Value};
use std::collections::{BTreeMap, BTreeSet};

use super::hash::fingerprint;
use super::holdout;
use super::proposal_store::{
    insert_proposal, insert_version, latest_apply_version, read_parameter, read_proposal_row,
    read_version, update_parameter, update_proposal_status,
};
use super::queries::read_all_events;
use super::types::*;
use crate::database::LearningDb;

const DEFAULT_PARAMETERS: &[(&str, &str)] = &[
    ("gap_confidence_threshold", "0.50"),
    ("narration_coverage_ratio", "1.00"),
    ("tag_rule_precision", "0.80"),
    ("timing_offset_ms", "0"),
    ("voice_provider_rank", "0"),
    ("narration_guidance_weight", "1.00"),
];

pub(crate) async fn create_proposal_tables(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS learning_parameters (
            parameter TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            origin TEXT NOT NULL DEFAULT 'manual',
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
        (),
    )
    .await?;
    for (parameter, value) in DEFAULT_PARAMETERS {
        conn.execute(
            "INSERT OR IGNORE INTO learning_parameters (parameter, value, origin)
             VALUES (?1, ?2, 'manual')",
            [
                Value::Text((*parameter).to_string()),
                Value::Text((*value).to_string()),
            ],
        )
        .await?;
    }

    conn.execute(
        "CREATE TABLE IF NOT EXISTS learning_proposals (
            proposal_id TEXT PRIMARY KEY,
            kind TEXT NOT NULL,
            parameter TEXT NOT NULL,
            current_value TEXT NOT NULL,
            proposed_value TEXT NOT NULL,
            evidence TEXT NOT NULL,
            expected_effect TEXT NOT NULL,
            origin TEXT NOT NULL,
            status TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            applied_at TEXT,
            rolled_back_at TEXT
        )",
        (),
    )
    .await?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS learning_versions (
            version_id INTEGER PRIMARY KEY AUTOINCREMENT,
            proposal_id TEXT NOT NULL,
            parameter TEXT NOT NULL,
            prior_value TEXT,
            new_value TEXT,
            prior_origin TEXT,
            new_origin TEXT,
            action TEXT NOT NULL,
            recorded_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
        (),
    )
    .await?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_learning_versions_proposal
         ON learning_versions(proposal_id)",
        (),
    )
    .await?;
    Ok(())
}

impl LearningDb {
    /// Derives proposals from correction events. Proposals are evidence only;
    /// nothing changes until [`Self::apply_learning_proposal`] is called.
    pub async fn derive_learning_proposals(&self) -> Result<Vec<LearningProposal>> {
        let events = read_all_events(&self.conn).await?;
        let mut by_parameter: BTreeMap<&'static str, Vec<&CorrectionEvent>> = BTreeMap::new();
        for event in &events {
            if let Some(parameter) = parameter_for_label(event.label) {
                by_parameter.entry(parameter).or_default().push(event);
            }
        }

        for (parameter, parameter_events) in by_parameter {
            let current = read_parameter(&self.conn, parameter).await?;
            let delta = delta_for(parameter, parameter_events.len());
            let proposed_value = apply_delta(parameter, &current.value, delta)?;
            let decision_ids: Vec<String> = parameter_events
                .iter()
                .map(|event| event.decision_id.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            let mut label_counts = BTreeMap::new();
            for event in &parameter_events {
                *label_counts
                    .entry(event.label.as_str().to_string())
                    .or_insert(0) += 1;
            }
            let proposal = LearningProposal {
                proposal_id: fingerprint(&[
                    ("parameter", parameter.to_string()),
                    ("evidence", decision_ids.join(",")),
                ]),
                kind: kind_for_parameter(parameter),
                parameter: parameter.to_string(),
                current_value: current.value.clone(),
                proposed_value: proposed_value.clone(),
                evidence: ProposalEvidence {
                    decision_ids,
                    label_counts,
                    sample_size: parameter_events.len(),
                },
                expected_effect: expected_effect(
                    parameter,
                    &current.value,
                    &proposed_value,
                    parameter_events.len(),
                ),
                origin: ProposalOrigin::Learned,
                status: ProposalStatus::Proposed,
                created_at: String::new(),
                applied_at: None,
                rolled_back_at: None,
            };
            insert_proposal(&self.conn, &proposal).await?;
        }
        self.get_learning_proposals().await
    }

    pub async fn get_learning_proposals(&self) -> Result<Vec<LearningProposal>> {
        let mut rows = self
            .conn
            .query(
                "SELECT proposal_id, kind, parameter, current_value, proposed_value,
                        evidence, expected_effect, origin, status, created_at,
                        applied_at, rolled_back_at
                 FROM learning_proposals ORDER BY created_at DESC, proposal_id ASC",
                (),
            )
            .await?;
        let mut proposals = Vec::new();
        while let Some(row) = rows.next().await? {
            proposals.push(read_proposal_row(&row)?);
        }
        Ok(proposals)
    }

    pub async fn get_learning_proposal(
        &self,
        proposal_id: &str,
    ) -> Result<Option<LearningProposal>> {
        let mut rows = self
            .conn
            .query(
                "SELECT proposal_id, kind, parameter, current_value, proposed_value,
                        evidence, expected_effect, origin, status, created_at,
                        applied_at, rolled_back_at
                 FROM learning_proposals WHERE proposal_id = ?1",
                [Value::Text(proposal_id.to_string())],
            )
            .await?;
        match rows.next().await? {
            Some(row) => Ok(Some(read_proposal_row(&row)?)),
            None => Ok(None),
        }
    }

    /// Applies a proposal and records the prior value and origin for rollback.
    pub async fn apply_learning_proposal(&self, proposal_id: &str) -> Result<LearningVersion> {
        let proposal = self
            .get_learning_proposal(proposal_id)
            .await?
            .with_context(|| format!("unknown learning proposal: {proposal_id}"))?;
        if proposal.status != ProposalStatus::Proposed {
            bail!(
                "learning proposal {proposal_id} is {} and cannot be applied",
                proposal.status.as_str()
            );
        }
        holdout::assert_evidence_not_holdout(&self.conn, &proposal.evidence.decision_ids).await?;

        let current = read_parameter(&self.conn, &proposal.parameter).await?;
        if current.value != proposal.current_value {
            bail!(
                "learning proposal {proposal_id} is stale: {} is now {} (proposal expected {})",
                proposal.parameter,
                current.value,
                proposal.current_value
            );
        }
        let version_id = insert_version(
            &self.conn,
            proposal_id,
            &proposal.parameter,
            Some(&current.value),
            Some(&proposal.proposed_value),
            Some(current.origin),
            Some(ProposalOrigin::Learned),
            VersionAction::Apply,
        )
        .await?;
        update_parameter(
            &self.conn,
            &proposal.parameter,
            &proposal.proposed_value,
            ProposalOrigin::Learned,
        )
        .await?;
        update_proposal_status(&self.conn, proposal_id, ProposalStatus::Applied).await?;
        read_version(&self.conn, version_id).await
    }

    /// Restores the value and origin recorded by the matching apply event.
    pub async fn rollback_learning_proposal(&self, proposal_id: &str) -> Result<LearningVersion> {
        let proposal = self
            .get_learning_proposal(proposal_id)
            .await?
            .with_context(|| format!("unknown learning proposal: {proposal_id}"))?;
        if proposal.status != ProposalStatus::Applied {
            bail!(
                "learning proposal {proposal_id} is {} and cannot be rolled back",
                proposal.status.as_str()
            );
        }
        let apply_version = latest_apply_version(&self.conn, proposal_id)
            .await?
            .with_context(|| format!("learning proposal {proposal_id} has no apply history"))?;
        let prior_value = apply_version.prior_value.clone().unwrap_or_default();
        let prior_origin = apply_version.prior_origin.unwrap_or(ProposalOrigin::Manual);
        let current = read_parameter(&self.conn, &proposal.parameter).await?;
        if Some(current.value.as_str()) != apply_version.new_value.as_deref() {
            bail!(
                "learning proposal {proposal_id} cannot be rolled back: {} changed since apply",
                proposal.parameter
            );
        }

        let version_id = insert_version(
            &self.conn,
            proposal_id,
            &proposal.parameter,
            Some(&current.value),
            Some(&prior_value),
            Some(current.origin),
            Some(prior_origin),
            VersionAction::Rollback,
        )
        .await?;
        update_parameter(&self.conn, &proposal.parameter, &prior_value, prior_origin).await?;
        update_proposal_status(&self.conn, proposal_id, ProposalStatus::RolledBack).await?;
        read_version(&self.conn, version_id).await
    }

    pub async fn get_learning_versions(
        &self,
        proposal_id: Option<&str>,
    ) -> Result<Vec<LearningVersion>> {
        let (sql, params) = match proposal_id {
            Some(id) => (
                "SELECT version_id, proposal_id, parameter, prior_value, new_value,
                        prior_origin, new_origin, action, recorded_at
                 FROM learning_versions WHERE proposal_id = ?1 ORDER BY version_id ASC",
                vec![Value::Text(id.to_string())],
            ),
            None => (
                "SELECT version_id, proposal_id, parameter, prior_value, new_value,
                        prior_origin, new_origin, action, recorded_at
                 FROM learning_versions ORDER BY version_id ASC",
                Vec::new(),
            ),
        };
        let mut rows = self.conn.query(sql, params).await?;
        let mut versions = Vec::new();
        while let Some(row) = rows.next().await? {
            versions.push(super::proposal_store::read_version_row(&row)?);
        }
        Ok(versions)
    }

    pub async fn list_learning_parameters(&self) -> Result<Vec<LearningParameter>> {
        let mut rows = self
            .conn
            .query(
                "SELECT parameter, value, origin, updated_at
                 FROM learning_parameters ORDER BY parameter ASC",
                (),
            )
            .await?;
        let mut parameters = Vec::new();
        while let Some(row) = rows.next().await? {
            let origin: String = row.get(2)?;
            parameters.push(LearningParameter {
                parameter: row.get(0)?,
                value: row.get(1)?,
                origin: ProposalOrigin::parse(&origin)?,
                updated_at: row.get(3)?,
            });
        }
        Ok(parameters)
    }
}

fn parameter_for_label(label: ReviewLabel) -> Option<&'static str> {
    match label {
        ReviewLabel::Reject | ReviewLabel::DialogueOverlap => Some("gap_confidence_threshold"),
        ReviewLabel::MissingDescription => Some("narration_coverage_ratio"),
        ReviewLabel::EditNarration => Some("narration_guidance_weight"),
        ReviewLabel::IncorrectSoundInference => Some("tag_rule_precision"),
        ReviewLabel::WrongTiming => Some("timing_offset_ms"),
        ReviewLabel::UnacceptableVoice => Some("voice_provider_rank"),
        ReviewLabel::Accept => None,
    }
}

fn kind_for_parameter(parameter: &str) -> ProposalKind {
    match parameter {
        "gap_confidence_threshold" => ProposalKind::GapThreshold,
        "narration_coverage_ratio" => ProposalKind::NarrationCoverage,
        "tag_rule_precision" => ProposalKind::TagRule,
        "timing_offset_ms" => ProposalKind::TimingOffset,
        "voice_provider_rank" => ProposalKind::VoiceRanking,
        _ => ProposalKind::NarrationGuidance,
    }
}

fn delta_for(parameter: &str, count: usize) -> f64 {
    let count = count as f64;
    match parameter {
        "gap_confidence_threshold" => (0.02 * count).min(0.20),
        "narration_coverage_ratio" => (0.05 * count).min(0.50),
        "narration_guidance_weight" => (0.05 * count).min(0.50),
        "tag_rule_precision" => (0.02 * count).min(0.15),
        "timing_offset_ms" => (10.0 * count).min(200.0),
        "voice_provider_rank" => count.min(5.0),
        _ => 0.0,
    }
}

fn apply_delta(parameter: &str, current: &str, delta: f64) -> Result<String> {
    let value: f64 = current
        .parse()
        .with_context(|| format!("parameter {parameter} has non-numeric value {current}"))?;
    let next = value + delta;
    let next = match parameter {
        "gap_confidence_threshold" => next.clamp(0.0, 0.99),
        "narration_coverage_ratio" | "narration_guidance_weight" => next.clamp(0.5, 2.0),
        "tag_rule_precision" => next.clamp(0.5, 0.99),
        "timing_offset_ms" => next.clamp(-500.0, 500.0),
        "voice_provider_rank" => next.clamp(0.0, 10.0),
        _ => next,
    };
    if matches!(parameter, "timing_offset_ms" | "voice_provider_rank") {
        Ok(format!("{}", next.round() as i64))
    } else {
        Ok(format!("{next:.2}"))
    }
}

fn expected_effect(parameter: &str, current: &str, proposed: &str, count: usize) -> String {
    match parameter {
        "gap_confidence_threshold" => format!(
            "Raise gap confidence threshold {current} -> {proposed} to reject {count} reviewer-flagged gap(s)."
        ),
        "narration_coverage_ratio" => format!(
            "Raise narration coverage {current} -> {proposed} to describe {count} gap(s) reviewers marked as missing."
        ),
        "narration_guidance_weight" => format!(
            "Raise narration guidance weight {current} -> {proposed} to follow {count} reviewer edit(s)."
        ),
        "tag_rule_precision" => format!(
            "Raise tag rule precision {current} -> {proposed} to correct {count} sound inference(s)."
        ),
        "timing_offset_ms" => format!(
            "Shift narration timing {current} -> {proposed} ms to correct {count} timing complaint(s)."
        ),
        "voice_provider_rank" => format!(
            "Raise voice provider rank {current} -> {proposed} after {count} unacceptable voice verdict(s)."
        ),
        _ => format!("Adjust {parameter} from {current} to {proposed} from {count} review event(s)."),
    }
}
