//! `engrams schema list` / `show` / `refine` (spec 0002, AC-6).
//!
//! Ranked listing puts agent-authored summaries above drafts of comparable
//! recency — the whole point of refining — and `show` additionally exposes
//! per-kind enriched members (capped, with truncation reported when the cap
//! bites — spec 0005 Architecture item 1) and the lexical centroid the
//! assimilation matcher scores against. All read-only except `refine`,
//! which rewrites exactly one schema row.
use anyhow::Result;
use chrono::{SecondsFormat, Utc};
use std::collections::HashMap;

use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Map, Value};

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
    kind: String,
    kind_reasons_json: String,
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
        "kind": r.kind,
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
        "SELECT s.id, s.name, s.summary, s.summary_source, s.status, s.kind, \
         s.kind_reasons_json, s.updated_at, \
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
            kind: r.get(5)?,
            kind_reasons_json: r.get(6)?,
            updated_at: r.get(7)?,
            last_confirmed_at: r.get(8)?,
            centroid_json: r.get(9)?,
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
        "SELECT name, summary, summary_source, status, kind, kind_reasons_json, \
         updated_at, last_confirmed_at, \
         centroid_json, id FROM schemas WHERE id = ?1",
        [id],
        |r| {
            Ok(Row {
                name: r.get(0)?,
                summary: r.get(1)?,
                summary_source: r.get(2)?,
                status: r.get(3)?,
                kind: r.get(4)?,
                kind_reasons_json: r.get(5)?,
                updated_at: r.get(6)?,
                last_confirmed_at: r.get(7)?,
                centroid_json: r.get(8)?,
                id: r.get(9)?,
            })
        },
    )?;
    let mut stmt = conn.prepare(
        "SELECT source_item_type, source_item_id FROM context_links \
         WHERE relationship_type = 'member_of' AND target_item_type = 'schema' \
         AND target_item_id = ?1 ORDER BY source_item_type, source_item_id",
    )?;
    let members: Vec<(String, String)> = stmt
        .query_map([id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let total = members.len();
    let cap = total.min(MEMBER_ENRICHMENT_CAP);
    let enriched = enrich_members(conn, &members[..cap])?;
    let mut out = row_json(&counts, &row);
    out["members"] = Value::Array(enriched);
    // Confirm-time reason sentences, projected as a JSON array; the v14
    // default '[]' reads as an empty array (spec 0004 AC-5).
    out["reasons"] = serde_json::from_str(&row.kind_reasons_json).unwrap_or(json!([]));
    // Enrichment is capped (spec 0005 Architecture item 1); `member_count`
    // above stays the true total. `total_members` only appears when the
    // cap bit, so the uncapped shape is unchanged.
    out["truncated"] = json!(total > cap);
    if total > cap {
        out["total_members"] = json!(total);
    }
    Ok(json!({
        "status": "success",
        "schema": out,
    }))
}

/// Cap on enriched members in `schema show` (spec 0005 Architecture
/// item 1): inventory schemas can carry hundreds of members and the read
/// path is a single agent-facing call, so past the cap the payload reports
/// `truncated`/`total_members` instead of more rows.
const MEMBER_ENRICHMENT_CAP: usize = 20;

/// The member kinds `show` enriches — the graph node universe
/// [`kind_table`] backs, spelled exactly as stored in `context_links`
/// (F-5: `code`, not `code_node`). Anything else passes through bare.
fn enrichable_kind(kind: &str) -> Option<&'static str> {
    match kind {
        "decision" => Some("decision"),
        "system_pattern" => Some("system_pattern"),
        "progress_entry" => Some("progress_entry"),
        "custom_data" => Some("custom_data"),
        "code" => Some("code"),
        "schema" => Some("schema"),
        _ => None,
    }
}

/// Enrich member references with the per-kind details `brief` surfaces
/// (spec 0005 Architecture item 1): decisions and patterns gain status,
/// contract/severity and anchors; code gains the symbol signature;
/// progress entries gain status; custom data gains its key and value
/// (JSON when parseable); nested schemas gain name and summary. Members
/// with no backing row — or an unenrichable kind like `pr` — stay bare
/// references. The detail maps die in this call, so entries are moved
/// out, never cloned (rule 004).
pub(crate) fn enrich_members(
    conn: &Connection,
    members: &[(String, String)],
) -> Result<Vec<Value>> {
    // Per-kind id buckets; only enrichable kinds with numeric ids enter.
    let mut buckets: HashMap<&'static str, Vec<i64>> = HashMap::new();
    for (kind, raw) in members {
        if let (Some(canon), Ok(id)) = (enrichable_kind(kind), raw.parse::<i64>()) {
            buckets.entry(canon).or_default().push(id);
        }
    }

    // One batched lookup per kind: id -> the fields the payload inlines.
    let mut details: HashMap<&'static str, HashMap<i64, Value>> = HashMap::new();
    if let Some(ids) = buckets.remove("decision") {
        details.insert(
            "decision",
            fetch_details(
                conn,
                &format!(
                    "SELECT id, summary, status, contract FROM decisions \
                     WHERE id IN ({})",
                    crate::ops::sql_placeholders(ids.len())
                ),
                &ids,
                |r| {
                    Ok(json!({
                        "summary": r.get::<_, String>(1)?,
                        "status": r.get::<_, String>(2)?,
                        "contract": r.get::<_, Option<String>>(3)?,
                    }))
                },
            )?,
        );
    }
    if let Some(ids) = buckets.remove("system_pattern") {
        details.insert(
            "system_pattern",
            fetch_details(
                conn,
                &format!(
                    "SELECT id, name, description, severity FROM system_patterns \
                     WHERE id IN ({})",
                    crate::ops::sql_placeholders(ids.len())
                ),
                &ids,
                |r| {
                    Ok(json!({
                        "name": r.get::<_, String>(1)?,
                        "description": r.get::<_, Option<String>>(2)?,
                        "severity": r.get::<_, String>(3)?,
                    }))
                },
            )?,
        );
    }
    if let Some(ids) = buckets.remove("progress_entry") {
        details.insert(
            "progress_entry",
            fetch_details(
                conn,
                &format!(
                    "SELECT id, description, status, timestamp FROM progress_entries \
                     WHERE id IN ({})",
                    crate::ops::sql_placeholders(ids.len())
                ),
                &ids,
                |r| {
                    Ok(json!({
                        "description": r.get::<_, String>(1)?,
                        "status": r.get::<_, String>(2)?,
                        "timestamp": r.get::<_, String>(3)?,
                    }))
                },
            )?,
        );
    }
    if let Some(ids) = buckets.remove("custom_data") {
        details.insert(
            "custom_data",
            fetch_details(
                conn,
                &format!(
                    "SELECT id, key, value FROM custom_data WHERE id IN ({})",
                    crate::ops::sql_placeholders(ids.len())
                ),
                &ids,
                |r| {
                    // Stored TEXT: surfaced as JSON when it parses as such
                    // (mirrors import's round-trip), else as the raw string.
                    let value: String = r.get(2)?;
                    Ok(json!({
                        "key": r.get::<_, String>(1)?,
                        "value": serde_json::from_str::<Value>(&value)
                            .unwrap_or(Value::String(value)),
                    }))
                },
            )?,
        );
    }
    if let Some(ids) = buckets.remove("code") {
        details.insert(
            "code",
            fetch_details(
                conn,
                &format!(
                    "SELECT id, path, symbol, line_count FROM code_nodes \
                     WHERE id IN ({})",
                    crate::ops::sql_placeholders(ids.len())
                ),
                &ids,
                |r| {
                    // code_nodes.kind ('file'/'function') is deliberately not
                    // projected: the member discriminator owns the `kind` key.
                    Ok(json!({
                        "path": r.get::<_, String>(1)?,
                        "symbol": r.get::<_, String>(2)?,
                        "line_count": r.get::<_, Option<i64>>(3)?,
                    }))
                },
            )?,
        );
    }
    if let Some(ids) = buckets.remove("schema") {
        details.insert(
            "schema",
            fetch_details(
                conn,
                &format!(
                    "SELECT id, name, summary FROM schemas WHERE id IN ({})",
                    crate::ops::sql_placeholders(ids.len())
                ),
                &ids,
                |r| {
                    Ok(json!({
                        "name": r.get::<_, String>(1)?,
                        "summary": r.get::<_, String>(2)?,
                    }))
                },
            )?,
        );
    }

    // Fold back in link order, moving each member's detail out of the
    // dying maps. Anchors ride along for the kinds `brief` anchors.
    let mut out = Vec::with_capacity(members.len());
    for (kind, raw) in members {
        let canon = enrichable_kind(kind);
        let id = raw.parse::<i64>().ok();
        let detail = canon
            .zip(id)
            .and_then(|(c, i)| details.get_mut(c).and_then(|m| m.remove(&i)));
        let mut obj = Map::new();
        obj.insert("kind".into(), json!(canon.unwrap_or(kind.as_str())));
        obj.insert(
            "id".into(),
            id.map(Value::from).unwrap_or_else(|| json!(raw)),
        );
        if let Some(Value::Object(fields)) = detail {
            for (k, v) in fields {
                obj.insert(k, v);
            }
        }
        if let (Some(c), Some(i)) = (canon, id) {
            if c == "decision" || c == "system_pattern" {
                obj.insert(
                    "anchors".into(),
                    json!(crate::ops::anchor::anchors_for(conn, c, i)?),
                );
            }
        }
        out.push(Value::Object(obj));
    }
    Ok(out)
}

/// Batched `IN` lookup for one member kind: id -> row fields. Id params
/// borrow (rule 002); the map is consumed by [`enrich_members`].
fn fetch_details(
    conn: &Connection,
    sql: &str,
    ids: &[i64],
    fields: impl Fn(&rusqlite::Row<'_>) -> rusqlite::Result<Value>,
) -> Result<HashMap<i64, Value>> {
    let mut stmt = conn.prepare(sql)?;
    let id_refs: Vec<&i64> = ids.iter().collect();
    let rows = stmt.query_map(rusqlite::params_from_iter(id_refs), |r| {
        Ok((r.get::<_, i64>(0)?, fields(r)?))
    })?;
    let mut map = HashMap::with_capacity(ids.len());
    for row in rows {
        let (id, fields) = row?;
        map.insert(id, fields);
    }
    Ok(map)
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
    fn list_surfaces_kind_next_to_status() {
        let conn = mem_db();
        seed(&conn, "routine", "drafted", "2026-09-05T10:00:00Z");
        seed(&conn, "pile", "agent", "2026-09-05T09:00:00Z");
        // A row labeled by the 0003 kind pass; the other keeps the v14
        // default and must read as `unclear` (spec 0004 AC-4).
        conn.execute(
            "UPDATE schemas SET kind = 'inventory' WHERE name = 'routine'",
            [],
        )
        .unwrap();

        let out = list(&conn, None).unwrap();
        let schemas = out["schemas"].as_array().unwrap();
        let by_name = |name: &str| schemas.iter().find(|s| s["name"] == name).unwrap();
        assert_eq!(by_name("routine")["kind"], "inventory");
        assert_eq!(by_name("routine")["status"], "active");
        assert_eq!(by_name("pile")["kind"], "unclear");
    }

    #[test]
    fn show_includes_kind_and_parsed_reasons() {
        let conn = mem_db();
        seed(&conn, "routine", "drafted", "2026-09-05T10:00:00Z");
        let reasons = serde_json::to_string(&[
            "Recurring awake stretches.",
            "A checkable rule fires on its members.",
        ])
        .unwrap();
        conn.execute(
            "UPDATE schemas SET kind = 'schema', kind_reasons_json = ?1 \
             WHERE name = 'routine'",
            rusqlite::params![reasons],
        )
        .unwrap();

        let out = show(&conn, "routine").unwrap();
        assert_eq!(out["schema"]["kind"], "schema");
        assert_eq!(
            out["schema"]["reasons"],
            json!([
                "Recurring awake stretches.",
                "A checkable rule fires on its members."
            ])
        );

        // The unlabeled v14 default: kind `unclear`, reasons an empty array.
        seed(&conn, "bare", "drafted", "2026-09-05T08:00:00Z");
        let out = show(&conn, "bare").unwrap();
        assert_eq!(out["schema"]["kind"], "unclear");
        assert_eq!(out["schema"]["reasons"], json!([]));
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
