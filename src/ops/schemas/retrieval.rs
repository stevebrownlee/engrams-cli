//! Retrieval tiering and surfacing telemetry (spec 0002, AC-7 / AC-10).
//!
//! One surfacing event is a `(ts, cmd, arg)` key: every node surfaced in that
//! call gets a `retrieval_surfaces` row — the schemas surfaced plus their
//! co-surfaced records — so the detection overlay's `(ts, cmd, arg)` grouping
//! sees each co-observation directly. The table is rolling-window pruned:
//! [`SURFACE_WINDOW_DAYS`] bounds storage with no schema change.
//!
//! Ranking here is reward-hits-then-centrality (spec Architecture line 123):
//! reward hits come straight off this telemetry table; centrality is the
//! schema's in-degree in `context_links` (its `member_of` membership), a
//! deterministic one-query proxy for graph degree at KB scale.

use std::collections::HashMap;

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Map, Value};

/// Rolling window for `retrieval_surfaces`. The spec names no length; 90 days
/// keeps the co-observation ramp (cap 5) and the reward gate fed while bound-
/// ing storage. Frozen constant; recorded with the phase commit.
pub const SURFACE_WINDOW_DAYS: i64 = 90;
/// Spec line 123: prime leads with a schema block of at most K=3, one line each.
pub const PRIME_SCHEMA_K: usize = 3;
/// Co-surfaced rows recorded per event. Schemas are always recorded in full;
/// this caps the surrounding result set so a fat prime payload cannot flood
/// the table (the overlay curve saturates at 5 co-observations anyway).
pub const CO_SURFACE_CAP: usize = 20;

/// Record one surfacing event: a row per surfaced schema plus up to
/// [`CO_SURFACE_CAP`] co-surfaced `(kind, id)` records, all sharing the
/// event key. Prunes the rolling window in the same statement batch.
pub fn record_surface(
    conn: &Connection,
    cmd: &str,
    arg: Option<&str>,
    schema_ids: &[i64],
    co_surfaced: &[(&str, i64)],
) -> Result<()> {
    let ts = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let tx = conn.unchecked_transaction()?;
    let mut stmt = tx.prepare(
        "INSERT INTO retrieval_surfaces (ts, cmd, arg, node_kind, node_id) \
         VALUES (?1, ?2, ?3, 'schema', ?4)",
    )?;
    for id in schema_ids {
        stmt.execute(rusqlite::params![ts, cmd, arg, id])?;
    }
    drop(stmt);
    let mut stmt = tx.prepare(
        "INSERT INTO retrieval_surfaces (ts, cmd, arg, node_kind, node_id) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;
    for (kind, id) in co_surfaced.iter().take(CO_SURFACE_CAP) {
        stmt.execute(rusqlite::params![ts, cmd, arg, kind, id])?;
    }
    drop(stmt);
    prune(&tx)?;
    tx.commit()?;
    Ok(())
}

/// Delete surface rows older than [`SURFACE_WINDOW_DAYS`]; returns the count.
/// Both paths (record-embedded and standalone) keep storage bounded.
pub fn prune(conn: &Connection) -> Result<usize> {
    let cutoff = (chrono::Utc::now() - chrono::Duration::days(SURFACE_WINDOW_DAYS))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let n = conn.execute(
        "DELETE FROM retrieval_surfaces WHERE ts < ?1",
        rusqlite::params![cutoff],
    )?;
    Ok(n)
}

/// Reward hits per schema id: surface rows recorded for that schema node.
/// The full map — schema sets are tiny at KB scale; callers pick their ids.
pub fn reward_hits(conn: &Connection) -> Result<HashMap<i64, i64>> {
    let mut stmt = conn.prepare(
        "SELECT node_id, COUNT(*) FROM retrieval_surfaces \
         WHERE node_kind = 'schema' GROUP BY node_id",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?;
    rows.collect::<std::result::Result<HashMap<_, _>, _>>()
        .map_err(Into::into)
}

/// Prime's leading schema block: top-K (K = [`PRIME_SCHEMA_K`]) confirmed
/// schemas, ranked by reward hits, then member centrality, then agent-authored
/// above drafts, then id. One flat line each — agent drafts are flagged via
/// `summary_source`, never hidden. Each entry reports its `kind` (spec 0004).
pub fn prime_block(conn: &Connection, k: usize) -> Result<Vec<Value>> {
    let mut stmt = conn.prepare(
        "SELECT s.id, s.name, s.summary, s.summary_source, s.status, s.kind \
         FROM schemas s WHERE s.status = 'active'",
    )?;
    let reward = reward_hits(conn)?;
    let members = super::list::member_counts(conn)?;
    let rows = stmt.query_map([], |r| {
        Ok(RankedSchema {
            id: r.get(0)?,
            name: r.get(1)?,
            summary: r.get(2)?,
            summary_source: r.get(3)?,
            status: r.get(4)?,
            kind: r.get(5)?,
        })
    })?;
    let mut ranked: Vec<(i64, i64, RankedSchema)> = Vec::new();
    for r in rows {
        let s: RankedSchema = r?;
        let hits = reward.get(&s.id).copied().unwrap_or(0);
        let centrality = members.get(&s.id).copied().unwrap_or(0);
        ranked.push((hits, centrality, s));
    }
    ranked.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| b.1.cmp(&a.1))
            .then_with(|| agent_rank(&a.2).cmp(&agent_rank(&b.2)))
            .then_with(|| a.2.id.cmp(&b.2.id))
    });
    Ok(ranked
        .into_iter()
        .take(k)
        .map(|(hits, centrality, s)| {
            json!({
                "id": s.id,
                "name": s.name,
                "summary": s.summary,
                "summary_source": s.summary_source,
                "status": s.status,
                "kind": s.kind,
                "reward_hits": hits,
                "member_count": centrality,
            })
        })
        .collect())
}

struct RankedSchema {
    id: i64,
    name: String,
    summary: String,
    summary_source: String,
    status: String,
    kind: String,
}

fn agent_rank(s: &RankedSchema) -> i32 {
    if s.summary_source == "agent" {
        0
    } else {
        1
    }
}

/// One convention-aware centroid hit: a schema whose centroid tags overlap the
/// query's word parts. `overlap` counts matched parts; rank is overlap, then
/// agent-authored, then reward hits, then id. Empty on no overlap — callers
/// fall through unchanged (AC-7).
pub fn centroid_match(conn: &Connection, query: &str, k: usize) -> Result<Vec<Value>> {
    let parts: std::collections::BTreeSet<String> =
        crate::ops::ident_parts(query).into_iter().collect();
    if parts.is_empty() {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(
        "SELECT s.id, s.name, s.summary, s.summary_source, s.centroid_json \
         FROM schemas s WHERE s.status = 'active'",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
        ))
    })?;
    let reward = reward_hits(conn)?;
    let mut hits: Vec<(usize, i32, i64, i64, Value)> = Vec::new(); // overlap, agent, reward, id, row
    for r in rows {
        let (id, name, summary, source, centroid_json) = r?;
        let centroid: Value = serde_json::from_str(&centroid_json).unwrap_or(Value::Null);
        let mut tag_parts: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        // `centroid()` writes tags as an object of tag → member count; the
        // keys are the vocabulary scored against the query.
        if let Some(tags) = centroid["tags"].as_object() {
            for t in tags.keys() {
                tag_parts.extend(crate::ops::ident_parts(t));
            }
        }
        if tag_parts.is_empty() {
            continue;
        }
        let overlap = parts.intersection(&tag_parts).count();
        if overlap == 0 {
            continue;
        }
        let agent = if source == "agent" { 0 } else { 1 };
        let reward = reward.get(&id).copied().unwrap_or(0);
        hits.push((
            overlap,
            agent,
            reward,
            id,
            json!({
                "type": "schema",
                "id": id,
                "name": name,
                "summary": summary,
                "summary_source": source,
                "overlap": overlap,
            }),
        ));
    }
    hits.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| b.2.cmp(&a.2))
            .then_with(|| a.3.cmp(&b.3))
    });
    Ok(hits.into_iter().take(k).map(|h| h.4).collect())
}

/// Composite `brief` payload for a schema node (AC-7): the schema row plus
/// per-kind enriched members (capped, with truncation reported when the cap
/// bites — spec 0005 Architecture item 2) and reward telemetry — everything
/// an agent needs to act on the concept without a second call.
pub fn node_payload(conn: &Connection, id: i64) -> Result<Value> {
    use anyhow::Context as _;
    let row: (String, String, String, String, String) = conn
        .query_row(
            "SELECT name, summary, summary_source, status, centroid_json \
             FROM schemas WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .optional()?
        .with_context(|| format!("schema {id} not found"))?;
    let centroid: Value = serde_json::from_str(&row.4).unwrap_or(Value::Null);
    let reward = reward_hits(conn)?.get(&id).copied().unwrap_or(0);
    let refs = member_refs(conn, id)?;
    let total = refs.len();
    let cap = total.min(MEMBER_ENRICHMENT_CAP);
    let members = enrich_members(conn, &refs[..cap])?;
    let mut payload = json!({
        "id": id,
        "name": row.0,
        "summary": row.1,
        "summary_source": row.2,
        "status": row.3,
        "centroid": centroid,
        "member_count": total,
        "members": members,
        "truncated": cap < total,
        "anchors": member_anchors(conn, id)?,
        "related_schemas": neighborhood(conn, id)?,
        "reward_hits": reward,
    });
    if cap < total {
        payload["total_members"] = json!(total);
    }
    Ok(payload)
}

/// Immediate member references of a schema in the link query's canonical
/// (kind, textual id) order — the order `schema show` and `brief
/// schema:<id>` both project (spec 0005 Architecture item 2).
pub(crate) fn member_refs(conn: &Connection, schema_id: i64) -> Result<Vec<(String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT source_item_type, source_item_id FROM context_links \
         WHERE relationship_type = 'member_of' AND target_item_type = 'schema' \
         AND target_item_id = ?1 ORDER BY source_item_type, source_item_id",
    )?;
    let members = stmt
        .query_map([schema_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(members)
}

/// Cap on enriched members in agent-facing schema reads (`schema show`,
/// `brief schema:<id>` — spec 0005 Architecture items 1-2): inventory
/// schemas can carry hundreds of members and the read path is a single
/// agent-facing call, so past the cap the payload reports
/// `truncated`/`total_members` instead of more rows.
pub(crate) const MEMBER_ENRICHMENT_CAP: usize = 20;

/// The member kinds enriched schema reads detail — the graph node universe
/// [`kind_table`](super::confirm::kind_table) backs, spelled exactly as
/// stored in `context_links` (F-5: `code`, not `code_node`). Anything else
/// passes through bare.
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

/// Enrich member references with the per-kind details both `schema show`
/// and `brief schema:<id>` surface (spec 0005 Architecture items 1-2):
/// decisions and patterns gain status, contract/severity and anchors; code
/// gains the symbol signature; progress entries gain status; custom data
/// gains its key and value (JSON when parseable); nested schemas gain name
/// and summary. Members with no backing row — or an unenrichable kind like
/// `pr` — stay bare references. The detail maps die in this call, so
/// entries are moved out, never cloned (rule 004).
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
/// Distinct anchor paths across the schema's anchored members.
pub fn member_anchors(conn: &Connection, schema_id: i64) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT a.path FROM item_anchors a \
         JOIN context_links l ON l.relationship_type = 'member_of' \
          AND l.target_item_type = 'schema' AND l.target_item_id = ?1 \
          AND l.source_item_type = a.item_type \
          AND l.source_item_id = CAST(a.item_id AS TEXT) \
         ORDER BY a.path",
    )?;
    let rows = stmt.query_map([schema_id], |r| r.get::<_, String>(0))?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Into::into)
}

/// The schema neighborhood: other schemas sharing at least one member, most
/// shared first. Bounded at 5 — a hint, not a walk.
pub fn neighborhood(conn: &Connection, schema_id: i64) -> Result<Vec<Value>> {
    let mut stmt = conn.prepare(
        "SELECT s.id, s.name, COUNT(*) AS shared \
         FROM schemas s \
         JOIN context_links l ON l.relationship_type = 'member_of' \
          AND l.target_item_type = 'schema' AND l.target_item_id = s.id \
         WHERE s.id != ?1 AND l.source_item_type || ':' || l.source_item_id IN \
           (SELECT source_item_type || ':' || source_item_id FROM context_links \
            WHERE relationship_type = 'member_of' AND target_item_type = 'schema' \
            AND target_item_id = ?1) \
         GROUP BY s.id ORDER BY shared DESC, s.id LIMIT 5",
    )?;
    let rows = stmt.query_map([schema_id], |r| {
        Ok(json!({
            "id": r.get::<_, i64>(0)?,
            "name": r.get::<_, String>(1)?,
            "shared_members": r.get::<_, i64>(2)?,
        }))
    })?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Into::into)
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

    fn schema_with_tags(conn: &Connection, id: i64, name: &str, source: &str, tags: &[&str]) {
        // Same shape `confirm()`'s centroid() writes: tag → member count.
        let mut counts = serde_json::Map::new();
        for t in tags {
            counts.insert((*t).to_string(), json!(1));
        }
        conn.execute(
            "INSERT INTO schemas (uuid, name, summary, summary_source, status, centroid_json, \
             created_at, updated_at) VALUES (?1, ?2, 's', ?3, 'active', ?4, 't0', 't0')",
            rusqlite::params![
                format!("u-{name}"),
                name,
                source,
                serde_json::to_string(&json!({ "tags": counts, "anchors": {} })).unwrap(),
            ],
        )
        .unwrap();
        let _ = id;
    }

    fn member(conn: &Connection, schema: i64, kind: &str, id: i64) {
        conn.execute(
            "INSERT INTO context_links (source_item_type, source_item_id, target_item_type, \
             target_item_id, relationship_type, timestamp) \
             VALUES (?1, ?2, 'schema', ?3, 'member_of', 't')",
            rusqlite::params![kind, id.to_string(), schema.to_string()],
        )
        .unwrap();
    }

    #[test]
    fn record_surface_groups_one_event_and_prunes_old_rows() {
        let conn = mem_db();
        // An old row outside the window.
        conn.execute(
            "INSERT INTO retrieval_surfaces (ts, cmd, arg, node_kind, node_id) \
             VALUES ('2020-01-01T00:00:00Z', 'query', 'x', 'schema', 1)",
            [],
        )
        .unwrap();

        record_surface(&conn, "query", Some("gateway"), &[1], &[("decision", 7)]).unwrap();

        let total: i64 = conn
            .query_row("SELECT COUNT(*) FROM retrieval_surfaces", [], |r| r.get(0))
            .unwrap();
        assert_eq!(total, 2, "old row pruned, two event rows remain: {total}");
        let rows: Vec<(String, String, i64)> = {
            let mut stmt = conn
                .prepare("SELECT ts, node_kind, node_id FROM retrieval_surfaces ORDER BY node_id")
                .unwrap();
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get::<_, i64>(2)?)))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        };
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].1, "schema");
        assert_eq!(rows[1].1, "decision");
        assert_eq!(rows[0].0, rows[1].0, "co-surfaced rows share the event ts");
    }

    #[test]
    fn prime_block_ranks_reward_then_centrality_then_agent() {
        let conn = mem_db();
        schema_with_tags(&conn, 1, "core", "drafted", &["core"]);
        schema_with_tags(&conn, 2, "cli", "agent", &["cli"]);
        schema_with_tags(&conn, 3, "render", "drafted", &["render"]);
        member(&conn, 1, "decision", 1);
        member(&conn, 1, "decision", 2);
        member(&conn, 2, "decision", 3);
        member(&conn, 3, "decision", 4);
        member(&conn, 3, "decision", 5);
        // reward: 2 hits for schema 2, 1 for schema 1, 0 for schema 3.
        for _ in 0..2 {
            record_surface(&conn, "query", Some("x"), &[2], &[]).unwrap();
        }
        record_surface(&conn, "prime", None, &[1], &[]).unwrap();

        let block = prime_block(&conn, 3).unwrap();
        let ids: Vec<i64> = block.iter().map(|s| s["id"].as_i64().unwrap()).collect();
        assert_eq!(
            ids,
            vec![2, 1, 3],
            "reward hits dominate: 2 (2 hits), 1 (1 hit), 3 (0 hits)"
        );
        assert_eq!(block[0]["reward_hits"], 2);
        assert_eq!(block[0]["summary_source"], "agent");
    }

    #[test]
    fn prime_block_surfaces_kind_from_schemas_table() {
        let conn = mem_db();
        schema_with_tags(&conn, 1, "routine", "drafted", &["routine"]);
        schema_with_tags(&conn, 2, "pile", "drafted", &["pile"]);
        // The scan's kind pass + confirm copy stamp labeled rows; a row that
        // predates a relabel keeps the v14 default and must display as-is.
        conn.execute(
            "UPDATE schemas SET kind = 'story', kind_reasons_json = ?1 \
             WHERE name = 'routine'",
            rusqlite::params![serde_json::to_string(&["One awake stretch."]).unwrap()],
        )
        .unwrap();

        let block = prime_block(&conn, 3).unwrap();
        let by_name = |name: &str| block.iter().find(|s| s["name"] == name).unwrap().clone();
        assert_eq!(by_name("routine")["kind"], "story");
        assert_eq!(by_name("pile")["kind"], "unclear");
    }

    #[test]
    fn centroid_match_is_convention_aware_and_ranked() {
        let conn = mem_db();
        schema_with_tags(&conn, 1, "gateway", "drafted", &["api_gateway"]);
        schema_with_tags(&conn, 2, "render", "agent", &["render pipeline"]);

        // camelCase and kebab-case queries match snake_case tags.
        let hits = centroid_match(&conn, "ApiGateway routing", 3).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0]["name"], "gateway");

        let hits = centroid_match(&conn, "render-pipeline", 3).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0]["name"], "render");

        // No overlap: empty — the caller falls through unchanged.
        assert!(centroid_match(&conn, "unrelated wibble", 3)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn composite_helpers_return_members_anchors_and_neighborhood() {
        let conn = mem_db();
        schema_with_tags(&conn, 1, "core", "agent", &["core"]);
        schema_with_tags(&conn, 2, "side", "drafted", &["side"]);
        member(&conn, 1, "decision", 1);
        member(&conn, 1, "decision", 2);
        member(&conn, 2, "decision", 2); // shared member → neighborhood
        conn.execute(
            "INSERT INTO item_anchors (item_type, item_id, path, timestamp) \
             VALUES ('decision', 1, 'src/a.rs', 't')",
            [],
        )
        .unwrap();

        let members = member_refs(&conn, 1).unwrap();
        assert_eq!(
            members,
            vec![
                ("decision".to_string(), "1".to_string()),
                ("decision".to_string(), "2".to_string())
            ]
        );

        let anchors = member_anchors(&conn, 1).unwrap();
        assert_eq!(anchors, vec!["src/a.rs".to_string()]);

        let hood = neighborhood(&conn, 1).unwrap();
        assert_eq!(hood.len(), 1);
        assert_eq!(hood[0]["id"], 2);
        assert_eq!(hood[0]["shared_members"], 1);
    }
}
