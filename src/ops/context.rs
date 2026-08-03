use anyhow::{Context, Result};
use chrono::{SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

use crate::cli::ContextUpdateArgs;
use crate::models::{ContextDoc, HistoryRow};

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

pub fn get(conn: &Connection, table: &str) -> Result<Value> {
    let row: Option<(String, i64, String)> = conn
        .query_row(
            &format!(
                "SELECT content, version, updated_at FROM {} WHERE id = 1",
                table
            ),
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;

    let doc = row.map(|(content_str, version, updated_at)| ContextDoc {
        content: serde_json::from_str(&content_str).unwrap_or(Value::Null),
        version,
        updated_at: Some(updated_at),
    });

    Ok(serde_json::to_value(doc)?)
}

pub fn update(conn: &Connection, table: &str, args: &ContextUpdateArgs) -> Result<Value> {
    let history_table = format!("{}_history", table);

    let old_row: Option<(String, i64)> = conn
        .query_row(
            &format!("SELECT content, version FROM {} WHERE id = 1", table),
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()?;

    let new_content = if let Some(patch_str) = &args.patch {
        let patch: Value = serde_json::from_str(patch_str).context("Invalid patch JSON")?;
        let mut current: Value = match &old_row {
            Some((s, _)) => serde_json::from_str(s).unwrap_or(json!({})),
            None => json!({}),
        };
        merge(&mut current, patch);
        current
    } else if let Some(content_str) = &args.content {
        serde_json::from_str(content_str).context("Invalid content JSON")?
    } else {
        unreachable!("clap group ensures one of content/patch is present")
    };

    let change_source = if args.patch.is_some() {
        "patch"
    } else {
        "replace"
    };
    let old_version = old_row.as_ref().map(|(_, v)| *v).unwrap_or(0);
    let new_version = old_version + 1;
    let ts = now();
    let new_content_str = serde_json::to_string(&new_content)?;

    let tx = conn.unchecked_transaction()?;

    // Archive the superseded content under its own version. The very first
    // write has nothing to record yet, so skip it.
    if let Some((old_content_str, _)) = &old_row {
        tx.execute(
            &format!(
                "INSERT INTO {}(version, content, timestamp, change_source) VALUES (?1, ?2, ?3, ?4)",
                history_table
            ),
            params![old_version, old_content_str, ts, change_source],
        )?;
    }

    tx.execute(
        &format!("INSERT INTO {}(id, content, version, updated_at) VALUES (1, ?1, ?2, ?3) ON CONFLICT(id) DO UPDATE SET content=excluded.content, version=excluded.version, updated_at=excluded.updated_at", table),
        params![new_content_str, new_version, ts],
    )?;

    tx.commit()?;

    Ok(json!({
        "status": "success",
        "version": new_version,
        "change_source": change_source
    }))
}

pub fn history(
    conn: &Connection,
    doc_table: &str,
    version: Option<i64>,
    limit: i64,
) -> Result<Value> {
    let history_table = format!("{}_history", doc_table);
    let mut result = Vec::new();
    if let Some(v) = version {
        let row: Option<(i64, String, String, Option<String>)> = conn
            .query_row(
                &format!(
                    "SELECT version, content, timestamp, change_source FROM {} WHERE version = ?1",
                    history_table
                ),
                params![v],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;
        if let Some(r) = row {
            let content: Value = serde_json::from_str(&r.1).unwrap_or(Value::Null);
            result.push(HistoryRow {
                version: r.0,
                content,
                timestamp: r.2,
                change_source: r.3,
            });
        }
    } else {
        let mut stmt = conn.prepare(&format!("SELECT version, content, timestamp, change_source FROM {} ORDER BY version DESC LIMIT ?1", history_table))?;
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

// --- Named active-context tracks (active_contexts / active_context_history) ---

fn track_doc(name: &str, content: Value, version: i64, updated_at: String) -> Value {
    json!({
        "name": name,
        "content": content,
        "version": version,
        "updated_at": updated_at,
    })
}

pub fn get_track(conn: &Connection, name: &str) -> Result<Value> {
    let row: Option<(String, i64, String)> = conn
        .query_row(
            "SELECT content, version, updated_at FROM active_contexts WHERE name = ?1",
            params![name],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;

    let doc = row.map(|(content_str, version, updated_at)| {
        track_doc(
            name,
            serde_json::from_str(&content_str).unwrap_or(Value::Null),
            version,
            updated_at,
        )
    });

    Ok(serde_json::to_value(doc)?)
}

pub fn update_track(conn: &Connection, name: &str, args: &ContextUpdateArgs) -> Result<Value> {
    let old_row: Option<(String, i64)> = conn
        .query_row(
            "SELECT content, version FROM active_contexts WHERE name = ?1",
            params![name],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()?;

    let new_content = if let Some(patch_str) = &args.patch {
        let patch: Value = serde_json::from_str(patch_str).context("Invalid patch JSON")?;
        let mut current: Value = match &old_row {
            Some((s, _)) => serde_json::from_str(s).unwrap_or(json!({})),
            None => json!({}),
        };
        merge(&mut current, patch);
        current
    } else if let Some(content_str) = &args.content {
        serde_json::from_str(content_str).context("Invalid content JSON")?
    } else {
        unreachable!("clap group ensures one of content/patch is present")
    };

    let change_source = if args.patch.is_some() {
        "patch"
    } else {
        "replace"
    };
    let old_version = old_row.as_ref().map(|(_, v)| *v).unwrap_or(0);
    let new_version = old_version + 1;
    let ts = now();
    let new_content_str = serde_json::to_string(&new_content)?;

    let tx = conn.unchecked_transaction()?;

    // Archive the superseded content under its own version. The very first
    // write to a track has nothing to record yet, so skip it.
    if let Some((old_content_str, _)) = &old_row {
        tx.execute(
            "INSERT INTO active_context_history(name, version, content, timestamp, change_source) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![name, old_version, old_content_str, ts, change_source],
        )?;
    }

    tx.execute(
        "INSERT INTO active_contexts(name, content, version, updated_at) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(name) DO UPDATE SET content=excluded.content, version=excluded.version, updated_at=excluded.updated_at",
        params![name, new_content_str, new_version, ts],
    )?;

    tx.commit()?;

    Ok(json!({
        "status": "success",
        "name": name,
        "version": new_version,
        "change_source": change_source
    }))
}

pub fn history_track(
    conn: &Connection,
    name: &str,
    version: Option<i64>,
    limit: i64,
) -> Result<Value> {
    let mut result = Vec::new();
    if let Some(v) = version {
        let row: Option<(i64, String, String, Option<String>)> = conn
            .query_row(
                "SELECT version, content, timestamp, change_source FROM active_context_history WHERE name = ?1 AND version = ?2",
                params![name, v],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;
        if let Some(r) = row {
            let content: Value = serde_json::from_str(&r.1).unwrap_or(Value::Null);
            result.push(HistoryRow {
                version: r.0,
                content,
                timestamp: r.2,
                change_source: r.3,
            });
        }
    } else {
        let mut stmt = conn.prepare(
            "SELECT version, content, timestamp, change_source FROM active_context_history WHERE name = ?1 ORDER BY version DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![name, limit], |row| {
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

pub fn history_all_tracks(conn: &Connection) -> Result<Value> {
    let mut stmt = conn.prepare(
        "SELECT name, COUNT(*), MAX(version) FROM active_context_history GROUP BY name ORDER BY name",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(json!({
            "name": row.get::<_, String>(0)?,
            "history_entries": row.get::<_, i64>(1)?,
            "latest_version": row.get::<_, i64>(2)?,
        }))
    })?;
    let mut tracks = Vec::new();
    for r in rows {
        tracks.push(r?);
    }
    Ok(json!(tracks))
}

pub fn list_tracks(conn: &Connection) -> Result<Value> {
    let mut stmt =
        conn.prepare("SELECT name, content, updated_at FROM active_contexts ORDER BY name")?;
    let rows = stmt.query_map([], |row| {
        let name: String = row.get(0)?;
        let content_str: String = row.get(1)?;
        let content: Value = serde_json::from_str(&content_str).unwrap_or(Value::Null);
        Ok(json!({
            "name": name,
            "focus": content.get("focus").cloned().unwrap_or(Value::Null),
            "status": content.get("status").cloned().unwrap_or(Value::Null),
            "updated_at": row.get::<_, String>(2)?,
        }))
    })?;
    let mut tracks = Vec::new();
    for r in rows {
        tracks.push(r?);
    }
    Ok(json!(tracks))
}

fn merge(target: &mut Value, patch: Value) {
    match (target, patch) {
        (Value::Object(t), Value::Object(p)) => {
            for (k, v) in p {
                if v.as_str() == Some("__DELETE__") {
                    t.remove(&k);
                } else {
                    merge(t.entry(k).or_insert(Value::Null), v);
                }
            }
        }
        (t, p) => *t = p,
    }
}
