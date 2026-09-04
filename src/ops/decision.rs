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

/// Run `f` in a transaction, committing on success. If the connection is
/// already inside a transaction (e.g. batch mode reusing an outer
/// transaction), run `f` directly rather than opening a nested one.
fn with_transaction<F, R>(conn: &Connection, f: F) -> Result<R>
where
    F: FnOnce(&Connection) -> Result<R>,
{
    if conn.is_autocommit() {
        let tx = conn.unchecked_transaction()?;
        let res = f(&tx)?;
        tx.commit()?;
        Ok(res)
    } else {
        f(conn)
    }
}

pub fn handle(conn: &Connection, cmd: DecisionCmd) -> Result<Value> {
    match cmd {
        DecisionCmd::Log {
            summary,
            status,
            rationale,
            details,
            tags,
            force,
            prs,
            anchors,
            importance,
            supersedes,
            conflicts_with,
            contract,
        } => {
            let status = status.unwrap_or_else(|| "active".to_string());
            let status_overridden = crate::ops::status::check(
                &status,
                crate::ops::status::DECISION_STATUSES,
                force,
                "decision",
            )?;

            let mut resolved_prs = Vec::new();
            for pr in prs {
                resolved_prs.push(crate::ops::pr::resolve_pr_url(&pr)?);
            }

            // Resolution flags imply intent: validate targets, then skip the gate.
            if let Some(old) = supersedes {
                ensure_decision_exists(conn, old)?;
            }
            for cid in &conflicts_with {
                ensure_decision_exists(conn, *cid)?;
            }

            if !force && supersedes.is_none() && conflicts_with.is_empty() {
                let similar = find_similar(conn, &summary, 5)?;
                if !similar.is_empty() {
                    let similar = classify_hits(conn, similar, &anchors)?;
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

            let id = with_transaction(conn, |tx| {
                tx.execute(
                    "INSERT INTO decisions (uuid, timestamp, summary, rationale, implementation_details, tags, status, commit_sha, importance, contract) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![uuid, timestamp, summary, rationale, details, tags_json, status, commit_sha, importance.unwrap_or(5), contract],
                )?;

                let id = tx.last_insert_rowid();

                if !resolved_prs.is_empty() {
                    crate::ops::pr::attach(tx, "decision", id, &resolved_prs)?;
                }
                if !anchors.is_empty() {
                    crate::ops::anchor::attach(tx, "decision", id, &anchors)?;
                }
                crate::ops::graph::rebuild::touch_item(tx, "decision", id)?;

                if let Some(old) = supersedes {
                    tx.execute(
                        "UPDATE decisions SET status = 'superseded' WHERE id = ?",
                        params![old],
                    )?;
                    insert_link_if_absent(tx, "decision", id, "decision", old, "supersedes")?;
                }
                for cid in &conflicts_with {
                    insert_link_if_absent(tx, "decision", id, "decision", *cid, "conflicts_with")?;
                }

                Ok(id)
            })?;

            let mut decision = get_decision(conn, id)?;
            if let Value::Object(map) = &mut decision {
                map.insert("inserted".into(), Value::Bool(true));
                if status_overridden {
                    map.insert("overrides".into(), serde_json::json!(["status_vocabulary"]));
                }
                if let Some(old) = supersedes {
                    map.insert("superseded_id".into(), serde_json::json!(old));
                    map.insert("superseded_decision".into(), get_decision(conn, old)?);
                }
                if !conflicts_with.is_empty() {
                    map.insert("conflicts_with".into(), serde_json::json!(conflicts_with));
                }
                if contract.is_none() {
                    map.insert(
                        "hint".into(),
                        serde_json::json!("no --contract given: if this decision introduces or changes an interface (signatures, struct shapes, error tuples), declare it so strategy queries don't need file reads"),
                    );
                }
            }
            Ok(decision)
        }
        DecisionCmd::List {
            tags,
            limit,
            all,
            filter,
        } => {
            let has_filter = filter.as_deref().is_some_and(|f| !f.trim().is_empty());
            // FTS join condition binds its ? before any WHERE placeholders,
            // so the match expression is the first query parameter.
            let fts_filter = has_filter
                .then(|| crate::ops::fts_match_expr(filter.as_deref().unwrap_or_default()));
            let fts_join = if fts_filter.is_some() {
                " JOIN decisions_fts ON decisions_fts.rowid = d.id AND decisions_fts MATCH ?"
            } else {
                ""
            };
            let col_prefix = if has_filter { "d." } else { "" };
            let status_term: String = if all {
                String::new()
            } else {
                format!("{}status = 'active'", col_prefix)
            };
            let tag_clause = if tags.is_empty() {
                String::new()
            } else {
                let placeholders = crate::ops::sql_placeholders(tags.len());
                format!(
                    "EXISTS (SELECT 1 FROM json_each({}tags) WHERE json_each.value IN ({}))",
                    col_prefix, placeholders
                )
            };
            let where_sql = match (tag_clause.is_empty(), status_term.is_empty()) {
                (true, true) => String::new(),
                (true, false) => format!(" WHERE {}", status_term),
                (false, true) => format!(" WHERE {}", tag_clause),
                (false, false) => format!(" WHERE {} AND {}", tag_clause, status_term),
            };
            let mut p = Vec::<&dyn rusqlite::ToSql>::new();
            if let Some(e) = &fts_filter {
                p.push(e);
            }
            for tag in &tags {
                p.push(tag);
            }
            p.push(&limit);
            // FTS tables share column names (summary, rationale, …): qualify
            // the select list whenever the join is present.
            let cols = if has_filter {
                "d.id, d.uuid, d.summary, d.rationale, d.implementation_details, d.tags, d.timestamp, d.status, d.commit_sha, d.importance, d.access_count, d.last_accessed_at, d.archived, d.contract"
            } else {
                "id, uuid, summary, rationale, implementation_details, tags, timestamp, status, commit_sha, importance, access_count, last_accessed_at, archived, contract"
            };
            let sql = format!(
                "SELECT {} FROM decisions d{}{} ORDER BY id DESC LIMIT ?",
                cols, fts_join, where_sql
            );
            let mut stmt = conn.prepare(&sql)?;
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
        DecisionCmd::Get { id } => {
            // access_count semantics (tier-4): `get` is a retrieval path.
            crate::ops::scoring::reinforce(conn, "decisions", &[id])?;
            get_decision(conn, id)
        }
        DecisionCmd::Stats {
            most_accessed,
            never_accessed,
            limit,
        } => {
            let mut out = serde_json::Map::new();
            if most_accessed {
                let mut stmt = conn.prepare(
                    "SELECT id, summary, access_count, last_accessed_at \
                     FROM decisions WHERE archived = 0 AND status = 'active' AND access_count > 0 \
                     ORDER BY access_count DESC, id DESC LIMIT ?1",
                )?;
                let rows = stmt
                    .query_map(params![limit], |row| {
                        Ok(serde_json::json!({
                            "id": row.get::<_, i64>(0)?,
                            "summary": row.get::<_, String>(1)?,
                            "access_count": row.get::<_, i64>(2)?,
                            "last_accessed_at": row.get::<_, Option<String>>(3)?,
                        }))
                    })?
                    .filter_map(|r| r.ok())
                    .collect::<Vec<_>>();
                out.insert("most_accessed".into(), serde_json::Value::Array(rows));
            }
            if never_accessed {
                let mut stmt = conn.prepare(
                    "SELECT id, summary, timestamp FROM decisions \
                     WHERE archived = 0 AND status = 'active' AND access_count = 0 \
                     ORDER BY id DESC LIMIT ?1",
                )?;
                let rows = stmt
                    .query_map(params![limit], |row| {
                        Ok(serde_json::json!({
                            "id": row.get::<_, i64>(0)?,
                            "summary": row.get::<_, String>(1)?,
                            "timestamp": row.get::<_, String>(2)?,
                        }))
                    })?
                    .filter_map(|r| r.ok())
                    .collect::<Vec<_>>();
                out.insert("never_accessed".into(), serde_json::Value::Array(rows));
            }
            Ok(serde_json::Value::Object(out))
        }
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
                let dcols = crate::models::decision_cols_qualified();
                let sql = if all {
                    format!(
                        "SELECT {dcols} \
                     FROM decisions d JOIN decisions_fts f ON d.id = f.rowid \
                     WHERE decisions_fts MATCH ?1 \
                     ORDER BY rank LIMIT ?2"
                    )
                } else {
                    format!(
                        "SELECT {dcols} \
                     FROM decisions d JOIN decisions_fts f ON d.id = f.rowid \
                     WHERE decisions_fts MATCH ?1 AND d.status = 'active' \
                     ORDER BY rank LIMIT ?2"
                    )
                };
                let mut stmt = conn.prepare(&sql)?;
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
        DecisionCmd::Update(DecisionUpdateArgs { id, force, fields }) => {
            // First check if exists
            let _: i64 = conn
                .query_row("SELECT id FROM decisions WHERE id = ?", params![id], |r| {
                    r.get(0)
                })
                .optional()?
                .context(format!("decision {} not found", id))?;

            let mut sets = Vec::new();
            let mut p: Vec<&dyn rusqlite::ToSql> = Vec::new();

            // Bind moved values to locals so they outlive the params vec
            let summary = fields.summary;
            let rationale = fields.rationale;
            let details = fields.details;
            let contract = fields.contract;
            let status = fields.status;
            let importance = fields.importance;

            let mut status_overridden = false;

            if let Some(ref v) = summary {
                sets.push("summary = ?");
                p.push(v);
            }
            if let Some(ref v) = rationale {
                sets.push("rationale = ?");
                p.push(v);
            }
            if let Some(ref v) = details {
                sets.push("implementation_details = ?");
                p.push(v);
            }
            if let Some(ref v) = contract {
                sets.push("contract = ?");
                p.push(v);
            }

            let tags_json: Option<Option<String>> = if let Some(tags) = &fields.tags {
                sets.push("tags = ?");
                Some(if tags.is_empty() {
                    None
                } else {
                    Some(serde_json::to_string(tags)?)
                })
            } else {
                None
            };
            if let Some(ref v) = tags_json {
                p.push(v);
            }

            if let Some(ref s) = status {
                status_overridden = crate::ops::status::check(
                    s,
                    crate::ops::status::DECISION_STATUSES,
                    force,
                    "decision",
                )?;
                sets.push("status = ?");
                p.push(s);
            }

            if let Some(ref v) = importance {
                sets.push("importance = ?");
                p.push(v);
            }

            if sets.is_empty() {
                return get_decision(conn, id);
            }

            let query = format!("UPDATE decisions SET {} WHERE id = ?", sets.join(", "));
            p.push(&id);

            conn.execute(&query, rusqlite::params_from_iter(p))?;
            let mut result = get_decision(conn, id)?;
            if status_overridden {
                if let Value::Object(map) = &mut result {
                    map.insert("overrides".into(), serde_json::json!(["status_vocabulary"]));
                }
            }
            Ok(result)
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

            with_transaction(conn, |tx| {
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

                Ok(())
            })?;

            let mut result = get_decision(conn, id)?;
            if let Value::Object(map) = &mut result {
                map.insert("superseded_status".into(), serde_json::json!("superseded"));
                if let Some(by_id) = by {
                    map.insert("superseded_by".into(), serde_json::json!(by_id));
                }
            }
            Ok(result)
        }
        DecisionCmd::Delete { id } => {
            let links_removed = with_transaction(conn, |tx| {
                // ensure it exists
                let _: i64 = tx
                    .query_row("SELECT id FROM decisions WHERE id = ?", params![id], |r| {
                        r.get(0)
                    })
                    .optional()?
                    .context(format!("decision {} not found", id))?;

                let links_removed = delete_links_for(tx, "decision", id)?;
                tx.execute(
                    "DELETE FROM item_anchors WHERE item_type='decision' AND item_id=?",
                    params![id],
                )?;
                let deleted = tx.execute("DELETE FROM decisions WHERE id = ?", params![id])?;
                if deleted == 0 {
                    anyhow::bail!("decision {} not found", id);
                }

                Ok(links_removed)
            })?;

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

            let repointed = with_transaction(conn, |tx| {
                let base_sql = format!(
                    "SELECT {} FROM decisions WHERE id = ?",
                    crate::models::DECISION_COLS
                );
                let source: Decision = tx
                    .query_row(&base_sql, params![source_id], parse_decision_row)
                    .optional()?
                    .context(format!("source decision {} not found", source_id))?;

                let target: Decision = tx
                    .query_row(&base_sql, params![into_id], parse_decision_row)
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
                // Merge contracts
                let merged_contract =
                    merge_text_fields(target.contract.as_deref(), source.contract.as_deref());

                // Merge tags (union, deduplicated)
                let merged_tags = merge_tags(&target.tags, &source.tags);
                let merged_tags_json = if merged_tags.is_empty() {
                    None
                } else {
                    Some(serde_json::to_string(&merged_tags)?)
                };

                // Update target with merged fields
                tx.execute(
                    "UPDATE decisions SET rationale = ?1, implementation_details = ?2, tags = ?3, contract = ?4 WHERE id = ?5",
                    params![merged_rationale, merged_details, merged_tags_json, merged_contract, into_id],
                )?;

                // Repoint links from source to target
                let repointed = repoint_links(tx, "decision", source_id, into_id)?;

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

                Ok(repointed)
            })?;

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
    let tags_str: Option<String> = row.get("tags")?;
    let tags = match tags_str {
        Some(s) => serde_json::from_str(&s).unwrap_or(Value::Null),
        None => Value::Null,
    };

    Ok(Decision {
        id: row.get("id")?,
        uuid: row.get("uuid")?,
        summary: row.get("summary")?,
        rationale: row.get("rationale")?,
        implementation_details: row.get("implementation_details")?,
        tags: if tags.is_null() { None } else { Some(tags) },
        timestamp: row.get("timestamp")?,
        status: row.get("status")?,
        commit_sha: row.get("commit_sha")?,
        pr_urls: Vec::new(),
        anchors: Vec::new(),
        importance: row.get("importance")?,
        access_count: row.get("access_count")?,
        last_accessed_at: row.get("last_accessed_at")?,
        archived: row.get("archived")?,
        contract: row.get("contract")?,
        score: None,
    })
}

fn get_decision(conn: &Connection, id: i64) -> Result<Value> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {} FROM decisions WHERE id = ?",
        crate::models::DECISION_COLS
    ))?;
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
pub(crate) fn find_similar(conn: &Connection, summary: &str, limit: i64) -> Result<Vec<Decision>> {
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

    let dcols = crate::models::decision_cols_qualified();
    let mut stmt = conn.prepare(&format!(
        "SELECT {dcols} \
         FROM decisions d JOIN decisions_fts f ON d.id = f.rowid \
         WHERE decisions_fts MATCH ?1 AND d.status = 'active' AND d.archived = 0 \
         ORDER BY rank LIMIT ?2"
    ))?;
    let rows = stmt.query_map(params![match_expr, limit], parse_decision_row)?;
    let mut results = Vec::new();
    for r in rows {
        results.push(r?);
    }
    Ok(results)
}

/// Lowercased content tokens shared by two summaries, using the same
/// tokenizer as the similarity gate. Exposed for consolidate's
/// merge-suggestion output.
pub(crate) fn shared_terms(a: &str, b: &str) -> Vec<String> {
    const STOPWORDS: &[&str] = &[
        "for", "the", "and", "but", "not", "its", "are", "was", "has", "this", "that", "with",
        "from", "will", "been", "have", "were", "they", "then", "than", "when", "what", "which",
        "their", "into", "also", "each", "does", "these", "those", "such", "only", "some", "very",
        "just", "over", "both", "more",
    ];
    let b_tokens: std::collections::HashSet<String> = b
        .split_whitespace()
        .filter(|t| t.len() >= 3 && !STOPWORDS.contains(&t.to_lowercase().as_str()))
        .map(|t| t.to_lowercase())
        .collect();
    let mut out: Vec<String> = a
        .split_whitespace()
        .filter(|t| t.len() >= 3 && !STOPWORDS.contains(&t.to_lowercase().as_str()))
        .map(|t| t.to_lowercase())
        .filter(|t| b_tokens.contains(t))
        .collect();
    out.sort();
    out.dedup();
    out.truncate(6);
    out
}

/// Ensure a decision exists, for validating resolution-flag targets.
fn ensure_decision_exists(conn: &Connection, id: i64) -> Result<()> {
    let _: i64 = conn
        .query_row("SELECT id FROM decisions WHERE id = ?", params![id], |r| {
            r.get(0)
        })
        .optional()?
        .with_context(|| format!("decision {} not found", id))?;
    Ok(())
}

/// Insert a manual context link unless an identical edge already exists.
pub(crate) fn insert_link_if_absent(
    conn: &Connection,
    src_type: &str,
    src_id: i64,
    tgt_type: &str,
    tgt_id: i64,
    rel: &str,
) -> Result<()> {
    let exists: bool = conn.query_row(
        "SELECT count(*) FROM context_links \
         WHERE source_item_type = ?1 AND source_item_id = ?2 \
         AND target_item_type = ?3 AND target_item_id = ?4 \
         AND relationship_type = ?5",
        params![
            src_type,
            src_id.to_string(),
            tgt_type,
            tgt_id.to_string(),
            rel
        ],
        |row| row.get(0),
    )?;
    if !exists {
        let timestamp = now();
        conn.execute(
            "INSERT INTO context_links (source_item_type, source_item_id, target_item_type, target_item_id, relationship_type, description, timestamp) \
             VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6)",
            params![src_type, src_id.to_string(), tgt_type, tgt_id.to_string(), rel, timestamp],
        )?;
    }
    Ok(())
}

/// Classify similarity-gate hits (S6): a hit sharing at least one file
/// anchor with the incoming decision is a supersession candidate
/// (`suggested_relation: supersedes`); otherwise a conflict candidate.
fn classify_hits(
    conn: &Connection,
    hits: Vec<Decision>,
    new_anchors: &[String],
) -> Result<Vec<Value>> {
    let cleaned: std::collections::HashSet<String> = new_anchors
        .iter()
        .map(|p| crate::ops::anchor::clean_path(p))
        .collect();
    let mut out = Vec::with_capacity(hits.len());
    for hit in hits {
        let hit_anchors = crate::ops::anchor::anchors_for(conn, "decision", hit.id)?;
        let shared = hit_anchors.iter().filter(|a| cleaned.contains(*a)).count();
        let mut v = serde_json::to_value(&hit)?;
        if let Value::Object(map) = &mut v {
            map.insert(
                "suggested_relation".into(),
                serde_json::json!(if shared > 0 {
                    "supersedes"
                } else {
                    "conflicts_with"
                }),
            );
            map.insert("shared_anchors".into(), serde_json::json!(shared));
        }
        out.push(v);
    }
    Ok(out)
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
