use anyhow::Result;
use rusqlite::Connection;
use serde_json::Value;

pub fn handle(conn: &Connection) -> Result<Value> {
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

    let ok = missing_anchor_paths.is_empty()
        && dangling_links.is_empty()
        && stale_decisions.is_empty()
        && unlinked_decisions.is_empty();

    Ok(serde_json::json!({
        "missing_anchor_paths": missing_anchor_paths,
        "dangling_links": dangling_links,
        "stale_decisions": stale_decisions,
        "unlinked_decisions": unlinked_decisions,
        "orphan_nodes": orphan_nodes,
        "graph_rebuild_recommended": graph_rebuild_recommended,
        "git": git_status,
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
