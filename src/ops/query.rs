use crate::cli::QueryType;
use anyhow::Result;
use rusqlite::Connection;
use serde_json::{json, Value};

struct QueryResult {
    r#type: String,
    id: i64,
    title: String,
    snippet: String,
    timestamp: String,
    score: f64,
    full: Option<Value>,
}

#[allow(clippy::too_many_arguments)] // 8 params mirrors the flat CLI surface
pub fn handle(
    conn: &Connection,
    query: String,
    types: Vec<QueryType>,
    tags: Vec<String>,
    since: Option<String>,
    limit: i64,
    all: bool,
    full: bool,
) -> Result<Value> {
    if query.trim().is_empty() {
        anyhow::bail!("search query cannot be empty");
    }
    let match_expr = crate::ops::fts_match_expr(&query);
    let mut results = Vec::new();

    // 1. Query Decisions
    let query_decisions = types.is_empty() || types.contains(&QueryType::Decision);
    if query_decisions {
        let dscore = crate::ops::scoring::query_score_expr("d.timestamp", "d.importance");
        let status_filter = if all {
            ""
        } else {
            " AND d.status = 'active' AND d.archived = 0"
        };
        let mut sql = format!(
            "SELECT d.id, d.summary, snippet(decisions_fts, -1, '>>', '<<', '…', 12), d.timestamp, rank, ({}) AS score, d.implementation_details \
             FROM decisions d JOIN decisions_fts f ON d.id = f.rowid \
             WHERE decisions_fts MATCH ?1 {}",
            dscore,
            status_filter
        );
        let mut p: Vec<&dyn rusqlite::ToSql> = vec![&match_expr];

        if !tags.is_empty() {
            let placeholders = crate::ops::sql_placeholders(tags.len());
            sql.push_str(&format!(
                " AND EXISTS (SELECT 1 FROM json_each(d.tags) WHERE json_each.value IN ({}))",
                placeholders
            ));
            for t in &tags {
                p.push(t);
            }
        }

        if let Some(since_ts) = &since {
            sql.push_str(" AND d.timestamp >= ?");
            p.push(since_ts);
        }

        sql.push_str(" ORDER BY score DESC LIMIT ?");
        p.push(&limit);

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(p), |row| {
            let summary: String = row.get(1)?;
            let details: Option<String> = row.get(6)?;
            Ok(QueryResult {
                r#type: "decision".to_string(),
                id: row.get(0)?,
                title: summary.clone(),
                snippet: row.get(2)?,
                timestamp: row.get(3)?,
                score: row.get(5)?,
                full: full.then(|| json!({"summary": summary, "implementation_details": details})),
            })
        })?;
        for r in rows {
            results.push(r?);
        }
    }

    // 2. Query System Patterns
    let query_patterns = types.is_empty() || types.contains(&QueryType::Pattern);
    if query_patterns {
        let pscore = crate::ops::scoring::query_score_expr("p.timestamp", "p.importance");
        let pat_archived_filter = if all { "" } else { " AND p.archived = 0" };
        let mut sql = format!("SELECT p.id, p.name, snippet(system_patterns_fts, -1, '>>', '<<', '…', 12), p.timestamp, rank, ({}) AS score, p.description, p.check_kind, p.check_expr \
                       FROM system_patterns p JOIN system_patterns_fts f ON p.id = f.rowid \
                       WHERE system_patterns_fts MATCH ?1{}", pscore, pat_archived_filter);
        let mut p: Vec<&dyn rusqlite::ToSql> = vec![&match_expr];

        if !tags.is_empty() {
            let placeholders = crate::ops::sql_placeholders(tags.len());
            sql.push_str(&format!(
                " AND EXISTS (SELECT 1 FROM json_each(p.tags) WHERE json_each.value IN ({}))",
                placeholders
            ));
            for t in &tags {
                p.push(t);
            }
        }

        if let Some(since_ts) = &since {
            sql.push_str(" AND p.timestamp >= ?");
            p.push(since_ts);
        }

        sql.push_str(" ORDER BY score DESC LIMIT ?");
        p.push(&limit);

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(p), |row| {
            let desc: Option<String> = row.get(6)?;
            let ckind: Option<String> = row.get(7)?;
            let cexpr: Option<String> = row.get(8)?;
            Ok(QueryResult {
                r#type: "system_pattern".to_string(),
                id: row.get(0)?,
                title: row.get(1)?,
                snippet: row.get(2)?,
                timestamp: row.get(3)?,
                score: row.get(5)?,
                full: full.then(
                    || json!({"description": desc, "check_kind": ckind, "check_expr": cexpr}),
                ),
            })
        })?;
        for r in rows {
            results.push(r?);
        }
    }

    // 3. Query Custom Data
    let query_custom = (types.is_empty() || types.contains(&QueryType::Custom)) && tags.is_empty();
    if query_custom {
        let cscore = crate::ops::scoring::query_score_expr("c.timestamp", "5");
        let mut sql = format!("SELECT c.id, c.category, c.key, snippet(custom_data_fts, -1, '>>', '<<', '…', 12), c.timestamp, rank, ({}) AS score, c.value \
                       FROM custom_data c JOIN custom_data_fts f ON c.id = f.rowid \
                       WHERE custom_data_fts MATCH ?1", cscore);
        let mut p: Vec<&dyn rusqlite::ToSql> = vec![&match_expr];

        if let Some(since_ts) = &since {
            sql.push_str(" AND c.timestamp >= ?");
            p.push(since_ts);
        }

        sql.push_str(" ORDER BY score DESC LIMIT ?");
        p.push(&limit);

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(p), |row| {
            let id = row.get::<_, i64>(0)?;
            let category = row.get::<_, String>(1)?;
            let key = row.get::<_, String>(2)?;
            let snippet = row.get::<_, String>(3)?;
            let timestamp = row.get::<_, String>(4)?;
            let score = row.get::<_, f64>(6)?;
            let value: Option<String> = row.get(7)?;
            Ok(QueryResult {
                r#type: "custom_data".to_string(),
                id,
                title: format!("{}/{}", category, key),
                snippet,
                timestamp,
                score,
                full: full.then(|| json!({"value": value})),
            })
        })?;
        for r in rows {
            results.push(r?);
        }
    }

    // Sort by blended score descending (highest score is most relevant)
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results.truncate(limit as usize);
    // Tier-4 telemetry: one usage row per retrieval call (zero-hit rows feed
    // `engrams usage --misses`).
    crate::ops::usage::record(conn, "query", &query, results.len(), results.is_empty());
    if results.is_empty() {
        return Ok(json!({
            "results": [],
            "miss_guidance": miss_guidance(conn, &query)?,
        }));
    }
    // Reinforce-on-read (v0.10.0).
    let dec_ids: Vec<i64> = results
        .iter()
        .filter(|r| r.r#type == "decision")
        .map(|r| r.id)
        .collect();
    crate::ops::scoring::reinforce(conn, "decisions", &dec_ids)?;
    let pat_ids: Vec<i64> = results
        .iter()
        .filter(|r| r.r#type == "system_pattern")
        .map(|r| r.id)
        .collect();
    crate::ops::scoring::reinforce(conn, "system_patterns", &pat_ids)?;

    let output: Vec<Value> = results
        .into_iter()
        .map(|r| {
            let mut hit = serde_json::json!({
                "type": r.r#type,
                "id": r.id,
                "title": r.title,
                "snippet": r.snippet,
                "timestamp": r.timestamp,
                "score": (r.score * 1000.0).round() / 1000.0,
            });
            if let Some(f) = r.full {
                if let (Some(hm), Some(fm)) = (hit.as_object_mut(), f.as_object()) {
                    for (k, v) in fm {
                        hm.insert(k.clone(), v.clone());
                    }
                }
            }
            hit
        })
        .collect();

    Ok(Value::Array(output))
}

/// Nearest existing clusters for a query that returned nothing, so an agent can
/// re-target without burning file reads: tag clusters, per-token hit counts,
/// recent decisions, and graph hubs.
fn miss_guidance(conn: &Connection, query: &str) -> Result<Value> {
    // Tag clusters across decisions + patterns.
    let mut stmt = conn.prepare(
        "SELECT value, COUNT(*) AS n FROM decisions, json_each(decisions.tags) GROUP BY value \
         UNION ALL \
         SELECT value, COUNT(*) AS n FROM system_patterns, json_each(system_patterns.tags) GROUP BY value",
    )?;
    let mut tag_counts: Vec<(String, i64)> = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    tag_counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    tag_counts.truncate(8);

    // Per-token hit counts: which tokens exist at all in the KB.
    let mut token_counts = Vec::new();
    for token in query.split_whitespace().take(8) {
        let expr = crate::ops::fts_match_expr(token);
        let d: i64 = conn.query_row(
            "SELECT COUNT(*) FROM decisions_fts WHERE decisions_fts MATCH ?1",
            [&expr],
            |row| row.get(0),
        )?;
        let p: i64 = conn.query_row(
            "SELECT COUNT(*) FROM system_patterns_fts WHERE system_patterns_fts MATCH ?1",
            [&expr],
            |row| row.get(0),
        )?;
        token_counts.push(json!({"token": token, "decisions": d, "patterns": p}));
    }

    // Most recent active decisions.
    let mut stmt = conn.prepare(
        "SELECT id, summary FROM decisions WHERE status = 'active' AND archived = 0 \
         ORDER BY timestamp DESC LIMIT 5",
    )?;
    let recent: Vec<Value> = stmt
        .query_map([], |row| {
            Ok(json!({
                "id": row.get::<_, i64>(0)?,
                "summary": row.get::<_, String>(1)?,
            }))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    // Cached at rebuild (graph_meta.summary_json); falls back to a live
    // PageRank pass only when the cache is missing (pre-V11 db, no rebuild).
    let hubs = match cached_hubs(conn) {
        Some(v) => v,
        None => crate::ops::graph::model::summary(conn)?
            .get("top_central")
            .cloned()
            .unwrap_or_else(|| json!([])),
    };

    Ok(json!({
        "hint": "query matched nothing; nearest clusters below — retry with one of these terms, tags, or nodes",
        "top_tags": tag_counts.iter().map(|(t, n)| json!({"tag": t, "count": n})).collect::<Vec<_>>(),
        "token_hits": token_counts,
        "recent_decisions": recent,
        "graph_hubs": hubs,
    }))
}

/// `top_central` from the rebuild-written graph summary, when present and
/// well-formed. One indexed row read; never recomputes PageRank.
fn cached_hubs(conn: &Connection) -> Option<Value> {
    let json: Option<String> = conn
        .query_row(
            "SELECT summary_json FROM graph_meta WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .ok()
        .flatten();
    json.and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .and_then(|v| v.get("top_central").cloned())
        .filter(Value::is_array)
}
