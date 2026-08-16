use anyhow::Result;
use rusqlite::Connection;
use serde_json::Value;

pub fn handle(conn: &Connection, db_path: &std::path::Path) -> Result<Value> {
    // 1. Audit missing anchor paths
    let mut missing_anchor_paths = Vec::new();
    let mut stmt = conn.prepare("SELECT item_type, item_id, path FROM item_anchors")?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    for r in rows {
        let (item_type, item_id, path) = r?;
        let p = std::path::Path::new(&path);
        if !p.exists() {
            missing_anchor_paths.push(serde_json::json!({
                "item_type": item_type,
                "item_id": item_id,
                "path": path,
            }));
        }
    }

    // 2. Audit dangling links
    let mut dangling_links = Vec::new();
    let mut stmt = conn.prepare("SELECT id, source_item_type, source_item_id, target_item_type, target_item_id FROM context_links WHERE target_item_type != 'pr' AND origin='manual'")?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
        ))
    })?;
    for r in rows {
        let (id, src_type, src_id_str, tgt_type, tgt_id_str) = r?;
        let src_id = src_id_str.parse::<i64>().unwrap_or(0);
        let tgt_id = tgt_id_str.parse::<i64>().unwrap_or(0);

        let mut is_dangling = false;
        if let Some(src_table) = get_table_name(&src_type) {
            let exists: bool = conn.query_row(
                &format!("SELECT count(*) FROM {} WHERE id = ?", src_table),
                [src_id],
                |row| row.get::<_, i64>(0).map(|c| c > 0),
            )?;
            if !exists {
                is_dangling = true;
            }
        }
        if let Some(tgt_table) = get_table_name(&tgt_type) {
            let exists: bool = conn.query_row(
                &format!("SELECT count(*) FROM {} WHERE id = ?", tgt_table),
                [tgt_id],
                |row| row.get::<_, i64>(0).map(|c| c > 0),
            )?;
            if !exists {
                is_dangling = true;
            }
        }

        if is_dangling {
            dangling_links.push(serde_json::json!({
                "id": id,
                "source": format!("{}:{}", src_type, src_id_str),
                "target": format!("{}:{}", tgt_type, tgt_id_str),
            }));
        }
    }

    // 3. Audit stale decisions
    let mut stale_decisions = Vec::new();
    let mut git_status = "ok".to_string();

    let mut stmt = conn.prepare("SELECT id, summary, commit_sha FROM decisions WHERE status = 'active' AND commit_sha IS NOT NULL")?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    for r in rows {
        let (id, summary, commit_sha) = r?;
        let anchors = crate::ops::anchor::anchors_for(conn, "decision", id)?;
        if !anchors.is_empty() {
            match crate::ops::git::changed_since(&commit_sha, &anchors) {
                Ok(changed) => {
                    if !changed.is_empty() {
                        stale_decisions.push(serde_json::json!({
                            "id": id,
                            "summary": summary,
                            "commit_sha": commit_sha,
                            "changed_paths": changed,
                        }));
                    }
                }
                Err(_) => {
                    git_status = "unavailable".to_string();
                    stale_decisions.clear();
                    break;
                }
            }
        }
    }

    // 4. Audit unlinked decisions
    let mut unlinked_decisions = Vec::new();
    let mut stmt = conn.prepare(
        "SELECT id, summary FROM decisions \
         WHERE status = 'active' AND commit_sha IS NOT NULL \
         AND NOT EXISTS (\
             SELECT 1 FROM context_links \
             WHERE source_item_type = 'decision' AND source_item_id = CAST(decisions.id AS TEXT) \
             AND target_item_type = 'pr'\
         )",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })?;
    for r in rows {
        let (id, summary) = r?;
        unlinked_decisions.push(serde_json::json!({
            "id": id,
            "summary": summary,
        }));
    }

    // 4b. Audit never-read records (written but never surfaced by a read path)
    let mut never_read = Vec::new();
    let mut stmt = conn.prepare(
        "SELECT id, summary FROM decisions \
         WHERE last_accessed_at IS NULL AND status = 'active' AND archived = 0 \
         ORDER BY id ASC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(serde_json::json!({
            "type": "decision",
            "id": row.get::<_, i64>(0)?,
            "title": row.get::<_, String>(1)?,
        }))
    })?;
    for r in rows {
        never_read.push(r?);
    }
    let mut stmt = conn.prepare(
        "SELECT id, name FROM system_patterns \
         WHERE last_accessed_at IS NULL AND archived = 0 \
         ORDER BY id ASC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(serde_json::json!({
            "type": "pattern",
            "id": row.get::<_, i64>(0)?,
            "title": row.get::<_, String>(1)?,
        }))
    })?;
    for r in rows {
        never_read.push(r?);
    }

    // 4b. Unconfirmed consolidation products (v0.11.0 tier-2): patterns with
    // derived_from evidence that were never confirmed or whose last
    // confirmation is more than 180 days old (confidence has decayed).
    let mut unconfirmed_patterns: Vec<Value> = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT p.id, p.name, p.last_confirmed_at, \
             CAST(julianday('now') - julianday(p.last_confirmed_at) AS INTEGER) \
             FROM system_patterns p \
             WHERE p.archived = 0 \
               AND EXISTS (SELECT 1 FROM context_links l \
                           WHERE l.relationship_type = 'derived_from' \
                             AND l.source_item_type = 'system_pattern' \
                             AND l.source_item_id = p.id) \
               AND (p.last_confirmed_at IS NULL \
                    OR julianday('now') - julianday(p.last_confirmed_at) > 180) \
             ORDER BY p.id ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, i64>(0)?,
                "name": row.get::<_, String>(1)?,
                "last_confirmed_at": row.get::<_, Option<String>>(2)?,
                "days_since_confirmation": row.get::<_, Option<i64>>(3)?,
            }))
        })?;
        for r in rows {
            unconfirmed_patterns.push(r?);
        }
    }

    // 4c. Audit archived records (pruned by prune-decay)
    let archived_decisions: i64 = conn.query_row(
        "SELECT count(*) FROM decisions WHERE archived = 1",
        [],
        |row| row.get(0),
    )?;
    let archived_patterns: i64 = conn.query_row(
        "SELECT count(*) FROM system_patterns WHERE archived = 1",
        [],
        |row| row.get(0),
    )?;

    // 5. Graph advisory: orphan nodes (weighted degree <= 1, capped 50)
    let graph = crate::ops::graph::model::load(conn)?;
    let orphan_nodes: Vec<String> = graph
        .orphans()
        .iter()
        .take(50)
        .map(crate::ops::graph::model::fmt_node)
        .collect();

    // 6. Graph advisory: rebuild recommended when never rebuilt or writes
    //    postdate the last rebuild.
    let last_rebuild: Option<String> = conn
        .query_row(
            "SELECT last_rebuild_at FROM graph_meta WHERE id = 1",
            [],
            |row| row.get::<_, Option<String>>(0),
        )
        .ok()
        .flatten();
    let max_write: Option<String> = conn.query_row(
        "SELECT MAX(ts) FROM (\
            SELECT MAX(timestamp) AS ts FROM decisions \
            UNION ALL SELECT MAX(timestamp) FROM system_patterns \
            UNION ALL SELECT MAX(timestamp) FROM item_anchors\
        )",
        [],
        |row| row.get(0),
    )?;
    let graph_rebuild_recommended = match (last_rebuild, max_write) {
        (None, _) => true,
        (Some(_), None) => false,
        (Some(rebuilt), Some(written)) => written > rebuilt,
    };

    // 7. Graph advisory: cycles in canonical transitive relations
    //    (supersedes, depends_on, part_of, refines, causes).
    let mut cycles: Vec<Vec<String>> = graph
        .cycles(&["supersedes", "depends_on", "part_of", "refines", "causes"])
        .iter()
        .map(|cyc| cyc.iter().map(crate::ops::graph::model::fmt_node).collect())
        .collect();
    cycles.sort();

    // 8. Rel vocabulary audit: relationship_type usage counts; non-canonical
    //    (free-form) rels are flagged but valid (passthrough).
    let mut stmt = conn.prepare(
        "SELECT relationship_type, COUNT(*) FROM context_links GROUP BY relationship_type ORDER BY relationship_type",
    )?;
    let rel_rows: Vec<(String, i64)> = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut rel_counts = serde_json::Map::new();
    let mut non_canonical = Vec::new();
    for (rel, n) in rel_rows {
        if crate::ops::graph::rel::lookup(&rel).is_some() {
            rel_counts.insert(rel, serde_json::json!(n));
        } else {
            non_canonical.push(serde_json::json!({"rel": rel, "count": n}));
        }
    }

    let ok = missing_anchor_paths.is_empty()
        && dangling_links.is_empty()
        && stale_decisions.is_empty()
        && unlinked_decisions.is_empty()
        && never_read.is_empty()
        && unconfirmed_patterns.is_empty();

    Ok(serde_json::json!({
        "missing_anchor_paths": missing_anchor_paths,
        "dangling_links": dangling_links,
        "stale_decisions": stale_decisions,
        "unlinked_decisions": unlinked_decisions,
        "never_read": never_read,
        "unconfirmed_patterns": unconfirmed_patterns,
        "archived": {
            "decisions": archived_decisions,
            "patterns": archived_patterns,
        },
        "orphan_nodes": orphan_nodes,
        "graph_rebuild_recommended": graph_rebuild_recommended,
        "cycles": cycles,
        "rel_vocabulary": {
            "canonical_counts": rel_counts,
            "non_canonical": non_canonical,
        },
        "git": git_status,
        "rules": crate::ops::rules::staleness(conn, db_path),
        "ok": ok,
    }))
}

fn get_table_name(item_type: &str) -> Option<&'static str> {
    match item_type {
        "decision" => Some("decisions"),
        "progress_entry" => Some("progress_entries"),
        "system_pattern" => Some("system_patterns"),
        "custom_data" => Some("custom_data"),
        _ => None,
    }
}
