//! `engrams brief <node|query>` — one-call composite read of a graph node.
//!
//! Returns everything an agent needs to act on a node — summary, contract,
//! rationale (clamped), tags, PRs, anchors, and enriched 1..depth-hop
//! neighbors — so strategy formation does not require opening files.

/// One code_nodes row: path, symbols JSON, module doc, line count.
type CodeRow = (String, Option<String>, Option<String>, Option<i64>);

use std::collections::{HashMap, HashSet, VecDeque};

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Value};

use crate::ops::graph::parse_node;

const MAX_TEXT: usize = 300;
const MAX_NEIGHBOR_TEXT: usize = 120;
const MAX_DOC_CHARS: usize = 200;

pub fn handle(conn: &Connection, target: &str, depth: i64) -> Result<Value> {
    let key = resolve(conn, target)?;
    let depth = depth.clamp(1, 3);
    crate::ops::usage::record(conn, "brief", target, 1, false);
    let node = node_payload(conn, &key)?;

    // BFS over context_links to `depth` hops; neighbors get full enrichment
    // at hop 1, bare refs beyond.
    let mut seen: HashSet<(String, String)> = HashSet::new();
    seen.insert(key.clone());
    let mut frontier: VecDeque<(String, String)> = VecDeque::new();
    frontier.push_back(key.clone());
    let mut neighbors: Vec<Value> = Vec::new();
    let mut current_depth = 0i64;
    while !frontier.is_empty() && current_depth < depth {
        current_depth += 1;
        let layer: Vec<(String, String)> = frontier.drain(..).collect();
        for (ty, id) in &layer {
            for n in direct_neighbors(conn, ty, id)? {
                let nk = (
                    n["kind"].as_str().unwrap_or_default().to_string(),
                    n["id"].as_str().unwrap_or_default().to_string(),
                );
                let is_new = seen.insert(nk.clone());
                if current_depth == 1 {
                    neighbors.push(n);
                }
                if is_new && current_depth < depth {
                    frontier.push_back(nk);
                }
            }
        }
    }
    // Batched enrichment: one query per node type, not one per neighbor.
    enrich_neighbors(conn, &mut neighbors)?;

    // Reinforce-on-read: a brief counts as retrieval of the item.
    match key.0.as_str() {
        "decision" => {
            if let Ok(id) = key.1.parse::<i64>() {
                crate::ops::scoring::reinforce(conn, "decisions", &[id])?;
            }
        }
        "system_pattern" => {
            if let Ok(id) = key.1.parse::<i64>() {
                crate::ops::scoring::reinforce(conn, "system_patterns", &[id])?;
            }
        }
        _ => {}
    }
    // Surfacing telemetry (AC-10): a schema brief is a schema retrieval, and
    // its hop-1 neighbors are the co-surfaced set for this event.
    if key.0 == "schema" {
        if let Ok(id) = key.1.parse::<i64>() {
            let co: Vec<(&str, i64)> = neighbors
                .iter()
                .filter_map(|n| {
                    let kind = n["kind"].as_str()?;
                    let nid = n["id"].as_str()?.parse::<i64>().ok()?;
                    Some((kind, nid))
                })
                .collect();
            crate::ops::schemas::retrieval::record_surface(
                conn,
                "brief",
                Some(target),
                &[id],
                &co,
            )?;
        }
    }

    Ok(json!({
        "node": format!("{}:{}", key.0, key.1),
        "kind": key.0,
        "depth": depth,
        "brief": node,
        "neighbors": neighbors,
    }))
}

/// Accept `type:id` refs; anything else is an FTS query over decisions, then
/// patterns, then custom data (best rank wins, in that priority).
fn resolve(conn: &Connection, target: &str) -> Result<(String, String)> {
    if target.contains(':') {
        if let Ok(k) = parse_node(target) {
            if node_exists(conn, &k)? {
                return Ok(k);
            }
            // Fall through to FTS when the ref names a node we don't have.
        }
    }
    let expr = crate::ops::fts_match_expr(target);
    for (table, fts, ty) in [
        ("decisions", "decisions_fts", "decision"),
        ("system_patterns", "system_patterns_fts", "system_pattern"),
        ("custom_data", "custom_data_fts", "custom_data"),
    ] {
        let sql = format!(
            "SELECT f.rowid FROM {fts} f JOIN {table} t ON t.id = f.rowid \
             WHERE {fts} MATCH ?1 ORDER BY rank LIMIT 1"
        );
        if let Ok(id) = conn.query_row(&sql, [&expr], |row| row.get::<_, i64>(0)) {
            return Ok((ty.to_string(), id.to_string()));
        }
    }
    anyhow::bail!("no node '{}' and no FTS match for the query", target)
}

fn node_exists(conn: &Connection, key: &(String, String)) -> Result<bool> {
    let found: Option<String> = match key.0.as_str() {
        "decision" => conn
            .query_row(
                "SELECT uuid FROM decisions WHERE id = ?1",
                [&key.1],
                |r| r.get(0),
            )
            .ok(),
        "system_pattern" => conn
            .query_row(
                "SELECT uuid FROM system_patterns WHERE id = ?1",
                [&key.1],
                |r| r.get(0),
            )
            .ok(),
        "progress_entry" => conn
            .query_row(
                "SELECT uuid FROM progress_entries WHERE id = ?1",
                [&key.1],
                |r| r.get(0),
            )
            .ok(),
        "custom_data" => conn
            .query_row(
                "SELECT key FROM custom_data WHERE id = ?1",
                [&key.1],
                |r| r.get(0),
            )
            .ok(),
        "code" => conn
            .query_row(
                "SELECT path FROM code_nodes WHERE id = ?1",
                [&key.1],
                |r| r.get(0),
            )
            .ok(),
        "schema" => conn
            .query_row(
                "SELECT name FROM schemas WHERE id = ?1",
                [&key.1],
                |r| r.get(0),
            )
            .ok(),
        // PR "nodes" are context_links rows; the id is the URL itself.
        "pr" => conn
            .query_row(
                "SELECT target_item_id FROM context_links WHERE target_item_type = 'pr' AND target_item_id = ?1 LIMIT 1",
                [&key.1],
                |r| r.get(0),
            )
            .ok(),
        _ => None,
    };
    Ok(found.is_some())
}

/// The node's own composite payload.
fn node_payload(conn: &Connection, key: &(String, String)) -> Result<Value> {
    let (ty, id) = key;
    match ty.as_str() {
        "decision" => {
            let idv: i64 = id.parse().context("bad decision id")?;
            let mut stmt = conn.prepare(
                "SELECT summary, rationale, implementation_details, contract, tags, status, timestamp, importance, commit_sha \
                 FROM decisions WHERE id = ?1",
            )?;
            let row = stmt
                .query_row([idv], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, Option<String>>(1)?,
                        r.get::<_, Option<String>>(2)?,
                        r.get::<_, Option<String>>(3)?,
                        r.get::<_, Option<String>>(4)?,
                        r.get::<_, String>(5)?,
                        r.get::<_, String>(6)?,
                        r.get::<_, i64>(7)?,
                        r.get::<_, Option<String>>(8)?,
                    ))
                })
                .optional()?
                .context(format!("decision {} not found", id))?;
            let tags_str: Option<String> = row.4;
            let tags: Vec<Value> = match tags_str {
                Some(s) => serde_json::from_str::<Value>(&s)
                    .ok()
                    .and_then(|v| v.as_array().cloned())
                    .unwrap_or_default(),
                None => Vec::new(),
            };
            let mut payload = json!({
                "summary": row.0,
                "rationale": clamp_opt(row.1.as_deref(), MAX_TEXT),
                "implementation_details": clamp_opt(row.2.as_deref(), MAX_TEXT),
                "contract": row.3,
                "tags": tags,
                "status": row.5,
                "timestamp": row.6,
                "importance": row.7,
                "commit_sha": row.8,
                "pr_urls": crate::ops::pr::pr_urls_for(conn, ty, idv)?,
                "anchors": anchor_paths(conn, ty, id)?,
            });
            // Staleness drift (2.3): trust signal — were the anchored files
            // committed after this decision? Null (absent) when no signal.
            if let Ok(root) = crate::db::workspace_root() {
                let mut drift = crate::ops::drift::Drift::scan(&root);
                let report = drift.report(conn, ty, idv, &row.6, row.8.as_deref());
                if !report.is_null() {
                    payload["drift"] = report;
                }
            }
            Ok(payload)
        }
        "system_pattern" => {
            let idv: i64 = id.parse().context("bad pattern id")?;
            let mut stmt = conn.prepare(
                "SELECT name, description, tags, check_kind, check_expr, severity, confidence \
                 FROM system_patterns WHERE id = ?1",
            )?;
            let row = stmt
                .query_row([idv], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, Option<String>>(1)?,
                        r.get::<_, Option<String>>(2)?,
                        r.get::<_, Option<String>>(3)?,
                        r.get::<_, Option<String>>(4)?,
                        r.get::<_, Option<String>>(5)?,
                        r.get::<_, f64>(6)?,
                    ))
                })
                .optional()?
                .context(format!("pattern {} not found", id))?;
            let tags_str: Option<String> = row.2;
            let tags: Vec<Value> = match tags_str {
                Some(s) => serde_json::from_str::<Value>(&s)
                    .ok()
                    .and_then(|v| v.as_array().cloned())
                    .unwrap_or_default(),
                None => Vec::new(),
            };
            Ok(json!({
                "name": row.0,
                "description": clamp_opt(row.1.as_deref(), MAX_TEXT),
                "tags": tags,
                "check_kind": row.3,
                "check_expr": row.4,
                "severity": row.5,
                "confidence": row.6,
                "pr_urls": crate::ops::pr::pr_urls_for(conn, ty, idv)?,
                "anchors": anchor_paths(conn, ty, id)?,
            }))
        }
        "progress_entry" => {
            let idv: i64 = id.parse().context("bad progress id")?;
            let row: (String, String, String, Option<String>) = conn
                .query_row(
                    "SELECT status, description, timestamp, parent_id FROM progress_entries WHERE id = ?1",
                    [idv],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )
                .optional()?
                .context(format!("progress entry {} not found", id))?;
            Ok(json!({
                "status": row.0,
                "description": clamp_opt(Some(row.1.as_str()), MAX_TEXT),
                "timestamp": row.2,
                "parent_id": row.3,
                "anchors": anchor_paths(conn, ty, id)?,
            }))
        }
        "custom_data" => {
            let idv: i64 = id.parse().context("bad custom-data id")?;
            let row: (String, String) = conn
                .query_row(
                    "SELECT key, value FROM custom_data WHERE id = ?1",
                    [idv],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?
                .context(format!("custom-data {} not found", id))?;
            Ok(json!({ "key": row.0, "value": row.1 }))
        }
        "code" => {
            let idv: i64 = id.parse().context("bad code id")?;
            let row: (String, Option<String>, Option<String>, Option<i64>) = conn
                .query_row(
                    "SELECT path, symbols, module_doc, line_count FROM code_nodes WHERE id = ?1",
                    [idv],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )
                .optional()?
                .context(format!("code node {} not found", id))?;
            Ok(json!({
                "path": row.0,
                "symbols": row.1.and_then(|s| serde_json::from_str::<Value>(&s).ok()),
                "module_doc": clamp_opt(row.2.as_deref(), MAX_TEXT),
                "line_count": row.3,
            }))
        }
        "pr" => Ok(json!({ "url": id })),
        "schema" => {
            crate::ops::schemas::retrieval::node_payload(conn, id.parse().context("bad schema id")?)
        }
        other => anyhow::bail!("brief does not support node type '{}'", other),
    }
}

/// Direct (1-hop) context_links neighbors of a node.
fn direct_neighbors(conn: &Connection, ty: &str, id: &str) -> Result<Vec<Value>> {
    let mut stmt = conn.prepare(
        "SELECT source_item_type, source_item_id, target_item_type, target_item_id, relationship_type, origin, weight \
         FROM context_links \
         WHERE (source_item_type = ?1 AND source_item_id = ?2) \
            OR (target_item_type = ?1 AND target_item_id = ?2)",
    )?;
    let rows = stmt.query_map([ty, id], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, Option<String>>(5)?,
            r.get::<_, f64>(6)?,
        ))
    })?;
    let mut out = Vec::new();
    for r in rows {
        let (src_ty, src_id, dst_ty, dst_id, rel, origin, weight) = r?;
        let outgoing = src_ty == ty && src_id == id;
        let (n_ty, n_id) = if outgoing {
            (dst_ty, dst_id)
        } else {
            (src_ty, src_id)
        };
        out.push(json!({
            "node": format!("{}:{}", n_ty, n_id),
            "kind": n_ty,
            "id": n_id,
            "rel": rel,
            "direction": if outgoing { "outgoing" } else { "incoming" },
            "origin": origin,
            "weight": weight,
        }));
    }
    Ok(out)
}

/// Enrich hop-1 neighbors with batched per-type lookups — one query per node
/// type instead of one per neighbor. Decisions missing from the live set are
/// flagged `archived`; PR ids are URLs, not integers, and surface as `url`.
fn enrich_neighbors(conn: &Connection, neighbors: &mut [Value]) -> Result<()> {
    let mut dec: Vec<(i64, usize)> = Vec::new();
    let mut pat: Vec<(i64, usize)> = Vec::new();
    let mut prog: Vec<(i64, usize)> = Vec::new();
    let mut code: Vec<(i64, usize)> = Vec::new();
    for (i, n) in neighbors.iter_mut().enumerate() {
        let ty = n["kind"].as_str().unwrap_or_default();
        let Ok(idv) = n["id"].as_str().unwrap_or_default().parse::<i64>() else {
            if ty == "pr" {
                n["url"] = json!(n["id"].as_str().unwrap_or_default());
            }
            continue;
        };
        match ty {
            "decision" => dec.push((idv, i)),
            "system_pattern" => pat.push((idv, i)),
            "progress_entry" => prog.push((idv, i)),
            "code" => code.push((idv, i)),
            _ => {}
        }
    }

    // Single-text-column batch shared by the three knowledge types.
    let name_map =
        |conn: &Connection, sql: &str, ids: &[(i64, usize)]| -> Result<HashMap<i64, String>> {
            let mut found: HashMap<i64, String> = HashMap::with_capacity(ids.len());
            if ids.is_empty() {
                return Ok(found);
            }
            let mut stmt = conn.prepare(sql)?;
            let id_refs: Vec<&i64> = ids.iter().map(|(id, _)| id).collect();
            let rows = stmt.query_map(rusqlite::params_from_iter(id_refs), |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
            })?;
            for row in rows {
                let row = row?;
                found.insert(row.0, row.1);
            }
            Ok(found)
        };

    {
        let ph = crate::ops::sql_placeholders(dec.len());
        let found = name_map(
            conn,
            &format!("SELECT id, summary FROM decisions WHERE archived = 0 AND id IN ({ph})"),
            &dec,
        )?;
        for (idv, i) in &dec {
            match found.get(idv) {
                Some(s) => neighbors[*i]["summary"] = json!(clamp(s, MAX_NEIGHBOR_TEXT)),
                None => neighbors[*i]["archived"] = json!(true),
            }
        }
    }
    {
        let ph = crate::ops::sql_placeholders(pat.len());
        let found = name_map(
            conn,
            &format!("SELECT id, name FROM system_patterns WHERE id IN ({ph})"),
            &pat,
        )?;
        for (idv, i) in &pat {
            if let Some(s) = found.get(idv) {
                neighbors[*i]["name"] = json!(s);
            }
        }
    }
    {
        let ph = crate::ops::sql_placeholders(prog.len());
        let found = name_map(
            conn,
            &format!("SELECT id, description FROM progress_entries WHERE id IN ({ph})"),
            &prog,
        )?;
        for (idv, i) in &prog {
            if let Some(s) = found.get(idv) {
                neighbors[*i]["description"] = json!(clamp(s, MAX_NEIGHBOR_TEXT));
            }
        }
    }
    if !code.is_empty() {
        let ph = crate::ops::sql_placeholders(code.len());
        let sql = format!(
            "SELECT id, path, symbols, module_doc, line_count FROM code_nodes WHERE id IN ({ph})"
        );
        let mut stmt = conn.prepare(&sql)?;
        let id_refs: Vec<&i64> = code.iter().map(|(id, _)| id).collect();
        let rows = stmt.query_map(rusqlite::params_from_iter(id_refs), |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<i64>>(4)?,
            ))
        })?;
        let mut found: HashMap<i64, CodeRow> = HashMap::with_capacity(code.len());
        for row in rows {
            let row = row?;
            found.insert(row.0, (row.1, row.2, row.3, row.4));
        }
        for (idv, i) in &code {
            if let Some((path, symbols, doc, lines)) = found.get(idv) {
                neighbors[*i]["path"] = json!(path);
                if let Some(s) = symbols
                    .as_deref()
                    .and_then(|s| serde_json::from_str::<Value>(s).ok())
                {
                    neighbors[*i]["symbols"] = s;
                }
                if let Some(d) = doc {
                    neighbors[*i]["module_doc"] = json!(clamp(d, MAX_DOC_CHARS));
                }
                if let Some(l) = lines {
                    neighbors[*i]["line_count"] = json!(l);
                }
            }
        }
    }
    Ok(())
}
fn anchor_paths(conn: &Connection, ty: &str, id: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT path FROM item_anchors WHERE item_type = ?1 AND item_id = ?2 ORDER BY id ASC",
    )?;
    let rows = stmt.query_map([ty, id], |r| r.get::<_, String>(0))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

fn clamp(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &s[..end])
}

fn clamp_opt(s: Option<&str>, max: usize) -> Option<String> {
    s.map(|t| clamp(t, max))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_respects_char_boundaries() {
        let s = "héllo world this is long";
        let c = clamp(s, 5);
        assert!(c.starts_with('h'));
        assert!(c.chars().count() <= 6);
    }

    #[test]
    fn clamp_short_is_identity() {
        assert_eq!(clamp("abc", 10), "abc");
    }
}
