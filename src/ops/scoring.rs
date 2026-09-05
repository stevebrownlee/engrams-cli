//! Retrieval scoring, prune-decay, and read-path observability (v0.10.0 tier-1).
//!
//! The blended retrieval score combines a recency-decay term (Ebbinghaus-style
//! exponential over age) with a normalized importance term:
//!
//! ```text
//! score = W_RECENCY * exp(-LAMBDA * age_days) + W_IMPORTANCE * (importance / 10)
//! ```
//!
//! Full-text queries fold in an FTS5 BM25 term (see [`query_score_expr`]).
//! References: Generative Agents (arXiv:2304.03442), MemoryBank (arXiv:2305.10250).
//!
//! Read paths (`prime` / `relevant` / `query`) reinforce the records they
//! surface by bumping `access_count` and `last_accessed_at`. `access_count`
//! feeds prune strength, and a NULL `last_accessed_at` marks a record that was
//! written but never surfaced.

use anyhow::Result;
use chrono::{SecondsFormat, Utc};
use rusqlite::Connection;

/// Exponential recency-decay constant (per day). Half-life ≈ ln(2)/LAMBDA ≈ 60 days.
pub const LAMBDA: f64 = 0.01155;

// Weights for the no-query ranking path (`prime` / `relevant`).
pub const W_RECENCY: f64 = 0.6;
pub const W_IMPORTANCE: f64 = 0.4;

// Weights for the FTS query path. Textual relevance dominates.
pub const Q_W_QUERY: f64 = 0.5;
pub const Q_W_RECENCY: f64 = 0.3;
pub const Q_W_IMPORTANCE: f64 = 0.2;

/// Prune-decay: each importance point or read adds this many days of retention strength.
pub const STRENGTH_DAYS: f64 = 30.0;
/// Default prune threshold — retention below this is pruned.
pub const DEFAULT_PRUNE_THRESHOLD: f64 = 0.1;

/// SQL expression for the no-query score, referencing the given columns.
/// `ts_col` is the timestamp column, `imp_col` the importance column.
/// Computed entirely in SQLite: `julianday('now') - julianday(ts)` gives age in days.
pub fn score_expr(ts_col: &str, imp_col: &str) -> String {
    format!(
        "({W_R} * exp(-{LAM} * (julianday('now') - julianday({ts}))) + {W_I} * ({imp}) / 10.0)",
        W_R = W_RECENCY,
        LAM = LAMBDA,
        ts = ts_col,
        W_I = W_IMPORTANCE,
        imp = imp_col,
    )
}

/// SQL expression for the FTS-query score. `rank` is the FTS5 BM25 rank (<= 0,
/// lower = better match), normalized into [0, 1) via `(-rank)/(1 - rank)` so a
/// near-perfect match approaches 1.
pub fn query_score_expr(ts_col: &str, imp_col: &str) -> String {
    format!(
        "({WQ} * ((-1.0 * rank) / (1.0 - rank)) + {WR} * exp(-{LAM} * (julianday('now') - julianday({ts}))) + {WI} * ({imp}) / 10.0)",
        WQ = Q_W_QUERY,
        WR = Q_W_RECENCY,
        LAM = LAMBDA,
        ts = ts_col,
        WI = Q_W_IMPORTANCE,
        imp = imp_col,
    )
}

/// Read-time consolidation confidence (v0.11.0 tier-2): stored confidence
/// decayed exponentially from its last confirmation. Reuses `LAMBDA`
/// (60-day half-life) per spec §4.2. A NULL `last_confirmed_at` anchors
/// decay at the creation `timestamp`. Clamped to [0, 1]; negative ages
/// (clock skew) are treated as 0.
pub fn effective_confidence(
    confidence: f64,
    last_confirmed_at: Option<&str>,
    timestamp: &str,
) -> f64 {
    let anchor = last_confirmed_at.unwrap_or(timestamp);
    let age_days = chrono::DateTime::parse_from_rfc3339(anchor)
        .ok()
        .map(|t| (Utc::now() - t.with_timezone(&Utc)).num_seconds() as f64 / 86_400.0)
        .unwrap_or(0.0)
        .max(0.0);
    (confidence * (-LAMBDA * age_days).exp()).clamp(0.0, 1.0)
}

/// SQL multiplier form of `effective_confidence` for ranking queries:
/// `confidence * exp(-LAMBDA * days_since(coalesce(last_confirmed_at, timestamp)))`,
/// floored at age 0 so clock skew cannot inflate confidence.
pub fn confidence_expr(conf_col: &str, confirmed_col: &str, ts_col: &str) -> String {
    format!(
        "({conf} * exp(-{LAM} * max(0.0, julianday('now') - julianday(coalesce({conf_at}, {ts})))))",
        conf = conf_col,
        LAM = LAMBDA,
        conf_at = confirmed_col,
        ts = ts_col,
    )
}

/// Reinforce access stats for the given row ids. `table` is one of
/// decisions / system_patterns / schemas (any scored row type).
pub fn reinforce(conn: &Connection, table: &str, ids: &[i64]) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    let now = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let placeholders = crate::ops::sql_placeholders(ids.len());
    let sql = format!(
        "UPDATE {t} SET access_count = access_count + 1, last_accessed_at = ? WHERE id IN ({ph})",
        t = table,
        ph = placeholders,
    );
    let mut params: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(ids.len() + 1);
    params.push(&now);
    for id in ids {
        params.push(id);
    }
    conn.execute(&sql, rusqlite::params_from_iter(params))?;
    Ok(())
}
