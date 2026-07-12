use crate::cli::{AnchorCmd, RefItemType};
use crate::models::{Decision, Pattern};
use crate::ops::git;
use anyhow::Result;
use chrono::{SecondsFormat, Utc};
use rusqlite::{params, Connection};
use serde_json::Value;
use std::collections::HashMap;

pub fn clean_path(path: &str) -> String {
    if let Some(stripped) = path.strip_prefix("./") {
        stripped.to_string()
    } else {
        path.to_string()
    }
}

pub fn attach(
    conn: &Connection,
    item_type: &str,
    item_id: i64,
    paths: &[String],
) -> Result<Vec<String>> {
    let timestamp = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    for path in paths {
        let cleaned = clean_path(path);
        conn.execute(
            "INSERT OR IGNORE INTO item_anchors (item_type, item_id, path, timestamp) VALUES (?1, ?2, ?3, ?4)",
            params![item_type, item_id, cleaned, timestamp],
        )?;
    }
    anchors_for(conn, item_type, item_id)
}

pub fn anchors_for(conn: &Connection, item_type: &str, item_id: i64) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT path FROM item_anchors WHERE item_type=?1 AND item_id=?2 ORDER BY id ASC",
    )?;
    let rows = stmt.query_map(params![item_type, item_id], |row| row.get(0))?;
    let mut paths = Vec::new();
    for r in rows {
        paths.push(r?);
    }
    Ok(paths)
}

pub fn anchors_map(conn: &Connection, item_type: &str) -> Result<HashMap<i64, Vec<String>>> {
    let mut stmt =
        conn.prepare("SELECT item_id, path FROM item_anchors WHERE item_type=?1 ORDER BY id ASC")?;
    let rows = stmt.query_map(params![item_type], |row| {
        let id: i64 = row.get(0)?;
        let path: String = row.get(1)?;
        Ok((id, path))
    })?;
    let mut map = HashMap::new();
    for r in rows {
        let (id, path) = r?;
        map.entry(id).or_insert_with(Vec::new).push(path);
    }
    Ok(map)
}

fn ref_item_exists(conn: &Connection, item_type: &RefItemType, id: i64) -> Result<bool> {
    let table = match item_type {
        RefItemType::Decision => "decisions",
        RefItemType::SystemPattern => "system_patterns",
    };
    let count: i64 = conn.query_row(
        &format!("SELECT count(*) FROM {} WHERE id = ?", table),
        params![id],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

pub fn handle(conn: &Connection, cmd: AnchorCmd) -> Result<Value> {
    match cmd {
        AnchorCmd::Add {
            item_type,
            id,
            paths,
        } => {
            if !ref_item_exists(conn, &item_type, id)? {
                anyhow::bail!("{} {} does not exist", item_type.as_str(), id);
            }
            let urls = attach(conn, item_type.as_str(), id, &paths)?;
            Ok(serde_json::to_value(urls)?)
        }
        AnchorCmd::List { item_type, id } => {
            if !ref_item_exists(conn, &item_type, id)? {
                anyhow::bail!("{} {} does not exist", item_type.as_str(), id);
            }
            let mut stmt = conn.prepare(
                "SELECT path, timestamp FROM item_anchors \
                 WHERE item_type = ?1 AND item_id = ?2 \
                 ORDER BY id ASC",
            )?;
            let rows = stmt.query_map(params![item_type.as_str(), id], |row| {
                let path: String = row.get(0)?;
                let timestamp: String = row.get(1)?;
                Ok(serde_json::json!({
                    "path": path,
                    "timestamp": timestamp,
                }))
            })?;
            let mut list = Vec::new();
            for r in rows {
                list.push(r?);
            }
            Ok(Value::Array(list))
        }
        AnchorCmd::Remove {
            item_type,
            id,
            path,
        } => {
            if !ref_item_exists(conn, &item_type, id)? {
                anyhow::bail!("{} {} does not exist", item_type.as_str(), id);
            }
            let cleaned = clean_path(&path);
            let count = conn.execute(
                "DELETE FROM item_anchors \
                 WHERE item_type = ?1 AND item_id = ?2 AND path = ?3",
                params![item_type.as_str(), id, cleaned],
            )?;
            if count == 0 {
                anyhow::bail!(
                    "anchor '{}' is not attached to {} {}",
                    path,
                    item_type.as_str(),
                    id
                );
            }
            Ok(serde_json::json!({
                "deleted": true,
                "path": cleaned,
            }))
        }
    }
}

pub fn query_relevant_ids(conn: &Connection, paths: &[String]) -> Result<Vec<(String, i64)>> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    let mut sql = "SELECT DISTINCT item_type, item_id FROM item_anchors WHERE ".to_string();
    let mut conditions = Vec::new();
    let mut params_vec = Vec::<&dyn rusqlite::ToSql>::new();
    for path in paths {
        conditions.push("(path = ? OR path LIKE ? || '/%' OR ? LIKE path || '/%')");
        params_vec.push(path);
        params_vec.push(path);
        params_vec.push(path);
    }
    sql.push_str(&conditions.join(" OR "));

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(params_vec), |row| {
        let itype: String = row.get(0)?;
        let id: i64 = row.get(1)?;
        Ok((itype, id))
    })?;

    let mut results = Vec::new();
    for r in rows {
        results.push(r?);
    }
    Ok(results)
}

pub fn handle_relevant(
    conn: &Connection,
    paths: Vec<String>,
    staged: bool,
    all: bool,
) -> Result<Value> {
    let mut query_paths = paths;
    if staged {
        let staged_files =
            git::staged_files().map_err(|e| anyhow::anyhow!("cannot read staged files: {}", e))?;
        query_paths.extend(staged_files);
    }

    if query_paths.is_empty() {
        if staged {
            return Ok(serde_json::json!({
                "decisions": [],
                "patterns": [],
            }));
        } else {
            anyhow::bail!("provide at least one path or --staged");
        }
    }

    let cleaned_paths: Vec<String> = query_paths.into_iter().map(|p| clean_path(&p)).collect();
    let matched = query_relevant_ids(conn, &cleaned_paths)?;

    let mut decision_ids = Vec::new();
    let mut pattern_ids = Vec::new();
    for (itype, id) in matched {
        if itype == "decision" {
            decision_ids.push(id);
        } else if itype == "system_pattern" {
            pattern_ids.push(id);
        }
    }

    let mut decisions = Vec::new();
    if !decision_ids.is_empty() {
        let placeholders = decision_ids
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        let sql = if all {
            format!("SELECT id, uuid, summary, rationale, implementation_details, tags, timestamp, status, commit_sha FROM decisions WHERE id IN ({}) ORDER BY id DESC", placeholders)
        } else {
            format!("SELECT id, uuid, summary, rationale, implementation_details, tags, timestamp, status, commit_sha FROM decisions WHERE id IN ({}) AND status = 'active' ORDER BY id DESC", placeholders)
        };
        let mut stmt = conn.prepare(&sql)?;
        let mut p = Vec::<&dyn rusqlite::ToSql>::new();
        for id in &decision_ids {
            p.push(id);
        }
        let rows = stmt.query_map(rusqlite::params_from_iter(p), |row| {
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
                status: row.get(7)?,
                commit_sha: row.get(8)?,
                pr_urls: Vec::new(),
                anchors: Vec::new(),
            })
        })?;
        for r in rows {
            let mut d = r?;
            d.pr_urls = crate::ops::pr::pr_urls_for(conn, "decision", d.id)?;
            d.anchors = anchors_for(conn, "decision", d.id)?;
            decisions.push(d);
        }
    }

    let mut patterns = Vec::new();
    if !pattern_ids.is_empty() {
        let placeholders = pattern_ids
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!("SELECT id, uuid, name, description, tags, timestamp FROM system_patterns WHERE id IN ({}) ORDER BY id DESC", placeholders);
        let mut stmt = conn.prepare(&sql)?;
        let mut p = Vec::<&dyn rusqlite::ToSql>::new();
        for id in &pattern_ids {
            p.push(id);
        }
        let rows = stmt.query_map(rusqlite::params_from_iter(p), |row| {
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
        })?;
        for r in rows {
            let mut pat = r?;
            pat.pr_urls = crate::ops::pr::pr_urls_for(conn, "system_pattern", pat.id)?;
            pat.anchors = anchors_for(conn, "system_pattern", pat.id)?;
            patterns.push(pat);
        }
    }

    Ok(serde_json::json!({
        "decisions": decisions,
        "patterns": patterns,
    }))
}
