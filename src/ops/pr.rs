use crate::cli::{PrCmd, RefItemType};
use crate::ops::git;
use anyhow::Result;
use chrono::{SecondsFormat, Utc};
use rusqlite::{params, Connection};
use serde_json::Value;
use std::collections::HashMap;

pub fn resolve_pr_url(value: &str) -> Result<String> {
    if value.starts_with("http://") || value.starts_with("https://") {
        return Ok(value.to_string());
    }
    if value.chars().all(|c| c.is_ascii_digit()) {
        let base = git::origin_base()?;
        let host_lower = base.to_lowercase();
        if host_lower.contains("github") {
            Ok(format!("{}/pull/{}", base, value))
        } else if host_lower.contains("gitlab") {
            Ok(format!("{}/-/merge_requests/{}", base, value))
        } else if host_lower.contains("bitbucket") {
            Ok(format!("{}/pull-requests/{}", base, value))
        } else {
            let host = base
                .strip_prefix("https://")
                .and_then(|s| s.split('/').next())
                .unwrap_or(&base);
            anyhow::bail!(
                "cannot derive PR URL for host '{}': pass the full URL",
                host
            );
        }
    } else {
        anyhow::bail!(
            "invalid --pr value '{}': pass a PR number or full URL",
            value
        );
    }
}

pub fn attach(
    conn: &Connection,
    item_type: &str,
    item_id: i64,
    urls: &[String],
) -> Result<Vec<String>> {
    let source_item_id = item_id.to_string();
    let timestamp = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    for url in urls {
        let exists: bool = conn.query_row(
            "SELECT count(*) FROM context_links WHERE source_item_type = ?1 AND source_item_id = ?2 AND target_item_type = 'pr' AND target_item_id = ?3",
            params![item_type, source_item_id, url],
            |row| row.get(0),
        )?;
        if !exists {
            conn.execute(
                "INSERT INTO context_links (source_item_type, source_item_id, target_item_type, target_item_id, relationship_type, description, timestamp) \
                 VALUES (?1, ?2, 'pr', ?3, 'implemented_in', NULL, ?4)",
                params![item_type, source_item_id, url, timestamp],
            )?;
        }
    }
    pr_urls_for(conn, item_type, item_id)
}

pub fn pr_urls_for(conn: &Connection, item_type: &str, item_id: i64) -> Result<Vec<String>> {
    let source_item_id = item_id.to_string();
    let mut stmt = conn.prepare(
        "SELECT DISTINCT target_item_id FROM context_links WHERE source_item_type=?1 AND source_item_id=?2 AND target_item_type='pr' ORDER BY id ASC"
    )?;
    let rows = stmt.query_map(params![item_type, source_item_id], |row| row.get(0))?;
    let mut urls = Vec::new();
    for r in rows {
        urls.push(r?);
    }
    Ok(urls)
}

pub fn pr_urls_map(conn: &Connection, item_type: &str) -> Result<HashMap<i64, Vec<String>>> {
    let mut stmt = conn.prepare(
        "SELECT source_item_id, target_item_id FROM context_links WHERE source_item_type=?1 AND target_item_type='pr' ORDER BY id ASC"
    )?;
    let rows = stmt.query_map(params![item_type], |row| {
        let sid_str: String = row.get(0)?;
        let tid: String = row.get(1)?;
        let sid = sid_str.parse::<i64>().unwrap_or(0);
        Ok((sid, tid))
    })?;
    let mut map = HashMap::new();
    for r in rows {
        let (sid, tid) = r?;
        if sid > 0 {
            map.entry(sid).or_insert_with(Vec::new).push(tid);
        }
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

pub fn handle(conn: &Connection, cmd: PrCmd) -> Result<Value> {
    match cmd {
        PrCmd::Add { item_type, id, prs } => {
            if !ref_item_exists(conn, &item_type, id)? {
                anyhow::bail!("{} {} does not exist", item_type.as_str(), id);
            }
            let mut resolved = Vec::new();
            for pr in prs {
                resolved.push(resolve_pr_url(&pr)?);
            }
            let urls = attach(conn, item_type.as_str(), id, &resolved)?;
            Ok(serde_json::to_value(urls)?)
        }
        PrCmd::List { item_type, id } => {
            if !ref_item_exists(conn, &item_type, id)? {
                anyhow::bail!("{} {} does not exist", item_type.as_str(), id);
            }
            let mut stmt = conn.prepare(
                "SELECT target_item_id, timestamp FROM context_links \
                 WHERE source_item_type = ?1 AND source_item_id = ?2 AND target_item_type = 'pr' \
                 ORDER BY id ASC",
            )?;
            let rows = stmt.query_map(params![item_type.as_str(), id.to_string()], |row| {
                let url: String = row.get(0)?;
                let timestamp: String = row.get(1)?;
                Ok(serde_json::json!({
                    "url": url,
                    "timestamp": timestamp,
                }))
            })?;
            let mut list = Vec::new();
            for r in rows {
                list.push(r?);
            }
            Ok(Value::Array(list))
        }
        PrCmd::Remove { item_type, id, url } => {
            if !ref_item_exists(conn, &item_type, id)? {
                anyhow::bail!("{} {} does not exist", item_type.as_str(), id);
            }
            let target_url = resolve_pr_url(&url).unwrap_or(url.clone());
            let count = conn.execute(
                "DELETE FROM context_links \
                 WHERE source_item_type = ?1 AND source_item_id = ?2 AND target_item_type = 'pr' AND target_item_id = ?3",
                params![item_type.as_str(), id.to_string(), target_url],
            )?;
            if count == 0 {
                anyhow::bail!(
                    "pr url '{}' is not attached to {} {}",
                    url,
                    item_type.as_str(),
                    id
                );
            }
            Ok(serde_json::json!({
                "deleted": true,
                "url": target_url,
            }))
        }
    }
}
