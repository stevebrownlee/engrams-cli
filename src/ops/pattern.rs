use anyhow::{Context, Result};
use chrono::{SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;
use uuid::Uuid;

use crate::cli::PatternCmd;
use crate::models::Pattern;
use crate::ops::link::delete_links_for;

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

pub fn handle(conn: &Connection, cmd: PatternCmd) -> Result<Value> {
    match cmd {
        PatternCmd::Log {
            name,
            description,
            tags,
            prs,
            anchors,
        } => {
            let mut resolved_prs = Vec::new();
            for pr in prs {
                resolved_prs.push(crate::ops::pr::resolve_pr_url(&pr)?);
            }

            let uuid = Uuid::new_v4().to_string();
            let timestamp = now();
            let tags_json = if tags.is_empty() {
                None
            } else {
                Some(serde_json::to_string(&tags)?)
            };

            conn.execute(
                "INSERT INTO system_patterns (uuid, timestamp, name, description, tags) VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT(name) DO UPDATE SET description=excluded.description, tags=excluded.tags, timestamp=excluded.timestamp",
                params![uuid, timestamp, name, description, tags_json],
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

            get_pattern(conn, id)
        }
        PatternCmd::List { tags, limit } => {
            if tags.is_empty() {
                let mut stmt = conn.prepare("SELECT id, uuid, name, description, tags, timestamp FROM system_patterns ORDER BY id DESC LIMIT ?")?;
                let rows = stmt.query_map(params![limit], parse_pattern_row)?;
                let mut results = Vec::new();
                for r in rows {
                    results.push(r?);
                }
                let prs_map = crate::ops::pr::pr_urls_map(conn, "system_pattern")?;
                let anchors_map = crate::ops::anchor::anchors_map(conn, "system_pattern")?;
                for d in &mut results {
                    if let Some(urls) = prs_map.get(&d.id) {
                        d.pr_urls = urls.clone();
                    }
                    if let Some(paths) = anchors_map.get(&d.id) {
                        d.anchors = paths.clone();
                    }
                }
                Ok(serde_json::to_value(results)?)
            } else {
                let placeholders = tags.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                let query = format!("SELECT id, uuid, name, description, tags, timestamp FROM system_patterns WHERE EXISTS (SELECT 1 FROM json_each(system_patterns.tags) WHERE json_each.value IN ({})) ORDER BY id DESC LIMIT ?", placeholders);
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
                let prs_map = crate::ops::pr::pr_urls_map(conn, "system_pattern")?;
                let anchors_map = crate::ops::anchor::anchors_map(conn, "system_pattern")?;
                for d in &mut results {
                    if let Some(urls) = prs_map.get(&d.id) {
                        d.pr_urls = urls.clone();
                    }
                    if let Some(paths) = anchors_map.get(&d.id) {
                        d.anchors = paths.clone();
                    }
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

            Ok(serde_json::json!({
                "deleted": true,
                "id": id,
                "links_removed": links_removed
            }))
        }
    }
}

fn parse_pattern_row(row: &rusqlite::Row) -> rusqlite::Result<Pattern> {
    let tags_str: Option<String> = row.get(4)?;
    let tags = match tags_str {
        Some(s) => serde_json::from_str(&s).unwrap_or(Value::Null),
        None => Value::Null,
    };

    Ok(Pattern {
        id: row.get(0)?,
        uuid: row.get(1)?,
        name: row.get(2)?,
        description: row.get(3)?,
        tags: if tags.is_null() { None } else { Some(tags) },
        timestamp: row.get(5)?,
        pr_urls: Vec::new(),
        anchors: Vec::new(),
    })
}

fn get_pattern(conn: &Connection, id: i64) -> Result<Value> {
    let mut stmt = conn.prepare(
        "SELECT id, uuid, name, description, tags, timestamp FROM system_patterns WHERE id = ?",
    )?;
    let mut pattern = stmt
        .query_row(params![id], parse_pattern_row)
        .optional()?
        .context(format!("pattern {} not found", id))?;
    pattern.pr_urls = crate::ops::pr::pr_urls_for(conn, "system_pattern", id)?;
    pattern.anchors = crate::ops::anchor::anchors_for(conn, "system_pattern", id)?;
    Ok(serde_json::to_value(pattern)?)
}
