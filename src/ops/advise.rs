//! `engrams advise` — purpose-built pre-edit advisory (v0.11.0).
//!
//! Distinct from `relevant` (which returns full structs, scores, and reinforces
//! on read): `advise` returns ONLY actionable constraints — patterns with check
//! expressions and decisions anchored to the given paths — plus any current
//! violations from `engrams check`. No scores, no progress, no reinforcement.
//! Designed for automatic harness injection: compact, fast, machine-readable.
//!
//! When output is empty, the caller can proceed with no constraints to respect.

use anyhow::Result;
use rusqlite::Connection;
use serde_json::{json, Value};

use crate::ops::anchor;

pub fn handle(
    conn: &Connection,
    paths: Vec<String>,
    staged: bool,
    db_path: &std::path::Path,
) -> Result<Value> {
    let mut query_paths = paths;
    if staged {
        let staged_files = crate::ops::git::staged_files()
            .map_err(|e| anyhow::anyhow!("cannot read staged files: {}", e))?;
        query_paths.extend(staged_files);
    }

    if query_paths.is_empty() {
        if staged {
            return Ok(json!({ "constraints": [], "violations": [] }));
        }
        anyhow::bail!("provide at least one path or --staged");
    }

    let cleaned: Vec<String> = query_paths.iter().map(|p| anchor::clean_path(p)).collect();
    let matched = anchor::query_relevant_ids(conn, &cleaned)?;

    let mut pattern_ids: Vec<i64> = Vec::new();
    let mut decision_ids: Vec<i64> = Vec::new();
    for (itype, id) in &matched {
        match itype.as_str() {
            "system_pattern" => pattern_ids.push(*id),
            "decision" => decision_ids.push(*id),
            _ => {}
        }
    }

    // Build constraint list: patterns first (they're enforceable), then decisions.
    let mut constraints = Vec::new();

    if !pattern_ids.is_empty() {
        let placeholders = pattern_ids
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT id, name, description, check_kind, check_expr, severity, tags \
             FROM system_patterns WHERE id IN ({}) AND archived = 0",
            placeholders
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(&pattern_ids), |row| {
            let tags_str: Option<String> = row.get(6)?;
            let tags: Value = tags_str
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or(Value::Null);
            Ok(json!({
                "type": "pattern",
                "id": row.get::<_, i64>(0)?,
                "name": row.get::<_, String>(1)?,
                "description": row.get::<_, Option<String>>(2)?,
                "check_kind": row.get::<_, Option<String>>(3)?,
                "check_expr": row.get::<_, Option<String>>(4)?,
                "severity": row.get::<_, String>(5)?,
                "tags": tags,
            }))
        })?;
        for r in rows {
            constraints.push(r?);
        }
    }

    if !decision_ids.is_empty() {
        let placeholders = decision_ids
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT id, summary, status, rationale \
             FROM decisions WHERE id IN ({}) AND archived = 0 AND status = 'active'",
            placeholders
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(&decision_ids), |row| {
            Ok(json!({
                "type": "decision",
                "id": row.get::<_, i64>(0)?,
                "summary": row.get::<_, String>(1)?,
                "status": row.get::<_, String>(2)?,
                "rationale": row.get::<_, Option<String>>(3)?,
            }))
        })?;
        for r in rows {
            constraints.push(r?);
        }
    }

    // Run check against the specific files for current violations.
    let violations = crate::ops::check::handle(conn, false, &cleaned, db_path)?;
    let violations = violations
        .get("violations")
        .cloned()
        .unwrap_or_else(|| json!([]));

    Ok(json!({
        "constraints": constraints,
        "violations": violations,
    }))
}
