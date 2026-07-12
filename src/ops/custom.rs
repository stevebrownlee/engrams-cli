use anyhow::{Context, Result};
use chrono::{SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;

use crate::cli::CustomCmd;
use crate::models::CustomData;
use crate::ops::link::delete_links_for;

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

pub fn handle(conn: &Connection, cmd: CustomCmd) -> Result<Value> {
    match cmd {
        CustomCmd::Set {
            category,
            key,
            value,
            json,
        } => {
            let value_json = if json {
                serde_json::from_str::<Value>(&value).context("invalid JSON in --value")?
            } else {
                Value::String(value)
            };
            let value_str = serde_json::to_string(&value_json)?;
            let timestamp = now();

            conn.execute(
                "INSERT INTO custom_data (timestamp, category, key, value) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(category, key) DO UPDATE SET value=excluded.value, timestamp=excluded.timestamp",
                params![timestamp, category, key, value_str],
            )?;

            let id: i64 = conn.query_row(
                "SELECT id FROM custom_data WHERE category = ? AND key = ?",
                params![category, key],
                |r| r.get(0),
            )?;
            get_custom(conn, id)
        }
        CustomCmd::Get { category, key } => {
            let mut conditions = Vec::new();
            let mut p = Vec::<Box<dyn rusqlite::ToSql>>::new();

            if let Some(c) = category {
                conditions.push("category = ?");
                p.push(Box::new(c));
            }
            if let Some(k) = key {
                conditions.push("key = ?");
                p.push(Box::new(k));
            }

            let where_clause = if conditions.is_empty() {
                String::new()
            } else {
                format!("WHERE {}", conditions.join(" AND "))
            };

            let query = format!(
                "SELECT id, timestamp, category, key, value FROM custom_data {} ORDER BY id ASC",
                where_clause
            );

            let p_refs: Vec<&dyn rusqlite::ToSql> = p.iter().map(|b| b.as_ref()).collect();

            let mut stmt = conn.prepare(&query)?;
            let rows = stmt.query_map(rusqlite::params_from_iter(p_refs), parse_custom_row)?;

            let mut results = Vec::new();
            for r in rows {
                results.push(r?);
            }
            Ok(serde_json::to_value(results)?)
        }
        CustomCmd::Search {
            query,
            category,
            limit,
        } => {
            if query.trim().is_empty() {
                anyhow::bail!("search query cannot be empty");
            }
            let match_expr = query
                .split_whitespace()
                .map(|token| format!("\"{}\"", token.replace('"', "\"\"")))
                .collect::<Vec<_>>()
                .join(" ");

            if let Some(c) = category {
                let mut stmt = conn.prepare("SELECT d.id, d.timestamp, d.category, d.key, d.value FROM custom_data d JOIN custom_data_fts f ON d.id = f.rowid WHERE custom_data_fts MATCH ?1 AND d.category = ?2 ORDER BY rank LIMIT ?3")?;
                let rows = stmt.query_map(params![match_expr, c, limit], parse_custom_row)?;
                let mut results = Vec::new();
                for r in rows {
                    results.push(r?);
                }
                Ok(serde_json::to_value(results)?)
            } else {
                let mut stmt = conn.prepare("SELECT d.id, d.timestamp, d.category, d.key, d.value FROM custom_data d JOIN custom_data_fts f ON d.id = f.rowid WHERE custom_data_fts MATCH ?1 ORDER BY rank LIMIT ?2")?;
                let rows = stmt.query_map(params![match_expr, limit], parse_custom_row)?;
                let mut results = Vec::new();
                for r in rows {
                    results.push(r?);
                }
                Ok(serde_json::to_value(results)?)
            }
        }
        CustomCmd::Delete { category, key } => {
            let tx = conn.unchecked_transaction()?;

            let id: i64 = tx
                .query_row(
                    "SELECT id FROM custom_data WHERE category = ? AND key = ?",
                    params![category, key],
                    |r| r.get(0),
                )
                .optional()?
                .context(format!("custom_data {}/{} not found", category, key))?;

            let links_removed = delete_links_for(&tx, "custom_data", id)?;
            tx.execute("DELETE FROM custom_data WHERE id = ?", params![id])?;

            tx.commit()?;

            Ok(serde_json::json!({
                "deleted": true,
                "category": category,
                "key": key,
                "links_removed": links_removed
            }))
        }
    }
}

pub(crate) fn parse_custom_row(row: &rusqlite::Row) -> rusqlite::Result<CustomData> {
    let value_str: String = row.get(4)?;
    let value = serde_json::from_str(&value_str).unwrap_or(Value::String(value_str));

    Ok(CustomData {
        id: row.get(0)?,
        timestamp: row.get(1)?,
        category: row.get(2)?,
        key: row.get(3)?,
        value,
    })
}

fn get_custom(conn: &Connection, id: i64) -> Result<Value> {
    let mut stmt =
        conn.prepare("SELECT id, timestamp, category, key, value FROM custom_data WHERE id = ?")?;
    let custom = stmt
        .query_row(params![id], parse_custom_row)
        .optional()?
        .context(format!("custom_data {} not found", id))?;
    Ok(serde_json::to_value(custom)?)
}
