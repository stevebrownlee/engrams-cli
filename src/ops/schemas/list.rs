//! `engrams schema list` / `show` / `refine` (spec 0002, AC-6).
//!
//! Ranked listing puts agent-authored summaries above drafts of comparable
//! recency — the whole point of refining — and the enriched schema reads
//! (`schema show`, `brief schema:<id>`) expose per-kind enriched members
//! (capped, with truncation reported when the cap bites — spec 0005
//! Architecture items 1-2) and the lexical centroid the assimilation
//! matcher scores against. The shared member enrichment lives in
//! [`super::retrieval`]. All read-only except `refine`, which rewrites
//! exactly one schema row.
use anyhow::Result;
use chrono::{SecondsFormat, Utc};
use std::borrow::Cow;

use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Value};

use super::retrieval;

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
    statement: Option<String>,
    summary: String,
    summary_source: String,
    scope_json: String,
    status: String,
    kind: String,
    kind_reasons_json: String,
    updated_at: String,
    last_confirmed_at: Option<String>,
    centroid_json: String,
}

fn row_json(
    counts: &std::collections::HashMap<i64, i64>,
    guided: &std::collections::HashMap<i64, usize>,
    r: &Row,
) -> Value {
    let centroid: Value = serde_json::from_str(&r.centroid_json).unwrap_or(Value::Null);
    json!({
        "id": r.id,
        "name": r.name,
        "statement": r.statement,
        "summary": r.summary,
        "summary_source": r.summary_source,
        "scope": retrieval::parse_scope(&r.scope_json),
        "status": r.status,
        "kind": r.kind,
        "member_count": counts.get(&r.id).copied().unwrap_or(0),
        "guided": guided.get(&r.id).copied().unwrap_or(0),
        "updated_at": r.updated_at,
        "last_confirmed_at": r.last_confirmed_at,
        "centroid": centroid,
    })
}

/// List confirmed schemas, ranked. `status` filters on the DDL vocabulary.
pub fn list(conn: &Connection, status: Option<&str>) -> Result<Value> {
    let counts = member_counts(conn)?;
    let guided = retrieval::surface_counts(conn)?;
    let mut stmt = conn.prepare(&format!(
        "SELECT s.id, s.name, s.statement, s.summary, s.summary_source, s.scope_json, \
         s.status, s.kind, s.kind_reasons_json, s.updated_at, \
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
            statement: r.get(2)?,
            summary: r.get(3)?,
            summary_source: r.get(4)?,
            scope_json: r.get(5)?,
            status: r.get(6)?,
            kind: r.get(7)?,
            kind_reasons_json: r.get(8)?,
            updated_at: r.get(9)?,
            last_confirmed_at: r.get(10)?,
            centroid_json: r.get(11)?,
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
    let schemas: Vec<Value> = rows.iter().map(|r| row_json(&counts, &guided, r)).collect();
    Ok(json!({
        "status": "success",
        "schemas": schemas,
    }))
}

/// Resolve a schema reference: numeric id first, then exact name, then the
/// slug form — name lowercased with non-alphanumeric runs folded to `-` —
/// so `multi_kind` and `Multi Kind` both hit a schema named `multi-kind`
/// (spec 0005 Architecture item 2). An exact name is never shadowed by a
/// slug collision; the slug form is only a fallback.
pub(crate) fn resolve_schema(conn: &Connection, target: &str) -> Result<Option<i64>> {
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
    if hit.is_some() {
        return Ok(hit);
    }
    let slug = slug_fold(target);
    // Slug matching folds each stored name in Rust — SQL cannot express the
    // fold — so the table is scanned in id order and the first fold match
    // wins. Only reached after id and exact-name misses; schema tables are
    // small, and the scan keeps "Multi Kind" and "multi_kind" hitting the
    // same row.
    let mut stmt = conn.prepare("SELECT id, name FROM schemas ORDER BY id")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
    for row in rows {
        let (id, name) = row?;
        if slug_fold(&name) == slug {
            return Ok(Some(id));
        }
    }
    Ok(None)
}

/// Fold a schema name to its slug form: lowercased, non-alphanumeric runs
/// collapsed to single `-` separators.
fn slug_fold(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut pending_sep = false;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            if pending_sep && !out.is_empty() {
                out.push('-');
            }
            pending_sep = false;
            out.push(c.to_ascii_lowercase());
        } else {
            pending_sep = true;
        }
    }
    out
}

/// Show one schema with its full member list and centroid.
pub fn show(conn: &Connection, target: &str) -> Result<Value> {
    let id = resolve_schema(conn, target)?
        .ok_or_else(|| anyhow::anyhow!("no schema matches '{target}'"))?;
    let counts = member_counts(conn)?;
    let guided = retrieval::surface_counts(conn)?;
    let row: Row = conn.query_row(
        "SELECT name, statement, summary, summary_source, scope_json, status, kind, \
         kind_reasons_json, updated_at, last_confirmed_at, centroid_json, id \
         FROM schemas WHERE id = ?1",
        [id],
        |r| {
            Ok(Row {
                name: r.get(0)?,
                statement: r.get(1)?,
                summary: r.get(2)?,
                summary_source: r.get(3)?,
                scope_json: r.get(4)?,
                status: r.get(5)?,
                kind: r.get(6)?,
                kind_reasons_json: r.get(7)?,
                updated_at: r.get(8)?,
                last_confirmed_at: r.get(9)?,
                centroid_json: r.get(10)?,
                id: r.get(11)?,
            })
        },
    )?;
    let members = retrieval::member_refs(conn, id)?;
    let total = members.len();
    let cap = total.min(retrieval::MEMBER_ENRICHMENT_CAP);
    let enriched = retrieval::enrich_members(conn, &members[..cap])?;
    let mut out = row_json(&counts, &guided, &row);
    out["members"] = Value::Array(enriched);
    out["origin"] = retrieval::origin_for_schema(conn, id)?;
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

/// Accepted opening phrases for a manual `--summary` (spec 0005 AC-4).
/// Canonical vocabulary — `&'static str`, never `String` (rule 006).
const TRIGGER_PREFIXES: [&str; 4] = ["Fires when", "When", "Applies if", "Activates when"];

/// Resolve the two refine modes (spec 0005 AC-3/AC-4): a manual `--summary`
/// must open with a conditional trigger phrase, or `--trigger` + `--rule`
/// together synthesize `"Fires when <CONDITION>: <DIRECTIVE>"`. Every other
/// flag combination is an error. The manual form stays borrowed; only the
/// synthesized form allocates, once, at this ownership boundary.
pub(crate) fn refine_text<'a>(
    summary: Option<&'a str>,
    trigger: Option<&str>,
    rule: Option<&str>,
) -> Result<Cow<'a, str>> {
    match (summary, trigger, rule) {
        (Some(s), None, None) => {
            if TRIGGER_PREFIXES.iter().any(|phrase| s.starts_with(phrase)) {
                Ok(Cow::Borrowed(s))
            } else {
                Err(anyhow::anyhow!(
                    "summary must begin with a conditional trigger phrase \
                     (e.g., 'Fires when...', 'When...', 'Applies if...', \
                     'Activates when...')"
                ))
            }
        }
        (None, Some(condition), Some(directive)) => {
            Ok(Cow::Owned(format!("Fires when {condition}: {directive}")))
        }
        (None, Some(_), None) => Err(anyhow::anyhow!(
            "--trigger requires --rule: pass both together to synthesize \
             'Fires when <CONDITION>: <DIRECTIVE>'"
        )),
        (None, None, Some(_)) => Err(anyhow::anyhow!(
            "--rule requires --trigger: pass both together to synthesize \
             'Fires when <CONDITION>: <DIRECTIVE>'"
        )),
        (Some(_), _, _) => Err(anyhow::anyhow!(
            "--summary cannot be combined with --trigger/--rule: pass either \
             --summary alone or --trigger with --rule"
        )),
        (None, None, None) => Err(anyhow::anyhow!(
            "nothing to refine: pass --summary, or --trigger with --rule"
        )),
    }
}

/// Replace a schema's summary as agent-authored (`summary_source = 'agent'`),
/// bumping `updated_at` so the refinement re-ranks immediately (AC-6).
/// The summary is either a validated manual `--summary` (AC-4) or synthesized
/// from `--trigger` + `--rule` (AC-3); validation precedes the single-row
/// write, so a rejected summary never reaches the database.
pub fn refine(
    conn: &Connection,
    target: &str,
    summary: Option<&str>,
    trigger: Option<&str>,
    rule: Option<&str>,
) -> Result<Value> {
    let id = resolve_schema(conn, target)?
        .ok_or_else(|| anyhow::anyhow!("no schema matches '{target}'"))?;
    let text = refine_text(summary, trigger, rule)?;
    let ts = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let n = conn.execute(
        "UPDATE schemas SET summary = ?1, summary_source = 'agent', \
         updated_at = ?2 WHERE id = ?3",
        rusqlite::params![text.as_ref(), ts, id],
    )?;
    if n == 0 {
        anyhow::bail!("no schema matches '{target}'");
    }
    let name: String =
        conn.query_row("SELECT name FROM schemas WHERE id = ?1", [id], |r| r.get(0))?;
    Ok(json!({
        "status": "success",
        "schema": { "id": id, "name": name, "summary": text,
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
        let out = refine(
            &conn,
            "core",
            Some("Fires when gateway requests route: consult the routing decisions"),
            None,
            None,
        )
        .unwrap();
        assert_eq!(out["schema"]["summary_source"], "agent");
        let (source, summary): (String, String) = conn
            .query_row(
                "SELECT summary_source, summary FROM schemas WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(source, "agent");
        assert_eq!(
            summary,
            "Fires when gateway requests route: consult the routing decisions"
        );
        let err = refine(&conn, "ghost", Some("x"), None, None)
            .unwrap_err()
            .to_string();
        assert!(err.contains("no schema matches"), "{err}");
    }

    #[test]
    fn refine_synthesizes_summary_from_trigger_and_rule() {
        // AC-3: --trigger + --rule become "Fires when <CONDITION>: <DIRECTIVE>"
        // recorded as agent-authored.
        let conn = mem_db();
        let id = seed(&conn, "core", "drafted", "2026-09-05T10:00:00Z");
        let out = refine(
            &conn,
            "core",
            None,
            Some("editing documentation"),
            Some("apply marketing framing"),
        )
        .unwrap();
        assert_eq!(
            out["schema"]["summary"],
            "Fires when editing documentation: apply marketing framing"
        );
        let (source, summary): (String, String) = conn
            .query_row(
                "SELECT summary_source, summary FROM schemas WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(source, "agent");
        assert_eq!(
            summary,
            "Fires when editing documentation: apply marketing framing"
        );
    }

    #[test]
    fn refine_accepts_every_trigger_prefix() {
        // AC-4: each documented conditional phrase opens a manual summary.
        let conn = mem_db();
        seed(&conn, "core", "drafted", "2026-09-05T10:00:00Z");
        for prefix in TRIGGER_PREFIXES {
            let text = format!("{prefix} deploying: run the smoke suite");
            let out = refine(&conn, "core", Some(&text), None, None).unwrap();
            assert_eq!(out["schema"]["summary"], text.as_str());
        }
    }

    #[test]
    fn refine_rejects_manual_summary_without_trigger_phrase() {
        // AC-4: the summary is unchanged when validation fails.
        let conn = mem_db();
        let id = seed(&conn, "core", "drafted", "2026-09-05T10:00:00Z");
        let err = refine(&conn, "core", Some("Gateway routing notes"), None, None)
            .unwrap_err()
            .to_string();
        assert!(err.contains("conditional trigger phrase"), "{err}");
        let (source, summary): (String, String) = conn
            .query_row(
                "SELECT summary_source, summary FROM schemas WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(source, "drafted", "failed refine must not promote to agent");
        assert_eq!(summary, "s", "failed refine must leave the summary alone");
    }

    #[test]
    fn refine_requires_trigger_and_rule_together() {
        let conn = mem_db();
        seed(&conn, "core", "drafted", "2026-09-05T10:00:00Z");
        let err = refine(&conn, "core", None, Some("editing docs"), None)
            .unwrap_err()
            .to_string();
        assert!(err.contains("--rule"), "{err}");
        let err = refine(&conn, "core", None, None, Some("apply framing"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("--trigger"), "{err}");
    }

    #[test]
    fn refine_rejects_summary_mixed_with_trigger_flags() {
        let conn = mem_db();
        seed(&conn, "core", "drafted", "2026-09-05T10:00:00Z");
        let err = refine(&conn, "core", Some("Fires when x: y"), Some("x"), None)
            .unwrap_err()
            .to_string();
        assert!(err.contains("--summary"), "{err}");
        let err = refine(&conn, "core", Some("Fires when x: y"), None, Some("y"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("--summary"), "{err}");
    }
}
