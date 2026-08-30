//! `engrams coverage <paths>` — anchored-knowledge coverage for a file set.
//!
//! Answers "what fraction of these files have live anchored knowledge, and how
//! far is the nearest decision/pattern from each?" in one call — the per-area
//! complement to `doctor`'s global dead-anchor audit.
//!
//! - live anchor: `item_anchors` row whose item is still active/non-archived
//! - dead anchor: anchor row whose path no longer exists on disk (moves/renames
//!   leave these behind; they silently stop producing graph edges)
//! - hop distance: BFS over `context_links` from the file's code node to the
//!   nearest decision/pattern

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

use anyhow::Result;
use rusqlite::Connection;
use serde_json::{json, Value};

pub fn handle(conn: &Connection, paths: Vec<String>, diff: Option<String>) -> Result<Value> {
    let root = crate::db::workspace_root()?;

    let (files, changed_only) = if let Some(base) = &diff {
        (diff_files(&root, base)?, true)
    } else {
        (walk_files(&root, &paths)?, false)
    };

    // Live-anchor map for the area: path → (type, id).
    let mut stmt = conn.prepare("SELECT item_type, item_id, path FROM item_anchors")?;
    let anchor_rows: Vec<(String, i64, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .filter_map(|r| r.ok())
        .collect();

    let mut anchored_files: HashSet<String> = HashSet::new();
    let mut dead_anchors: Vec<Value> = Vec::new();
    let live_by_type = live_ids(conn, &anchor_rows)?;

    for (ty, id, path) in &anchor_rows {
        if live_by_type.contains(&(ty.as_str(), *id)) {
            anchored_files.insert(path.clone());
        }
        if files.contains(path) && !Path::new(&root).join(path).exists() {
            dead_anchors.push(json!({
                "path": path,
                "item_type": ty,
                "item_id": id,
            }));
        }
    }

    // Hop distance: BFS from each file's code node to the nearest
    // decision/pattern over the whole link graph.
    let adjacency = load_adjacency(conn)?;
    let code_id_by_path = code_node_ids(conn)?;
    let mut hops: Vec<f64> = Vec::new();
    for f in &files {
        if let Some(start) = code_id_by_path.get(f) {
            if let Some(d) = bfs_hops(&adjacency, ("code", start.as_str())) {
                hops.push(d);
            }
        }
    }
    hops.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median_hops = if hops.is_empty() {
        None
    } else {
        Some(hops[hops.len() / 2])
    };

    let anchored = files.iter().filter(|f| anchored_files.contains(*f)).count();
    let total = files.len();
    let mut out = serde_json::Map::new();
    out.insert("files".into(), json!(total));
    out.insert("anchored".into(), json!(anchored));
    out.insert(
        "anchored_pct".into(),
        json!(if total == 0 {
            None
        } else {
            Some(((anchored as f64 / total as f64) * 1000.0).round() / 1000.0)
        }),
    );
    out.insert("dead_anchors".into(), json!(dead_anchors));
    out.insert("median_hops".into(), json!(median_hops));
    if changed_only {
        out.insert(
            "changed_files".into(),
            json!(files.iter().cloned().collect::<Vec<_>>()),
        );
        if total <= 50 {
            out.insert(
                "unanchored_files".into(),
                json!(files
                    .iter()
                    .filter(|f| !anchored_files.contains(*f))
                    .cloned()
                    .collect::<Vec<_>>()),
            );
        }
    }
    Ok(Value::Object(out))
}

/// Live (active + non-archived) item ids per type, restricted to the anchored
/// set. One query per type, not one per anchor.
fn live_ids(
    conn: &Connection,
    anchor_rows: &[(String, i64, String)],
) -> Result<HashSet<(&'static str, i64)>> {
    let mut live = HashSet::new();
    let mut check = |conn: &Connection, table: &str, ty: &'static str| -> Result<()> {
        let ids: Vec<i64> = anchor_rows
            .iter()
            .filter(|(t, _, _)| t == ty)
            .map(|(_, i, _)| *i)
            .collect();
        if ids.is_empty() {
            return Ok(());
        }
        let status_col = if table == "decisions" {
            " AND status = 'active'"
        } else {
            ""
        };
        let placeholders = crate::ops::sql_placeholders(ids.len());
        let sql = format!(
            "SELECT id FROM {} WHERE id IN ({}) AND archived = 0{}",
            table, placeholders, status_col
        );
        let mut stmt = conn.prepare(&sql)?;
        let p: Vec<&dyn rusqlite::ToSql> = ids.iter().map(|i| i as &dyn rusqlite::ToSql).collect();
        let rows = stmt.query_map(rusqlite::params_from_iter(p), |r| r.get::<_, i64>(0))?;
        for r in rows {
            live.insert((ty, r?));
        }
        Ok(())
    };
    check(conn, "decisions", "decision")?;
    check(conn, "system_patterns", "system_pattern")?;
    Ok(live)
}

/// Node keys are `(item_type, id-string)`; values are that node's neighbors.
type Adjacency = HashMap<(String, String), Vec<(String, String)>>;

/// Undirected adjacency over all context_links, node keys normalized to
/// `(type, id-string)` to match code-node id strings.
fn load_adjacency(conn: &Connection) -> Result<Adjacency> {
    let mut stmt = conn.prepare(
        "SELECT source_item_type, source_item_id, target_item_type, target_item_id \
         FROM context_links",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
        ))
    })?;
    let mut adj: HashMap<(String, String), Vec<(String, String)>> = HashMap::new();
    for r in rows {
        let (st, si, tt, ti) = r?;
        let a = (st, si);
        let b = (tt, ti);
        adj.entry(a.clone()).or_default().push(b.clone());
        adj.entry(b).or_default().push(a);
    }
    Ok(adj)
}

fn code_node_ids(conn: &Connection) -> Result<HashMap<String, String>> {
    let mut stmt =
        conn.prepare("SELECT path, id FROM code_nodes WHERE kind = 'file' AND symbol = ''")?;
    let rows = stmt.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?.to_string()))
    })?;
    rows.collect::<std::result::Result<HashMap<_, _>, _>>()
        .map_err(Into::into)
}

/// BFS hop distance to the nearest decision or system_pattern.
fn bfs_hops(
    adj: &HashMap<(String, String), Vec<(String, String)>>,
    start: (&str, &str),
) -> Option<f64> {
    let start = (start.0.to_string(), start.1.to_string());
    let mut seen: HashSet<(String, String)> = HashSet::new();
    seen.insert(start.clone());
    let mut frontier = VecDeque::from([(start, 0u64)]);
    while let Some((node, d)) = frontier.pop_front() {
        if node.0 == "decision" || node.0 == "system_pattern" {
            return Some(d as f64);
        }
        if let Some(next) = adj.get(&node) {
            for n in next {
                if seen.insert(n.clone()) {
                    frontier.push_back((n.clone(), d + 1));
                }
            }
        }
    }
    None
}

/// Workspace-relative files under the given paths (existing `check` convention:
/// `ignore::WalkBuilder`, hidden dirs skipped, workspace-relative output).
fn walk_files(root: &Path, paths: &[String]) -> Result<HashSet<String>> {
    let mut files = HashSet::new();
    if paths.is_empty() {
        for result in ignore::WalkBuilder::new(root).build() {
            let Ok(entry) = result else { continue };
            if entry.file_type().is_some_and(|t| t.is_file()) {
                if let Ok(rel) = entry.path().strip_prefix(root) {
                    files.insert(rel.to_string_lossy().into_owned());
                }
            }
        }
        return Ok(files);
    }
    for p in paths {
        let abs = root.join(p);
        if abs.is_file() {
            files.insert(p.trim_start_matches("./").to_string());
        } else {
            for result in ignore::WalkBuilder::new(&abs).build() {
                let Ok(entry) = result else { continue };
                if entry.file_type().is_some_and(|t| t.is_file()) {
                    if let Ok(rel) = entry.path().strip_prefix(root) {
                        files.insert(rel.to_string_lossy().into_owned());
                    }
                }
            }
        }
    }
    Ok(files)
}

/// Paths changed between `<base>...HEAD` (or plain `<base>` when the
/// merge-base range is empty, e.g. a fresh branch).
fn diff_files(root: &Path, base: &str) -> Result<HashSet<String>> {
    let spec = format!("{}...HEAD", base);
    let mut out = std::process::Command::new("git")
        .args(["diff", "--name-only", &spec])
        .current_dir(root)
        .output()?;
    if !out.status.success() {
        out = std::process::Command::new("git")
            .args(["diff", "--name-only", base])
            .current_dir(root)
            .output()?;
    }
    anyhow::ensure!(
        out.status.success(),
        "git diff failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.trim().to_string())
        .collect())
}
