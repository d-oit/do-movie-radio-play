//! Holdout registry and guards that keep evaluation fixtures out of learning.

use anyhow::{bail, Result};
use libsql::{Connection, Value};
use std::path::Path;

use super::types::{HoldoutFixture, ReviewDecision};

pub(crate) async fn create_holdout_tables(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS holdout_fixtures (
            fixture_id TEXT PRIMARY KEY,
            path TEXT NOT NULL,
            checksum TEXT NOT NULL,
            registered_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
        (),
    )
    .await?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_holdout_checksum ON holdout_fixtures(checksum)",
        (),
    )
    .await?;
    Ok(())
}

pub(crate) async fn list_fixtures(conn: &Connection) -> Result<Vec<HoldoutFixture>> {
    let mut rows = conn
        .query(
            "SELECT fixture_id, path, checksum, registered_at
             FROM holdout_fixtures ORDER BY fixture_id ASC",
            (),
        )
        .await?;
    let mut fixtures = Vec::new();
    while let Some(row) = rows.next().await? {
        fixtures.push(HoldoutFixture {
            fixture_id: row.get(0)?,
            path: row.get(1)?,
            checksum: row.get(2)?,
            registered_at: row.get(3)?,
        });
    }
    Ok(fixtures)
}

/// Fails closed when a decision references a registered holdout fixture.
pub(crate) async fn assert_not_holdout(conn: &Connection, decision: &ReviewDecision) -> Result<()> {
    for fixture in list_fixtures(conn).await? {
        if fixture.checksum == decision.source_fingerprint {
            bail!(
                "review decision {} references holdout fixture {} by source fingerprint",
                decision.stable_id(),
                fixture.fixture_id
            );
        }
        if fixture.fixture_id == decision.run_id {
            bail!(
                "review decision {} references holdout fixture {} by run id",
                decision.stable_id(),
                fixture.fixture_id
            );
        }
        if let Some(tts_output) = &decision.tts_output {
            if path_is_within(tts_output, &fixture.path) {
                bail!(
                    "review decision {} references holdout artifact {}",
                    decision.stable_id(),
                    fixture.fixture_id
                );
            }
        }
    }
    Ok(())
}

/// Re-checks proposal evidence at apply time, so a proposal cannot be applied
/// after its source was registered as a holdout.
pub(crate) async fn assert_evidence_not_holdout(
    conn: &Connection,
    decision_ids: &[String],
) -> Result<()> {
    let fixtures = list_fixtures(conn).await?;
    if fixtures.is_empty() {
        return Ok(());
    }
    for decision_id in decision_ids {
        let mut rows = conn
            .query(
                "SELECT source_fingerprint, run_id FROM review_decisions WHERE decision_id = ?1",
                [Value::Text(decision_id.clone())],
            )
            .await?;
        let Some(row) = rows.next().await? else {
            continue;
        };
        let source: String = row.get(0)?;
        let run_id: String = row.get(1)?;
        for fixture in &fixtures {
            if fixture.checksum == source || fixture.fixture_id == run_id {
                bail!(
                    "learning proposal evidence {} is holdout fixture {}",
                    decision_id,
                    fixture.fixture_id
                );
            }
        }
    }
    Ok(())
}

fn path_is_within(path: &str, root: &str) -> bool {
    let path = Path::new(path);
    let root = Path::new(root);
    path == root || path.starts_with(root)
}
