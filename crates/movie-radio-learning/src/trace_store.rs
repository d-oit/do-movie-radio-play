use anyhow::{Context, Result};
use libsql::{Connection, Value};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunTrace {
    pub id: String,
    pub movie_hash: String,
    #[serde(default)]
    pub created_at: Option<String>,
    pub quality_score: Option<f64>,
    pub total_cost_usd: Option<f64>,
    pub duration_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EmotionOutcome {
    #[serde(default)]
    pub id: Option<i64>,
    pub segment_tag: String,
    pub emotion_used: String,
    pub provider: String,
    pub quality_score: Option<f64>,
    pub user_approved: Option<bool>,
    pub run_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderPerformance {
    #[serde(default)]
    pub id: Option<i64>,
    pub provider: String,
    pub scene_type: Option<String>,
    pub avg_quality: Option<f64>,
    pub avg_latency_ms: Option<i64>,
    pub failure_rate: Option<f64>,
    pub cost_per_char: Option<f64>,
    #[serde(default)]
    pub last_updated: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AdaptationLog {
    #[serde(default)]
    pub id: Option<i64>,
    pub parameter: String,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub reason: Option<String>,
    pub improvement_delta: Option<f64>,
    #[serde(default)]
    pub applied_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningsExport {
    pub threshold_history: Vec<crate::threshold_store::ThresholdHistoryEntry>,
    pub run_traces: Vec<RunTrace>,
    pub emotion_outcomes: Vec<EmotionOutcome>,
    pub provider_performance: Vec<ProviderPerformance>,
    pub adaptation_log: Vec<AdaptationLog>,
}

pub(crate) async fn create_trace_tables(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS run_traces (
            id TEXT PRIMARY KEY,
            movie_hash TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            quality_score REAL,
            total_cost_usd REAL,
            duration_ms INTEGER
        )",
        (),
    )
    .await?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS emotion_outcomes (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            segment_tag TEXT NOT NULL,
            emotion_used TEXT NOT NULL,
            provider TEXT NOT NULL,
            quality_score REAL,
            user_approved INTEGER DEFAULT NULL,
            run_id TEXT REFERENCES run_traces(id)
        )",
        (),
    )
    .await?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS provider_performance (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            provider TEXT NOT NULL,
            scene_type TEXT,
            avg_quality REAL,
            avg_latency_ms INTEGER,
            failure_rate REAL,
            cost_per_char REAL,
            last_updated TEXT DEFAULT (datetime('now'))
        )",
        (),
    )
    .await?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS adaptation_log (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            parameter TEXT NOT NULL,
            old_value TEXT,
            new_value TEXT,
            reason TEXT,
            improvement_delta REAL,
            applied_at TEXT DEFAULT (datetime('now'))
        )",
        (),
    )
    .await?;

    Ok(())
}

pub(crate) async fn record_run_trace(conn: &Connection, trace: &RunTrace) -> Result<()> {
    conn.execute(
        "INSERT INTO run_traces (id, movie_hash, quality_score, total_cost_usd, duration_ms)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        [
            Value::Text(trace.id.clone()),
            Value::Text(trace.movie_hash.clone()),
            trace.quality_score.map(Value::Real).unwrap_or(Value::Null),
            trace.total_cost_usd.map(Value::Real).unwrap_or(Value::Null),
            trace.duration_ms.map(Value::Integer).unwrap_or(Value::Null),
        ],
    )
    .await?;
    Ok(())
}

pub(crate) async fn record_emotion_outcome(
    conn: &Connection,
    outcome: &EmotionOutcome,
) -> Result<i64> {
    let user_app = outcome.user_approved.map(|b| if b { 1i64 } else { 0i64 });
    conn.execute(
        "INSERT INTO emotion_outcomes (segment_tag, emotion_used, provider, quality_score, user_approved, run_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        [
            Value::Text(outcome.segment_tag.clone()),
            Value::Text(outcome.emotion_used.clone()),
            Value::Text(outcome.provider.clone()),
            outcome.quality_score.map(Value::Real).unwrap_or(Value::Null),
            user_app.map(Value::Integer).unwrap_or(Value::Null),
            outcome
                .run_id
                .as_ref()
                .map(|s| Value::Text(s.clone()))
                .unwrap_or(Value::Null),
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

pub(crate) async fn record_provider_performance(
    conn: &Connection,
    perf: &ProviderPerformance,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO provider_performance (provider, scene_type, avg_quality, avg_latency_ms, failure_rate, cost_per_char)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        [
            Value::Text(perf.provider.clone()),
            perf.scene_type
                .as_ref()
                .map(|s| Value::Text(s.clone()))
                .unwrap_or(Value::Null),
            perf.avg_quality.map(Value::Real).unwrap_or(Value::Null),
            perf.avg_latency_ms.map(Value::Integer).unwrap_or(Value::Null),
            perf.failure_rate.map(Value::Real).unwrap_or(Value::Null),
            perf.cost_per_char.map(Value::Real).unwrap_or(Value::Null),
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

pub(crate) async fn record_adaptation_log(
    conn: &Connection,
    log: &AdaptationLog,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO adaptation_log (parameter, old_value, new_value, reason, improvement_delta)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        [
            Value::Text(log.parameter.clone()),
            log.old_value
                .as_ref()
                .map(|s| Value::Text(s.clone()))
                .unwrap_or(Value::Null),
            log.new_value
                .as_ref()
                .map(|s| Value::Text(s.clone()))
                .unwrap_or(Value::Null),
            log.reason
                .as_ref()
                .map(|s| Value::Text(s.clone()))
                .unwrap_or(Value::Null),
            log.improvement_delta.map(Value::Real).unwrap_or(Value::Null),
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

pub(crate) async fn get_run_traces(
    conn: &Connection,
    limit: usize,
) -> Result<Vec<RunTrace>> {
    let mut results = Vec::new();
    let mut rows = conn
        .query(
            "SELECT id, movie_hash, created_at, quality_score, total_cost_usd, duration_ms
             FROM run_traces ORDER BY created_at DESC LIMIT ?1",
            [Value::Integer(limit as i64)],
        )
        .await?;

    while let Some(row) = rows.next().await? {
        results.push(RunTrace {
            id: row.get(0)?,
            movie_hash: row.get(1)?,
            created_at: row.get(2)?,
            quality_score: row.get(3)?,
            total_cost_usd: row.get(4)?,
            duration_ms: row.get(5)?,
        });
    }
    Ok(results)
}

pub(crate) async fn get_adaptation_logs(
    conn: &Connection,
    limit: usize,
) -> Result<Vec<AdaptationLog>> {
    let mut results = Vec::new();
    let mut rows = conn
        .query(
            "SELECT id, parameter, old_value, new_value, reason, improvement_delta, applied_at
             FROM adaptation_log ORDER BY id DESC LIMIT ?1",
            [Value::Integer(limit as i64)],
        )
        .await?;

    while let Some(row) = rows.next().await? {
        results.push(AdaptationLog {
            id: row.get(0)?,
            parameter: row.get(1)?,
            old_value: row.get(2)?,
            new_value: row.get(3)?,
            reason: row.get(4)?,
            improvement_delta: row.get(5)?,
            applied_at: row.get(6)?,
        });
    }
    Ok(results)
}

pub(crate) async fn get_emotion_outcomes(
    conn: &Connection,
    run_id: Option<&str>,
) -> Result<Vec<EmotionOutcome>> {
    let mut results = Vec::new();
    let (sql, params): (&str, Vec<Value>) = match run_id {
        Some(rid) => (
            "SELECT id, segment_tag, emotion_used, provider, quality_score, user_approved, run_id
             FROM emotion_outcomes WHERE run_id = ?1 ORDER BY id ASC",
            vec![Value::Text(rid.to_string())],
        ),
        None => (
            "SELECT id, segment_tag, emotion_used, provider, quality_score, user_approved, run_id
             FROM emotion_outcomes ORDER BY id DESC",
            vec![],
        ),
    };

    let mut rows = conn.query(sql, params).await?;
    while let Some(row) = rows.next().await? {
        let user_app: Option<i64> = row.get(5)?;
        results.push(EmotionOutcome {
            id: row.get(0)?,
            segment_tag: row.get(1)?,
            emotion_used: row.get(2)?,
            provider: row.get(3)?,
            quality_score: row.get(4)?,
            user_approved: user_app.map(|v| v != 0),
            run_id: row.get(6)?,
        });
    }
    Ok(results)
}

pub(crate) async fn get_provider_performances(
    conn: &Connection,
) -> Result<Vec<ProviderPerformance>> {
    let mut results = Vec::new();
    let mut rows = conn
        .query(
            "SELECT id, provider, scene_type, avg_quality, avg_latency_ms, failure_rate, cost_per_char, last_updated
             FROM provider_performance ORDER BY id DESC",
            (),
        )
        .await?;

    while let Some(row) = rows.next().await? {
        results.push(ProviderPerformance {
            id: row.get(0)?,
            provider: row.get(1)?,
            scene_type: row.get(2)?,
            avg_quality: row.get(3)?,
            avg_latency_ms: row.get(4)?,
            failure_rate: row.get(5)?,
            cost_per_char: row.get(6)?,
            last_updated: row.get(7)?,
        });
    }
    Ok(results)
}

pub(crate) async fn reset_learnings(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM threshold_history", ()).await?;
    conn.execute("DELETE FROM adaptation_log", ()).await?;
    Ok(())
}

pub(crate) async fn export_learnings(conn: &Connection) -> Result<LearningsExport> {
    let mut thresholds = Vec::new();
    let mut rows = conn
        .query(
            "SELECT id, flatness_max, entropy_min, centroid_min, centroid_max, created_at
             FROM threshold_history ORDER BY id ASC",
            (),
        )
        .await?;
    while let Some(row) = rows.next().await? {
        thresholds.push(crate::threshold_store::ThresholdHistoryEntry {
            id: row.get(0)?,
            flatness_max: row.get(1)?,
            entropy_min: row.get(2)?,
            centroid_min: row.get(3)?,
            centroid_max: row.get(4)?,
            created_at: row.get(5)?,
        });
    }

    let run_traces = get_run_traces(conn, 1000).await?;
    let emotion_outcomes = get_emotion_outcomes(conn, None).await?;
    let provider_performance = get_provider_performances(conn).await?;
    let adaptation_log = get_adaptation_logs(conn, 1000).await?;

    Ok(LearningsExport {
        threshold_history: thresholds,
        run_traces,
        emotion_outcomes,
        provider_performance,
        adaptation_log,
    })
}
