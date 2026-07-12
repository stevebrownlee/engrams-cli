use anyhow::{Context, Result};
use chrono::{SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;

use crate::cli::{ProgressCmd, ProgressUpdateArgs};
use crate::models::Progress;
use crate::ops::link::delete_links_for;

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

pub fn handle(conn: &Connection, cmd: ProgressCmd) -> Result<Value> {
    match cmd {
        ProgressCmd::Log {
            status,
            description,
            parent_id,
            check_similar,
        } => {
            if let Some(pid) = parent_id {
                let _: i64 = conn
                    .query_row(
                        "SELECT id FROM progress_entries WHERE id = ?",
                        params![pid],
                        |r| r.get(0),
                    )
                    .optional()?
                    .context(format!("parent_id {} does not exist", pid))?;
            }

            // Check for recent similar entries when requested
            if check_similar {
                let mut stmt = conn.prepare(
                    "SELECT id, timestamp, status, description, parent_id, commit_sha \
                     FROM progress_entries \
                     WHERE LOWER(description) = LOWER(?1) AND LOWER(status) = LOWER(?2) \
                     ORDER BY id DESC LIMIT 1",
                )?;
                let existing: Option<Progress> = stmt
                    .query_row(params![description, status], parse_progress_row)
                    .optional()?;
                if let Some(entry) = existing {
                    return Ok(serde_json::json!({
                        "inserted": false,
                        "existing": serde_json::to_value(entry)?,
                    }));
                }
            }

            let timestamp = now();
            let commit_sha = crate::ops::git::head_sha();
            conn.execute(
                "INSERT INTO progress_entries (timestamp, status, description, parent_id, commit_sha) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![timestamp, status, description, parent_id, commit_sha],
            )?;

            let id = conn.last_insert_rowid();
            let mut result = get_progress(conn, id)?;
            if let Value::Object(map) = &mut result {
                map.insert("inserted".into(), Value::Bool(true));
            }
            Ok(result)
        }
        ProgressCmd::List {
            status,
            parent_id,
            limit,
        } => {
            let mut conditions = Vec::new();
            let mut p = Vec::<Box<dyn rusqlite::ToSql>>::new();

            if let Some(s) = status {
                conditions.push("status = ?");
                p.push(Box::new(s));
            }
            if let Some(pid) = parent_id {
                conditions.push("parent_id = ?");
                p.push(Box::new(pid));
            }

            let where_clause = if conditions.is_empty() {
                String::new()
            } else {
                format!("WHERE {}", conditions.join(" AND "))
            };

            let query = format!("SELECT id, timestamp, status, description, parent_id, commit_sha FROM progress_entries {} ORDER BY id DESC LIMIT ?", where_clause);

            let mut p_refs: Vec<&dyn rusqlite::ToSql> = p.iter().map(|b| b.as_ref()).collect();
            p_refs.push(&limit);

            let mut stmt = conn.prepare(&query)?;
            let rows = stmt.query_map(rusqlite::params_from_iter(p_refs), parse_progress_row)?;

            let mut results = Vec::new();
            for r in rows {
                results.push(r?);
            }
            Ok(serde_json::to_value(results)?)
        }
        ProgressCmd::Get { id } => get_progress(conn, id),
        ProgressCmd::Update(ProgressUpdateArgs { id, fields }) => {
            let _: i64 = conn
                .query_row(
                    "SELECT id FROM progress_entries WHERE id = ?",
                    params![id],
                    |r| r.get(0),
                )
                .optional()?
                .context(format!("progress {} not found", id))?;

            if let Some(pid) = fields.parent_id {
                let _: i64 = conn
                    .query_row(
                        "SELECT id FROM progress_entries WHERE id = ?",
                        params![pid],
                        |r| r.get(0),
                    )
                    .optional()?
                    .context(format!("parent_id {} does not exist", pid))?;
            }

            let mut sets = Vec::new();
            let mut p = Vec::<Box<dyn rusqlite::ToSql>>::new();

            if let Some(s) = fields.status {
                sets.push("status = ?");
                p.push(Box::new(s));
            }
            if let Some(d) = fields.description {
                sets.push("description = ?");
                p.push(Box::new(d));
            }
            if let Some(pid) = fields.parent_id {
                sets.push("parent_id = ?");
                p.push(Box::new(pid));
            }

            if sets.is_empty() {
                return get_progress(conn, id);
            }

            let query = format!(
                "UPDATE progress_entries SET {} WHERE id = ?",
                sets.join(", ")
            );
            let mut p_refs: Vec<&dyn rusqlite::ToSql> = p.iter().map(|b| b.as_ref()).collect();
            p_refs.push(&id);

            conn.execute(&query, rusqlite::params_from_iter(p_refs))?;
            get_progress(conn, id)
        }
        ProgressCmd::Delete { id } => {
            let tx = conn.unchecked_transaction()?;

            let _: i64 = tx
                .query_row(
                    "SELECT id FROM progress_entries WHERE id = ?",
                    params![id],
                    |r| r.get(0),
                )
                .optional()?
                .context(format!("progress {} not found", id))?;

            let links_removed = delete_links_for(&tx, "progress_entry", id)?;
            let deleted = tx.execute("DELETE FROM progress_entries WHERE id = ?", params![id])?;

            if deleted == 0 {
                anyhow::bail!("progress {} not found", id);
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

fn parse_progress_row(row: &rusqlite::Row) -> rusqlite::Result<Progress> {
    Ok(Progress {
        id: row.get(0)?,
        timestamp: row.get(1)?,
        status: row.get(2)?,
        description: row.get(3)?,
        parent_id: row.get(4)?,
        commit_sha: row.get(5)?,
    })
}

fn get_progress(conn: &Connection, id: i64) -> Result<Value> {
    let mut stmt = conn.prepare(
        "SELECT id, timestamp, status, description, parent_id, commit_sha FROM progress_entries WHERE id = ?",
    )?;
    let progress = stmt
        .query_row(params![id], parse_progress_row)
        .optional()?
        .context(format!("progress {} not found", id))?;
    Ok(serde_json::to_value(progress)?)
}
