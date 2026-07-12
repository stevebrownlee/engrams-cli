use crate::cli::QueryType;
use anyhow::Result;
use rusqlite::Connection;
use serde_json::Value;

struct QueryResult {
    r#type: String,
    id: i64,
    title: String,
    snippet: String,
    timestamp: String,
    rank: f64,
}

pub fn handle(
    conn: &Connection,
    query: String,
    types: Vec<QueryType>,
    tags: Vec<String>,
    since: Option<String>,
    limit: i64,
    all: bool,
) -> Result<Value> {
    if query.trim().is_empty() {
        anyhow::bail!("search query cannot be empty");
    }
    let match_expr = crate::ops::fts_match_expr(&query);
    let mut results = Vec::new();

    // 1. Query Decisions
    let query_decisions = types.is_empty() || types.contains(&QueryType::Decision);
    if query_decisions {
        let status_filter = if all { "" } else { " AND d.status = 'active'" };
        let mut sql = format!(
            "SELECT d.id, d.summary, snippet(decisions_fts, -1, '>>', '<<', '…', 12), d.timestamp, rank \
             FROM decisions d JOIN decisions_fts f ON d.id = f.rowid \
             WHERE decisions_fts MATCH ?1 {}",
            status_filter
        );
        let mut p: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(match_expr.clone())];

        if !tags.is_empty() {
            let placeholders = tags.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            sql.push_str(&format!(
                " AND EXISTS (SELECT 1 FROM json_each(d.tags) WHERE json_each.value IN ({}))",
                placeholders
            ));
            for t in &tags {
                p.push(Box::new(t.clone()));
            }
        }

        if let Some(since_ts) = &since {
            sql.push_str(" AND d.timestamp >= ?");
            p.push(Box::new(since_ts.clone()));
        }

        sql.push_str(" ORDER BY rank LIMIT ?");
        p.push(Box::new(limit));

        let mut stmt = conn.prepare(&sql)?;
        let p_refs: Vec<&dyn rusqlite::ToSql> = p.iter().map(|b| b.as_ref()).collect();
        let rows = stmt.query_map(rusqlite::params_from_iter(p_refs), |row| {
            Ok(QueryResult {
                r#type: "decision".to_string(),
                id: row.get(0)?,
                title: row.get(1)?,
                snippet: row.get(2)?,
                timestamp: row.get(3)?,
                rank: row.get(4)?,
            })
        })?;
        for r in rows {
            results.push(r?);
        }
    }

    // 2. Query System Patterns
    let query_patterns = types.is_empty() || types.contains(&QueryType::Pattern);
    if query_patterns {
        let mut sql = "SELECT p.id, p.name, snippet(system_patterns_fts, -1, '>>', '<<', '…', 12), p.timestamp, rank \
                       FROM system_patterns p JOIN system_patterns_fts f ON p.id = f.rowid \
                       WHERE system_patterns_fts MATCH ?1".to_string();
        let mut p: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(match_expr.clone())];

        if !tags.is_empty() {
            let placeholders = tags.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            sql.push_str(&format!(
                " AND EXISTS (SELECT 1 FROM json_each(p.tags) WHERE json_each.value IN ({}))",
                placeholders
            ));
            for t in &tags {
                p.push(Box::new(t.clone()));
            }
        }

        if let Some(since_ts) = &since {
            sql.push_str(" AND p.timestamp >= ?");
            p.push(Box::new(since_ts.clone()));
        }

        sql.push_str(" ORDER BY rank LIMIT ?");
        p.push(Box::new(limit));

        let mut stmt = conn.prepare(&sql)?;
        let p_refs: Vec<&dyn rusqlite::ToSql> = p.iter().map(|b| b.as_ref()).collect();
        let rows = stmt.query_map(rusqlite::params_from_iter(p_refs), |row| {
            Ok(QueryResult {
                r#type: "system_pattern".to_string(),
                id: row.get(0)?,
                title: row.get(1)?,
                snippet: row.get(2)?,
                timestamp: row.get(3)?,
                rank: row.get(4)?,
            })
        })?;
        for r in rows {
            results.push(r?);
        }
    }

    // 3. Query Custom Data
    let query_custom = (types.is_empty() || types.contains(&QueryType::Custom)) && tags.is_empty();
    if query_custom {
        let mut sql = "SELECT c.id, c.category, c.key, snippet(custom_data_fts, -1, '>>', '<<', '…', 12), c.timestamp, rank \
                       FROM custom_data c JOIN custom_data_fts f ON c.id = f.rowid \
                       WHERE custom_data_fts MATCH ?1".to_string();
        let mut p: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(match_expr.clone())];

        if let Some(since_ts) = &since {
            sql.push_str(" AND c.timestamp >= ?");
            p.push(Box::new(since_ts.clone()));
        }

        sql.push_str(" ORDER BY rank LIMIT ?");
        p.push(Box::new(limit));

        let mut stmt = conn.prepare(&sql)?;
        let p_refs: Vec<&dyn rusqlite::ToSql> = p.iter().map(|b| b.as_ref()).collect();
        let rows = stmt.query_map(rusqlite::params_from_iter(p_refs), |row| {
            let id = row.get::<_, i64>(0)?;
            let category = row.get::<_, String>(1)?;
            let key = row.get::<_, String>(2)?;
            let snippet = row.get::<_, String>(3)?;
            let timestamp = row.get::<_, String>(4)?;
            let rank = row.get::<_, f64>(5)?;
            Ok(QueryResult {
                r#type: "custom_data".to_string(),
                id,
                title: format!("{}/{}", category, key),
                snippet,
                timestamp,
                rank,
            })
        })?;
        for r in rows {
            results.push(r?);
        }
    }

    // Sort by rank ascending (lowest rank is most relevant)
    results.sort_by(|a, b| {
        a.rank
            .partial_cmp(&b.rank)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results.truncate(limit as usize);

    let output: Vec<Value> = results
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "type": r.r#type,
                "id": r.id,
                "title": r.title,
                "snippet": r.snippet,
                "timestamp": r.timestamp,
            })
        })
        .collect();

    Ok(Value::Array(output))
}
