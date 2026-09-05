//! Lexical assimilation hooks (spec 0002, AC-8): at log time, new items are
//! matched against confirmed schemas' `centroid_json` and either fire a
//! suggestion (persisted to `schema_suggestions`, no membership write) or —
//! with the explicit `--schema` flag — attach `member_of` at write time and
//! bump the schema's confirmation recency.
//!
//! Matching is convention-aware token overlap only (no model): fit is the
//! share of a schema's centroid tags whose word parts appear in the item's
//! summary + tags. Thresholds are decision 78's frozen launch defaults.

use std::collections::BTreeSet;

use anyhow::{Context as _, Result};
use rusqlite::{params, Connection, OptionalExtension};

use serde_json::{json, Value};

/// Minimum token-overlap fit for a suggestion to fire (decision 78).
pub const FIT_GATE: f64 = 0.4;
/// Top-N schemas suggested per item (decision 78).
pub const TOP_SUGGESTIONS: usize = 2;

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// One fired or resolved suggestion, as surfaced in log-command output.
pub struct Suggestion {
    pub schema_id: i64,
    pub name: String,
    pub fit: f64,
}

impl Suggestion {
    fn to_json(&self) -> Value {
        json!({ "id": self.schema_id, "name": self.name, "fit": self.fit })
    }
}

/// Convention-aware token set for an item's lexical surface: ident parts of
/// the summary words plus every tag (snake/kebab/camel variants collapse).
fn item_tokens(summary: &str, tags: &[String]) -> BTreeSet<String> {
    let mut parts = BTreeSet::new();
    for word in summary.split_whitespace() {
        parts.extend(crate::ops::ident_parts(word));
    }
    for tag in tags {
        parts.extend(crate::ops::ident_parts(tag));
    }
    parts
}

/// Match one item's lexical surface against all active schemas' centroids.
/// Fit = matched centroid tags / total centroid tags; only schemas at or
/// above [`FIT_GATE`] return, best fit first, capped at [`TOP_SUGGESTIONS`].
/// Read-only: firing happens in [`record_suggestions`].
pub fn match_schemas(conn: &Connection, summary: &str, tags: &[String]) -> Result<Vec<Suggestion>> {
    let query_parts = item_tokens(summary, tags);
    if query_parts.is_empty() {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(
        "SELECT id, name, centroid_json FROM schemas \
         WHERE status = 'active' ORDER BY id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })?;
    let mut hits: Vec<Suggestion> = Vec::new();
    for r in rows {
        let (id, name, centroid_json) = r?;
        let Ok(centroid) = serde_json::from_str::<Value>(&centroid_json) else {
            continue;
        };
        let Some(tag_keys) = centroid["tags"].as_object() else {
            continue;
        };
        let total = tag_keys.len();
        if total == 0 {
            continue;
        }
        let matched = tag_keys
            .keys()
            .filter(|t| {
                crate::ops::ident_parts(t)
                    .iter()
                    .any(|p| query_parts.contains(p))
            })
            .count();
        let fit = matched as f64 / total as f64;
        if fit >= FIT_GATE {
            hits.push(Suggestion {
                schema_id: id,
                name,
                fit,
            });
        }
    }
    hits.sort_by(|a, b| {
        b.fit
            .partial_cmp(&a.fit)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.schema_id.cmp(&b.schema_id))
    });
    hits.truncate(TOP_SUGGESTIONS);
    Ok(hits)
}

/// Persist fired suggestions for one item (status `suggested`). Re-firing
/// refreshes the fit but never revives a row the user already resolved.
pub fn record_suggestions(
    conn: &Connection,
    kind: &str,
    item_id: i64,
    suggestions: &[Suggestion],
) -> Result<()> {
    if suggestions.is_empty() {
        return Ok(());
    }
    let ts = now();
    // Composable with an outer transaction (batch): join it instead of
    // nesting a second BEGIN on the same connection.
    if conn.is_autocommit() {
        let tx = conn.unchecked_transaction()?;
        insert_suggestions(&tx, kind, item_id, suggestions, &ts)?;
        tx.commit()?;
    } else {
        insert_suggestions(conn, kind, item_id, suggestions, &ts)?;
    }
    Ok(())
}

fn insert_suggestions(
    conn: &Connection,
    kind: &str,
    item_id: i64,
    suggestions: &[Suggestion],
    ts: &str,
) -> Result<()> {
    for s in suggestions {
        conn.execute(
            "INSERT INTO schema_suggestions (ts, schema_id, item_kind, item_id, fit, status) \
             VALUES (?1, ?2, ?3, ?4, ?5, 'suggested') \
             ON CONFLICT(schema_id, item_kind, item_id) DO UPDATE SET \
             fit = excluded.fit, \
             status = CASE WHEN schema_suggestions.status = 'suggested' \
                           THEN 'suggested' ELSE schema_suggestions.status END",
            params![ts, s.schema_id, kind, item_id, s.fit],
        )?;
    }
    Ok(())
}

/// `--schema <id|name>`: attach `member_of` at write time, mark any matching
/// suggestion accepted, and bump the schema's `last_confirmed_at` (AC-8).
/// Idempotent — a repeated attach updates recency, never duplicates edges.
pub fn attach(conn: &Connection, kind: &str, item_id: i64, target: &str) -> Result<Suggestion> {
    let schema_id: i64 = match target.parse::<i64>() {
        Ok(id) => {
            let hit: Option<i64> = conn
                .query_row("SELECT id FROM schemas WHERE id = ?1", params![id], |r| {
                    r.get(0)
                })
                .optional()?;
            hit.ok_or_else(|| anyhow::anyhow!("no schema matches '{target}'"))?
        }
        Err(_) => {
            let hit: Option<i64> = conn
                .query_row(
                    "SELECT id FROM schemas WHERE name = ?1",
                    params![target],
                    |r| r.get(0),
                )
                .optional()?;
            hit.ok_or_else(|| anyhow::anyhow!("no schema matches '{target}'"))?
        }
    };
    let name: String = conn.query_row(
        "SELECT name FROM schemas WHERE id = ?1",
        params![schema_id],
        |r| r.get(0),
    )?;

    let ts = now();
    let graph_kind = match kind {
        "pattern" => "system_pattern",
        "progress-entry" => "progress_entry",
        other => other,
    };
    // Same composability rule as record_suggestions.
    if conn.is_autocommit() {
        let tx = conn.unchecked_transaction()?;
        attach_writes(&tx, graph_kind, item_id, schema_id, kind, &ts)?;
        tx.commit()?;
    } else {
        attach_writes(conn, graph_kind, item_id, schema_id, kind, &ts)?;
    }

    /// The three attach writes: edge (idempotent), suggestion acceptance,
    /// confirmation recency. Runs on the caller's transaction scope.
    fn attach_writes(
        conn: &Connection,
        graph_kind: &str,
        item_id: i64,
        schema_id: i64,
        kind: &str,
        ts: &str,
    ) -> Result<()> {
        conn.execute(
            "INSERT INTO context_links (source_item_type, source_item_id, target_item_type, \
         target_item_id, relationship_type, timestamp, origin, source, weight) \
         SELECT ?1, ?2, 'schema', ?3, 'member_of', ?4, 'manual', 'schema_assimilate', 1.0 \
         WHERE NOT EXISTS (SELECT 1 FROM context_links WHERE source_item_type = ?1 \
         AND source_item_id = ?2 AND target_item_type = 'schema' AND target_item_id = ?3 \
         AND relationship_type = 'member_of')",
            params![graph_kind, item_id.to_string(), schema_id.to_string(), ts],
        )
        .context("attaching member_of edge")?;
        conn.execute(
            "UPDATE schema_suggestions SET status = 'accepted', ts = ?4 \
         WHERE schema_id = ?1 AND item_kind = ?2 AND item_id = ?3",
            params![schema_id, kind, item_id, ts],
        )?;
        conn.execute(
            "UPDATE schemas SET last_confirmed_at = ?2, updated_at = ?2 WHERE id = ?1",
            params![schema_id, ts],
        )?;
        Ok(())
    }

    let fit: Option<f64> = conn
        .query_row(
            "SELECT fit FROM schema_suggestions \
             WHERE schema_id = ?1 AND item_kind = ?2 AND item_id = ?3",
            params![schema_id, kind, item_id],
            |r| r.get(0),
        )
        .optional()?;
    Ok(Suggestion {
        schema_id,
        name,
        fit: fit.unwrap_or(0.0),
    })
}

/// `--schema none`: the decision-79 opt-out. Every still-`suggested` row for
/// this item flips to `declined`; an item that never fired has nothing to
/// decline and stays absent from the table (a never-attached suggestion is
/// ambiguous between declined, ignored, and session-ended — only explicit
/// opt-out is observable). Returns the number of rows flipped.
pub fn decline_fired(conn: &Connection, kind: &str, item_id: i64) -> Result<usize> {
    let n = conn.execute(
        "UPDATE schema_suggestions SET status = 'declined', ts = ?3 \
         WHERE item_kind = ?1 AND item_id = ?2 AND status = 'suggested'",
        params![kind, item_id, now()],
    )?;
    Ok(n)
}

/// The output block shared by the three log commands: the fired/attached
/// suggestion list, plus the attach/decline markers when the flags ran.
pub fn output_block(
    conn: &Connection,
    kind: &str,
    item_id: i64,
    attached: Option<&Suggestion>,
    declined: usize,
) -> Result<Value> {
    let mut suggestions = Vec::new();
    if attached.is_none() {
        let mut stmt = conn.prepare(
            "SELECT s.id, s.name, g.fit FROM schema_suggestions g \
             JOIN schemas s ON s.id = g.schema_id \
             WHERE g.item_kind = ?1 AND g.item_id = ?2 AND g.status = 'suggested' \
             ORDER BY g.fit DESC, s.id",
        )?;
        let rows = stmt.query_map(params![kind, item_id], |r| {
            Ok(Suggestion {
                schema_id: r.get(0)?,
                name: r.get(1)?,
                fit: r.get(2)?,
            })
        })?;
        for r in rows {
            suggestions.push(r?.to_json());
        }
    }
    Ok(json!({
        "schema_suggestions": suggestions,
        "schema_attached": attached.map(|s| s.to_json()),
        "schema_declined": if declined > 0 { Some(declined) } else { None },
    }))
}

/// The one call each log command makes after its write commits: `--schema
/// <id|name>` attaches, `--schema none` declines fired suggestions, absence
/// matches and fires. `kind` is the schema_suggestions vocabulary
/// (`decision` / `pattern` / `progress-entry`); item ids are global per kind.
pub fn at_log_time(
    conn: &Connection,
    kind: &str,
    item_id: i64,
    summary: &str,
    tags: &[String],
    schema: Option<&str>,
) -> Result<Value> {
    let (attached, declined) = match schema {
        // Fire first, then flip: the item DID lexically match, and the
        // declined row is the audit trail that it will not be re-suggested.
        Some("none") => {
            let hits = match_schemas(conn, summary, tags)?;
            record_suggestions(conn, kind, item_id, &hits)?;
            (None, decline_fired(conn, kind, item_id)?)
        }
        Some(target) => (Some(attach(conn, kind, item_id, target)?), 0),
        None => {
            let hits = match_schemas(conn, summary, tags)?;
            record_suggestions(conn, kind, item_id, &hits)?;
            (None, 0)
        }
    };
    output_block(conn, kind, item_id, attached.as_ref(), declined)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::SCHEMA;
    use rusqlite::params;

    fn mem_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        conn
    }

    /// Confirmed gateway schema with a real-shaped centroid.
    fn gateway_schema(conn: &Connection) -> i64 {
        conn.execute(
            "INSERT INTO schemas (uuid, name, summary, summary_source, status, centroid_json, \
             last_confirmed_at, created_at, updated_at) \
             VALUES ('u', 'gateway', 's', 'agent', 'active', \
             '{\"tags\": {\"api_gateway\": 2, \"routing\": 3}, \"anchors\": {}}', \
             '2026-01-01T00:00:00Z', 't0', 't0')",
            [],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn fit_gate_and_convention_aware_tokenization() {
        let conn = mem_db();
        let id = gateway_schema(&conn);

        // kebab-case query tokenizes to the same parts as the centroid tag.
        let hits = match_schemas(&conn, "Rewire the api-gateway routing table", &[]).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].schema_id, id);
        assert!((hits[0].fit - 1.0).abs() < 1e-9);

        // Half the centroid matches: fires at exactly the 0.4 gate.
        let half = match_schemas(&conn, "unrelated note about routing", &[]).unwrap();
        assert_eq!(half.len(), 1);
        assert!((half[0].fit - 0.5).abs() < 1e-9);

        // Below the gate: no suggestion.
        assert!(
            match_schemas(&conn, "completely unrelated storage engine", &[])
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn suggestions_persist_and_never_write_membership() {
        let conn = mem_db();
        gateway_schema(&conn);
        let hits = match_schemas(&conn, "api-gateway routing", &[]).unwrap();
        record_suggestions(&conn, "decision", 7, &hits).unwrap();

        let (fit, status): (f64, String) = conn
            .query_row(
                "SELECT fit, status FROM schema_suggestions WHERE item_kind = 'decision' AND item_id = 7",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(status, "suggested");
        assert!((fit - 1.0).abs() < 1e-9);
        let edges: i64 = conn
            .query_row("SELECT COUNT(*) FROM context_links", [], |r| r.get(0))
            .unwrap();
        assert_eq!(edges, 0, "suggestion must not write membership (AC-8)");

        // Re-firing refreshes fit but never flips a resolved row.
        decline_fired(&conn, "decision", 7).unwrap();
        record_suggestions(&conn, "decision", 7, &hits).unwrap();
        let status: String = conn
            .query_row(
                "SELECT status FROM schema_suggestions WHERE item_kind = 'decision' AND item_id = 7",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "declined");
    }

    #[test]
    fn attach_links_marks_accepted_and_bumps_confirmation() {
        let conn = mem_db();
        let id = gateway_schema(&conn);
        let hits = match_schemas(&conn, "api-gateway routing", &[]).unwrap();
        record_suggestions(&conn, "decision", 7, &hits).unwrap();
        let before: String = conn
            .query_row(
                "SELECT last_confirmed_at FROM schemas WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap();

        let attached = attach(&conn, "decision", 7, &id.to_string()).unwrap();
        assert_eq!(attached.schema_id, id);

        // member_of edge in the canonical graph direction and vocabulary.
        let (skind, tid, rel): (String, String, String) = conn
            .query_row(
                "SELECT source_item_type, target_item_id, relationship_type FROM context_links",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!((skind.as_str(), rel.as_str()), ("decision", "member_of"));
        assert_eq!(tid, id.to_string());

        let status: String = conn
            .query_row("SELECT status FROM schema_suggestions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(status, "accepted");
        let after: String = conn
            .query_row(
                "SELECT last_confirmed_at FROM schemas WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap();
        assert_ne!(before, after, "attach bumps confirmation recency (AC-8)");

        // Idempotent: a repeated attach keeps one edge.
        attach(&conn, "decision", 7, "gateway").unwrap();
        let edges: i64 = conn
            .query_row("SELECT COUNT(*) FROM context_links", [], |r| r.get(0))
            .unwrap();
        assert_eq!(edges, 1);
    }

    #[test]
    fn decline_is_explicit_opt_out_only() {
        let conn = mem_db();
        gateway_schema(&conn);
        // No suggestions fired: --schema none is a no-op.
        assert_eq!(decline_fired(&conn, "decision", 1).unwrap(), 0);
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_suggestions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 0);

        let hits = match_schemas(&conn, "api-gateway routing", &[]).unwrap();
        record_suggestions(&conn, "decision", 1, &hits).unwrap();
        assert_eq!(decline_fired(&conn, "decision", 1).unwrap(), 1);
    }

    #[test]
    fn schema_none_fires_then_declines_a_fresh_match() {
        let conn = mem_db();
        gateway_schema(&conn);
        // A matching fresh item opted out: the suggestion is recorded AND
        // declined — the row proves the match happened and stays resolved.
        let out = at_log_time(
            &conn,
            "decision",
            9,
            "api-gateway routing",
            &[],
            Some("none"),
        )
        .unwrap();
        assert_eq!(out["schema_declined"], 1);
        assert_eq!(out["schema_suggestions"], json!([]));
        let (status, n): (String, i64) = conn
            .query_row(
                "SELECT status, COUNT(*) FROM schema_suggestions \
                 WHERE item_kind = 'decision' AND item_id = 9",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((status, n), ("declined".to_string(), 1));
    }

    /// AC-9 stability seam: member growth from attach flows through scan's
    /// identity machinery — the enlarged cluster re-stages, and re-applying
    /// is rejected by the confirmed-Jaccard guard instead of duplicating.
    #[test]
    fn scan_after_attach_recognizes_the_schema_without_duplicating() {
        let conn = mem_db();
        let sid = gateway_schema(&conn);
        for i in 1..=3 {
            conn.execute(
                "INSERT INTO decisions (uuid, timestamp, summary) \
                 VALUES (?1, '2026-01-01T00:00:00Z', 'gateway routing note')",
                params![format!("u{i}")],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO context_links (source_item_type, source_item_id, \
                 target_item_type, target_item_id, relationship_type, timestamp) \
                 VALUES ('decision', ?1, 'decision', ?2, 'relates_to', 't')",
                params![i.to_string(), (i % 3 + 1).to_string()],
            )
            .unwrap();
        }
        // The schema's own members, so the cluster carries the centroid's
        // lexical territory and a new gateway item genuinely matches it.
        for i in 1..=3 {
            conn.execute(
                "INSERT INTO context_links (source_item_type, source_item_id, \
                 target_item_type, target_item_id, relationship_type, timestamp) \
                 VALUES ('decision', ?1, 'schema', ?2, 'member_of', 't')",
                params![i.to_string(), sid.to_string()],
            )
            .unwrap();
        }

        // New item attaches at log time (member growth through the seam).
        conn.execute(
            "INSERT INTO decisions (uuid, timestamp, summary) \
             VALUES ('u4', '2026-01-01T00:00:00Z', 'api-gateway routing')",
            [],
        )
        .unwrap();
        attach(&conn, "decision", 4, &sid.to_string()).unwrap();

        // Scan stages the enlarged cluster (three sightings to clear the
        // stability gate); scan --apply must NOT duplicate the schema — the
        // confirmed-Jaccard covenant rejects it (AC-9).
        for _ in 0..3 {
            super::super::scan::scan(&conn, false).unwrap();
        }
        let out = super::super::scan::scan(&conn, true).unwrap();
        let applied = out["applied"].as_array().unwrap();
        let skipped = out["skipped"].as_array().unwrap();
        assert!(
            applied.is_empty() && !skipped.is_empty(),
            "re-apply must be rejected, not duplicated: {out}"
        );
        let schemas: i64 = conn
            .query_row("SELECT COUNT(*) FROM schemas", [], |r| r.get(0))
            .unwrap();
        assert_eq!(schemas, 1);
        let members: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM context_links WHERE relationship_type = 'member_of' \
                 AND target_item_type = 'schema' AND target_item_id = ?1",
                params![sid],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(members, 4, "attached member survives scan untouched");
    }
}
