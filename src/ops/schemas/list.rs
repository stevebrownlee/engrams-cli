//! `engrams schema list` / `show` / `refine` (spec 0002, AC-6).
//!
//! Ranked listing puts agent-authored summaries above drafts of comparable
//! recency — the whole point of refining — and `show` additionally exposes
//! members and the lexical centroid the assimilation matcher scores
//! against. All read-only except `refine`, which rewrites exactly one
//! schema row.
use anyhow::Result;
use chrono::{SecondsFormat, Utc};
use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Value};

/// Rank order: agent summaries first (`summary_source`), then recency
/// (newest `updated_at`), then richer schemas (member count). Deterministic
/// total order — no ties survive the id tiebreak.
pub(crate) fn rank_expr() -> &'static str {
    "CASE WHEN s.summary_source = 'agent' THEN 0 ELSE 1 END, \
     s.updated_at DESC, \
     (SELECT COUNT(*) FROM context_links l WHERE l.relationship_type = 'member_of' \
      AND l.target_item_type = 'schema' \
      AND l.target_item_id = CAST(s.id AS TEXT)) DESC, s.id ASC"
}

/// Member count per schema id, from the `member_of` link table.
pub(crate) fn member_counts(conn: &Connection) -> Result<std::collections::HashMap<i64, i64>> {
    let mut stmt = conn.prepare(
        "SELECT target_item_id, COUNT(*) FROM context_links \
         WHERE relationship_type = 'member_of' AND target_item_type = 'schema' \
         GROUP BY target_item_id",
    )?;
    let rows = stmt.query_map([], |r| {
        let id: String = r.get(0)?;
        let id = id.parse::<i64>().map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })?;
        Ok((id, r.get::<_, i64>(1)?))
    })?;
    let mut out = std::collections::HashMap::new();
    for r in rows {
        let (id, n) = r?;
        out.insert(id, n);
    }
    Ok(out)
}

/// The scalar columns of one `schemas` row, as read from a listing query.
struct Row {
    id: i64,
    name: String,
    summary: String,
    summary_source: String,
    status: String,
    updated_at: String,
    last_confirmed_at: Option<String>,
    centroid_json: String,
}

fn row_json(counts: &std::collections::HashMap<i64, i64>, r: &Row) -> Value {
    let centroid: Value = serde_json::from_str(&r.centroid_json).unwrap_or(Value::Null);
    json!({
        "id": r.id,
        "name": r.name,
        "summary": r.summary,
        "summary_source": r.summary_source,
        "status": r.status,
        "member_count": counts.get(&r.id).copied().unwrap_or(0),
        "updated_at": r.updated_at,
        "last_confirmed_at": r.last_confirmed_at,
        "centroid": centroid,
    })
}

/// List confirmed schemas, ranked. `status` filters on the DDL vocabulary.
pub fn list(conn: &Connection, status: Option<&str>) -> Result<Value> {
    let counts = member_counts(conn)?;
    let mut stmt = conn.prepare(&format!(
        "SELECT s.id, s.name, s.summary, s.summary_source, s.status, s.updated_at, \
         s.last_confirmed_at, s.centroid_json \
         FROM schemas s {} ORDER BY {}",
        match status {
            Some(_) => "WHERE s.status = ?1",
            None => "",
        },
        rank_expr(),
    ))?;
    let map_row = |r: &rusqlite::Row| -> rusqlite::Result<Row> {
        Ok(Row {
            id: r.get(0)?,
            name: r.get(1)?,
            summary: r.get(2)?,
            summary_source: r.get(3)?,
            status: r.get(4)?,
            updated_at: r.get(5)?,
            last_confirmed_at: r.get(6)?,
            centroid_json: r.get(7)?,
        })
    };
    let rows: Vec<_> = match status {
        Some(s) => stmt
            .query_map([s], map_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?,
        None => stmt
            .query_map([], map_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?,
    };
    let schemas: Vec<Value> = rows.iter().map(|r| row_json(&counts, r)).collect();
    Ok(json!({
        "status": "success",
        "schemas": schemas,
    }))
}

/// Resolve a schema reference: numeric id first, then exact name.
fn resolve_schema(conn: &Connection, target: &str) -> Result<Option<i64>> {
    if let Ok(id) = target.parse::<i64>() {
        let hit: Option<i64> = conn
            .query_row("SELECT id FROM schemas WHERE id = ?1", [id], |r| r.get(0))
            .optional()?;
        if hit.is_some() {
            return Ok(hit);
        }
    }
    let hit: Option<i64> = conn
        .query_row("SELECT id FROM schemas WHERE name = ?1", [target], |r| {
            r.get(0)
        })
        .optional()?;
    Ok(hit)
}

/// Show one schema with its full member list and centroid.
pub fn show(conn: &Connection, target: &str) -> Result<Value> {
    let id = resolve_schema(conn, target)?
        .ok_or_else(|| anyhow::anyhow!("no schema matches '{target}'"))?;
    let counts = member_counts(conn)?;
    let row: Row = conn.query_row(
        "SELECT name, summary, summary_source, status, updated_at, last_confirmed_at, \
         centroid_json, id FROM schemas WHERE id = ?1",
        [id],
        |r| {
            Ok(Row {
                name: r.get(0)?,
                summary: r.get(1)?,
                summary_source: r.get(2)?,
                status: r.get(3)?,
                updated_at: r.get(4)?,
                last_confirmed_at: r.get(5)?,
                centroid_json: r.get(6)?,
                id: r.get(7)?,
            })
        },
    )?;
    let mut stmt = conn.prepare(
        "SELECT source_item_type, source_item_id FROM context_links \
         WHERE relationship_type = 'member_of' AND target_item_type = 'schema' \
         AND target_item_id = ?1 ORDER BY source_item_type, source_item_id",
    )?;
    let members: Vec<String> = stmt
        .query_map([id], |r| {
            Ok(format!(
                "{}:{}",
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut out = row_json(&counts, &row);
    out["members"] = json!(members);
    Ok(json!({
        "status": "success",
        "schema": out,
    }))
}

/// Replace a schema's summary as agent-authored (`summary_source = 'agent'`),
/// bumping `updated_at` so the refinement re-ranks immediately (AC-6).
pub fn refine(conn: &Connection, target: &str, summary: &str) -> Result<Value> {
    let id = resolve_schema(conn, target)?
        .ok_or_else(|| anyhow::anyhow!("no schema matches '{target}'"))?;
    let ts = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let n = conn.execute(
        "UPDATE schemas SET summary = ?1, summary_source = 'agent', \
         updated_at = ?2 WHERE id = ?3",
        rusqlite::params![summary, ts, id],
    )?;
    if n == 0 {
        anyhow::bail!("no schema matches '{target}'");
    }
    let name: String =
        conn.query_row("SELECT name FROM schemas WHERE id = ?1", [id], |r| r.get(0))?;
    Ok(json!({
        "status": "success",
        "schema": { "id": id, "name": name, "summary": summary,
                    "summary_source": "agent", "updated_at": ts },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::SCHEMA;

    fn mem_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        conn
    }

    fn seed(conn: &Connection, name: &str, source: &str, updated: &str) -> i64 {
        conn.execute(
            "INSERT INTO schemas (uuid, name, summary, summary_source, status, \
             centroid_json, created_at, updated_at) \
             VALUES (?1, ?2, 's', ?3, 'active', '{}', 't0', ?4)",
            rusqlite::params![format!("u-{name}"), name, source, updated],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn agent_schemas_rank_above_drafted_at_equal_recency() {
        let conn = mem_db();
        seed(&conn, "draft-newer", "drafted", "2026-09-05T10:00:00Z");
        seed(&conn, "agent-older", "agent", "2026-09-05T09:00:00Z");
        let out = list(&conn, None).unwrap();
        let schemas = out["schemas"].as_array().unwrap();
        assert_eq!(schemas[0]["name"], "agent-older");
        assert_eq!(schemas[1]["name"], "draft-newer");
    }

    #[test]
    fn status_filter_is_exact() {
        let conn = mem_db();
        seed(&conn, "a", "drafted", "2026-09-05T10:00:00Z");
        let id = seed(&conn, "b", "drafted", "2026-09-05T11:00:00Z");
        conn.execute("UPDATE schemas SET status = 'retired' WHERE id = ?1", [id])
            .unwrap();
        let out = list(&conn, Some("active")).unwrap();
        let schemas = out["schemas"].as_array().unwrap();
        assert_eq!(schemas.len(), 1);
        assert_eq!(schemas[0]["name"], "a");
    }

    #[test]
    fn show_resolves_id_and_name_and_lists_members() {
        let conn = mem_db();
        let id = seed(&conn, "core", "drafted", "2026-09-05T10:00:00Z");
        for (kind, num) in [("decision", "1"), ("decision", "2")] {
            conn.execute(
                "INSERT INTO context_links (source_item_type, source_item_id, \
                 target_item_type, target_item_id, relationship_type, timestamp) \
                 VALUES (?1, ?2, 'schema', ?3, 'member_of', 't')",
                rusqlite::params![kind, num, id.to_string()],
            )
            .unwrap();
        }
        let by_name = show(&conn, "core").unwrap();
        assert_eq!(by_name["schema"]["members"].as_array().unwrap().len(), 2);
        assert_eq!(by_name["schema"]["member_count"], 2);
        let by_id = show(&conn, &id.to_string()).unwrap();
        assert_eq!(by_id["schema"]["name"], "core");
        let err = show(&conn, "nope").unwrap_err().to_string();
        assert!(err.contains("no schema matches"), "{err}");
    }

    #[test]
    fn refine_marks_agent_and_updates_timestamp() {
        let conn = mem_db();
        let id = seed(&conn, "core", "drafted", "2026-09-05T10:00:00Z");
        let out = refine(&conn, "core", "better summary").unwrap();
        assert_eq!(out["schema"]["summary_source"], "agent");
        let (source, summary): (String, String) = conn
            .query_row(
                "SELECT summary_source, summary FROM schemas WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(source, "agent");
        assert_eq!(summary, "better summary");
        let err = refine(&conn, "ghost", "x").unwrap_err().to_string();
        assert!(err.contains("no schema matches"), "{err}");
    }
}
