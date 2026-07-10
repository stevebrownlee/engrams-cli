use anyhow::{Context, Result};
use chrono::{SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;

use crate::cli::{ItemType, LinkCmd};
use crate::models::Link;

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

pub fn delete_links_for(conn: &Connection, item_type: &str, id: i64) -> Result<usize> {
    let id_str = id.to_string();
    let count = conn.execute(
        "DELETE FROM context_links WHERE (source_item_type=?1 AND source_item_id=?2) OR (target_item_type=?1 AND target_item_id=?2)",
        params![item_type, id_str],
    )?;
    Ok(count)
}

pub fn item_exists(conn: &Connection, item_type: ItemType, id: i64) -> Result<bool> {
    let table = item_type.table_name();
    let count: i64 = conn.query_row(
        &format!("SELECT count(*) FROM {} WHERE id = ?", table),
        params![id],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

pub fn handle(conn: &Connection, cmd: LinkCmd) -> Result<Value> {
    match cmd {
        LinkCmd::Add { source_type, source_id, target_type, target_id, rel, description } => {
            let s_id: i64 = source_id.parse().context(format!("invalid source id: {}", source_id))?;
            let t_id: i64 = target_id.parse().context(format!("invalid target id: {}", target_id))?;

            if !item_exists(conn, source_type, s_id)? {
                anyhow::bail!("{} {} does not exist", source_type.as_str(), s_id);
            }
            if !item_exists(conn, target_type, t_id)? {
                anyhow::bail!("{} {} does not exist", target_type.as_str(), t_id);
            }

            let timestamp = now();
            conn.execute(
                "INSERT INTO context_links (source_item_type, source_item_id, target_item_type, target_item_id, relationship_type, description, timestamp) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![source_type.as_str(), source_id, target_type.as_str(), target_id, rel, description, timestamp],
            )?;

            let id = conn.last_insert_rowid();
            get_link(conn, id)
        }
        LinkCmd::List { item_type, item_id, rel, linked_type } => {
            let item_type_str = item_type.as_str();

            let mut conditions = Vec::new();
            let mut p = Vec::<Box<dyn rusqlite::ToSql>>::new();

            conditions.push("((source_item_type = ? AND source_item_id = ?) OR (target_item_type = ? AND target_item_id = ?))");
            p.push(Box::new(item_type_str.to_string()));
            p.push(Box::new(item_id.clone()));
            p.push(Box::new(item_type_str.to_string()));
            p.push(Box::new(item_id.clone()));

            if let Some(r) = rel {
                conditions.push("relationship_type = ?");
                p.push(Box::new(r));
            }

            if let Some(lt) = linked_type {
                let lt_str = lt.as_str().to_string();
                conditions.push("((source_item_type = ? AND target_item_type = ?) OR (target_item_type = ? AND source_item_type = ?))");
                p.push(Box::new(lt_str.clone()));
                p.push(Box::new(item_type_str.to_string()));
                p.push(Box::new(lt_str.clone()));
                p.push(Box::new(item_type_str.to_string()));
            }

            let where_clause = format!("WHERE {}", conditions.join(" AND "));
            let query = format!("SELECT id, source_item_type, source_item_id, target_item_type, target_item_id, relationship_type, description, timestamp FROM context_links {} ORDER BY id ASC", where_clause);

            let p_refs: Vec<&dyn rusqlite::ToSql> = p.iter().map(|b| b.as_ref()).collect();
            let mut stmt = conn.prepare(&query)?;
            let rows = stmt.query_map(rusqlite::params_from_iter(p_refs), |row| {
                let mut link = parse_link_row(row)?;
                let s_type: String = row.get(1)?;
                let s_id: String = row.get(2)?;
                if s_type == item_type_str && s_id == item_id {
                    link.direction = Some("outgoing".to_string());
                } else {
                    link.direction = Some("incoming".to_string());
                }
                Ok(link)
            })?;

            let mut results = Vec::new();
            for r in rows {
                results.push(r?);
            }
            Ok(serde_json::to_value(results)?)
        }
    }
}

fn parse_link_row(row: &rusqlite::Row) -> rusqlite::Result<Link> {
    Ok(Link {
        id: row.get(0)?,
        source_item_type: row.get(1)?,
        source_item_id: row.get(2)?,
        target_item_type: row.get(3)?,
        target_item_id: row.get(4)?,
        relationship_type: row.get(5)?,
        description: row.get(6)?,
        timestamp: row.get(7)?,
        direction: None,
    })
}

fn get_link(conn: &Connection, id: i64) -> Result<Value> {
    let mut stmt = conn.prepare("SELECT id, source_item_type, source_item_id, target_item_type, target_item_id, relationship_type, description, timestamp FROM context_links WHERE id = ?")?;
    let link = stmt.query_row(params![id], parse_link_row)
        .optional()?
        .context(format!("link {} not found", id))?;
    Ok(serde_json::to_value(link)?)
}
