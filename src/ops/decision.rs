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
        } => {
            // Unless --force, check FTS for similar existing decisions
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

            conn.execute(
                "INSERT INTO decisions (uuid, timestamp, summary, rationale, implementation_details, tags) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![uuid, timestamp, summary, rationale, details, tags_json],
            )?;

            let id = conn.last_insert_rowid();
            let mut decision = get_decision(conn, id)?;
            if let Value::Object(map) = &mut decision {
                map.insert("inserted".into(), Value::Bool(true));
            }
            Ok(decision)
        }
        DecisionCmd::List { tags, limit } => {
            if tags.is_empty() {
                let mut stmt = conn.prepare("SELECT id, uuid, summary, rationale, implementation_details, tags, timestamp FROM decisions ORDER BY id DESC LIMIT ?")?;
                let rows = stmt.query_map(params![limit], parse_decision_row)?;
                let mut results = Vec::new();
                for r in rows {
                    results.push(r?);
                }
                Ok(serde_json::to_value(results)?)
            } else {
                // Filter by tags using EXISTS (SELECT 1 FROM json_each(decisions.tags) WHERE json_each.value IN (...))
                let placeholders = tags.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                let query = format!("SELECT id, uuid, summary, rationale, implementation_details, tags, timestamp FROM decisions WHERE EXISTS (SELECT 1 FROM json_each(decisions.tags) WHERE json_each.value IN ({})) ORDER BY id DESC LIMIT ?", placeholders);
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
                Ok(serde_json::to_value(results)?)
            }
        }
        DecisionCmd::Get { id } => get_decision(conn, id),
        DecisionCmd::Search { query, limit } => {
            if query.trim().is_empty() {
                anyhow::bail!("search query cannot be empty");
            }
            // Tokenize by whitespace and wrap in double quotes
            let match_expr = query
                .split_whitespace()
                .map(|token| format!("\"{}\"", token.replace('"', "\"\"")))
                .collect::<Vec<_>>()
                .join(" ");

            let mut stmt = conn.prepare("SELECT d.id, d.uuid, d.summary, d.rationale, d.implementation_details, d.tags, d.timestamp FROM decisions d JOIN decisions_fts f ON d.id = f.rowid WHERE decisions_fts MATCH ?1 ORDER BY rank LIMIT ?2")?;
            let rows = stmt.query_map(params![match_expr, limit], parse_decision_row)?;
            let mut results = Vec::new();
            for r in rows {
                results.push(r?);
            }
            Ok(serde_json::to_value(results)?)
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

            if sets.is_empty() {
                // Shouldn't happen due to clap ArgGroup but just in case
                return get_decision(conn, id);
            }

            let query = format!("UPDATE decisions SET {} WHERE id = ?", sets.join(", "));
            let mut p_refs: Vec<&dyn rusqlite::ToSql> = p.iter().map(|b| b.as_ref()).collect();
            p_refs.push(&id);

            conn.execute(&query, rusqlite::params_from_iter(p_refs))?;
            get_decision(conn, id)
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
                    "SELECT id, uuid, summary, rationale, implementation_details, tags, timestamp FROM decisions WHERE id = ?",
                    params![source_id],
                    parse_decision_row,
                )
                .optional()?
                .context(format!("source decision {} not found", source_id))?;

            let target: Decision = tx
                .query_row(
                    "SELECT id, uuid, summary, rationale, implementation_details, tags, timestamp FROM decisions WHERE id = ?",
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
    })
}

fn get_decision(conn: &Connection, id: i64) -> Result<Value> {
    let mut stmt = conn.prepare("SELECT id, uuid, summary, rationale, implementation_details, tags, timestamp FROM decisions WHERE id = ?")?;
    let decision = stmt
        .query_row(params![id], parse_decision_row)
        .optional()?
        .context(format!("decision {} not found", id))?;
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
        "SELECT d.id, d.uuid, d.summary, d.rationale, d.implementation_details, d.tags, d.timestamp \
         FROM decisions d JOIN decisions_fts f ON d.id = f.rowid \
         WHERE decisions_fts MATCH ?1 \
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
