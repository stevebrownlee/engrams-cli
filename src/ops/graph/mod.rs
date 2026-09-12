//! `graph` command family: rebuild/ingest plus in-memory analytics.

pub mod code;
pub mod louvain;
pub mod model;
pub mod rebuild;
pub mod rel;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension};
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
        GraphCmd::Chain {
            node,
            item_type,
            item_id,
            rel,
        } => chain(conn, node, item_type, item_id, &rel),
        GraphCmd::Why {
            target,
            node,
            item_type,
            item_id,
            down,
        } => why(conn, target.or(node), item_type, item_id, down),
    }
}

/// Parse a `type:id` node reference, expanding short aliases and accepting
/// both the CLI's hyphenated item types (`system-pattern`) and the store's
/// underscored forms (`system_pattern`).
pub fn parse_node(s: &str) -> Result<NodeKey> {
    let (ty, id) = s
        .split_once(':')
        .ok_or_else(|| anyhow::anyhow!("invalid node reference '{}' (expected type:id)", s))?;
    if ty.is_empty() || id.is_empty() {
        anyhow::bail!("invalid node reference '{}' (expected type:id)", s);
    }
    let ty = match ty {
        "pattern" | "system-pattern" | "system_pattern" => "system_pattern",
        "progress" | "progress-entry" | "progress_entry" => "progress_entry",
        "custom" | "custom-data" | "custom_data" => "custom_data",
        other => other,
    };
    Ok((ty.to_string(), id.to_string()))
}

fn stats(conn: &Connection) -> Result<Value> {
    let g = model::load(conn)?;
    let deg = g.degree();

    // BTreeMap, not HashMap: these serialize into the JSON output and the
    // repo's byte-determinism standard (scan-twice-identical) forbids
    // per-process hash ordering.
    let mut nodes_by_type: BTreeMap<&str, usize> = BTreeMap::new();
    for (kind, _) in &g.nodes {
        *nodes_by_type.entry(kind.as_str()).or_insert(0) += 1;
    }
    let mut edges_by_rel: BTreeMap<&str, usize> = BTreeMap::new();
    let mut edges_by_origin: BTreeMap<&str, usize> = BTreeMap::new();
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

/// Enrichment metadata for a code node, loaded for graph rendering.
struct CodeInfo {
    path: String,
    symbols: Option<Value>,
    module_doc: Option<String>,
    line_count: Option<i64>,
}

/// code_nodes id-string → info, for human-readable query output.
fn code_paths(conn: &Connection) -> Result<HashMap<String, CodeInfo>> {
    let mut stmt = conn.prepare(
        "SELECT id, path, symbols, module_doc, line_count FROM code_nodes WHERE kind = 'file'",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?.to_string(),
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<i64>>(4)?,
        ))
    })?;
    let mut map = HashMap::new();
    for r in rows {
        let (k, path, symbols, module_doc, line_count) = r?;
        map.insert(
            k,
            CodeInfo {
                path,
                symbols: symbols.and_then(|s| serde_json::from_str(&s).ok()),
                module_doc,
                line_count,
            },
        );
    }
    Ok(map)
}

/// `{"node": "code:5", "path": "src/main.rs", ...}` — enrichment fields for
/// code nodes so strategy queries can describe a file without reading it.
fn node_json(key: &NodeKey, paths: &HashMap<String, CodeInfo>) -> Value {
    let mut v = json!({"node": model::fmt_node(key)});
    if key.0 == "code" {
        if let Some(info) = paths.get(&key.1) {
            v["path"] = json!(info.path);
            if let Some(s) = &info.symbols {
                v["symbols"] = s.clone();
            }
            if let Some(d) = &info.module_doc {
                v["module_doc"] = json!(d);
            }
            if let Some(n) = info.line_count {
                v["line_count"] = json!(n);
            }
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

fn chain(
    conn: &Connection,
    node: Option<String>,
    item_type: Option<String>,
    item_id: Option<String>,
    rel: &str,
) -> Result<Value> {
    if !rel::lookup(rel).map(|s| s.transitive).unwrap_or(false) {
        anyhow::bail!(
            "rel '{}' is not a canonical transitive relationship (supersedes, depends_on, part_of, refines, causes)",
            rel
        );
    }
    let key = match (node, item_type, item_id) {
        (Some(n), None, None) => parse_node(&n)?,
        (None, Some(t), Some(i)) => parse_node(&format!("{}:{}", t, i))?,
        _ => anyhow::bail!("chain requires --node type:id or --item-type + --item-id"),
    };
    let g = model::load(conn)?;
    if !g.contains(&key) {
        anyhow::bail!("unknown node '{}:{}", key.0, key.1);
    }
    let paths = code_paths(conn)?;
    let reachable = g.transitive_reachable(&key, rel);
    Ok(json!({
        "node": model::fmt_node(&key),
        "rel": rel,
        "reachable": reachable.iter().map(|(k, d)| {
            let mut n = node_json(k, &paths);
            n["depth"] = json!(d);
            n
        }).collect::<Vec<_>>(),
    }))
}

/// `graph why`: upstream causal chain over `causes` (transitive, reverse
/// walk), or downstream impact with `--down`. Roots are chain nodes that
/// are not the parent of any other chain node.
fn why(
    conn: &Connection,
    node: Option<String>,
    item_type: Option<String>,
    item_id: Option<String>,
    down: bool,
) -> Result<Value> {
    let key = match (node, item_type, item_id) {
        (Some(n), None, None) => parse_node(&n)?,
        (None, Some(t), Some(i)) => parse_node(&format!("{}:{}", t, i))?,
        _ => anyhow::bail!("why requires a node (type:id) or --item-type + --item-id"),
    };
    let g = model::load(conn)?;
    if !g.contains(&key) {
        anyhow::bail!("unknown node '{}'", model::fmt_node(&key));
    }
    let paths = code_paths(conn)?;
    let traced = g.transitive_reachable_traced(&key, "causes", !down);
    let mut chain = Vec::with_capacity(traced.len());
    for (k, depth, parent) in &traced {
        let mut n = node_json(k, &paths);
        n["depth"] = json!(depth);
        n["via_edge_description"] = edge_description(conn, "causes", parent, k, down)?;
        chain.push(n);
    }
    // Roots: chain nodes that no other chain node was discovered from.
    let roots = traced
        .iter()
        .filter(|(k, _, _)| !traced.iter().any(|(_, _, p)| p == k))
        .map(|(k, _, _)| json!(model::fmt_node(k)))
        .collect::<Vec<_>>();
    Ok(json!({
        "node": model::fmt_node(&key),
        "rel": "causes",
        "direction": if down { "downstream" } else { "upstream" },
        "chain": chain,
        "roots": roots,
    }))
}

/// Description of the `rel` edge between `a` and `b` in walk direction
/// (`down`: a→b; upstream: b→a). Nodes without a stored link (e.g. code)
/// yield null.
fn edge_description(
    conn: &Connection,
    rel: &str,
    a: &NodeKey,
    b: &NodeKey,
    down: bool,
) -> Result<Value> {
    let (src, tgt) = if down { (a, b) } else { (b, a) };
    let desc = conn
        .query_row(
            "SELECT description FROM context_links \
             WHERE relationship_type = ?1 AND source_item_type = ?2 AND source_item_id = ?3 \
             AND target_item_type = ?4 AND target_item_id = ?5",
            rusqlite::params![rel, src.0, src.1, tgt.0, tgt.1],
            |r| r.get::<_, Option<String>>(0),
        )
        .optional()?
        .flatten();
    Ok(json!(desc))
}
