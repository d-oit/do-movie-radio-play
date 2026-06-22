use anyhow::{Context, Result};
use libsql::{Connection, Value};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GapDecision {
    pub movie_hash: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub confidence: f64,
    pub reason: String,
    pub priority: u32,
    pub user_approved: Option<bool>,
}

pub(crate) async fn create_gap_tables(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS gap_decisions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            movie_hash TEXT NOT NULL,
            start_ms INTEGER NOT NULL,
            end_ms INTEGER NOT NULL,
            confidence REAL NOT NULL,
            reason TEXT,
            priority INTEGER NOT NULL,
            user_approved INTEGER,
            created_at TEXT DEFAULT (datetime('now'))
        )",
        (),
    )
    .await?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_gap_movie ON gap_decisions(movie_hash)",
        (),
    )
    .await?;

    Ok(())
}

pub(crate) async fn record_gap_decision(conn: &Connection, decision: GapDecision) -> Result<i64> {
    let approved: Option<i64> = decision.user_approved.map(|b| if b { 1 } else { 0 });

    conn.execute(
        "INSERT INTO gap_decisions (
            movie_hash, start_ms, end_ms, confidence, reason, priority, user_approved
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        [
            Value::Text(decision.movie_hash),
            Value::Integer(decision.start_ms),
            Value::Integer(decision.end_ms),
            Value::Real(decision.confidence),
            Value::Text(decision.reason),
            Value::Integer(decision.priority as i64),
            approved.map(Value::Integer).unwrap_or(Value::Null),
        ],
    )
    .await?;

    let mut rows = conn.query("SELECT last_insert_rowid()", ()).await?;
    let row = rows
        .next()
        .await?
        .context("failed to get last insert rowid")?;
    let last_id: i64 = row.get(0)?;
    Ok(last_id)
}

pub(crate) async fn get_gap_decisions(
    conn: &Connection,
    movie_hash: &str,
) -> Result<Vec<GapDecision>> {
    let mut results = Vec::new();
    let mut rows = conn
        .query(
            "SELECT movie_hash, start_ms, end_ms, confidence, reason, priority, user_approved
             FROM gap_decisions
             WHERE movie_hash = ?1
             ORDER BY start_ms",
            [Value::Text(movie_hash.to_string())],
        )
        .await?;

    while let Some(row) = rows.next().await? {
        let approved: Option<i64> = row.get(6)?;
        results.push(GapDecision {
            movie_hash: row.get(0)?,
            start_ms: row.get(1)?,
            end_ms: row.get(2)?,
            confidence: row.get(3)?,
            reason: row.get(4)?,
            priority: row.get(5).map(|p: i64| p as u32)?,
            user_approved: approved.map(|a| a == 1),
        });
    }
    Ok(results)
}
