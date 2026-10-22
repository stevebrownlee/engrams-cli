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
//! The candidate's kind label rides along as a snapshot (spec 0004): the
//! resolved candidate's `kind` and `kind_reasons_json` are copied as-is
//! into the new schema row — staged rows are mutable and prunable, so the
//! confirmed pack keeps the label it was confirmed with.
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

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// One `kind:id` member reference from a candidate's member set.
pub(crate) struct Member {
    pub(crate) kind: String,
    pub(crate) id: i64,
}

pub(super) use crate::models::kind_table;

/// A confirmed schema with its member set read from the `member_of` links
/// (member → schema). Membership lives in the edges, so later attaches are
/// visible here without duplicated state.
pub(crate) struct Confirmed {
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) members: Vec<String>,
}

/// All confirmed schemas with their member sets, ordered by schema id.
/// Members arrive ascending-sorted — the binary-collation ORDER BY over
/// (kind, id) equals key-string order for the fixed kind vocabulary —
/// which is the sortedness `jaccard`'s merge walk requires.
pub(crate) fn confirmed_schemas(conn: &Connection) -> Result<Vec<Confirmed>> {
    let mut stmt = conn.prepare(
        "SELECT s.id, s.name, l.source_item_type, l.source_item_id \
         FROM schemas s \
         JOIN context_links l ON l.relationship_type = 'member_of' \
          AND l.target_item_type = 'schema' AND l.target_item_id = s.id \
         WHERE s.status = 'active' \
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

/// A staged candidate row resolved from `schema_candidates`.
pub(crate) struct Candidate {
    pub(crate) cluster_sig: String,
    pub(crate) members: Vec<String>,
}

/// Resolve a candidate by exact signature first — an exact match always
/// wins even when the signature is also a prefix of longer signatures —
/// then by unique prefix. Ambiguous prefixes error with the match count; a
/// missing signature yields `None` so the caller can fall through to the
/// existing-schema bump path.
pub(crate) fn try_resolve_candidate(conn: &Connection, sig: &str) -> Result<Option<Candidate>> {
    let exact: Option<String> = conn
        .query_row(
            "SELECT member_keys_json FROM schema_candidates WHERE cluster_sig = ?1",
            params![sig],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(members_json) = exact {
        return Ok(Some(Candidate {
            cluster_sig: sig.to_string(),
            members: serde_json::from_str(&members_json)?,
        }));
    }
    let mut stmt = conn.prepare(
        "SELECT cluster_sig, member_keys_json \
         FROM schema_candidates WHERE cluster_sig LIKE ?1 ESCAPE '\\' ORDER BY cluster_sig",
    )?;
    let pattern = format!(
        "{}%",
        sig.replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    );
    let rows = stmt.query_map(params![pattern], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut matches: Vec<Candidate> = Vec::new();
    for r in rows {
        let (cluster_sig, members_json) = r?;
        matches.push(Candidate {
            cluster_sig,
            members: serde_json::from_str(&members_json)?,
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
    let mut out = Vec::with_capacity(members.len());
    for m in members {
        let (kind, id_str) = m
            .split_once(':')
            .with_context(|| format!("malformed member key '{m}'"))?;
        let Some(table) = kind_table(kind) else {
            // Non-entity nodes (e.g. pull request nodes `pr:https://...`) are
            // external graph anchors that do not have integer-keyed entity backing
            // tables in SQLite. Skip them so external references never block promotion.
            continue;
        };
        let id: i64 = id_str
            .parse()
            .with_context(|| format!("malformed integer id in member key '{m}'"))?;
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
        out.push(Member {
            kind: kind.to_string(),
            id,
        });
    }
    if out.is_empty() && !members.is_empty() {
        bail!("candidate has no database entity members to promote");
    }
    Ok(out)
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
/// Tag/anchor frequencies over the member set — the lexical centroid the
/// phase-6 assimilation matcher scores new items against.
pub(crate) fn centroid(conn: &Connection, members: &[Member]) -> Result<String> {
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

/// Confirm a pending proposal (`proposal:<id>`) or re-confirm an existing
/// schema (spec 0007 API surface item 2). Detector candidate signatures are
/// refused with the `schema propose --from` command (AC-8).
#[allow(clippy::too_many_arguments)]
pub fn confirm(
    conn: &Connection,
    target: &str,
    name: Option<&str>,
    statement: Option<&str>,
    summary: Option<&str>,
    trigger: Option<&str>,
    rule: Option<&str>,
    scope_tags: Option<&[String]>,
    scope_paths: Option<&[String]>,
) -> Result<Value> {
    let trimmed = target.trim();
    if trimmed.starts_with("proposal:") {
        let Some(proposal_id) = super::propose::parse_proposal_target(trimmed) else {
            bail!("invalid proposal target '{trimmed}' (expected proposal:<id>)");
        };
        return confirm_proposal(
            conn,
            proposal_id,
            name,
            statement,
            summary,
            trigger,
            rule,
            scope_tags,
            scope_paths,
        );
    }

    if try_resolve_candidate(conn, trimmed)
        .ok()
        .flatten()
        .is_some()
        || trimmed.contains(':')
    {
        bail!(
            "detector groups are evidence, not schemas: run engrams schema propose --from {trimmed} --statement ... --summary ..."
        );
    }

    if name.is_some()
        || statement.is_some()
        || summary.is_some()
        || trigger.is_some()
        || rule.is_some()
        || scope_tags.is_some()
        || scope_paths.is_some()
    {
        bail!("edit flags are only valid when confirming a proposal (proposal:<id>)");
    }

    bump_existing_schema(conn, trimmed)
}

#[allow(clippy::too_many_arguments)]
fn confirm_proposal(
    conn: &Connection,
    proposal_id: i64,
    name: Option<&str>,
    statement: Option<&str>,
    summary: Option<&str>,
    trigger: Option<&str>,
    rule: Option<&str>,
    scope_tags: Option<&[String]>,
    scope_paths: Option<&[String]>,
) -> Result<Value> {
    let row = conn
        .query_row(
            "SELECT name, statement, summary, scope_json, members_json, contrary_json, \
                    source_kind, source_ref, session, status, created_at \
             FROM schema_proposals WHERE id = ?1",
            params![proposal_id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, Option<String>>(7)?,
                    r.get::<_, Option<String>>(8)?,
                    r.get::<_, String>(9)?,
                    r.get::<_, String>(10)?,
                ))
            },
        )
        .optional()?;
    let Some((
        stored_name,
        stored_statement,
        stored_summary,
        stored_scope_json,
        stored_members_json,
        stored_contrary_json,
        source_kind,
        source_ref,
        session,
        status,
        created_at,
    )) = row
    else {
        bail!("proposal {proposal_id} does not exist");
    };

    if status != super::propose::ProposalStatus::Proposed.as_str() {
        bail!("proposal {proposal_id} is already {status}");
    }

    let stored_scope: super::propose::Scope =
        serde_json::from_str(&stored_scope_json).unwrap_or_default();
    let stored_members: Vec<String> =
        serde_json::from_str(&stored_members_json).unwrap_or_default();
    let stored_contrary: Vec<String> =
        serde_json::from_str(&stored_contrary_json).unwrap_or_default();

    let eff_name = name
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(&stored_name);
    let eff_statement = statement.unwrap_or(&stored_statement);
    let (eff_summary, eff_trigger, eff_rule) =
        if summary.is_some() || trigger.is_some() || rule.is_some() {
            (summary, trigger, rule)
        } else {
            (Some(stored_summary.as_str()), None, None)
        };
    let eff_tags = scope_tags.unwrap_or(&stored_scope.tags);
    let eff_paths = scope_paths.unwrap_or(&stored_scope.paths);

    // Re-run checks 1–6 on the possibly edited values (skipping self-overlap checks 7–8 per F-1).
    let validated = super::propose::validate_proposal(
        conn,
        eff_statement,
        eff_summary,
        eff_trigger,
        eff_rule,
        &stored_members,
        &stored_contrary,
        Some(eff_tags),
        Some(eff_paths),
        Some(eff_name),
    )?;

    // Re-run check 9: active-schema territory check.
    if let Some((schema_id, schema_name)) =
        super::gates::claimed_territory(conn, &validated.members)?
    {
        bail!("notes already covered by schema {schema_id} ({schema_name})");
    }

    let final_name = unique_name(conn, &validated.name)?;
    let renamed_from = (final_name != validated.name).then(|| validated.name.clone());

    let members = parse_members(conn, &validated.members)?;
    let centroid_json = centroid(conn, &members)?;
    let scope_json = serde_json::to_string(&validated.scope)?;
    let kind_reasons_json: String = match source_ref.as_deref() {
        Some(sig) if source_kind == "candidate" => conn
            .query_row(
                "SELECT kind_reasons_json FROM schema_candidates WHERE cluster_sig = ?1",
                params![sig],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or_else(|| "[]".to_string()),
        _ => "[]".to_string(),
    };
    let ts = now();

    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO schemas \
         (uuid, name, summary, summary_source, statement, scope_json, centroid_json, \
          status, kind, kind_reasons_json, last_confirmed_at, created_at, updated_at) \
         VALUES (?1, ?2, ?3, 'agent', ?4, ?5, ?6, 'active', 'schema', ?7, ?8, ?8, ?8)",
        params![
            Uuid::new_v4().to_string(),
            final_name,
            validated.summary,
            validated.statement,
            scope_json,
            centroid_json,
            kind_reasons_json,
            ts,
        ],
    )
    .context("inserting schema row")?;
    let schema_id = tx.last_insert_rowid();

    for m in &members {
        tx.execute(
            "INSERT INTO context_links \
             (source_item_type, source_item_id, target_item_type, target_item_id, \
              relationship_type, timestamp, origin, source, weight) \
             VALUES (?1, ?2, 'schema', ?3, 'member_of', ?4, 'manual', 'schema_confirm', 1.0)",
            params![m.kind, m.id.to_string(), schema_id.to_string(), ts],
        )
        .context("inserting member_of edge")?;
    }

    tx.execute(
        "UPDATE schema_proposals \
         SET name = ?1, statement = ?2, summary = ?3, scope_json = ?4, \
             status = ?5, schema_id = ?6, decided_at = ?7 \
         WHERE id = ?8",
        params![
            final_name,
            validated.statement,
            validated.summary,
            scope_json,
            super::propose::ProposalStatus::Confirmed.as_str(),
            schema_id,
            ts,
            proposal_id,
        ],
    )
    .context("updating confirmed proposal")?;
    tx.commit()?;

    let origin = json!({
        "proposal_id": proposal_id,
        "source": {
            "kind": source_kind,
            "ref": source_ref,
        },
        "proposed_at": created_at,
        "confirmed_at": ts,
        "session": session,
    });

    let mut out = json!({
        "status": "success",
        "schema": {
            "id": schema_id,
            "name": final_name,
            "statement": validated.statement,
            "summary": validated.summary,
            "summary_source": "agent",
            "scope": validated.scope,
            "kind": "schema",
            "member_count": validated.members.len(),
            "members": validated.members,
            "origin": origin,
        },
        "proposal": {
            "id": proposal_id,
            "status": super::propose::ProposalStatus::Confirmed.as_str(),
        },
    });
    if let Some(orig) = renamed_from {
        out["renamed_from"] = json!(orig);
    }
    Ok(out)
}

/// Re-confirm an existing schema id or exact name in place:
/// `last_confirmed_at` bumps, nothing else changes.
fn bump_existing_schema(conn: &Connection, target: &str) -> Result<Value> {
    let resolved: Option<(i64, String, String)> = match target.parse::<i64>() {
        Ok(id) => conn
            .query_row(
                "SELECT id, name, kind FROM schemas WHERE id = ?1",
                params![id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?,
        Err(_) => conn
            .query_row(
                "SELECT id, name, kind FROM schemas WHERE name = ?1",
                params![target],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?,
    };
    let Some((id, name, kind_b)) = resolved else {
        bail!("no schema matches '{target}'");
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
            "kind": kind_b,
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

    fn add_decision_with_anchor(
        conn: &Connection,
        id: i64,
        summary: &str,
        tags: &str,
        ts: &str,
        path: &str,
    ) {
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
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, format!("u{id}"), ts, summary, stored],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO item_anchors (item_type, item_id, path, timestamp) \
             VALUES ('decision', ?1, ?2, ?3)",
            params![id, path, ts],
        )
        .unwrap();
    }

    fn seed_three_notes(conn: &Connection) {
        add_decision_with_anchor(
            conn,
            1,
            "alpha gateway routing",
            "core,graph",
            "2026-01-01T00:00:00Z",
            "src/gateway/a.rs",
        );
        add_decision_with_anchor(
            conn,
            2,
            "beta rendering pipeline",
            "core,graph",
            "2026-01-02T00:00:00Z",
            "src/gateway/b.rs",
        );
        add_decision_with_anchor(
            conn,
            3,
            "gamma policy engine",
            "core,graph",
            "2026-01-03T00:00:00Z",
            "src/gateway/c.rs",
        );
    }

    fn propose_default(conn: &Connection, name: Option<&str>) -> i64 {
        let members = vec![
            "decision:1".to_string(),
            "decision:2".to_string(),
            "decision:3".to_string(),
        ];
        let out = super::super::propose::propose(
            conn,
            "Gateway routing, rendering, and policy share one core pipeline",
            Some("When touching gateway routing: follow the shared core pipeline"),
            None,
            None,
            &members,
            None,
            None,
            None,
            name,
            None,
            None,
            &[],
            Some("ses-test"),
            false,
        )
        .unwrap();
        out["proposal"]["id"].as_i64().unwrap()
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
    fn confirm_proposal_creates_agent_schema_links_members_and_records_origin() {
        let conn = mem_db();
        seed_three_notes(&conn);
        let pid = propose_default(&conn, Some("gateway-design"));

        let out = confirm(
            &conn,
            &format!("proposal:{pid}"),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();

        assert_eq!(out["status"], "success");
        assert_eq!(out["proposal"]["id"], pid);
        assert_eq!(out["proposal"]["status"], "confirmed");
        assert!(out.get("renamed_from").is_none());

        let schema = &out["schema"];
        let schema_id = schema["id"].as_i64().unwrap();
        assert_eq!(schema["name"], "gateway-design");
        assert_eq!(
            schema["statement"],
            "Gateway routing, rendering, and policy share one core pipeline"
        );
        assert_eq!(
            schema["summary"],
            "When touching gateway routing: follow the shared core pipeline"
        );
        assert_eq!(schema["summary_source"], "agent");
        assert_eq!(schema["kind"], "schema");
        assert_eq!(
            schema["scope"],
            json!({"tags": ["core", "graph"], "paths": ["src/gateway"]})
        );
        assert_eq!(
            schema["members"],
            json!(["decision:1", "decision:2", "decision:3"])
        );
        assert_eq!(schema["origin"]["proposal_id"], pid);
        assert_eq!(schema["origin"]["source"]["kind"], "manual");
        assert_eq!(schema["origin"]["session"], "ses-test");
        assert_eq!(member_of_count(&conn, schema_id), 3);

        // Confirming again fails with already confirmed.
        let err = confirm(
            &conn,
            &format!("proposal:{pid}"),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("proposal 1 is already confirmed"), "{err}");
    }

    #[test]
    fn confirm_proposal_applies_inline_edits_and_preserves_unedited_scope_half() {
        let conn = mem_db();
        seed_three_notes(&conn);
        let pid = propose_default(&conn, Some("gateway-design"));

        let new_tags = vec!["custom-tag".to_string()];
        let out = confirm(
            &conn,
            &format!("proposal:{pid}"),
            Some("edited-gateway"),
            Some("Edited human claim over the three gateway notes"),
            None,
            Some("editing gateway modules"),
            Some("keep routing and policy aligned"),
            Some(&new_tags),
            None,
        )
        .unwrap();

        let schema = &out["schema"];
        assert_eq!(schema["name"], "edited-gateway");
        assert_eq!(
            schema["statement"],
            "Edited human claim over the three gateway notes"
        );
        assert_eq!(
            schema["summary"],
            "Fires when editing gateway modules: keep routing and policy aligned"
        );
        assert_eq!(
            schema["scope"],
            json!({"tags": ["custom-tag"], "paths": ["src/gateway"]}),
            "passing --scope-tags replaces tags while preserving stored scope.paths"
        );
    }

    #[test]
    fn confirm_proposal_suffixes_taken_name_and_reports_renamed_from() {
        let conn = mem_db();
        seed_three_notes(&conn);
        let pid = propose_default(&conn, Some("mara"));

        // Another schema claims "mara" before proposal confirmation.
        conn.execute(
            "INSERT INTO schemas (uuid, name, summary, centroid_json, created_at, updated_at) \
             VALUES ('u-taken', 'mara', 'When x: y', '{}', 't0', 't0')",
            [],
        )
        .unwrap();

        let out = confirm(
            &conn,
            &format!("proposal:{pid}"),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(out["schema"]["name"], "mara-2");
        assert_eq!(out["renamed_from"], "mara");
    }

    #[test]
    fn confirm_refuses_detector_candidate_signature_with_propose_command() {
        let conn = mem_db();
        seed_three_notes(&conn);
        conn.execute(
            "INSERT INTO schema_candidates (cluster_sig, member_keys_json, density, \
             stability_count, reward_hits, first_seen_at, last_seen_at, kind) \
             VALUES ('decision:1,decision:2,decision:3', '[\"decision:1\",\"decision:2\",\"decision:3\"]', \
                     1.0, 3, 1, 't0', 't0', 'schema')",
            [],
        )
        .unwrap();

        let err = confirm(
            &conn,
            "decision:1,decision:2",
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap_err()
        .to_string();
        assert_eq!(
            err,
            "detector groups are evidence, not schemas: run engrams schema propose --from decision:1,decision:2 --statement ... --summary ..."
        );
    }

    #[test]
    fn confirm_existing_schema_bumps_and_rejects_edit_flags() {
        let conn = mem_db();
        conn.execute(
            "INSERT INTO schemas (id, uuid, name, summary, centroid_json, created_at, updated_at, kind) \
             VALUES (1, 'u-1', 'core', 'When x: y', '{}', 't0', 't0', 'schema')",
            [],
        )
        .unwrap();

        let bump = confirm(&conn, "core", None, None, None, None, None, None, None).unwrap();
        assert_eq!(bump["schema"]["bumped"], true);
        assert_eq!(bump["schema"]["name"], "core");

        let err = confirm(
            &conn,
            "core",
            Some("renamed"),
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap_err()
        .to_string();
        assert!(
            err.contains("edit flags are only valid when confirming a proposal"),
            "{err}"
        );
    }

    #[test]
    fn exact_signature_wins_over_ambiguous_prefix() {
        let conn = mem_db();
        seed_three_notes(&conn);
        conn.execute(
            "INSERT INTO schema_candidates (cluster_sig, member_keys_json, density, \
             stability_count, reward_hits, first_seen_at, last_seen_at) \
             VALUES ('decision:1', '[\"decision:1\"]', 1.0, 3, 0, 't0', 't0'), \
                    ('decision:1,decision:2', '[\"decision:1\",\"decision:2\"]', 1.0, 3, 0, 't0', 't0')",
            [],
        )
        .unwrap();
        let got = try_resolve_candidate(&conn, "decision:1").unwrap().unwrap();
        assert_eq!(got.members, vec!["decision:1".to_string()]);
    }

    #[test]
    fn parse_members_skips_external_pr_nodes_cleanly() {
        let conn = mem_db();
        seed_three_notes(&conn);

        let members = vec![
            "decision:1".to_string(),
            "decision:2".to_string(),
            "pr:https://github.com/stevebrownlee/engrams-cli/pull/3".to_string(),
        ];

        let parsed = parse_members(&conn, &members).unwrap();
        assert_eq!(parsed.len(), 2, "PR node is safely skipped without bailing");
        assert_eq!(parsed[0].id, 1);
        assert_eq!(parsed[1].id, 2);
    }
}
