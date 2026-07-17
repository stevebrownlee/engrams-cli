//! `graph` command family: rebuild/ingest plus in-memory analytics.

pub mod code;
pub mod model;
pub mod rebuild;
pub mod rel;

use std::collections::{HashMap, HashSet};
use std::path::Path;

use anyhow::Result;
use rusqlite::Connection;
use serde_json::{json, Value};

use crate::cli::GraphCmd;

use model::NodeKey;

pub fn handle(conn: &mut Connection, cmd: GraphCmd, _db_path: &Path) -> Result<Value> {
    match cmd {
        GraphCmd::Rebuild {
            no_git,
            min_cochange,
            max_commits,
        } => rebuild::rebuild(
            conn,
            &rebuild::RebuildOpts {
                no_git,
                min_cochange,
                max_commits,
            },
        ),
        GraphCmd::Ingest {
            since,
            max_commits,
            min_cochange,
        } => rebuild::ingest(conn, since, max_commits, min_cochange),
        GraphCmd::Stats => stats(conn),
        GraphCmd::Central { limit, node_type } => central(conn, limit, node_type),
        GraphCmd::Clusters { limit } => clusters(conn, limit),
        GraphCmd::Orphans { limit } => orphans(conn, limit),
        GraphCmd::Path { from, to } => path(conn, &from, &to),
        GraphCmd::Neighbors { node, depth, rel } => neighbors(conn, &node, depth, rel),
    }
}

/// Parse a `type:id` node reference, expanding short aliases.
pub fn parse_node(s: &str) -> Result<NodeKey> {
    let (ty, id) = s
        .split_once(':')
        .ok_or_else(|| anyhow::anyhow!("invalid node reference '{}' (expected type:id)", s))?;
    if ty.is_empty() || id.is_empty() {
        anyhow::bail!("invalid node reference '{}' (expected type:id)", s);
    }
    let ty = match ty {
        "pattern" => "system_pattern",
        "progress" => "progress_entry",
        "custom" => "custom_data",
        other => other,
    };
    Ok((ty.to_string(), id.to_string()))
}

fn stats(conn: &Connection) -> Result<Value> {
    let g = model::load(conn)?;
    let deg = g.degree();

    let mut nodes_by_type: HashMap<&str, usize> = HashMap::new();
    for (kind, _) in &g.nodes {
        *nodes_by_type.entry(kind.as_str()).or_insert(0) += 1;
    }
    let mut edges_by_rel: HashMap<&str, usize> = HashMap::new();
    let mut edges_by_origin: HashMap<&str, usize> = HashMap::new();
    for e in &g.edges {
        *edges_by_rel.entry(e.rel.as_str()).or_insert(0) += 1;
        *edges_by_origin.entry(e.origin.as_str()).or_insert(0) += 1;
    }

    let components: HashSet<usize> = g.components().into_iter().collect();

    Ok(json!({
        "nodes": {
            "total": g.nodes.len(),
            "by_type": nodes_by_type,
        },
        "edges": {
            "total": g.edges.len(),
            "by_relationship": edges_by_rel,
            "by_origin": edges_by_origin,
        },
        "density": g.density(),
        "components": components.len(),
        "orphans": g.orphans().len(),
        "degree": model::degree_stats(&deg),
    }))
}

/// code_nodes id-string → path, for human-readable query output.
fn code_paths(conn: &Connection) -> Result<HashMap<String, String>> {
    let mut stmt = conn.prepare("SELECT id, path FROM code_nodes WHERE kind = 'file'")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, i64>(0)?.to_string(), row.get::<_, String>(1)?))
    })?;
    let mut map = HashMap::new();
    for r in rows {
        let (k, v) = r?;
        map.insert(k, v);
    }
    Ok(map)
}

/// `{"node": "code:5", "path": "src/main.rs"}` — `path` only for code nodes.
fn node_json(key: &NodeKey, paths: &HashMap<String, String>) -> Value {
    let mut v = json!({"node": model::fmt_node(key)});
    if key.0 == "code" {
        if let Some(p) = paths.get(&key.1) {
            v["path"] = json!(p);
        }
    }
    v
}

fn central(conn: &Connection, limit: i64, node_type: Option<String>) -> Result<Value> {
    let g = model::load(conn)?;
    let paths = code_paths(conn)?;
    let ranks = g.pagerank();
    let mut order: Vec<usize> = (0..g.nodes.len()).collect();
    order.sort_by(|a, b| {
        ranks[*b]
            .partial_cmp(&ranks[*a])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut out = Vec::new();
    for i in order {
        if let Some(t) = &node_type {
            if &g.nodes[i].0 != t {
                continue;
            }
        }
        let mut entry = node_json(&g.nodes[i], &paths);
        entry["score"] = json!(ranks[i]);
        out.push(entry);
        if out.len() >= limit.max(0) as usize {
            break;
        }
    }
    Ok(json!({
        "centrality": "pagerank",
        "ranked": out,
    }))
}

fn clusters(conn: &Connection, limit: i64) -> Result<Value> {
    let g = model::load(conn)?;
    let paths = code_paths(conn)?;
    let comps = g.components();
    let mut by_comp: HashMap<usize, Vec<usize>> = HashMap::new();
    for (i, c) in comps.iter().enumerate() {
        by_comp.entry(*c).or_default().push(i);
    }
    let mut clusters: Vec<Vec<usize>> = by_comp.into_values().collect();
    clusters.sort_by_key(|c| std::cmp::Reverse(c.len()));
    let out: Vec<Value> = clusters
        .iter()
        .take(limit.max(0) as usize)
        .map(|members| {
            let mut nodes: Vec<Value> = members
                .iter()
                .map(|&i| node_json(&g.nodes[i], &paths))
                .collect();
            nodes.sort_by(|a, b| a["node"].as_str().cmp(&b["node"].as_str()));
            json!({
                "size": members.len(),
                "nodes": nodes,
            })
        })
        .collect();
    Ok(json!({
        "total": out.len(),
        "clusters": out,
    }))
}

fn orphans(conn: &Connection, limit: i64) -> Result<Value> {
    let g = model::load(conn)?;
    let paths = code_paths(conn)?;
    let mut list: Vec<Value> = g.orphans().iter().map(|k| node_json(k, &paths)).collect();
    list.sort_by(|a, b| a["node"].as_str().cmp(&b["node"].as_str()));
    list.truncate(limit.max(0) as usize);
    Ok(json!({ "orphans": list }))
}

fn path(conn: &Connection, from: &str, to: &str) -> Result<Value> {
    let g = model::load(conn)?;
    let paths = code_paths(conn)?;
    let from_key = parse_node(from)?;
    let to_key = parse_node(to)?;
    if !g.contains(&from_key) {
        anyhow::bail!("unknown node '{}'", from);
    }
    if !g.contains(&to_key) {
        anyhow::bail!("unknown node '{}'", to);
    }
    match g.shortest_path(&from_key, &to_key) {
        Some(p) => {
            let hops = p.len().saturating_sub(1);
            Ok(json!({
                "path": p.iter().map(|(k, r)| {
                    let mut hop = node_json(k, &paths);
                    hop["rel"] = json!(r);
                    hop
                }).collect::<Vec<_>>(),
                "hops": hops,
            }))
        }
        None => anyhow::bail!("no path from '{}' to '{}'", from, to),
    }
}

fn neighbors(conn: &Connection, node: &str, depth: i64, rel: Option<String>) -> Result<Value> {
    let g = model::load(conn)?;
    let paths = code_paths(conn)?;
    let key = parse_node(node)?;
    if !g.contains(&key) {
        anyhow::bail!("unknown node '{}'", node);
    }
    let rows = g.neighbors(&key, depth.max(0) as u32, rel.as_deref());
    Ok(json!({
        "node": node,
        "depth": depth,
        "neighbors": rows.iter().map(|(k, d)| {
            let mut n = node_json(k, &paths);
            n["distance"] = json!(d);
            n
        }).collect::<Vec<_>>(),
    }))
}
