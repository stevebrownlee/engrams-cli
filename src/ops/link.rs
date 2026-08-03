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
        LinkCmd::Add {
            source_type,
            source_id,
            target_type,
            target_id,
            rel,
            description,
            force,
        } => {
            let s_id: i64 = source_id
                .parse()
                .context(format!("invalid source id: {}", source_id))?;
            let t_id: i64 = target_id
                .parse()
                .context(format!("invalid target id: {}", target_id))?;

            if !item_exists(conn, source_type, s_id)? {
                anyhow::bail!("{} {} does not exist", source_type.as_str(), s_id);
            }
            if !item_exists(conn, target_type, t_id)? {
                anyhow::bail!("{} {} does not exist", target_type.as_str(), t_id);
            }

            // Normalize to the canonical relationship, swapping direction when
            // the user supplied a known inverse. Unknown rels pass through.
            let (canonical_rel, swap) = crate::ops::graph::rel::normalize(&rel);
            let (source_type, source_id, target_type, target_id) = if swap {
                (target_type, target_id, source_type, source_id)
            } else {
                (source_type, source_id, target_type, target_id)
            };

            // Validate ontology constraints for canonical rels only; unknown
            // rels skip all checks. With --force, violations are computed and
            // reported as overrides instead of rejecting.
            let violations = match crate::ops::graph::rel::lookup(&canonical_rel) {
                Some(spec) => validate_constraints(
                    conn,
                    spec,
                    source_type.as_str(),
                    &source_id,
                    target_type.as_str(),
                    &target_id,
                )?,
                None => Vec::new(),
            };
            if !force && !violations.is_empty() {
                anyhow::bail!(
                    "{}",
                    violations
                        .iter()
                        .map(|(_, msg)| msg.as_str())
                        .collect::<Vec<_>>()
                        .join("; ")
                );
            }

            let timestamp = now();
            conn.execute(
                "INSERT INTO context_links (source_item_type, source_item_id, target_item_type, target_item_id, relationship_type, description, timestamp) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![source_type.as_str(), source_id, target_type.as_str(), target_id, canonical_rel, description, timestamp],
            )?;

            let id = conn.last_insert_rowid();
            let mut result = get_link(conn, id)?;
            if crate::ops::graph::rel::lookup(&canonical_rel).is_none() {
                if let Value::Object(map) = &mut result {
                    map.insert("unknown_rel".into(), Value::Bool(true));
                }
            }
            if force && !violations.is_empty() {
                if let Value::Object(map) = &mut result {
                    map.insert(
                        "overrides".into(),
                        Value::Array(
                            violations
                                .iter()
                                .map(|(name, _)| Value::String(name.to_string()))
                                .collect(),
                        ),
                    );
                }
            }
            Ok(result)
        }
        LinkCmd::List {
            item_type,
            item_id,
            rel,
            linked_type,
        } => {
            let item_type_str = item_type.as_str();

            let mut conditions = Vec::new();
            let mut p = Vec::<&str>::with_capacity(8);

            conditions.push("((source_item_type = ? AND source_item_id = ?) OR (target_item_type = ? AND target_item_id = ?))");
            p.push(item_type_str);
            p.push(item_id.as_str());
            p.push(item_type_str);
            p.push(item_id.as_str());

            if let Some(r) = &rel {
                conditions.push("relationship_type = ?");
                p.push(r.as_str());
            }

            if let Some(lt) = &linked_type {
                let lt_str = lt.as_str();
                conditions.push("((source_item_type = ? AND target_item_type = ?) OR (target_item_type = ? AND source_item_type = ?))");
                p.push(lt_str);
                p.push(item_type_str);
                p.push(lt_str);
                p.push(item_type_str);
            }

            let where_clause = format!("WHERE {}", conditions.join(" AND "));
            let query = format!("SELECT id, source_item_type, source_item_id, target_item_type, target_item_id, relationship_type, description, timestamp, origin, weight FROM context_links {} ORDER BY id ASC", where_clause);

            let mut stmt = conn.prepare(&query)?;
            let rows = stmt.query_map(rusqlite::params_from_iter(p), |row| {
                let mut link = parse_link_row(row)?;
                let s_type: String = row.get(1)?;
                let s_id: String = row.get(2)?;
                if s_type == item_type_str && s_id == *item_id {
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

/// Compute ontology constraint violations for a canonical rel edge.
///
/// Returns `(constraint_name, message)` pairs for every constraint that
/// would fire; the caller either bails (no --force) or reports the names
/// as overrides (--force).
fn validate_constraints(
    conn: &Connection,
    spec: &crate::ops::graph::rel::RelSpec,
    source_type: &str,
    source_id: &str,
    target_type: &str,
    target_id: &str,
) -> Result<Vec<(&'static str, String)>> {
    let rel = spec.canonical;
    let mut violations = Vec::new();

    if !spec.domain.is_empty() && !spec.domain.contains(&source_type) {
        violations.push((
            "domain",
            format!(
                "link violates domain constraint: {} sources must be one of [{}] (got {}); use --force to override",
                rel,
                spec.domain.join(", "),
                source_type
            ),
        ));
    }

    if !spec.range.is_empty() && !spec.range.contains(&target_type) {
        violations.push((
            "range",
            format!(
                "link violates range constraint: {} targets must be one of [{}] (got {}); use --force to override",
                rel,
                spec.range.join(", "),
                target_type
            ),
        ));
    }

    if spec.same_type && source_type != target_type {
        violations.push((
            "same_type",
            format!(
                "link violates same_type constraint: {} requires source and target item types to match (got {} -> {}); use --force to override",
                rel, source_type, target_type
            ),
        ));
    }

    for partner in spec.disjoint_with {
        // Links are stored in canonical direction. A disjoint partner edge is
        // contradictory in either direction of the pair: for symmetric
        // partners the stored direction is arbitrary, and for directed
        // partners a reverse edge is still contradictory (a reverse
        // depends_on plus a conflicts_with is still a contradiction).
        let count: i64 = conn.query_row(
            "SELECT count(*) FROM context_links WHERE relationship_type = ?1 AND ((source_item_type = ?2 AND source_item_id = ?3 AND target_item_type = ?4 AND target_item_id = ?5) OR (source_item_type = ?4 AND source_item_id = ?5 AND target_item_type = ?2 AND target_item_id = ?3))",
            params![partner, source_type, source_id, target_type, target_id],
            |row| row.get(0),
        )?;
        if count > 0 {
            violations.push((
                "disjoint",
                format!(
                    "link violates disjoint constraint: {} is disjoint with {} (an existing {} link connects these items); use --force to override",
                    rel, partner, partner
                ),
            ));
        }
    }

    if spec.functional_to {
        let count: i64 = conn.query_row(
            "SELECT count(*) FROM context_links WHERE relationship_type = ?1 AND target_item_type = ?2 AND target_item_id = ?3 AND NOT (source_item_type = ?4 AND source_item_id = ?5)",
            params![rel, target_type, target_id, source_type, source_id],
            |row| row.get(0),
        )?;
        if count > 0 {
            violations.push((
                "functional_to",
                format!(
                    "link violates functional_to constraint: {} allows at most one incoming edge per target ({} {} already has one from a different source); use --force to override",
                    rel, target_type, target_id
                ),
            ));
        }
    }

    Ok(violations)
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
        origin: row.get(8)?,
        weight: row.get(9)?,
        direction: None,
    })
}

fn get_link(conn: &Connection, id: i64) -> Result<Value> {
    let mut stmt = conn.prepare("SELECT id, source_item_type, source_item_id, target_item_type, target_item_id, relationship_type, description, timestamp, origin, weight FROM context_links WHERE id = ?")?;
    let link = stmt
        .query_row(params![id], parse_link_row)
        .optional()?
        .context(format!("link {} not found", id))?;
    Ok(serde_json::to_value(link)?)
}
