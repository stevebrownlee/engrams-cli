use anyhow::{Context, Result};
use chrono::{SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;
use uuid::Uuid;

use crate::cli::{DecisionCmd, DecisionUpdateArgs};
use crate::models::Decision;
use crate::ops::link::delete_links_for;

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

pub fn handle(conn: &Connection, cmd: DecisionCmd) -> Result<Value> {
    match cmd {
        DecisionCmd::Log {
            summary,
            rationale,
            details,
            tags,
        } => {
            let uuid = Uuid::new_v4().to_string();
            let timestamp = now();
            let tags_json = if tags.is_empty() {
                None
            } else {
                Some(serde_json::to_string(&tags)?)
            };

            conn.execute(
                "INSERT INTO decisions (uuid, timestamp, summary, rationale, implementation_details, tags) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![uuid, timestamp, summary, rationale, details, tags_json],
            )?;

            let id = conn.last_insert_rowid();
            get_decision(conn, id)
        }
        DecisionCmd::List { tags, limit } => {
            if tags.is_empty() {
                let mut stmt = conn.prepare("SELECT id, uuid, summary, rationale, implementation_details, tags, timestamp FROM decisions ORDER BY id DESC LIMIT ?")?;
                let rows = stmt.query_map(params![limit], parse_decision_row)?;
                let mut results = Vec::new();
                for r in rows {
                    results.push(r?);
                }
                Ok(serde_json::to_value(results)?)
            } else {
                // Filter by tags using EXISTS (SELECT 1 FROM json_each(decisions.tags) WHERE json_each.value IN (...))
                let placeholders = tags.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                let query = format!("SELECT id, uuid, summary, rationale, implementation_details, tags, timestamp FROM decisions WHERE EXISTS (SELECT 1 FROM json_each(decisions.tags) WHERE json_each.value IN ({})) ORDER BY id DESC LIMIT ?", placeholders);
                let mut stmt = conn.prepare(&query)?;
                let mut p = Vec::<&dyn rusqlite::ToSql>::new();
                for tag in &tags {
                    p.push(tag);
                }
                p.push(&limit);
                let rows = stmt.query_map(rusqlite::params_from_iter(p), parse_decision_row)?;
                let mut results = Vec::new();
                for r in rows {
                    results.push(r?);
                }
                Ok(serde_json::to_value(results)?)
            }
        }
        DecisionCmd::Get { id } => get_decision(conn, id),
        DecisionCmd::Search { query, limit } => {
            if query.trim().is_empty() {
                anyhow::bail!("search query cannot be empty");
            }
            // Tokenize by whitespace and wrap in double quotes
            let match_expr = query
                .split_whitespace()
                .map(|token| format!("\"{}\"", token.replace('"', "\"\"")))
                .collect::<Vec<_>>()
                .join(" ");

            let mut stmt = conn.prepare("SELECT d.id, d.uuid, d.summary, d.rationale, d.implementation_details, d.tags, d.timestamp FROM decisions d JOIN decisions_fts f ON d.id = f.rowid WHERE decisions_fts MATCH ?1 ORDER BY rank LIMIT ?2")?;
            let rows = stmt.query_map(params![match_expr, limit], parse_decision_row)?;
            let mut results = Vec::new();
            for r in rows {
                results.push(r?);
            }
            Ok(serde_json::to_value(results)?)
        }
        DecisionCmd::Update(DecisionUpdateArgs { id, fields }) => {
            // First check if exists
            let _: i64 = conn
                .query_row("SELECT id FROM decisions WHERE id = ?", params![id], |r| {
                    r.get(0)
                })
                .optional()?
                .context(format!("decision {} not found", id))?;

            let mut sets = Vec::new();
            let mut p = Vec::<Box<dyn rusqlite::ToSql>>::new();

            if let Some(summary) = fields.summary {
                sets.push("summary = ?");
                p.push(Box::new(summary));
            }
            if let Some(rationale) = fields.rationale {
                sets.push("rationale = ?");
                p.push(Box::new(rationale));
            }
            if let Some(details) = fields.details {
                sets.push("implementation_details = ?");
                p.push(Box::new(details));
            }
            if let Some(tags) = fields.tags {
                sets.push("tags = ?");
                let tags_json = if tags.is_empty() {
                    None
                } else {
                    Some(serde_json::to_string(&tags)?)
                };
                p.push(Box::new(tags_json));
            }

            if sets.is_empty() {
                // Shouldn't happen due to clap ArgGroup but just in case
                return get_decision(conn, id);
            }

            let query = format!("UPDATE decisions SET {} WHERE id = ?", sets.join(", "));
            let mut p_refs: Vec<&dyn rusqlite::ToSql> = p.iter().map(|b| b.as_ref()).collect();
            p_refs.push(&id);

            conn.execute(&query, rusqlite::params_from_iter(p_refs))?;
            get_decision(conn, id)
        }
        DecisionCmd::Delete { id } => {
            let tx = conn.unchecked_transaction()?;

            // ensure it exists
            let _: i64 = tx
                .query_row("SELECT id FROM decisions WHERE id = ?", params![id], |r| {
                    r.get(0)
                })
                .optional()?
                .context(format!("decision {} not found", id))?;

            let links_removed = delete_links_for(&tx, "decision", id)?;
            let deleted = tx.execute("DELETE FROM decisions WHERE id = ?", params![id])?;

            if deleted == 0 {
                anyhow::bail!("decision {} not found", id);
            }

            tx.commit()?;

            Ok(serde_json::json!({
                "deleted": true,
                "id": id,
                "links_removed": links_removed
            }))
        }
    }
}

fn parse_decision_row(row: &rusqlite::Row) -> rusqlite::Result<Decision> {
    let tags_str: Option<String> = row.get(5)?;
    let tags = match tags_str {
        Some(s) => serde_json::from_str(&s).unwrap_or(Value::Null),
        None => Value::Null,
    };

    Ok(Decision {
        id: row.get(0)?,
        uuid: row.get(1)?,
        summary: row.get(2)?,
        rationale: row.get(3)?,
        implementation_details: row.get(4)?,
        tags: if tags.is_null() { None } else { Some(tags) },
        timestamp: row.get(6)?,
    })
}

fn get_decision(conn: &Connection, id: i64) -> Result<Value> {
    let mut stmt = conn.prepare("SELECT id, uuid, summary, rationale, implementation_details, tags, timestamp FROM decisions WHERE id = ?")?;
    let decision = stmt
        .query_row(params![id], parse_decision_row)
        .optional()?
        .context(format!("decision {} not found", id))?;
    Ok(serde_json::to_value(decision)?)
}
