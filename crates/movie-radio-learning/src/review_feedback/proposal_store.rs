//! SQL helpers for learning parameters, proposals and version history.

use anyhow::{Context, Result};
use libsql::{Connection, Value};

use super::types::*;

pub(crate) async fn read_parameter(
    conn: &Connection,
    parameter: &str,
) -> Result<LearningParameter> {
    let mut rows = conn
        .query(
            "SELECT parameter, value, origin, updated_at
             FROM learning_parameters WHERE parameter = ?1",
            [Value::Text(parameter.to_string())],
        )
        .await?;
    let row = rows
        .next()
        .await?
        .with_context(|| format!("unknown learning parameter: {parameter}"))?;
    let origin: String = row.get(2)?;
    Ok(LearningParameter {
        parameter: row.get(0)?,
        value: row.get(1)?,
        origin: ProposalOrigin::parse(&origin)?,
        updated_at: row.get(3)?,
    })
}

pub(crate) async fn update_parameter(
    conn: &Connection,
    parameter: &str,
    value: &str,
    origin: ProposalOrigin,
) -> Result<()> {
    conn.execute(
        "UPDATE learning_parameters
         SET value = ?1, origin = ?2, updated_at = datetime('now')
         WHERE parameter = ?3",
        [
            Value::Text(value.to_string()),
            Value::Text(origin.as_str().to_string()),
            Value::Text(parameter.to_string()),
        ],
    )
    .await?;
    Ok(())
}

pub(crate) async fn insert_proposal(conn: &Connection, proposal: &LearningProposal) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO learning_proposals (
            proposal_id, kind, parameter, current_value, proposed_value,
            evidence, expected_effect, origin, status
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        [
            Value::Text(proposal.proposal_id.clone()),
            Value::Text(proposal.kind.as_str().to_string()),
            Value::Text(proposal.parameter.clone()),
            Value::Text(proposal.current_value.clone()),
            Value::Text(proposal.proposed_value.clone()),
            Value::Text(serde_json::to_string(&proposal.evidence)?),
            Value::Text(proposal.expected_effect.clone()),
            Value::Text(proposal.origin.as_str().to_string()),
            Value::Text(proposal.status.as_str().to_string()),
        ],
    )
    .await?;
    Ok(())
}

pub(crate) async fn update_proposal_status(
    conn: &Connection,
    proposal_id: &str,
    status: ProposalStatus,
) -> Result<()> {
    let sql = match status {
        ProposalStatus::Applied => {
            "UPDATE learning_proposals SET status = ?1, applied_at = datetime('now')
             WHERE proposal_id = ?2"
        }
        ProposalStatus::RolledBack => {
            "UPDATE learning_proposals SET status = ?1, rolled_back_at = datetime('now')
             WHERE proposal_id = ?2"
        }
        ProposalStatus::Proposed => {
            "UPDATE learning_proposals SET status = ?1 WHERE proposal_id = ?2"
        }
    };
    conn.execute(
        sql,
        [
            Value::Text(status.as_str().to_string()),
            Value::Text(proposal_id.to_string()),
        ],
    )
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn insert_version(
    conn: &Connection,
    proposal_id: &str,
    parameter: &str,
    prior_value: Option<&str>,
    new_value: Option<&str>,
    prior_origin: Option<ProposalOrigin>,
    new_origin: Option<ProposalOrigin>,
    action: VersionAction,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO learning_versions (
            proposal_id, parameter, prior_value, new_value,
            prior_origin, new_origin, action
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        [
            Value::Text(proposal_id.to_string()),
            Value::Text(parameter.to_string()),
            prior_value.map_or(Value::Null, |value| Value::Text(value.to_string())),
            new_value.map_or(Value::Null, |value| Value::Text(value.to_string())),
            prior_origin.map_or(Value::Null, |origin| {
                Value::Text(origin.as_str().to_string())
            }),
            new_origin.map_or(Value::Null, |origin| {
                Value::Text(origin.as_str().to_string())
            }),
            Value::Text(action.as_str().to_string()),
        ],
    )
    .await?;
    let mut rows = conn.query("SELECT last_insert_rowid()", ()).await?;
    let row = rows
        .next()
        .await?
        .context("failed to get learning version rowid")?;
    Ok(row.get(0)?)
}

pub(crate) async fn read_version(conn: &Connection, version_id: i64) -> Result<LearningVersion> {
    let mut rows = conn
        .query(
            "SELECT version_id, proposal_id, parameter, prior_value, new_value,
                    prior_origin, new_origin, action, recorded_at
             FROM learning_versions WHERE version_id = ?1",
            [Value::Integer(version_id)],
        )
        .await?;
    let row = rows
        .next()
        .await?
        .context("learning version disappeared after insert")?;
    read_version_row(&row)
}

pub(crate) async fn latest_apply_version(
    conn: &Connection,
    proposal_id: &str,
) -> Result<Option<LearningVersion>> {
    let mut rows = conn
        .query(
            "SELECT version_id, proposal_id, parameter, prior_value, new_value,
                    prior_origin, new_origin, action, recorded_at
             FROM learning_versions
             WHERE proposal_id = ?1 AND action = 'apply'
             ORDER BY version_id DESC LIMIT 1",
            [Value::Text(proposal_id.to_string())],
        )
        .await?;
    match rows.next().await? {
        Some(row) => Ok(Some(read_version_row(&row)?)),
        None => Ok(None),
    }
}

pub(crate) fn read_proposal_row(row: &libsql::Row) -> Result<LearningProposal> {
    let kind: String = row.get(1)?;
    let evidence_json: String = row.get(5)?;
    let origin: String = row.get(7)?;
    let status: String = row.get(8)?;
    Ok(LearningProposal {
        proposal_id: row.get(0)?,
        kind: ProposalKind::parse(&kind)?,
        parameter: row.get(2)?,
        current_value: row.get(3)?,
        proposed_value: row.get(4)?,
        evidence: serde_json::from_str(&evidence_json).context("corrupted proposal evidence")?,
        expected_effect: row.get(6)?,
        origin: ProposalOrigin::parse(&origin)?,
        status: ProposalStatus::parse(&status)?,
        created_at: row.get(9)?,
        applied_at: row.get(10)?,
        rolled_back_at: row.get(11)?,
    })
}

pub(crate) fn read_version_row(row: &libsql::Row) -> Result<LearningVersion> {
    let prior_origin: Option<String> = row.get(5)?;
    let new_origin: Option<String> = row.get(6)?;
    let action: String = row.get(7)?;
    Ok(LearningVersion {
        version_id: row.get(0)?,
        proposal_id: row.get(1)?,
        parameter: row.get(2)?,
        prior_value: row.get(3)?,
        new_value: row.get(4)?,
        prior_origin: prior_origin
            .map(|value| ProposalOrigin::parse(&value))
            .transpose()?,
        new_origin: new_origin
            .map(|value| ProposalOrigin::parse(&value))
            .transpose()?,
        action: VersionAction::parse(&action)?,
        recorded_at: row.get(8)?,
    })
}
