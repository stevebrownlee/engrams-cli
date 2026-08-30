use anyhow::{Context, Result};
use chrono::{SecondsFormat, Utc};
use regex::Regex;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;
use uuid::Uuid;

use crate::cli::PatternCmd;
use crate::models::Pattern;
use crate::ops::link::delete_links_for;

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// Validate the check triple at write time (S2 / R6) and resolve defaults.
/// Returns `(check_kind, check_expr, severity)` as stored.
fn validate_check(
    check_kind: Option<String>,
    check_expr: Option<String>,
    severity: Option<String>,
) -> Result<(Option<String>, Option<String>, String)> {
    let severity = severity.unwrap_or_else(|| "warn".to_string());
    match severity.as_str() {
        "info" | "warn" | "error" => {}
        other => anyhow::bail!("invalid severity '{}': must be info, warn, or error", other),
    }

    match (check_kind.as_deref(), check_expr.as_deref()) {
        (None, None) => Ok((None, None, severity)),
        (Some(k), Some(e)) => match k {
            "regex" => {
                Regex::new(e).map_err(|err| anyhow::anyhow!("invalid regex check: {}", err))?;
                Ok((Some(k.to_string()), Some(e.to_string()), severity))
            }
            "ast" => {
                if e.trim().is_empty() {
                    anyhow::bail!("invalid ast check: expression is empty");
                }
                Ok((Some(k.to_string()), Some(e.to_string()), severity))
            }
            other => anyhow::bail!("invalid check_kind '{}': must be regex or ast", other),
        },
        (Some(_), None) | (None, Some(_)) => {
            anyhow::bail!("--check-kind and --check must be provided together")
        }
    }
}

pub fn handle(conn: &Connection, cmd: PatternCmd, db_path: &std::path::Path) -> Result<Value> {
    match cmd {
        PatternCmd::Log {
            name,
            description,
            tags,
            prs,
            anchors,
            check_kind,
            check_expr,
            severity,
            importance,
        } => {
            let mut resolved_prs = Vec::new();
            for pr in prs {
                resolved_prs.push(crate::ops::pr::resolve_pr_url(&pr)?);
            }

            let (check_kind, check_expr, severity) =
                validate_check(check_kind, check_expr, severity)?;

            let uuid = Uuid::new_v4().to_string();
            let timestamp = now();
            let tags_json = if tags.is_empty() {
                None
            } else {
                Some(serde_json::to_string(&tags)?)
            };

            conn.execute(
                "INSERT INTO system_patterns (uuid, timestamp, name, description, tags, check_kind, check_expr, severity, importance) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) ON CONFLICT(name) DO UPDATE SET description=excluded.description, tags=excluded.tags, timestamp=excluded.timestamp, check_kind=excluded.check_kind, check_expr=excluded.check_expr, severity=excluded.severity, importance=excluded.importance",
                params![uuid, timestamp, name, description, tags_json, check_kind, check_expr, severity, importance.unwrap_or(5)],
            )?;

            let id: i64 = conn.query_row(
                "SELECT id FROM system_patterns WHERE name = ?",
                params![name],
                |r| r.get(0),
            )?;

            if !resolved_prs.is_empty() {
                crate::ops::pr::attach(conn, "system_pattern", id, &resolved_prs)?;
            }
            if !anchors.is_empty() {
                crate::ops::anchor::attach(conn, "system_pattern", id, &anchors)?;
            }
            crate::ops::graph::rebuild::touch_item(conn, "system_pattern", id)?;

            // Write-through (S7): keep any installed omp rulebook in sync.
            crate::ops::rules::write_through(conn, db_path);

            get_pattern(conn, id)
        }
        PatternCmd::List { tags, limit } => {
            if tags.is_empty() {
                let mut stmt = conn.prepare("SELECT id, uuid, name, description, tags, timestamp, check_kind, check_expr, severity, importance, access_count, last_accessed_at, archived, confidence, last_confirmed_at FROM system_patterns ORDER BY id DESC LIMIT ?")?;
                let rows = stmt.query_map(params![limit], parse_pattern_row)?;
                let mut results = Vec::new();
                for r in rows {
                    results.push(r?);
                }
                let mut prs_map = crate::ops::pr::pr_urls_map(conn, "system_pattern")?;
                let mut anchors_map = crate::ops::anchor::anchors_map(conn, "system_pattern")?;
                for d in &mut results {
                    d.pr_urls = prs_map.remove(&d.id).unwrap_or_default();
                    d.anchors = anchors_map.remove(&d.id).unwrap_or_default();
                }
                Ok(serde_json::to_value(results)?)
            } else {
                let placeholders = crate::ops::sql_placeholders(tags.len());
                let query = format!("SELECT id, uuid, name, description, tags, timestamp, check_kind, check_expr, severity, importance, access_count, last_accessed_at, archived, confidence, last_confirmed_at FROM system_patterns WHERE EXISTS (SELECT 1 FROM json_each(system_patterns.tags) WHERE json_each.value IN ({})) ORDER BY id DESC LIMIT ?", placeholders);
                let mut stmt = conn.prepare(&query)?;
                let mut p = Vec::<&dyn rusqlite::ToSql>::new();
                for tag in &tags {
                    p.push(tag);
                }
                p.push(&limit);
                let rows = stmt.query_map(rusqlite::params_from_iter(p), parse_pattern_row)?;
                let mut results = Vec::new();
                for r in rows {
                    results.push(r?);
                }
                let mut prs_map = crate::ops::pr::pr_urls_map(conn, "system_pattern")?;
                let mut anchors_map = crate::ops::anchor::anchors_map(conn, "system_pattern")?;
                for d in &mut results {
                    d.pr_urls = prs_map.remove(&d.id).unwrap_or_default();
                    d.anchors = anchors_map.remove(&d.id).unwrap_or_default();
                }
                Ok(serde_json::to_value(results)?)
            }
        }
        PatternCmd::Get { id } => get_pattern(conn, id),
        PatternCmd::Delete { id } => {
            let tx = conn.unchecked_transaction()?;

            let _: i64 = tx
                .query_row(
                    "SELECT id FROM system_patterns WHERE id = ?",
                    params![id],
                    |r| r.get(0),
                )
                .optional()?
                .context(format!("pattern {} not found", id))?;

            let links_removed = delete_links_for(&tx, "system_pattern", id)?;
            tx.execute(
                "DELETE FROM item_anchors WHERE item_type='system_pattern' AND item_id=?",
                params![id],
            )?;
            let deleted = tx.execute("DELETE FROM system_patterns WHERE id = ?", params![id])?;

            if deleted == 0 {
                anyhow::bail!("pattern {} not found", id);
            }

            tx.commit()?;

            // Write-through (S7): prune removed pattern from any installed rulebook.
            crate::ops::rules::write_through(conn, db_path);

            Ok(serde_json::json!({
                "deleted": true,
                "id": id,
                "links_removed": links_removed
            }))
        }
        PatternCmd::Update {
            id,
            confidence,
            confirm,
            description,
            importance,
        } => {
            let _: i64 = conn
                .query_row(
                    "SELECT id FROM system_patterns WHERE id = ?",
                    params![id],
                    |r| r.get(0),
                )
                .optional()?
                .context(format!("pattern {} not found", id))?;

            let mut sets = Vec::new();
            let mut params_vec: Vec<&dyn rusqlite::ToSql> = Vec::new();
            let now = now();
            let c_val: f64;
            if let Some(c) = confidence {
                if c <= 0.0 || c > 1.0 {
                    anyhow::bail!("confidence must be in (0, 1]");
                }
                c_val = c;
                sets.push("confidence = ?");
                params_vec.push(&c_val);
                sets.push("last_confirmed_at = ?");
                params_vec.push(&now);
            }
            if confirm {
                sets.push("last_confirmed_at = ?");
                params_vec.push(&now);
            }
            if let Some(d) = &description {
                sets.push("description = ?");
                params_vec.push(d);
            }
            let imp_val: i64;
            if let Some(imp) = importance {
                if !(0..=10).contains(&imp) {
                    anyhow::bail!("importance must be between 0 and 10");
                }
                imp_val = imp;
                sets.push("importance = ?");
                params_vec.push(&imp_val);
            }
            if sets.is_empty() {
                anyhow::bail!(
                    "nothing to update: pass --confidence, --confirm, --description, or --importance"
                );
            }
            let sql = format!(
                "UPDATE system_patterns SET {} WHERE id = ?",
                sets.join(", ")
            );
            params_vec.push(&id);
            conn.execute(&sql, rusqlite::params_from_iter(params_vec))?;

            get_pattern(conn, id)
        }
    }
}

pub(crate) fn parse_pattern_row(row: &rusqlite::Row) -> rusqlite::Result<Pattern> {
    let tags_str: Option<String> = row.get(4)?;
    let tags = match tags_str {
        Some(s) => serde_json::from_str(&s).unwrap_or(Value::Null),
        None => Value::Null,
    };
    let confidence: f64 = row.get(13)?;
    let last_confirmed_at: Option<String> = row.get(14)?;
    let timestamp: String = row.get(5)?;
    let effective_confidence = crate::ops::scoring::effective_confidence(
        confidence,
        last_confirmed_at.as_deref(),
        &timestamp,
    );

    Ok(Pattern {
        id: row.get(0)?,
        uuid: row.get(1)?,
        name: row.get(2)?,
        description: row.get(3)?,
        tags: if tags.is_null() { None } else { Some(tags) },
        timestamp,
        check_kind: row.get(6)?,
        check_expr: row.get(7)?,
        severity: row.get(8)?,
        pr_urls: Vec::new(),
        anchors: Vec::new(),
        importance: row.get(9)?,
        access_count: row.get(10)?,
        last_accessed_at: row.get(11)?,
        archived: row.get(12)?,
        confidence,
        effective_confidence,
        last_confirmed_at,
        score: None,
    })
}

fn get_pattern(conn: &Connection, id: i64) -> Result<Value> {
    let mut stmt = conn.prepare(
        "SELECT id, uuid, name, description, tags, timestamp, check_kind, check_expr, severity, importance, access_count, last_accessed_at, archived, confidence, last_confirmed_at FROM system_patterns WHERE id = ?",
    )?;
    let mut pattern = stmt
        .query_row(params![id], parse_pattern_row)
        .optional()?
        .context(format!("pattern {} not found", id))?;
    pattern.pr_urls = crate::ops::pr::pr_urls_for(conn, "system_pattern", id)?;
    pattern.anchors = crate::ops::anchor::anchors_for(conn, "system_pattern", id)?;
    Ok(serde_json::to_value(pattern)?)
}
