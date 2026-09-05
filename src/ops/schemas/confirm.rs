//! `engrams schema confirm`: promotion of a passing candidate into a
//! confirmed schema (spec 0002 formation pipeline stage 5).
//!
//! Confirm is an explicit human/agent act (AC-4): it is the only writer of
//! `schemas` rows, and the `member_of` edges it inserts carry
//! `origin = 'manual'` — direction member → schema per the `member_of`
//! ontology registered in phase 1. Members are referenced, never absorbed:
//! the underlying fact rows are untouched and stay individually retrievable
//! (AC-5).
//!
//! Name and summary are mechanical drafts (no model): the name defaults to
//! the members' dominant tag with `-N` collision suffixes, the summary is a
//! deterministic template over the member set, and `centroid_json` records
//! tag/anchor frequencies for the phase-6 lexical assimilation matcher.
//!
//! Coherence with staging: a confirmed schema's membership lives in its
//! `member_of` edges (no duplicated snapshot column). [`confirmed_schemas`]
//! exposes those member sets, and this command rejects any candidate whose
//! knowledge members J-match a confirmed schema (>= 0.7). `scan` itself
//! never consults confirmed schemas — after a confirm, the consumed
//! candidate's exact signature no longer recurs (the schema node joins the
//! graph), and an evolved cluster re-stages under a new signature at
//! stability 1, re-earning its gates. The candidate row is left in place
//! (the staging table has no consumed column) and simply goes dormant.

use std::collections::BTreeMap;

use anyhow::{bail, Context as _, Result};
use chrono::{SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use uuid::Uuid;

use super::scan::{jaccard, DENSITY_GATE, JACCARD_IDENTITY, REWARD_GATE, STABILITY_GATE};

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// One `kind:id` member reference from a candidate's member set.
pub(crate) struct Member {
    pub(crate) kind: String,
    pub(crate) id: i64,
}

impl Member {
    fn key(&self) -> String {
        format!("{}:{}", self.kind, self.id)
    }
}

/// Database table backing each member kind (the graph's node universe).
fn kind_table(kind: &str) -> Option<&'static str> {
    match kind {
        "decision" => Some("decisions"),
        "system_pattern" => Some("system_patterns"),
        "progress_entry" => Some("progress_entries"),
        "custom_data" => Some("custom_data"),
        "code" => Some("code_nodes"),
        "schema" => Some("schemas"),
        _ => None,
    }
}

/// A confirmed schema with its member set read from the `member_of` links
/// (member → schema). Membership lives in the edges, so later attaches are
/// visible here without duplicated state.
pub(crate) struct Confirmed {
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) members: Vec<String>,
}

/// All confirmed schemas with their member sets, ordered by schema id.
pub(crate) fn confirmed_schemas(conn: &Connection) -> Result<Vec<Confirmed>> {
    let mut stmt = conn.prepare(
        "SELECT s.id, s.name, l.source_item_type, l.source_item_id \
         FROM schemas s \
         JOIN context_links l ON l.relationship_type = 'member_of' \
          AND l.target_item_type = 'schema' AND l.target_item_id = s.id \
         ORDER BY s.id, l.source_item_type, l.source_item_id",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;
    let mut out: Vec<Confirmed> = Vec::new();
    for r in rows {
        let (id, name, kind, member_id) = r?;
        let key = format!("{kind}:{member_id}");
        match out.last_mut() {
            Some(c) if c.id == id => c.members.push(key),
            _ => out.push(Confirmed {
                id,
                name,
                members: vec![key],
            }),
        }
    }
    Ok(out)
}

/// The staged candidate row to promote.
pub(crate) struct Candidate {
    pub(crate) members: Vec<String>,
    density: f64,
    stability_count: i64,
    reward_hits: i64,
}

/// Resolve a candidate by exact signature first — an exact match always
/// wins even when the signature is also a prefix of longer signatures —
/// then by unique prefix. Ambiguous prefixes error with the match count; a
/// missing signature yields `None` so the caller can fall through to the
/// existing-schema bump path.
pub(crate) fn try_resolve_candidate(conn: &Connection, sig: &str) -> Result<Option<Candidate>> {
    let exact: Option<(String, f64, i64, i64)> = conn
        .query_row(
            "SELECT member_keys_json, density, stability_count, reward_hits \
             FROM schema_candidates WHERE cluster_sig = ?1",
            params![sig],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?;
    if let Some((members_json, density, stability_count, reward_hits)) = exact {
        return Ok(Some(Candidate {
            members: serde_json::from_str(&members_json)?,
            density,
            stability_count,
            reward_hits,
        }));
    }
    let mut stmt = conn.prepare(
        "SELECT cluster_sig, member_keys_json, density, stability_count, reward_hits \
         FROM schema_candidates WHERE cluster_sig LIKE ?1 ESCAPE '\\' ORDER BY cluster_sig",
    )?;
    let pattern = format!(
        "{}%",
        sig.replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    );
    let rows = stmt.query_map(params![pattern], |row| {
        Ok((
            row.get::<_, String>(1)?,
            row.get::<_, f64>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, i64>(4)?,
        ))
    })?;
    let mut matches: Vec<Candidate> = Vec::new();
    for r in rows {
        let (members_json, density, stability_count, reward_hits) = r?;
        matches.push(Candidate {
            members: serde_json::from_str(&members_json)?,
            density,
            stability_count,
            reward_hits,
        });
    }
    match matches.len() {
        0 => Ok(None),
        1 => Ok(Some(matches.remove(0))),
        n => bail!("'{sig}' is ambiguous: {n} candidates share this prefix"),
    }
}

/// Parse and validate a candidate's member strings against the backing
/// tables, so confirm never materializes ghost nodes in the graph.
pub(crate) fn parse_members(conn: &Connection, members: &[String]) -> Result<Vec<Member>> {
    members
        .iter()
        .map(|m| {
            let (kind, id) = m
                .split_once(':')
                .with_context(|| format!("malformed member key '{m}'"))?;
            let id: i64 = id
                .parse()
                .with_context(|| format!("malformed member key '{m}'"))?;
            let table =
                kind_table(kind).with_context(|| format!("unknown member kind '{kind}'"))?;
            let exists: Option<i64> = conn
                .query_row(
                    &format!("SELECT 1 FROM {table} WHERE id = ?1"),
                    params![id],
                    |r| r.get(0),
                )
                .optional()?;
            if exists.is_none() {
                bail!("member {m} has no backing row");
            }
            Ok(Member {
                kind: kind.to_string(),
                id,
            })
        })
        .collect()
}

/// Most frequent tag across member decisions/patterns (highest count,
/// lexicographic tiebreak), or `None` when members carry no tags.
fn dominant_tag(conn: &Connection, members: &[Member]) -> Result<Option<String>> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for m in members {
        let table = kind_table(&m.kind);
        if !matches!(table, Some("decisions") | Some("system_patterns")) {
            continue;
        }
        let tags: Option<Option<String>> = conn
            .query_row(
                &format!("SELECT tags FROM {} WHERE id = ?1", table.unwrap()),
                params![m.id],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(Some(t)) = tags {
            for tag in crate::models::parse_tags(Some(&t)) {
                *counts.entry(tag).or_insert(0) += 1;
            }
        }
    }
    Ok(counts
        .into_iter()
        .max_by_key(|(tag, n)| (*n, std::cmp::Reverse(tag.clone())))
        .map(|(t, _)| t))
}

/// First unused `base`, `base-2`, `base-3`, … against `schemas.name`.
pub(crate) fn unique_name(conn: &Connection, base: &str) -> Result<String> {
    let taken = |name: &str| -> Result<bool> {
        Ok(conn
            .query_row(
                "SELECT 1 FROM schemas WHERE name = ?1",
                params![name],
                |r| r.get::<_, i64>(0),
            )
            .optional()?
            .is_some())
    };
    if !taken(base)? {
        return Ok(base.to_string());
    }
    for n in 2.. {
        let candidate = format!("{base}-{n}");
        if !taken(&candidate)? {
            return Ok(candidate);
        }
    }
    unreachable!("name space exhausted")
}

/// Member-kind histogram for the drafted summary.
fn kind_histogram(members: &[Member]) -> Vec<(String, usize)> {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for m in members {
        let label = match m.kind.as_str() {
            "decision" => "decisions",
            "system_pattern" => "patterns",
            "progress_entry" => "progress entries",
            "custom_data" => "custom data",
            "code" => "code nodes",
            "schema" => "schemas",
            _ => "items",
        };
        *counts.entry(label).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .map(|(k, n)| (k.to_string(), n))
        .collect()
}

/// The mechanically drafted summary: member census, up to three member
/// summaries in member order, and the copied gate metrics. Deterministic
/// for a given member set and candidate row.
fn draft_summary(
    conn: &Connection,
    members: &[Member],
    density: f64,
    stability_count: i64,
    reward_hits: i64,
) -> Result<String> {
    let mut titles: Vec<String> = Vec::new();
    for m in members {
        if titles.len() == 3 {
            break;
        }
        let text: Option<String> = match kind_table(&m.kind) {
            Some("decisions") => conn
                .query_row(
                    "SELECT summary FROM decisions WHERE id = ?1",
                    params![m.id],
                    |r| r.get(0),
                )
                .optional()?,
            Some("system_patterns") => conn
                .query_row(
                    "SELECT description FROM system_patterns WHERE id = ?1",
                    params![m.id],
                    |r| r.get(0),
                )
                .optional()?,
            _ => None,
        };
        if let Some(t) = text {
            titles.push(t);
        }
    }
    let mut out = format!("Drafted from {} members (", members.len());
    let census = kind_histogram(members);
    for (i, (k, n)) in census.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&format!("{n} {k}"));
    }
    out.push_str("): ");
    if titles.is_empty() {
        out.push_str("no titled members");
    } else {
        for (i, t) in titles.iter().enumerate() {
            if i > 0 {
                out.push_str("; ");
            }
            out.push_str(&format!("\"{t}\""));
        }
    }
    out.push_str(&format!(
        ". Density {density:.2}, stability {stability_count}, reward hits {reward_hits}."
    ));
    Ok(out)
}

/// Tag/anchor frequencies over the member set — the lexical centroid the
/// phase-6 assimilation matcher scores new items against.
fn centroid(conn: &Connection, members: &[Member]) -> Result<String> {
    let mut tags: BTreeMap<String, usize> = BTreeMap::new();
    let mut anchors: BTreeMap<String, usize> = BTreeMap::new();
    for m in members {
        if matches!(
            kind_table(&m.kind),
            Some("decisions") | Some("system_patterns")
        ) {
            let t: Option<Option<String>> = conn
                .query_row(
                    &format!(
                        "SELECT tags FROM {} WHERE id = ?1",
                        kind_table(&m.kind).unwrap()
                    ),
                    params![m.id],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(Some(t)) = t {
                for tag in crate::models::parse_tags(Some(&t)) {
                    *tags.entry(tag).or_insert(0) += 1;
                }
            }
        }
        if matches!(
            m.kind.as_str(),
            "decision" | "system_pattern" | "progress_entry"
        ) {
            let mut stmt = conn
                .prepare("SELECT path FROM item_anchors WHERE item_type = ?1 AND item_id = ?2")?;
            let paths = stmt.query_map(params![m.kind, m.id], |r| r.get::<_, String>(0))?;
            for p in paths {
                *anchors.entry(p?).or_insert(0) += 1;
            }
        }
    }
    Ok(serde_json::to_string(&json!({
        "tags": tags,
        "anchors": anchors,
    }))?)
}

/// Promote a staged candidate or re-confirm an existing schema (spec API
/// surface). Resolution order: staged-candidate signature/prefix first, then
/// existing schema numeric id or exact name — a hit there bumps
/// `last_confirmed_at` only (no new promotion).
pub fn confirm(conn: &Connection, sig: &str, name: Option<&str>) -> Result<Value> {
    let cand = try_resolve_candidate(conn, sig)?;
    let Some(cand) = cand else {
        return bump_existing_schema(conn, sig);
    };
    let members = parse_members(conn, &cand.members)?;

    // Compare knowledge-item members only: a candidate that already
    // contains the confirmed schema's own node must not dilute its
    // Jaccard below the identity threshold.
    let knowledge_members: Vec<String> = cand
        .members
        .iter()
        .filter(|m| !m.starts_with("schema:"))
        .cloned()
        .collect();
    for s in confirmed_schemas(conn)? {
        // Territory checks: a candidate whose members sit inside — or fully
        // cover — an existing schema's knowledge members IS that schema,
        // not a new concept; both orientations dilute or tighten past the
        // J >= 0.7 identity rule, so they are pinned explicitly here.
        // Drifted/partial shapes (some overlap, some not) stay with Jaccard.
        // Claimed territory frees only via prune/archive — the v0.14
        // adaptation owns the retire path (decision 76); nothing in the
        // confirm covenant un-claims it.
        let s_knowledge: Vec<&String> = s
            .members
            .iter()
            .filter(|m| !m.starts_with("schema:"))
            .collect();
        let covered = knowledge_members
            .iter()
            .filter(|m| s_knowledge.contains(m))
            .count();
        let covers = s_knowledge
            .iter()
            .filter(|k| knowledge_members.contains(k))
            .count();
        if covered == knowledge_members.len()
            || (!s_knowledge.is_empty() && covers == s_knowledge.len())
            || jaccard(&knowledge_members, &s.members) >= JACCARD_IDENTITY
        {
            bail!(
                "candidate already confirmed as schema {} ({})",
                s.id,
                s.name
            );
        }
    }

    // The propose-confirm covenant: only gate-passing candidates promote.
    let mut failed: Vec<&str> = Vec::new();
    if cand.density < DENSITY_GATE {
        failed.push("density");
    }
    if cand.stability_count < STABILITY_GATE {
        failed.push("stability");
    }
    if cand.reward_hits < REWARD_GATE {
        failed.push("reward");
    }
    if !failed.is_empty() {
        bail!(
            "candidate has not passed the gates (failing: {}); only passing candidates confirm",
            failed.join(", ")
        );
    }

    promote(conn, &cand, &members, name)
}

/// Create the schema row with a mechanical draft, link every member with
/// `member_of`, and leave the candidate row and member rows untouched.
pub(crate) fn promote(
    conn: &Connection,
    cand: &Candidate,
    members: &[Member],
    name: Option<&str>,
) -> Result<Value> {
    let ts = now();
    let name = match name {
        Some(n) => unique_name(conn, n)?,
        None => {
            let base = dominant_tag(conn, members)?.unwrap_or_else(|| "schema".to_string());
            unique_name(conn, &base)?
        }
    };
    let summary = draft_summary(
        conn,
        members,
        cand.density,
        cand.stability_count,
        cand.reward_hits,
    )?;
    let centroid_json = centroid(conn, members)?;

    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO schemas \
         (uuid, name, summary, summary_source, status, centroid_json, last_confirmed_at, \
          created_at, updated_at) \
         VALUES (?1, ?2, ?3, 'drafted', 'active', ?4, ?5, ?5, ?5)",
        params![Uuid::new_v4().to_string(), name, summary, centroid_json, ts],
    )
    .context("inserting schema row")?;
    let schema_id = tx.last_insert_rowid();
    for m in members {
        tx.execute(
            "INSERT INTO context_links \
             (source_item_type, source_item_id, target_item_type, target_item_id, \
              relationship_type, timestamp, origin, source, weight) \
             VALUES (?1, ?2, 'schema', ?3, 'member_of', ?4, 'manual', 'schema_confirm', 1.0)",
            params![m.kind, m.id.to_string(), schema_id.to_string(), ts],
        )
        .context("inserting member_of edge")?;
    }
    tx.commit()?;

    Ok(json!({
        "status": "success",
        "schema": {
            "id": schema_id,
            "name": name,
            "summary": summary,
            "summary_source": "drafted",
            "member_count": members.len(),
            "members": members.iter().map(|m| m.key()).collect::<Vec<_>>(),
            "density": cand.density,
            "stability_count": cand.stability_count,
            "reward_hits": cand.reward_hits,
        },
    }))
}

/// The no-candidate-match arm of [`confirm`]: an existing schema id or
/// exact name re-confirms in place — `last_confirmed_at` bumps, nothing
/// else changes (spec API surface, line 195).
fn bump_existing_schema(conn: &Connection, target: &str) -> Result<Value> {
    let resolved: Option<(i64, String)> = match target.parse::<i64>() {
        Ok(id) => conn
            .query_row(
                "SELECT id, name FROM schemas WHERE id = ?1",
                params![id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?,
        Err(_) => conn
            .query_row(
                "SELECT id, name FROM schemas WHERE name = ?1",
                params![target],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?,
    };
    let Some((id, name)) = resolved else {
        bail!("no staged candidate or schema matches '{target}' (run `engrams schema scan`)");
    };
    let ts = now();
    conn.execute(
        "UPDATE schemas SET last_confirmed_at = ?1 WHERE id = ?2",
        params![ts, id],
    )?;
    Ok(json!({
        "status": "success",
        "schema": {
            "id": id,
            "name": name,
            "bumped": true,
            "last_confirmed_at": ts,
        },
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

    fn add_decision(conn: &Connection, id: i64, summary: &str, tags: &str) {
        // Real `decision log` stores tags as a JSON array.
        let stored = serde_json::to_string(
            &tags
                .split(',')
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .collect::<Vec<_>>(),
        )
        .unwrap();
        conn.execute(
            "INSERT INTO decisions (id, uuid, timestamp, summary, tags) \
             VALUES (?1, ?2, '2026-01-01T00:00:00Z', ?3, ?4)",
            params![id, format!("u{id}"), summary, stored],
        )
        .unwrap();
    }

    fn link(conn: &Connection, a: i64, b: i64) {
        conn.execute(
            "INSERT INTO context_links (source_item_type, source_item_id, target_item_type, \
             target_item_id, relationship_type, timestamp) \
             VALUES ('decision', ?1, 'decision', ?2, 'relates_to', '2026-01-01T00:00:00Z')",
            params![a, b],
        )
        .unwrap();
    }

    /// Dense passing trio staged at stability 3 by three real scans.
    fn passing_candidate(conn: &Connection) -> String {
        for (id, summary) in [
            (1, "alpha gateway"),
            (2, "beta rendering"),
            (3, "gamma policy"),
        ] {
            add_decision(conn, id, summary, "core,graph");
        }
        link(conn, 1, 2);
        link(conn, 2, 3);
        link(conn, 1, 3);
        for _ in 0..3 {
            super::super::scan::scan(conn, false).unwrap();
        }
        let sig: String = conn
            .query_row("SELECT cluster_sig FROM schema_candidates", [], |r| {
                r.get(0)
            })
            .unwrap();
        sig
    }

    fn member_of_count(conn: &Connection, schema_id: i64) -> i64 {
        conn.query_row(
            "SELECT COUNT(*) FROM context_links WHERE relationship_type = 'member_of' \
             AND target_item_type = 'schema' AND target_item_id = ?1",
            params![schema_id],
            |r| r.get(0),
        )
        .unwrap()
    }

    #[test]
    fn confirm_creates_schema_links_members_and_spares_members() {
        let conn = mem_db();
        let sig = passing_candidate(&conn);

        let members_before: Vec<String> = (1..=3)
            .map(|id| {
                conn.query_row(
                    "SELECT summary || '|' || tags FROM decisions WHERE id = ?1",
                    params![id],
                    |r| r.get(0),
                )
                .unwrap()
            })
            .collect();

        let out = confirm(&conn, &sig, Some("Gateway design")).unwrap();
        let schema = &out["schema"];
        assert_eq!(schema["name"], "Gateway design");
        assert_eq!(schema["summary_source"], "drafted");
        assert_eq!(schema["member_count"], 3);
        assert_eq!(member_of_count(&conn, schema["id"].as_i64().unwrap()), 3);

        // AC-5: members referenced, never absorbed — fact rows unchanged.
        for (id, before) in (1..=3).zip(&members_before) {
            let after: String = conn
                .query_row(
                    "SELECT summary || '|' || tags FROM decisions WHERE id = ?1",
                    params![id],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(after, *before, "decision {id} was mutated by confirm");
        }

        // Candidate row left in place (no consumed column; it goes dormant).
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_candidates", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 1);

        // Status uses the DDL vocabulary; confirm sets the timestamp.
        let (status, confirmed_at): (String, String) = conn
            .query_row("SELECT status, last_confirmed_at FROM schemas", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(status, "active");
        assert!(!confirmed_at.is_empty());
    }
    #[test]
    fn drafted_name_defaults_to_dominant_tag_and_suffixes_on_collision() {
        let conn = mem_db();
        let sig = passing_candidate(&conn); // members all tagged "core,graph"
        let out = confirm(&conn, &sig, None).unwrap();
        // "core" and "graph" tie at 3; the lexicographic tiebreak takes core.
        assert_eq!(out["schema"]["name"], "core");

        // A second candidate with the same dominant tag gets a suffix.
        add_decision(&conn, 4, "delta ingest", "core");
        add_decision(&conn, 5, "epsilon export", "core");
        link(&conn, 4, 5);
        for _ in 0..3 {
            super::super::scan::scan(&conn, false).unwrap();
        }
        let sig2: String = conn
            .query_row(
                "SELECT cluster_sig FROM schema_candidates WHERE cluster_sig LIKE 'decision:4%'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let out = confirm(&conn, &sig2, None).unwrap();
        assert_eq!(out["schema"]["name"], "core-2");
    }

    #[test]
    fn prefix_resolution_works_and_ambiguity_errors() {
        let conn = mem_db();
        let sig = passing_candidate(&conn);
        assert!(sig.starts_with("decision:1,"));

        // A unique prefix resolves to the same row.
        let out = confirm(&conn, &sig[..12], Some("Gateway design")).unwrap();
        assert_eq!(out["schema"]["member_count"], 3);

        // Two rows sharing a prefix are ambiguous (seeded directly: distinct
        // detected clusters never share a signature prefix, since identity
        // assignment updates a matched row in place).
        add_decision(&conn, 9, "zeta ninth", "");
        conn.execute(
            "INSERT INTO schema_candidates \
             (cluster_sig, member_keys_json, density, stability_count, first_seen_at, \
              last_seen_at) \
             VALUES ('decision:1,decision:2,decision:9', ?1, 1.0, 3, 't0', 't0')",
            params![serde_json::to_string(&["decision:1", "decision:2", "decision:9"]).unwrap()],
        )
        .unwrap();
        let err = confirm(&conn, &sig[..12], None).unwrap_err().to_string();
        assert!(err.contains("ambiguous"), "unexpected error: {err}");

        // The exact signature still resolves uniquely.
        let out = confirm(&conn, &sig, None).unwrap_err();
        assert!(
            out.to_string().contains("already confirmed"),
            "first confirm consumed the row: {out}"
        );
    }

    #[test]
    fn non_passing_candidate_is_rejected_with_failing_gates() {
        let conn = mem_db();
        for (id, summary) in [
            (1, "alpha gateway"),
            (2, "beta rendering"),
            (3, "gamma policy"),
        ] {
            add_decision(&conn, id, summary, "core,graph");
        }
        link(&conn, 1, 2);
        link(&conn, 2, 3);
        link(&conn, 1, 3);
        // One scan only: density passes, stability (1 of 3) does not.
        super::super::scan::scan(&conn, false).unwrap();
        let sig: String = conn
            .query_row("SELECT cluster_sig FROM schema_candidates", [], |r| {
                r.get(0)
            })
            .unwrap();

        let err = confirm(&conn, &sig, None).unwrap_err().to_string();
        assert!(err.contains("not passed the gates"), "unexpected: {err}");
        assert!(
            err.contains("stability"),
            "must name the failing gate: {err}"
        );

        // Rejection writes nothing: no schema row, no member_of edges.
        let schemas: i64 = conn
            .query_row("SELECT COUNT(*) FROM schemas", [], |r| r.get(0))
            .unwrap();
        assert_eq!(schemas, 0);
        let edges: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM context_links WHERE relationship_type = 'member_of'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(edges, 0);
    }

    #[test]
    fn scan_after_confirm_excludes_confirmed_candidate_but_keeps_members() {
        let conn = mem_db();
        let sig = passing_candidate(&conn);
        confirm(&conn, &sig, Some("gateway design")).unwrap();

        // The confirmed candidate is no longer staged for promotion.
        let out = super::super::scan::scan(&conn, false).unwrap();
        let cands = out["candidates"].as_array().unwrap();
        assert!(
            !cands
                .iter()
                .any(|c| c["cluster_sig"].as_str() == Some(sig.as_str())),
            "confirmed candidate must not re-stage: {cands:?}"
        );

        // The confirmed identity is never re-detected as such — but the
        // schema node it created now participates in the graph, so the
        // members cohere *with it* instead.
        assert_eq!(cands.len(), 1);
        assert_eq!(
            cands[0]["cluster_sig"].as_str(),
            Some("decision:1,decision:2,decision:3,schema:1")
        );
        // Member rows and their declared edges are untouched.
        let decisions: i64 = conn
            .query_row("SELECT COUNT(*) FROM decisions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(decisions, 3);
    }

    #[test]
    fn distinct_candidate_with_overlapping_members_is_rejected() {
        let conn = mem_db();
        let sig = passing_candidate(&conn);
        confirm(&conn, &sig, Some("gateway design")).unwrap();

        // A *distinct* cluster sharing 3 of 4 members (J = 0.6 < 0.7 — no;
        // share 3 of 3 plus one new = 0.75 ≥ 0.7): stage a second candidate
        // whose member set is a near-copy of the confirmed one.
        add_decision(&conn, 4, "delta ingest", "core,graph");
        link(&conn, 1, 4);
        link(&conn, 2, 4);
        link(&conn, 3, 4);
        for _ in 0..3 {
            super::super::scan::scan(&conn, false).unwrap();
        }
        let cands: Vec<String> = {
            let mut stmt = conn
                .prepare("SELECT cluster_sig FROM schema_candidates ORDER BY cluster_sig")
                .unwrap();
            let rows = stmt.query_map([], |r| r.get(0)).unwrap();
            rows.map(|r| r.unwrap()).collect()
        };
        assert_eq!(cands.len(), 2, "expected both staged rows: {cands:?}");

        // The *new* near-copy candidate: knowledge members {1,2,3,4} vs
        // confirmed {1,2,3} (its own schema node excluded): J = 3/4 >= 0.7.
        let new_sig = cands
            .iter()
            .find(|s| s.contains("decision:4"))
            .expect("new candidate staged");
        let err = confirm(&conn, new_sig, None).unwrap_err().to_string();
        assert!(
            err.contains("already confirmed"),
            "overlapping candidate must be rejected: {err}"
        );
    }

    /// Stage a candidate row directly (no Louvain): members must reference
    /// existing decision rows, gates pre-passed so only the covenant runs.
    fn stage(conn: &Connection, members: &[i64], density: f64) -> String {
        let keys: Vec<String> = members.iter().map(|id| format!("decision:{id}")).collect();
        let sig = keys.join(",");
        conn.execute(
            "INSERT INTO schema_candidates (cluster_sig, member_keys_json, density, \
             stability_count, reward_hits, first_seen_at, last_seen_at) \
             VALUES (?1, ?2, ?3, 3, 0, 't0', 't0')",
            params![sig, serde_json::to_string(&keys).unwrap(), density],
        )
        .unwrap();
        sig
    }

    #[test]
    fn exact_signature_wins_over_ambiguous_prefix() {
        let conn = mem_db();
        for id in 1..=2 {
            add_decision(&conn, id, &format!("d{id}"), "x");
        }
        // `abc` is an exact candidate AND a prefix of `abc,decision:2` — the
        // exact match must resolve, not error ambiguous.
        let sig = stage(&conn, &[1], 1.0);
        stage(&conn, &[1, 2], 1.0);
        assert_eq!(sig, "decision:1");
        let got = try_resolve_candidate(&conn, &sig).unwrap().unwrap();
        assert_eq!(got.members, vec!["decision:1".to_string()]);
    }

    #[test]
    fn strict_subset_of_confirmed_bails_already_confirmed() {
        let conn = mem_db();
        for id in 1..=3 {
            add_decision(&conn, id, &format!("d{id}"), "core");
        }
        link(&conn, 1, 2);
        link(&conn, 2, 3);
        link(&conn, 1, 3);
        let whole = stage(&conn, &[1, 2, 3], 1.0);
        confirm(&conn, &whole, Some("core")).unwrap();

        // Strict subset {1,2}: every member already claimed by core. This is
        // the branch Jaccard alone misses (J = 1/3 < 0.7) — the subset-bail
        // is what pins members-already-claimed territory.
        let part = stage(&conn, &[1, 2], 1.0);
        let err = confirm(&conn, &part, None).unwrap_err().to_string();
        assert!(
            err.contains("already confirmed"),
            "subset of a confirmed schema must bail: {err}"
        );
        let schemas: i64 = conn
            .query_row("SELECT COUNT(*) FROM schemas", [], |r| r.get(0))
            .unwrap();
        assert_eq!(schemas, 1, "no second schema row may exist");
    }

    #[test]
    fn strict_superset_of_confirmed_bails_already_confirmed() {
        // Reviewer repro: core-2 {4,5} confirmed, then a candidate {4,5,7}
        // escapes the J >= 0.7 rule by dilution (J = 2/3 < 0.7) and would
        // swallow the smaller schema as a "new" cluster. The containment
        // check mirrored onto the confirmed side rejects it.
        let conn = mem_db();
        for id in 4..=5 {
            add_decision(&conn, id, &format!("d{id}"), "infra");
        }
        link(&conn, 4, 5);
        let small = stage(&conn, &[4, 5], 1.0);
        confirm(&conn, &small, Some("core-2")).unwrap();

        add_decision(&conn, 7, "delta seven", "infra");
        link(&conn, 4, 7);
        link(&conn, 5, 7);
        let big = stage(&conn, &[4, 5, 7], 1.0);
        let err = confirm(&conn, &big, None).unwrap_err().to_string();
        assert!(
            err.contains("already confirmed"),
            "superset must not swallow the confirmed schema: {err}"
        );
        let schemas: i64 = conn
            .query_row("SELECT COUNT(*) FROM schemas", [], |r| r.get(0))
            .unwrap();
        assert_eq!(schemas, 1, "core-2 stays the only schema");
    }

    #[test]
    fn overlap_without_containment_still_promotes() {
        // Safe direction of the mirrored coverage clause: plain overlap is
        // NOT territory — confirmed {1,2} vs candidate {1,3,4} has
        // covered = 1/3, covers = 1/2, J = 1/4, so it promotes as its own
        // schema. Over-firing here would strangle legitimate distinct
        // concepts that share one member.
        let conn = mem_db();
        for id in 1..=4 {
            add_decision(&conn, id, &format!("d{id}"), "x");
        }
        let first = stage(&conn, &[1, 2], 1.0);
        confirm(&conn, &first, Some("core")).unwrap();

        let second = stage(&conn, &[1, 3, 4], 1.0);
        let out = confirm(&conn, &second, Some("adjacent")).unwrap();
        assert_eq!(out["schema"]["name"], "adjacent");
        let schemas: i64 = conn
            .query_row("SELECT COUNT(*) FROM schemas", [], |r| r.get(0))
            .unwrap();
        assert_eq!(schemas, 2, "overlapping-but-distinct promotes");
    }
}
