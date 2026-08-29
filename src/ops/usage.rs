//! Retrieval usage telemetry (tier-4).
//!
//! Every retrieval command (`query`, `relevant`, `advise`, `brief`) records one
//! `usage_log` row: command, argument, hit count, zero-hit flag, session. The
//! zero-hit rows are the curation feedback loop's highest-value signal — each
//! one is a vocabulary gap that caused a fallback (e.g. dumping a whole table
//! into context). `engrams usage` aggregates; `--misses` ranks the gaps;
//! `--daily` buckets by day.
//!
//! Session grouping uses the `ENGRAMS_SESSION` env var when set; the same value
//! on `session close` ties a rollup to the retrieval rows it summarizes.

use anyhow::Result;
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::str::FromStr;

/// Record one retrieval call. Best-effort: telemetry must never fail or slow a
/// retrieval command, so insert errors are swallowed.
pub fn record(conn: &Connection, command: &str, arg: &str, hits: usize, miss: bool) {
    let ts = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let _ = conn.execute(
        "INSERT INTO usage_log (timestamp, command, arg, hits, miss, session) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            ts,
            command,
            arg,
            hits as i64,
            i64::from(miss),
            std::env::var("ENGRAMS_SESSION").ok(),
        ],
    );
}

pub fn handle(
    conn: &Connection,
    since: Option<String>,
    daily: bool,
    misses: bool,
) -> Result<Value> {
    let since_ts = match &since {
        Some(s) => Some(parse_since(s)?),
        None => None,
    };

    if misses {
        return misses_report(conn, since_ts.as_deref());
    }
    if daily {
        return daily_report(conn, since_ts.as_deref());
    }

    // Per-command rollup. Command-specific naming keeps the JSON self-describing:
    // hits mean different things per command (constraints/decisions/results).
    let mut sql = String::from(
        "SELECT command, COUNT(*), SUM(hits), SUM(miss), COUNT(DISTINCT arg) \
         FROM usage_log",
    );
    if since_ts.is_some() {
        sql.push_str(" WHERE timestamp >= ?1");
    }
    sql.push_str(" GROUP BY command ORDER BY command");
    let mut stmt = conn.prepare(&sql)?;
    let bind_since = since_ts.clone();
    let rows = stmt.query_map(rusqlite::params_from_iter(bind_since.iter()), |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, i64>(4)?,
        ))
    })?;

    let mut commands = serde_json::Map::new();
    let mut total_calls = 0i64;
    for r in rows {
        let (cmd, calls, hits, miss, distinct) = r?;
        total_calls += calls;
        let mut entry = serde_json::Map::new();
        entry.insert("calls".into(), json!(calls));
        match cmd.as_str() {
            "advise" => {
                entry.insert("constraints_fired".into(), json!(hits));
                entry.insert("distinct_targets".into(), json!(distinct));
            }
            "relevant" => {
                entry.insert("decisions_surfaced".into(), json!(hits));
            }
            "query" => {
                entry.insert("zero_hit".into(), json!(miss));
            }
            _ => {
                entry.insert("hits".into(), json!(hits));
            }
        }
        commands.insert(cmd, Value::Object(entry));
    }

    let mut out = serde_json::Map::new();
    if let Some(s) = since_ts {
        out.insert("since".into(), json!(s));
    }
    out.insert("total_calls".into(), json!(total_calls));
    out.insert("commands".into(), Value::Object(commands));
    Ok(Value::Object(out))
}

/// Ranked list of zero-hit retrievals — what agents searched for and didn't
/// find. Each entry is a vocabulary/index gap and a curation target.
fn misses_report(conn: &Connection, since_ts: Option<&str>) -> Result<Value> {
    let mut sql = String::from("SELECT arg, COUNT(*) FROM usage_log WHERE miss = 1");
    if since_ts.is_some() {
        sql.push_str(" AND timestamp >= ?1");
    }
    sql.push_str(" GROUP BY arg ORDER BY COUNT(*) DESC, arg LIMIT 50");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(since_ts.iter()), |row| {
        Ok((row.get::<_, Option<String>>(0)?, row.get::<_, i64>(1)?))
    })?;
    let misses: Vec<Value> = rows
        .filter_map(|r| r.ok())
        .map(|(arg, count)| json!({"query": arg, "count": count}))
        .collect();
    Ok(json!({
        "misses": misses,
        "hint": "each miss is a search that found nothing — a vocabulary gap that likely triggered a file-dump fallback",
    }))
}

/// Per-day, per-command buckets for trend reading.
fn daily_report(conn: &Connection, since_ts: Option<&str>) -> Result<Value> {
    let mut sql = String::from(
        "SELECT substr(timestamp, 1, 10), command, COUNT(*), SUM(hits), SUM(miss) \
         FROM usage_log",
    );
    if since_ts.is_some() {
        sql.push_str(" WHERE timestamp >= ?1");
    }
    sql.push_str(" GROUP BY substr(timestamp, 1, 10), command ORDER BY 1 DESC, command");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(since_ts.iter()), |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, i64>(4)?,
        ))
    })?;
    let days: Vec<Value> = rows
        .filter_map(|r| r.ok())
        .map(|(day, cmd, calls, hits, miss)| {
            json!({
                "day": day,
                "command": cmd,
                "calls": calls,
                "hits": hits,
                "misses": miss,
            })
        })
        .collect();
    Ok(json!({"daily": days}))
}

/// Accept RFC3339 (the codebase convention) or a relative `<n><unit>` span
/// (`2w`, `48h`, `30d`, `3mo`, `1y`).
fn parse_since(s: &str) -> Result<String> {
    if let Ok(dt) = DateTime::<Utc>::from_str(s) {
        return Ok(dt.to_rfc3339_opts(SecondsFormat::Secs, true));
    }
    let rel_err = || anyhow::anyhow!("--since must be RFC3339 or <n><m|h|d|w|mo|y>, got '{}'", s);
    let (num, unit) = s.split_at(
        s.find(|c: char| c.is_ascii_alphabetic())
            .ok_or_else(rel_err)?,
    );
    let n: i64 = num.parse().map_err(|_| rel_err())?;
    if n <= 0 {
        return Err(rel_err());
    }
    let delta = match unit {
        "m" => Duration::minutes(n),
        "h" => Duration::hours(n),
        "d" => Duration::days(n),
        "w" => Duration::weeks(n),
        "mo" => Duration::days(n * 30),
        "y" => Duration::days(n * 365),
        _ => return Err(rel_err()),
    };
    Ok((Utc::now() + delta).to_rfc3339_opts(SecondsFormat::Secs, true))
}
