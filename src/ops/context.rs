use anyhow::{Context, Result};
use chrono::{SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;

use crate::cli::ContextUpdateArgs;
use crate::models::{ContextDoc, HistoryRow};

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

pub fn get(conn: &Connection, table: &str) -> Result<Value> {
    let row: Option<(String, i64, String)> = conn.query_row(
        &format!("SELECT content, version, updated_at FROM {} WHERE id = 1", table),
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ).optional()?;

    let doc = match row {
        Some((content_str, version, updated_at)) => ContextDoc {
            content: serde_json::from_str(&content_str).unwrap_or(Value::Null),
            version,
            updated_at: Some(updated_at),
        },
        None => ContextDoc {
            content: serde_json::json!({}),
            version: 0,
            updated_at: None,
        },
    };

    Ok(serde_json::to_value(doc)?)
}

pub fn update(conn: &Connection, table: &str, args: ContextUpdateArgs) -> Result<Value> {
    let current = get(conn, table)?;
    let mut current_content = current.get("content").cloned().unwrap_or(serde_json::json!({}));
    let current_version = current.get("version").and_then(|v| v.as_i64()).unwrap_or(0);

    let new_content = if let Some(content_str) = args.content {
        let parsed: Value = serde_json::from_str(&content_str).context("invalid JSON content")?;
        if !parsed.is_object() {
            anyhow::bail!("content must be a JSON object");
        }
        parsed
    } else if let Some(patch_str) = args.patch {
        let patch: Value = serde_json::from_str(&patch_str).context("invalid JSON patch")?;
        if !patch.is_object() {
            anyhow::bail!("patch must be a JSON object");
        }
        let patch_obj = patch.as_object().unwrap();
        let mut obj = current_content.as_object_mut().unwrap().clone();
        for (k, v) in patch_obj {
            if v == "__DELETE__" {
                obj.remove(k);
            } else {
                obj.insert(k.clone(), v.clone());
            }
        }
        Value::Object(obj)
    } else {
        anyhow::bail!("must provide --content or --patch");
    };

    let new_version = current_version + 1;
    let new_updated_at = now();
    let new_content_str = serde_json::to_string(&new_content)?;

    let tx = conn.unchecked_transaction()?;

    if current_version > 0 {
        let current_content_str = serde_json::to_string(&current_content)?;
        tx.execute(
            &format!("INSERT INTO {}_history (version, content, timestamp, change_source) VALUES (?1, ?2, ?3, ?4)", table),
            params![current_version, current_content_str, new_updated_at, "cli"],
        )?;
    }

    tx.execute(
        &format!("INSERT INTO {}(id, content, version, updated_at) VALUES (1, ?1, ?2, ?3) ON CONFLICT(id) DO UPDATE SET content=excluded.content, version=excluded.version, updated_at=excluded.updated_at", table),
        params![new_content_str, new_version, new_updated_at],
    )?;

    tx.commit()?;

    Ok(serde_json::to_value(ContextDoc {
        content: new_content,
        version: new_version,
        updated_at: Some(new_updated_at),
    })?)
}

pub fn history(conn: &Connection, doc_table: &str, version: Option<i64>, limit: i64) -> Result<Value> {
    let table = format!("{}_history", doc_table);
    let mut stmt = if let Some(_v) = version {
        conn.prepare(&format!("SELECT version, content, timestamp, change_source FROM {} WHERE version = ? ORDER BY version DESC LIMIT ?", table))?
    } else {
        conn.prepare(&format!("SELECT version, content, timestamp, change_source FROM {} ORDER BY version DESC LIMIT ?", table))?
    };

    let mut result = Vec::new();
    
    if let Some(v) = version {
        let rows = stmt.query_map(params![v, limit], |row| {
            let content_str: String = row.get(1)?;
            Ok(HistoryRow {
                version: row.get(0)?,
                content: serde_json::from_str(&content_str).unwrap_or(Value::Null),
                timestamp: row.get(2)?,
                change_source: row.get(3)?,
            })
        })?;
        for r in rows {
            result.push(r?);
        }
    } else {
        let rows = stmt.query_map(params![limit], |row| {
            let content_str: String = row.get(1)?;
            Ok(HistoryRow {
                version: row.get(0)?,
                content: serde_json::from_str(&content_str).unwrap_or(Value::Null),
                timestamp: row.get(2)?,
                change_source: row.get(3)?,
            })
        })?;
        for r in rows {
            result.push(r?);
        }
    }

    Ok(serde_json::to_value(result)?)
}
