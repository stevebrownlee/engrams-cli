//! Derived-edge sources and graph rebuild/ingest/touch.
//!
//! Derived edges live in `context_links` tagged `origin='derived'` with the
//! producing source name in `source`. A partial unique index
//! (`ix_links_derived_uniq`) makes writes idempotent via `INSERT OR IGNORE`.
//! `rebuild` is the source of truth for exact weights; `touch_item` keeps the
//! graph warm incrementally on writes.

use std::collections::{HashMap, HashSet};

use anyhow::Result;
use chrono::{SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

use crate::ops::git;

use super::code;
use super::rel;

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

pub struct DerivedEdge {
    pub src_type: String,
    pub src_id: String,
    pub tgt_type: String,
    pub tgt_id: String,
    pub rel: String,
    pub weight: f64,
}

pub trait EdgeSource {
    fn name(&self) -> &'static str; // stored in context_links.source
    fn emit(&self, conn: &Connection) -> Result<Vec<DerivedEdge>>;
}

const INSERT_DERIVED: &str = "INSERT OR IGNORE INTO context_links \
     (source_item_type, source_item_id, target_item_type, target_item_id, \
     relationship_type, description, timestamp, origin, source, weight) \
     VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6, 'derived', ?7, ?8)";

fn insert_edges(conn: &Connection, source: &str, edges: &[DerivedEdge], ts: &str) -> Result<()> {
    for e in edges {
        conn.execute(
            INSERT_DERIVED,
            params![e.src_type, e.src_id, e.tgt_type, e.tgt_id, e.rel, ts, source, e.weight],
        )?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// AnchorSource: item_anchors rows → anchored_to edges to code file nodes
// ---------------------------------------------------------------------------

pub struct AnchorSource;

impl EdgeSource for AnchorSource {
    fn name(&self) -> &'static str {
        "anchor"
    }

    fn emit(&self, conn: &Connection) -> Result<Vec<DerivedEdge>> {
        let ts = now();
        let mut stmt =
            conn.prepare("SELECT item_type, item_id, path FROM item_anchors ORDER BY id ASC")?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        let mut edges = Vec::new();
        for r in rows {
            let (item_type, item_id, path) = r?;
            let fid = code::upsert_file(conn, &path, &ts)?;
            edges.push(DerivedEdge {
                src_type: item_type,
                src_id: item_id.to_string(),
                tgt_type: "code".to_string(),
                tgt_id: fid.to_string(),
                rel: "anchored_to".to_string(),
                weight: 1.0,
            });
        }
        Ok(edges)
    }
}

// ---------------------------------------------------------------------------
// CoAnchorSource: shared anchors / tag overlap → relates_to between items
// ---------------------------------------------------------------------------

struct KnowledgeItem {
    kind: &'static str,
    id: i64,
    anchors: HashSet<String>,
    tags: HashSet<String>,
}

fn parse_tags(raw: Option<String>) -> HashSet<String> {
    raw.and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
        .unwrap_or_default()
        .into_iter()
        .collect()
}

fn load_knowledge_items(conn: &Connection) -> Result<Vec<KnowledgeItem>> {
    let mut items: Vec<KnowledgeItem> = Vec::new();
    for (kind, table) in [
        ("decision", "decisions"),
        ("system_pattern", "system_patterns"),
    ] {
        let mut stmt = conn.prepare(&format!("SELECT id, tags FROM {}", table))?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?))
        })?;
        for r in rows {
            let (id, tags) = r?;
            items.push(KnowledgeItem {
                kind,
                id,
                anchors: HashSet::new(),
                tags: parse_tags(tags),
            });
        }
    }
    let mut idx: HashMap<(&str, i64), usize> = HashMap::new();
    for (i, item) in items.iter().enumerate() {
        idx.insert((item.kind, item.id), i);
    }
    let mut stmt =
        conn.prepare("SELECT item_type, item_id, path FROM item_anchors ORDER BY id ASC")?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    for r in rows {
        let (item_type, item_id, path) = r?;
        if let Some(&i) = idx.get(&(item_type.as_str(), item_id)) {
            items[i].anchors.insert(path);
        }
    }
    Ok(items)
}

/// One symmetric `relates_to` edge per qualifying pair, in canonical order
/// (smaller `(type, id)` is the source) so the derived unique index never
/// sees the same pair twice.
fn coanchor_edges(a: &KnowledgeItem, b: &KnowledgeItem) -> Option<DerivedEdge> {
    let shared_anchors = a.anchors.intersection(&b.anchors).count();
    let shared_tags = a.tags.intersection(&b.tags).count();
    if shared_anchors < 1 && shared_tags < 2 {
        return None;
    }
    let weight = (shared_anchors + shared_tags) as f64;
    let (first, second) = if (a.kind, a.id) <= (b.kind, b.id) {
        (a, b)
    } else {
        (b, a)
    };
    Some(DerivedEdge {
        src_type: first.kind.to_string(),
        src_id: first.id.to_string(),
        tgt_type: second.kind.to_string(),
        tgt_id: second.id.to_string(),
        rel: "relates_to".to_string(),
        weight,
    })
}

pub struct CoAnchorSource;

impl EdgeSource for CoAnchorSource {
    fn name(&self) -> &'static str {
        "co_anchor"
    }

    fn emit(&self, conn: &Connection) -> Result<Vec<DerivedEdge>> {
        let items = load_knowledge_items(conn)?;
        let mut edges = Vec::new();
        for i in 0..items.len() {
            for j in (i + 1)..items.len() {
                if let Some(e) = coanchor_edges(&items[i], &items[j]) {
                    edges.push(e);
                }
            }
        }
        Ok(edges)
    }
}

// ---------------------------------------------------------------------------
// CoChangeSource: git co-change → co_changes edges between code file nodes
// ---------------------------------------------------------------------------

pub struct CoChangeSource {
    pub since: Option<String>,
    pub max_commits: usize,
    pub min_cochange: i64,
}

impl EdgeSource for CoChangeSource {
    fn name(&self) -> &'static str {
        "cochange"
    }

    fn emit(&self, conn: &Connection) -> Result<Vec<DerivedEdge>> {
        let groups = git::commit_file_groups(self.since.as_deref(), self.max_commits)?;
        let toplevel = git::toplevel().unwrap_or_default();
        let ts = now();
        let mut pair_counts: HashMap<(i64, i64), i64> = HashMap::new();
        for group in &groups {
            let mut ids: Vec<i64> = Vec::new();
            let mut seen: HashSet<&str> = HashSet::new();
            for path in group {
                if !seen.insert(path.as_str()) {
                    continue;
                }
                // Vendored / generated / tool-internal files are not codebase
                // topology (committed node_modules, dist output, lockfiles).
                if code::is_generated(path) {
                    continue;
                }
                // Only files that still exist on disk become nodes/edges.
                if !toplevel.is_empty() && !std::path::Path::new(&toplevel).join(path).exists() {
                    continue;
                }
                ids.push(code::upsert_file(conn, path, &ts)?);
            }
            ids.sort_unstable();
            for i in 0..ids.len() {
                for j in (i + 1)..ids.len() {
                    *pair_counts.entry((ids[i], ids[j])).or_insert(0) += 1;
                }
            }
        }
        let mut pairs: Vec<((i64, i64), i64)> = pair_counts.into_iter().collect();
        pairs.sort();
        let edges = pairs
            .into_iter()
            .filter(|(_, count)| *count >= self.min_cochange)
            .map(|((a, b), count)| DerivedEdge {
                src_type: "code".to_string(),
                src_id: a.to_string(),
                tgt_type: "code".to_string(),
                tgt_id: b.to_string(),
                rel: "co_changes".to_string(),
                weight: count as f64,
            })
            .collect();
        if let Some(sha) = git::head_sha() {
            conn.execute(
                "INSERT INTO graph_meta (id, last_ingest_sha) VALUES (1, ?1) \
                 ON CONFLICT(id) DO UPDATE SET last_ingest_sha = excluded.last_ingest_sha",
                params![sha],
            )?;
        }
        Ok(edges)
    }
}

// ---------------------------------------------------------------------------
// InverseEdgeSource: manual canonical edges → derived inverse-direction edges
// ---------------------------------------------------------------------------

/// Build the derived inverse edge for a manual edge, if its relationship is
/// canonical and declares an inverse. Unknown (free-form) rels and
/// inverse-less canonical rels (e.g. symmetric `relates_to`) emit nothing.
fn inverse_edge(
    src_type: &str,
    src_id: &str,
    tgt_type: &str,
    tgt_id: &str,
    rel_name: &str,
    weight: f64,
) -> Option<DerivedEdge> {
    let spec = rel::lookup(rel_name)?;
    let inverse = spec.inverse?;
    Some(DerivedEdge {
        src_type: tgt_type.to_string(),
        src_id: tgt_id.to_string(),
        tgt_type: src_type.to_string(),
        tgt_id: src_id.to_string(),
        rel: inverse.to_string(),
        weight,
    })
}

pub struct InverseEdgeSource;

impl EdgeSource for InverseEdgeSource {
    fn name(&self) -> &'static str {
        "inverse"
    }

    fn emit(&self, conn: &Connection) -> Result<Vec<DerivedEdge>> {
        let mut stmt = conn.prepare(
            "SELECT source_item_type, source_item_id, target_item_type, target_item_id, relationship_type, weight \
             FROM context_links WHERE origin = 'manual' ORDER BY id ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, f64>(5)?,
            ))
        })?;
        let mut edges = Vec::new();
        for r in rows {
            let (src_type, src_id, tgt_type, tgt_id, rel_name, weight) = r?;
            if let Some(e) = inverse_edge(&src_type, &src_id, &tgt_type, &tgt_id, &rel_name, weight)
            {
                edges.push(e);
            }
        }
        Ok(edges)
    }
}

// ---------------------------------------------------------------------------
// rebuild / ingest / touch_item
// ---------------------------------------------------------------------------

pub struct RebuildOpts {
    pub no_git: bool,
    pub min_cochange: i64,
    pub max_commits: i64,
}

pub fn rebuild(conn: &mut Connection, opts: &RebuildOpts) -> Result<Value> {
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM context_links WHERE origin = 'derived'", [])?;

    let mut sources: Vec<Box<dyn EdgeSource>> = vec![
        Box::new(AnchorSource),
        Box::new(CoAnchorSource),
        Box::new(InverseEdgeSource),
    ];
    if !opts.no_git {
        sources.push(Box::new(CoChangeSource {
            since: None,
            max_commits: opts.max_commits.max(0) as usize,
            min_cochange: opts.min_cochange,
        }));
    }

    let ts = now();
    let mut edges_by_source = serde_json::Map::new();
    for source in &sources {
        let edges = source.emit(&tx)?;
        insert_edges(&tx, source.name(), &edges, &ts)?;
        edges_by_source.insert(source.name().to_string(), json!(edges.len()));
    }

    // Prune code nodes that no derived/manual edge references anymore
    // (e.g. vendored files ingested before filtering existed). Rebuild is
    // the source of truth; ingest stays cumulative and never prunes.
    let pruned = tx.execute(
        "DELETE FROM code_nodes WHERE id NOT IN ( \
            SELECT CAST(source_item_id AS INTEGER) FROM context_links WHERE source_item_type = 'code' \
            UNION \
            SELECT CAST(target_item_id AS INTEGER) FROM context_links WHERE target_item_type = 'code' \
        )",
        [],
    )?;

    tx.execute(
        "INSERT INTO graph_meta (id, last_rebuild_at) VALUES (1, ?1) \
         ON CONFLICT(id) DO UPDATE SET last_rebuild_at = excluded.last_rebuild_at",
        params![ts],
    )?;
    tx.commit()?;

    Ok(json!({
        "rebuilt": true,
        "edges_by_source": Value::Object(edges_by_source),
        "code_nodes_pruned": pruned,
    }))
}

pub fn ingest(
    conn: &mut Connection,
    since: Option<String>,
    max_commits: i64,
    min_cochange: i64,
) -> Result<Value> {
    // Default to incremental: resume from the last ingested commit.
    let since = match since {
        Some(s) => Some(s),
        None => conn
            .query_row(
                "SELECT last_ingest_sha FROM graph_meta WHERE id = 1",
                [],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten(),
    };

    // Guard: no-op when there is nothing new to ingest. Without this,
    // delete-then-window would wipe full-history co_changes edges whenever
    // the window is empty (e.g. right after `rebuild`, or off-repo).
    let head = git::head_sha();
    match (&since, &head) {
        (Some(s), Some(h)) if s == h => {
            return Ok(json!({
                "ingested": false,
                "reason": "no new commits since last ingest",
                "edges_by_source": { "cochange": 0 },
            }));
        }
        (_, None) => {
            return Ok(json!({
                "ingested": false,
                "reason": "not a git repository",
                "edges_by_source": { "cochange": 0 },
            }));
        }
        _ => {}
    }

    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "DELETE FROM context_links WHERE origin = 'derived' AND source = 'cochange'",
        [],
    )?;
    let source = CoChangeSource {
        since,
        max_commits: max_commits.max(0) as usize,
        min_cochange,
    };
    let edges = source.emit(&tx)?;
    let count = edges.len();
    let ts = now();
    insert_edges(&tx, source.name(), &edges, &ts)?;
    tx.commit()?;

    Ok(json!({
        "ingested": true,
        "edges_by_source": { "cochange": count },
    }))
}

/// Incremental refresh for a single item after a write: ensures code nodes
/// plus `anchored_to` edges for its anchors, materializes inverse edges for
/// manual links touching it, and recomputes its `relates_to` edges against
/// all other knowledge items. Idempotent (`INSERT OR IGNORE`), never deletes
/// — `rebuild` remains the source of truth for exact weights.
pub fn touch_item(conn: &Connection, item_type: &str, id: i64) -> Result<()> {
    let ts = now();
    let anchors = crate::ops::anchor::anchors_for(conn, item_type, id)?;
    for path in &anchors {
        let fid = code::upsert_file(conn, path, &ts)?;
        conn.execute(
            INSERT_DERIVED,
            params![
                item_type,
                id.to_string(),
                "code",
                fid.to_string(),
                "anchored_to",
                ts,
                "anchor",
                1.0
            ],
        )?;
    }

    // Manual edges touching this item get their canonical inverses
    // materialized, mirroring InverseEdgeSource at rebuild time. Applies to
    // every item type, like the anchor pass above.
    let id_str = id.to_string();
    let mut stmt = conn.prepare(
        "SELECT source_item_type, source_item_id, target_item_type, target_item_id, relationship_type, weight \
         FROM context_links \
         WHERE origin = 'manual' \
           AND ((source_item_type = ?1 AND source_item_id = ?2) \
                OR (target_item_type = ?1 AND target_item_id = ?2)) \
         ORDER BY id ASC",
    )?;
    let manual: Vec<(String, String, String, String, String, f64)> = stmt
        .query_map(params![item_type, id_str], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, f64>(5)?,
            ))
        })?
        .collect::<rusqlite::Result<_>>()?;
    drop(stmt);
    for (src_type, src_id, tgt_type, tgt_id, rel_name, weight) in &manual {
        if let Some(e) = inverse_edge(src_type, src_id, tgt_type, tgt_id, rel_name, *weight) {
            conn.execute(
                INSERT_DERIVED,
                params![e.src_type, e.src_id, e.tgt_type, e.tgt_id, e.rel, ts, "inverse", e.weight],
            )?;
        }
    }

    if item_type != "decision" && item_type != "system_pattern" {
        return Ok(());
    }
    let my_anchors: HashSet<String> = anchors.into_iter().collect();
    let my_tags: HashSet<String> = {
        let table = if item_type == "decision" {
            "decisions"
        } else {
            "system_patterns"
        };
        let raw: Option<String> = conn
            .query_row(
                &format!("SELECT tags FROM {} WHERE id = ?", table),
                params![id],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        parse_tags(raw)
    };
    let kind: &'static str = if item_type == "decision" {
        "decision"
    } else {
        "system_pattern"
    };
    let me = KnowledgeItem {
        kind,
        id,
        anchors: my_anchors,
        tags: my_tags,
    };
    for other in load_knowledge_items(conn)? {
        if other.kind == item_type && other.id == id {
            continue;
        }
        if let Some(e) = coanchor_edges(&me, &other) {
            conn.execute(
                INSERT_DERIVED,
                params![
                    e.src_type,
                    e.src_id,
                    e.tgt_type,
                    e.tgt_id,
                    e.rel,
                    ts,
                    "co_anchor",
                    e.weight
                ],
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::TempDir::new().unwrap();
        let conn = crate::db::open(&dir.path().join("context.db")).unwrap();
        (dir, conn)
    }

    fn add_decision(conn: &Connection, summary: &str) -> i64 {
        conn.execute(
            "INSERT INTO decisions (uuid, timestamp, summary) VALUES (?1, ?2, ?3)",
            params![format!("uuid-{}", summary), now(), summary],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn add_manual_link(
        conn: &Connection,
        src_type: &str,
        src_id: i64,
        tgt_type: &str,
        tgt_id: i64,
        rel_name: &str,
        weight: f64,
    ) {
        conn.execute(
            "INSERT INTO context_links (source_item_type, source_item_id, target_item_type, target_item_id, relationship_type, timestamp, weight) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                src_type,
                src_id.to_string(),
                tgt_type,
                tgt_id.to_string(),
                rel_name,
                now(),
                weight
            ],
        )
        .unwrap();
    }

    fn derived_inverse_edges(
        conn: &Connection,
    ) -> Vec<(String, String, String, String, String, f64)> {
        let mut stmt = conn
            .prepare(
                "SELECT source_item_type, source_item_id, target_item_type, target_item_id, relationship_type, weight \
                 FROM context_links WHERE origin = 'derived' AND source = 'inverse' ORDER BY id ASC",
            )
            .unwrap();
        stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, f64>(5)?,
            ))
        })
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
    }

    fn rebuild_no_git(conn: &mut Connection) -> Value {
        rebuild(
            conn,
            &RebuildOpts {
                no_git: true,
                min_cochange: 2,
                max_commits: 500,
            },
        )
        .unwrap()
    }

    #[test]
    fn inverse_edges_emitted_with_reverse_direction_rel_and_weight() {
        let (_dir, mut conn) = scratch_db();
        let older = add_decision(&conn, "older");
        let newer = add_decision(&conn, "newer");
        add_manual_link(
            &conn,
            "decision",
            newer,
            "decision",
            older,
            "supersedes",
            2.5,
        );
        add_manual_link(
            &conn,
            "decision",
            newer,
            "decision",
            older,
            "depends_on",
            1.0,
        );

        let result = rebuild_no_git(&mut conn);
        assert_eq!(result["edges_by_source"]["inverse"], json!(2));

        let edges = derived_inverse_edges(&conn);
        assert_eq!(edges.len(), 2);
        assert!(edges.contains(&(
            "decision".to_string(),
            older.to_string(),
            "decision".to_string(),
            newer.to_string(),
            "superseded_by".to_string(),
            2.5,
        )));
        assert!(edges.contains(&(
            "decision".to_string(),
            older.to_string(),
            "decision".to_string(),
            newer.to_string(),
            "depended_on_by".to_string(),
            1.0,
        )));
    }

    #[test]
    fn inverse_edges_skip_symmetric_canonical_and_free_form_rels() {
        let (_dir, mut conn) = scratch_db();
        let a = add_decision(&conn, "a");
        let b = add_decision(&conn, "b");
        // relates_to / conflicts_with: canonical but symmetric, no inverse
        add_manual_link(&conn, "decision", a, "decision", b, "relates_to", 1.0);
        add_manual_link(&conn, "decision", a, "decision", b, "conflicts_with", 1.0);
        // free-form passthrough rels are unknown to the canonical table
        add_manual_link(&conn, "decision", a, "decision", b, "blocks", 1.0);

        let result = rebuild_no_git(&mut conn);
        assert_eq!(result["edges_by_source"]["inverse"], json!(0));
        assert!(derived_inverse_edges(&conn).is_empty());
    }

    #[test]
    fn inverse_edge_source_ignores_derived_edges() {
        let (_dir, mut conn) = scratch_db();
        let a = add_decision(&conn, "a");
        let b = add_decision(&conn, "b");
        add_manual_link(&conn, "decision", a, "decision", b, "supersedes", 1.0);
        // A derived supersedes edge must not spawn a derived inverse.
        conn.execute(
            "INSERT INTO context_links (source_item_type, source_item_id, target_item_type, target_item_id, relationship_type, timestamp, origin, source, weight) \
             VALUES ('decision', ?1, 'decision', ?2, 'supersedes', ?3, 'derived', 'co_anchor', 1.0)",
            params![b.to_string(), a.to_string(), now()],
        )
        .unwrap();

        rebuild_no_git(&mut conn);
        let edges = derived_inverse_edges(&conn);
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].4, "superseded_by");
    }

    #[test]
    fn rebuild_is_idempotent_for_inverse_edges() {
        let (_dir, mut conn) = scratch_db();
        let a = add_decision(&conn, "a");
        let b = add_decision(&conn, "b");
        add_manual_link(&conn, "decision", a, "decision", b, "supersedes", 1.0);

        rebuild_no_git(&mut conn);
        rebuild_no_git(&mut conn);
        assert_eq!(derived_inverse_edges(&conn).len(), 1);
    }

    #[test]
    fn touch_item_materializes_inverse_edges_idempotently() {
        let (_dir, conn) = scratch_db();
        let a = add_decision(&conn, "a");
        let b = add_decision(&conn, "b");
        add_manual_link(&conn, "decision", a, "decision", b, "supersedes", 1.0);

        touch_item(&conn, "decision", b).unwrap();
        let edges = derived_inverse_edges(&conn);
        assert_eq!(edges.len(), 1);
        assert_eq!(
            edges[0],
            (
                "decision".to_string(),
                b.to_string(),
                "decision".to_string(),
                a.to_string(),
                "superseded_by".to_string(),
                1.0,
            )
        );

        // Touching either endpoint again must not duplicate the edge.
        touch_item(&conn, "decision", a).unwrap();
        touch_item(&conn, "decision", b).unwrap();
        assert_eq!(derived_inverse_edges(&conn).len(), 1);
    }
}
