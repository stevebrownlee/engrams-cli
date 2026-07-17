use anyhow::{Context, Result};
use chrono::{SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;
use uuid::Uuid;

use crate::cli::{DecisionCmd, DecisionUpdateArgs};
use crate::models::Decision;
use crate::ops::link::delete_links_for;

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

pub fn handle(conn: &Connection, cmd: DecisionCmd) -> Result<Value> {
    match cmd {
        DecisionCmd::Log {
            summary,
            rationale,
            details,
            tags,
            force,
            prs,
            anchors,
        } => {
            let mut resolved_prs = Vec::new();
            for pr in prs {
                resolved_prs.push(crate::ops::pr::resolve_pr_url(&pr)?);
            }

            if !force {
                let similar = find_similar(conn, &summary, 5)?;
                if !similar.is_empty() {
                    return Ok(serde_json::json!({
                        "inserted": false,
                        "similar": similar,
                    }));
                }
            }

            let uuid = Uuid::new_v4().to_string();
            let timestamp = now();
            let tags_json = if tags.is_empty() {
                None
            } else {
                Some(serde_json::to_string(&tags)?)
            };

            let commit_sha = crate::ops::git::head_sha();

            conn.execute(
                "INSERT INTO decisions (uuid, timestamp, summary, rationale, implementation_details, tags, commit_sha) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![uuid, timestamp, summary, rationale, details, tags_json, commit_sha],
            )?;

            let id = conn.last_insert_rowid();

            if !resolved_prs.is_empty() {
                crate::ops::pr::attach(conn, "decision", id, &resolved_prs)?;
            }
            if !anchors.is_empty() {
                crate::ops::anchor::attach(conn, "decision", id, &anchors)?;
            }
            crate::ops::graph::rebuild::touch_item(conn, "decision", id)?;

            let mut decision = get_decision(conn, id)?;
            if let Value::Object(map) = &mut decision {
                map.insert("inserted".into(), Value::Bool(true));
            }
            Ok(decision)
        }
        DecisionCmd::List { tags, limit, all } => {
            if tags.is_empty() {
                let sql = if all {
                    "SELECT id, uuid, summary, rationale, implementation_details, tags, timestamp, status, commit_sha FROM decisions ORDER BY id DESC LIMIT ?"
                } else {
                    "SELECT id, uuid, summary, rationale, implementation_details, tags, timestamp, status, commit_sha FROM decisions WHERE status = 'active' ORDER BY id DESC LIMIT ?"
                };
                let mut stmt = conn.prepare(sql)?;
                let rows = stmt.query_map(params![limit], parse_decision_row)?;
                let mut results = Vec::new();
                for r in rows {
                    results.push(r?);
                }
                let prs_map = crate::ops::pr::pr_urls_map(conn, "decision")?;
                let anchors_map = crate::ops::anchor::anchors_map(conn, "decision")?;
                for d in &mut results {
                    if let Some(urls) = prs_map.get(&d.id) {
                        d.pr_urls = urls.clone();
                    }
                    if let Some(paths) = anchors_map.get(&d.id) {
                        d.anchors = paths.clone();
                    }
                }
                Ok(serde_json::to_value(results)?)
            } else {
                let placeholders = tags.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                let filter = if all {
                    String::new()
                } else {
                    "AND status = 'active'".to_string()
                };
                let query = format!("SELECT id, uuid, summary, rationale, implementation_details, tags, timestamp, status, commit_sha FROM decisions WHERE EXISTS (SELECT 1 FROM json_each(decisions.tags) WHERE json_each.value IN ({})) {} ORDER BY id DESC LIMIT ?", placeholders, filter);
                let mut stmt = conn.prepare(&query)?;
                let mut p = Vec::<&dyn rusqlite::ToSql>::new();
                for tag in &tags {
                    p.push(tag);
                }
                p.push(&limit);
                let rows = stmt.query_map(rusqlite::params_from_iter(p), parse_decision_row)?;
                let mut results = Vec::new();
                for r in rows {
                    results.push(r?);
                }
                let prs_map = crate::ops::pr::pr_urls_map(conn, "decision")?;
                let anchors_map = crate::ops::anchor::anchors_map(conn, "decision")?;
                for d in &mut results {
                    if let Some(urls) = prs_map.get(&d.id) {
                        d.pr_urls = urls.clone();
                    }
                    if let Some(paths) = anchors_map.get(&d.id) {
                        d.anchors = paths.clone();
                    }
                }
                Ok(serde_json::to_value(results)?)
            }
        }
        DecisionCmd::Get { id } => get_decision(conn, id),
        DecisionCmd::Search {
            query,
            limit,
            all,
            snippets,
        } => {
            if query.trim().is_empty() {
                anyhow::bail!("search query cannot be empty");
            }
            let match_expr = crate::ops::fts_match_expr(&query);

            if snippets {
                let sql = if all {
                    "SELECT d.id, d.summary, snippet(decisions_fts, -1, '>>', '<<', '…', 12) \
                     FROM decisions d JOIN decisions_fts f ON d.id = f.rowid \
                     WHERE decisions_fts MATCH ?1 \
                     ORDER BY rank LIMIT ?2"
                } else {
                    "SELECT d.id, d.summary, snippet(decisions_fts, -1, '>>', '<<', '…', 12) \
                     FROM decisions d JOIN decisions_fts f ON d.id = f.rowid \
                     WHERE decisions_fts MATCH ?1 AND d.status = 'active' \
                     ORDER BY rank LIMIT ?2"
                };
                let mut stmt = conn.prepare(sql)?;
                let rows = stmt.query_map(params![match_expr, limit], |row| {
                    Ok(serde_json::json!({
                        "id": row.get::<_, i64>(0)?,
                        "summary": row.get::<_, String>(1)?,
                        "snippet": row.get::<_, String>(2)?,
                    }))
                })?;
                let mut results = Vec::new();
                for r in rows {
                    results.push(r?);
                }
                Ok(serde_json::to_value(results)?)
            } else {
                let sql = if all {
                    "SELECT d.id, d.uuid, d.summary, d.rationale, d.implementation_details, d.tags, d.timestamp, d.status, d.commit_sha \
                     FROM decisions d JOIN decisions_fts f ON d.id = f.rowid \
                     WHERE decisions_fts MATCH ?1 \
                     ORDER BY rank LIMIT ?2"
                } else {
                    "SELECT d.id, d.uuid, d.summary, d.rationale, d.implementation_details, d.tags, d.timestamp, d.status, d.commit_sha \
                     FROM decisions d JOIN decisions_fts f ON d.id = f.rowid \
                     WHERE decisions_fts MATCH ?1 AND d.status = 'active' \
                     ORDER BY rank LIMIT ?2"
                };
                let mut stmt = conn.prepare(sql)?;
                let rows = stmt.query_map(params![match_expr, limit], parse_decision_row)?;
                let mut results = Vec::new();
                for r in rows {
                    results.push(r?);
                }
                let prs_map = crate::ops::pr::pr_urls_map(conn, "decision")?;
                let anchors_map = crate::ops::anchor::anchors_map(conn, "decision")?;
                for d in &mut results {
                    if let Some(urls) = prs_map.get(&d.id) {
                        d.pr_urls = urls.clone();
                    }
                    if let Some(paths) = anchors_map.get(&d.id) {
                        d.anchors = paths.clone();
                    }
                }
                Ok(serde_json::to_value(results)?)
            }
        }
        DecisionCmd::Update(DecisionUpdateArgs { id, fields }) => {
            // First check if exists
            let _: i64 = conn
                .query_row("SELECT id FROM decisions WHERE id = ?", params![id], |r| {
                    r.get(0)
                })
                .optional()?
                .context(format!("decision {} not found", id))?;

            let mut sets = Vec::new();
            let mut p = Vec::<Box<dyn rusqlite::ToSql>>::new();

            if let Some(summary) = fields.summary {
                sets.push("summary = ?");
                p.push(Box::new(summary));
            }
            if let Some(rationale) = fields.rationale {
                sets.push("rationale = ?");
                p.push(Box::new(rationale));
            }
            if let Some(details) = fields.details {
                sets.push("implementation_details = ?");
                p.push(Box::new(details));
            }
            if let Some(tags) = fields.tags {
                sets.push("tags = ?");
                let tags_json = if tags.is_empty() {
                    None
                } else {
                    Some(serde_json::to_string(&tags)?)
                };
                p.push(Box::new(tags_json));
            }

            if let Some(status) = fields.status {
                sets.push("status = ?");
                p.push(Box::new(status.as_str().to_string()));
            }

            if sets.is_empty() {
                return get_decision(conn, id);
            }

            let query = format!("UPDATE decisions SET {} WHERE id = ?", sets.join(", "));
            let mut p_refs: Vec<&dyn rusqlite::ToSql> = p.iter().map(|b| b.as_ref()).collect();
            p_refs.push(&id);

            conn.execute(&query, rusqlite::params_from_iter(p_refs))?;
            get_decision(conn, id)
        }
        DecisionCmd::Supersede { id, by } => {
            let _: i64 = conn
                .query_row("SELECT id FROM decisions WHERE id = ?", params![id], |r| {
                    r.get(0)
                })
                .optional()?
                .context(format!("decision {} not found", id))?;

            if let Some(by_id) = by {
                if by_id == id {
                    anyhow::bail!("a decision cannot supersede itself");
                }
                let _: i64 = conn
                    .query_row(
                        "SELECT id FROM decisions WHERE id = ?",
                        params![by_id],
                        |r| r.get(0),
                    )
                    .optional()?
                    .context(format!("decision {} not found", by_id))?;
            }

            let tx = conn.unchecked_transaction()?;

            tx.execute(
                "UPDATE decisions SET status = 'superseded' WHERE id = ?",
                params![id],
            )?;

            if let Some(by_id) = by {
                let exists: bool = tx.query_row(
                    "SELECT count(*) FROM context_links \
                     WHERE source_item_type = 'decision' AND source_item_id = ?1 \
                     AND target_item_type = 'decision' AND target_item_id = ?2 \
                     AND relationship_type = 'supersedes'",
                    params![by_id.to_string(), id.to_string()],
                    |row| row.get(0),
                )?;
                if !exists {
                    let timestamp = now();
                    tx.execute(
                        "INSERT INTO context_links (source_item_type, source_item_id, target_item_type, target_item_id, relationship_type, description, timestamp) \
                         VALUES ('decision', ?1, 'decision', ?2, 'supersedes', NULL, ?3)",
                        params![by_id.to_string(), id.to_string(), timestamp],
                    )?;
                }
            }

            tx.commit()?;

            let mut result = get_decision(conn, id)?;
            if let Some(by_id) = by {
                if let Value::Object(map) = &mut result {
                    map.insert("superseded_by".into(), serde_json::json!(by_id));
                }
            }
            Ok(result)
        }
        DecisionCmd::Delete { id } => {
            let tx = conn.unchecked_transaction()?;

            // ensure it exists
            let _: i64 = tx
                .query_row("SELECT id FROM decisions WHERE id = ?", params![id], |r| {
                    r.get(0)
                })
                .optional()?
                .context(format!("decision {} not found", id))?;

            let links_removed = delete_links_for(&tx, "decision", id)?;
            tx.execute(
                "DELETE FROM item_anchors WHERE item_type='decision' AND item_id=?",
                params![id],
            )?;
            let deleted = tx.execute("DELETE FROM decisions WHERE id = ?", params![id])?;
            if deleted == 0 {
                anyhow::bail!("decision {} not found", id);
            }

            tx.commit()?;

            Ok(serde_json::json!({
                "deleted": true,
                "id": id,
                "links_removed": links_removed
            }))
        }
        DecisionCmd::Consolidate { source_id, into_id } => {
            if source_id == into_id {
                anyhow::bail!("source and target must be different decisions");
            }

            let tx = conn.unchecked_transaction()?;

            // Fetch both decisions
            let source: Decision = tx
                .query_row(
                    "SELECT id, uuid, summary, rationale, implementation_details, tags, timestamp, status, commit_sha FROM decisions WHERE id = ?",
                    params![source_id],
                    parse_decision_row,
                )
                .optional()?
                .context(format!("source decision {} not found", source_id))?;

            let target: Decision = tx
                .query_row(
                    "SELECT id, uuid, summary, rationale, implementation_details, tags, timestamp, status, commit_sha FROM decisions WHERE id = ?",
                    params![into_id],
                    parse_decision_row,
                )
                .optional()?
                .context(format!("target decision {} not found", into_id))?;
            // Merge rationale
            let merged_rationale =
                merge_text_fields(target.rationale.as_deref(), source.rationale.as_deref());

            // Merge implementation_details
            let merged_details = merge_text_fields(
                target.implementation_details.as_deref(),
                source.implementation_details.as_deref(),
            );

            // Merge tags (union, deduplicated)
            let merged_tags = merge_tags(&target.tags, &source.tags);
            let merged_tags_json = if merged_tags.is_empty() {
                None
            } else {
                Some(serde_json::to_string(&merged_tags)?)
            };

            // Update target with merged fields
            tx.execute(
                "UPDATE decisions SET rationale = ?1, implementation_details = ?2, tags = ?3 WHERE id = ?4",
                params![merged_rationale, merged_details, merged_tags_json, into_id],
            )?;

            // Repoint links from source to target
            let repointed = repoint_links(&tx, "decision", source_id, into_id)?;

            // Repoint anchors from source to target, then delete source anchors
            tx.execute(
                "UPDATE OR IGNORE item_anchors SET item_id = ?1 WHERE item_type = 'decision' AND item_id = ?2",
                params![into_id, source_id],
            )?;
            tx.execute(
                "DELETE FROM item_anchors WHERE item_type = 'decision' AND item_id = ?1",
                params![source_id],
            )?;

            // Delete source
            tx.execute("DELETE FROM decisions WHERE id = ?", params![source_id])?;

            tx.commit()?;

            let mut result = get_decision(conn, into_id)?;
            if let Value::Object(map) = &mut result {
                map.insert("consolidated_from".into(), serde_json::json!(source_id));
                map.insert("links_repointed".into(), serde_json::json!(repointed));
            }
            Ok(result)
        }
    }
}

fn parse_decision_row(row: &rusqlite::Row) -> rusqlite::Result<Decision> {
    let tags_str: Option<String> = row.get(5)?;
    let tags = match tags_str {
        Some(s) => serde_json::from_str(&s).unwrap_or(Value::Null),
        None => Value::Null,
    };

    Ok(Decision {
        id: row.get(0)?,
        uuid: row.get(1)?,
        summary: row.get(2)?,
        rationale: row.get(3)?,
        implementation_details: row.get(4)?,
        tags: if tags.is_null() { None } else { Some(tags) },
        timestamp: row.get(6)?,
        status: row.get(7)?,
        commit_sha: row.get(8)?,
        pr_urls: Vec::new(),
        anchors: Vec::new(),
    })
}

fn get_decision(conn: &Connection, id: i64) -> Result<Value> {
    let mut stmt = conn.prepare("SELECT id, uuid, summary, rationale, implementation_details, tags, timestamp, status, commit_sha FROM decisions WHERE id = ?")?;
    let mut decision = stmt
        .query_row(params![id], parse_decision_row)
        .optional()?
        .context(format!("decision {} not found", id))?;
    decision.pr_urls = crate::ops::pr::pr_urls_for(conn, "decision", id)?;
    decision.anchors = crate::ops::anchor::anchors_for(conn, "decision", id)?;
    Ok(serde_json::to_value(decision)?)
}

/// Query FTS5 for decisions with similar summaries.
/// Uses OR between tokens so any shared term surfaces a match, ranked by BM25.
fn find_similar(conn: &Connection, summary: &str, limit: i64) -> Result<Vec<Decision>> {
    // Keep tokens ≥ 3 chars but exclude common English stopwords.
    // Short technical terms like CLI, API, SQL are preserved.
    const STOPWORDS: &[&str] = &[
        "for", "the", "and", "but", "not", "its", "are", "was", "has", "this", "that", "with",
        "from", "will", "been", "have", "were", "they", "then", "than", "when", "what", "which",
        "their", "into", "also", "each", "does", "these", "those", "such", "only", "some", "very",
        "just", "over", "both", "more",
    ];
    let tokens: Vec<_> = summary
        .split_whitespace()
        .map(|t| t.replace('"', "\"\""))
        .filter(|t| t.len() >= 3 && !STOPWORDS.contains(&t.to_lowercase().as_str()))
        .collect();
    if tokens.is_empty() {
        return Ok(Vec::new());
    }
    // Scope to the summary column; OR between tokens so partial overlap matches
    let match_expr = format!(
        "summary : ({})",
        tokens
            .iter()
            .map(|t| format!("\"{}\"", t))
            .collect::<Vec<_>>()
            .join(" OR ")
    );

    let mut stmt = conn.prepare(
        "SELECT d.id, d.uuid, d.summary, d.rationale, d.implementation_details, d.tags, d.timestamp, d.status, d.commit_sha \
         FROM decisions d JOIN decisions_fts f ON d.id = f.rowid \
         WHERE decisions_fts MATCH ?1 AND d.status = 'active' \
         ORDER BY rank LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![match_expr, limit], parse_decision_row)?;
    let mut results = Vec::new();
    for r in rows {
        results.push(r?);
    }
    Ok(results)
}

/// Merge two optional text fields. If both present, concatenate with a separator.
fn merge_text_fields(target: Option<&str>, source: Option<&str>) -> Option<String> {
    match (target, source) {
        (Some(t), Some(s)) => Some(format!("{}\n\n---\n\n{}", t, s)),
        (Some(t), None) => Some(t.to_owned()),
        (None, Some(s)) => Some(s.to_owned()),
        (None, None) => None,
    }
}

/// Union two JSON tag arrays, deduplicated, preserving target order first.
fn merge_tags(target: &Option<Value>, source: &Option<Value>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut merged = Vec::new();

    for tags_val in [target, source] {
        if let Some(Value::Array(arr)) = tags_val {
            for v in arr {
                if let Value::String(s) = v {
                    if seen.insert(s.clone()) {
                        merged.push(s.clone());
                    }
                }
            }
        }
    }
    merged
}

/// Repoint all context_links referencing source to target. Returns count repointed.
fn repoint_links(
    conn: &Connection,
    item_type: &str,
    source_id: i64,
    target_id: i64,
) -> Result<usize> {
    let src = source_id.to_string();
    let tgt = target_id.to_string();
    let c1 = conn.execute(
        "UPDATE context_links SET source_item_id = ?1 WHERE source_item_type = ?2 AND source_item_id = ?3",
        params![tgt, item_type, src],
    )?;
    let c2 = conn.execute(
        "UPDATE context_links SET target_item_id = ?1 WHERE target_item_type = ?2 AND target_item_id = ?3",
        params![tgt, item_type, src],
    )?;
    Ok(c1 + c2)
}
