//! `engrams schema propose` / `reject` / `proposals` (spec 0007).
//!
//! Schemas come into existence only through human-confirmed proposals stored
//! in `schema_proposals`. `propose` validates evidence count, file/day span,
//! note currency, claim non-restatement, trigger-phrased instruction, and
//! overlap against pending proposals, rejected proposals, and active schemas
//! before recording a proposal row.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{bail, Context as _, Result};
use chrono::{SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use super::gates::{claimed_territory, jaccard, JACCARD_IDENTITY};
use super::retrieval;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Scope {
    pub tags: Vec<String>,
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    pub notes: usize,
    pub files: usize,
    pub days: usize,
    pub contrary: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProposalStatus {
    Proposed,
    Confirmed,
    Rejected,
}

impl ProposalStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Proposed => "proposed",
            Self::Confirmed => "confirmed",
            Self::Rejected => "rejected",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchVia {
    MemberAnchor,
    ScopePath,
    ScopeTag,
    MemberTag,
}

impl MatchVia {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MemberAnchor => "member_anchor",
            Self::ScopePath => "scope_path",
            Self::ScopeTag => "scope_tag",
            Self::MemberTag => "member_tag",
        }
    }
}

pub const MIN_EVIDENCE: usize = 3;
pub const RESTATEMENT_GATE: f64 = 0.8;
pub const PENDING_IN_PRIME: usize = 3;
fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// Normalize a user- or candidate-supplied entity type to its canonical
/// underscored knowledge note kind (`&'static str`, rule 006).
/// Returns `Err(Some("code" | "schema"))` for known non-note graph kinds and
/// `Err(None)` for unknown kinds.
pub(crate) fn normalize_note_kind(
    raw: &str,
) -> std::result::Result<&'static str, Option<&'static str>> {
    match raw {
        "decision" => Ok("decision"),
        "system_pattern" | "system-pattern" | "pattern" => Ok("system_pattern"),
        "progress_entry" | "progress-entry" | "progress" => Ok("progress_entry"),
        "custom_data" | "custom-data" | "custom" => Ok("custom_data"),
        "code" => Err(Some("code")),
        "schema" => Err(Some("schema")),
        _ => Err(None),
    }
}

/// Parse one `kind:id` note key into `(canonical_kind, id, canonical_key)`.
/// Slices stay borrowed until the single canonical key allocation (rule 003).
pub(crate) fn parse_note_key(raw: &str) -> Result<(&'static str, i64, String)> {
    let trimmed = raw.trim();
    let Some((raw_kind, raw_id)) = trimmed.split_once(':') else {
        bail!("invalid note key '{trimmed}' (expected kind:id)");
    };
    let kind = match normalize_note_kind(raw_kind) {
        Ok(k) => k,
        Err(Some("code")) => {
            bail!("code:{raw_id} is a file, not a note: cite notes and put files in --scope-paths")
        }
        Err(Some("schema")) => {
            bail!(
                "schema:{raw_id} is a schema, not a note: cite knowledge notes \
                 (decision, system_pattern, progress_entry, custom_data)"
            )
        }
        Err(_) => {
            bail!(
                "'{trimmed}' is not a valid knowledge note key \
                 (expected decision, system_pattern, progress_entry, or custom_data)"
            )
        }
    };
    let id: i64 = raw_id
        .parse()
        .with_context(|| format!("invalid note id in '{trimmed}'"))?;
    Ok((kind, id, format!("{kind}:{id}")))
}

/// Parse a `proposal:<id>` target without intermediate string allocations (rule 003).
pub(crate) fn parse_proposal_target(target: &str) -> Option<i64> {
    target.trim().strip_prefix("proposal:")?.parse::<i64>().ok()
}

pub(crate) struct CitedNote {
    pub(crate) key: String,
    pub(crate) kind: &'static str,
    pub(crate) id: i64,
    pub(crate) text: String,
    pub(crate) timestamp: String,
    pub(crate) tags: Vec<String>,
}

fn fetch_current_note(
    conn: &Connection,
    kind: &'static str,
    id: i64,
    key: &str,
) -> Result<CitedNote> {
    match kind {
        "decision" => {
            let row = conn
                .query_row(
                    "SELECT summary, status, archived, timestamp, tags FROM decisions WHERE id = ?1",
                    params![id],
                    |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, i64>(2)?,
                            r.get::<_, String>(3)?,
                            r.get::<_, Option<String>>(4)?,
                        ))
                    },
                )
                .optional()?;
            let Some((summary, status, archived, timestamp, tags_raw)) = row else {
                bail!("{key} does not exist");
            };
            if status != "active" || archived != 0 {
                bail!("{key} is not current");
            }
            Ok(CitedNote {
                key: key.to_string(),
                kind,
                id,
                text: summary,
                timestamp,
                tags: crate::models::parse_tags(tags_raw.as_deref()),
            })
        }
        "system_pattern" => {
            let row = conn
                .query_row(
                    "SELECT name, description, archived, timestamp, tags FROM system_patterns WHERE id = ?1",
                    params![id],
                    |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, Option<String>>(1)?,
                            r.get::<_, i64>(2)?,
                            r.get::<_, String>(3)?,
                            r.get::<_, Option<String>>(4)?,
                        ))
                    },
                )
                .optional()?;
            let Some((name, description, archived, timestamp, tags_raw)) = row else {
                bail!("{key} does not exist");
            };
            if archived != 0 {
                bail!("{key} is not current");
            }
            let text = match description {
                Some(desc) if !desc.is_empty() => format!("{name} {desc}"),
                _ => name,
            };
            Ok(CitedNote {
                key: key.to_string(),
                kind,
                id,
                text,
                timestamp,
                tags: crate::models::parse_tags(tags_raw.as_deref()),
            })
        }
        "progress_entry" => {
            let row: Option<(String, String)> = conn
                .query_row(
                    "SELECT description, timestamp FROM progress_entries WHERE id = ?1",
                    params![id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            let Some((description, timestamp)) = row else {
                bail!("{key} does not exist");
            };
            Ok(CitedNote {
                key: key.to_string(),
                kind,
                id,
                text: description,
                timestamp,
                tags: Vec::new(),
            })
        }
        "custom_data" => {
            let row: Option<(String, String, String)> = conn
                .query_row(
                    "SELECT key, value, timestamp FROM custom_data WHERE id = ?1",
                    params![id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()?;
            let Some((cd_key, value, timestamp)) = row else {
                bail!("{key} does not exist");
            };
            Ok(CitedNote {
                key: key.to_string(),
                kind,
                id,
                text: format!("{cd_key} {value}"),
                timestamp,
                tags: Vec::new(),
            })
        }
        _ => bail!("'{key}' is not a valid knowledge note key"),
    }
}

/// Deepest directory shared by every anchor path in `anchors`, or `[]` when
/// cited notes have no anchors or share only the workspace root (`""`/`.`).
pub(crate) fn default_scope_paths(anchors: &BTreeSet<String>) -> Vec<String> {
    if anchors.is_empty() {
        return Vec::new();
    }
    let mut shared: Option<Vec<&str>> = None;
    for path in anchors {
        let cleaned = path.trim_start_matches("./").trim_matches('/');
        let comps: Vec<&str> = match cleaned.rfind('/') {
            Some(idx) => cleaned[..idx]
                .split('/')
                .filter(|s| !s.is_empty() && *s != ".")
                .collect(),
            None => Vec::new(),
        };
        if comps.is_empty() {
            return Vec::new();
        }
        match &mut shared {
            None => shared = Some(comps),
            Some(cur) => {
                let common = cur
                    .iter()
                    .zip(comps.iter())
                    .take_while(|(a, b)| a == b)
                    .count();
                cur.truncate(common);
                if cur.is_empty() {
                    return Vec::new();
                }
            }
        }
    }
    match shared {
        Some(comps) if !comps.is_empty() => vec![comps.join("/")],
        _ => Vec::new(),
    }
}

/// Tags carried by at least two cited notes, sorted and deduplicated.
fn default_scope_tags(notes: &[CitedNote]) -> Vec<String> {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for note in notes {
        let mut seen_on_note: BTreeSet<&str> = BTreeSet::new();
        for tag in &note.tags {
            let trimmed = tag.trim();
            if !trimmed.is_empty() && seen_on_note.insert(trimmed) {
                *counts.entry(trimmed).or_insert(0) += 1;
            }
        }
    }
    counts
        .into_iter()
        .filter(|(_, count)| *count >= 2)
        .map(|(tag, _)| tag.to_string())
        .collect()
}

pub(crate) fn normalize_scope_tags(raw: &[String]) -> Vec<String> {
    let mut set = BTreeSet::new();
    for t in raw {
        let trimmed = t.trim();
        if !trimmed.is_empty() {
            set.insert(trimmed.to_string());
        }
    }
    set.into_iter().collect()
}

pub(crate) fn normalize_scope_paths(raw: &[String]) -> Vec<String> {
    let mut set = BTreeSet::new();
    for p in raw {
        let cleaned = crate::ops::anchor::clean_path(p.trim());
        let trimmed = cleaned.trim_matches('/');
        if !trimmed.is_empty() && trimmed != "." {
            set.insert(trimmed.to_string());
        }
    }
    set.into_iter().collect()
}

pub(crate) fn normalize_scope(
    explicit_tags: Option<&[String]>,
    explicit_paths: Option<&[String]>,
    notes: &[CitedNote],
    anchors: &BTreeSet<String>,
) -> Scope {
    let tags = match explicit_tags {
        Some(raw) => normalize_scope_tags(raw),
        None => default_scope_tags(notes),
    };
    let paths = match explicit_paths {
        Some(raw) => normalize_scope_paths(raw),
        None => default_scope_paths(anchors),
    };
    Scope { tags, paths }
}

pub(crate) struct ValidatedProposal {
    pub(crate) name: String,
    pub(crate) statement: String,
    pub(crate) summary: String,
    pub(crate) scope: Scope,
    pub(crate) members: Vec<String>,
    pub(crate) member_notes: Vec<CitedNote>,
    pub(crate) contrary: Vec<String>,
    pub(crate) evidence: Evidence,
}

/// Validate proposal fields through checks 1–6 (and optionally check 9).
/// Exposed as `pub(crate)` so Phase 2 `confirm proposal:<id>` re-runs the
/// exact same checks on edited values without self-blocking on pending/rejected
/// overlap checks 7–8 (review finding F-1).
#[allow(clippy::too_many_arguments)]
pub(crate) fn validate_proposal(
    conn: &Connection,
    statement: &str,
    summary: Option<&str>,
    trigger: Option<&str>,
    rule: Option<&str>,
    raw_members: &[String],
    raw_contrary: &[String],
    scope_tags: Option<&[String]>,
    scope_paths: Option<&[String]>,
    name: Option<&str>,
) -> Result<ValidatedProposal> {
    // Check 1: knowledge keys only (decision, system_pattern, progress_entry, custom_data).
    let mut parsed_members: Vec<(&'static str, i64, String)> =
        Vec::with_capacity(raw_members.len());
    for raw in raw_members {
        if raw.trim().is_empty() {
            continue;
        }
        parsed_members.push(parse_note_key(raw)?);
    }
    parsed_members.sort_by(|a, b| a.2.cmp(&b.2));
    parsed_members.dedup_by(|a, b| a.2 == b.2);

    let mut parsed_contrary: Vec<(&'static str, i64, String)> =
        Vec::with_capacity(raw_contrary.len());
    for raw in raw_contrary {
        if raw.trim().is_empty() {
            continue;
        }
        parsed_contrary.push(parse_note_key(raw)?);
    }
    parsed_contrary.sort_by(|a, b| a.2.cmp(&b.2));
    parsed_contrary.dedup_by(|a, b| a.2 == b.2);

    // Check 2: existence, currency, and disjointness between members and contrary.
    let mut member_notes: Vec<CitedNote> = Vec::with_capacity(parsed_members.len());
    let mut member_key_set: BTreeSet<&str> = BTreeSet::new();
    for (kind, id, key) in &parsed_members {
        member_notes.push(fetch_current_note(conn, kind, *id, key)?);
        member_key_set.insert(key.as_str());
    }

    let mut contrary_keys: Vec<String> = Vec::with_capacity(parsed_contrary.len());
    for (kind, id, key) in &parsed_contrary {
        if member_key_set.contains(key.as_str()) {
            bail!("{key} is cited as both evidence and contrary");
        }
        let _ = fetch_current_note(conn, kind, *id, key)?;
        contrary_keys.push(key.clone());
    }

    // Check 3: at least MIN_EVIDENCE notes.
    if member_notes.len() < MIN_EVIDENCE {
        bail!(
            "proposal needs at least {MIN_EVIDENCE} notes (got {})",
            member_notes.len()
        );
    }

    // Check 4: cited notes span >= 2 distinct anchor files or >= 2 distinct UTC days.
    let mut anchor_stmt = conn.prepare(
        "SELECT path FROM item_anchors WHERE item_type = ?1 AND item_id = ?2 ORDER BY id ASC",
    )?;
    let mut distinct_files: BTreeSet<String> = BTreeSet::new();
    let mut distinct_days: BTreeSet<&str> = BTreeSet::new();
    for note in &member_notes {
        let rows = anchor_stmt.query_map(params![note.kind, note.id], |r| r.get::<_, String>(0))?;
        for path in rows {
            distinct_files.insert(path?);
        }
        let day = note.timestamp.get(..10).unwrap_or(note.timestamp.as_str());
        if !day.is_empty() {
            distinct_days.insert(day);
        }
    }
    let files = distinct_files.len();
    let days = distinct_days.len();
    if files < 2 && days < 2 {
        bail!("cited notes must span 2 files or 2 days");
    }

    // Check 5: claim-vs-note token overlap < RESTATEMENT_GATE (without tag dilution, F-3).
    let statement_trimmed = statement.trim();
    if statement_trimmed.is_empty() {
        bail!("statement cannot be empty");
    }
    let claim_tokens = super::assimilate::item_tokens(statement_trimmed, &[]);
    let claim_strs: Vec<&str> = claim_tokens.iter().map(String::as_str).collect();
    for note in &member_notes {
        let note_tokens = super::assimilate::item_tokens(&note.text, &[]);
        let note_strs: Vec<&str> = note_tokens.iter().map(String::as_str).collect();
        let overlap = jaccard(&claim_strs, &note_strs);
        if overlap >= RESTATEMENT_GATE {
            bail!(
                "statement repeats {} (overlap {overlap:.2} >= {RESTATEMENT_GATE})",
                note.key
            );
        }
    }

    // Check 6: instruction passes refine_text.
    let summary_text = super::list::refine_text(summary, trigger, rule)?.into_owned();

    let members: Vec<String> = parsed_members.into_iter().map(|(_, _, k)| k).collect();
    let scope = normalize_scope(
        scope_tags,
        explicit_paths_slice(scope_paths),
        &member_notes,
        &distinct_files,
    );
    let resolved_name = match name.map(str::trim).filter(|s| !s.is_empty()) {
        Some(n) => n.to_string(),
        None => suggest_base_schema_name(conn, &members)?,
    };
    let evidence = Evidence {
        notes: members.len(),
        files,
        days,
        contrary: contrary_keys.len(),
    };

    Ok(ValidatedProposal {
        name: resolved_name,
        statement: statement_trimmed.to_string(),
        summary: summary_text,
        scope,
        members,
        member_notes,
        contrary: contrary_keys,
        evidence,
    })
}

#[inline]
fn explicit_paths_slice(scope_paths: Option<&[String]>) -> Option<&[String]> {
    scope_paths
}

pub(crate) fn is_timestamp_newer(note_ts: &str, decided_at: &str) -> bool {
    if let (Ok(n), Ok(d)) = (
        chrono::DateTime::parse_from_rfc3339(note_ts),
        chrono::DateTime::parse_from_rfc3339(decided_at),
    ) {
        n > d
    } else {
        note_ts > decided_at
    }
}

fn detect_name_conflict(conn: &Connection, name: &str) -> Result<Option<Value>> {
    if let Some(id) = conn
        .query_row(
            "SELECT id FROM schemas WHERE name = ?1 ORDER BY id ASC LIMIT 1",
            params![name],
            |r| r.get::<_, i64>(0),
        )
        .optional()?
    {
        return Ok(Some(json!({ "with": "schema", "id": id })));
    }
    if let Some(id) = conn
        .query_row(
            "SELECT id FROM schema_proposals WHERE name = ?1 AND status = 'proposed' ORDER BY id ASC LIMIT 1",
            params![name],
            |r| r.get::<_, i64>(0),
        )
        .optional()?
    {
        return Ok(Some(json!({ "with": "proposal", "id": id })));
    }
    Ok(None)
}

/// Propose a base kebab-case schema name from members' dominant tags and anchor
/// directories, without uniqueness suffixing (so `schema propose` can surface
/// `name_conflict` when the base name is already taken).
pub(crate) fn suggest_base_schema_name(conn: &Connection, members: &[String]) -> Result<String> {
    let mut tag_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut dir_counts: BTreeMap<String, usize> = BTreeMap::new();

    for m in members {
        let Some((kind, id_str)) = m.split_once(':') else {
            continue;
        };
        let Ok(id) = id_str.parse::<i64>() else {
            continue;
        };

        let table = match kind {
            "decision" => Some("decisions"),
            "system_pattern" => Some("system_patterns"),
            _ => None,
        };
        if let Some(table) = table {
            let tags_raw: Option<Option<String>> = conn
                .query_row(
                    &format!("SELECT tags FROM {table} WHERE id = ?1"),
                    params![id],
                    |r| r.get(0),
                )
                .ok();
            if let Some(Some(raw)) = tags_raw {
                for t in crate::models::parse_tags(Some(&raw)) {
                    *tag_counts.entry(t).or_default() += 1;
                }
            }
        }

        let mut stmt =
            conn.prepare("SELECT path FROM item_anchors WHERE item_type = ?1 AND item_id = ?2")?;
        let paths = stmt.query_map(params![kind, id], |r| r.get::<_, String>(0))?;
        for p in paths {
            let Ok(p) = p else {
                continue;
            };
            if let Some(parent) = std::path::Path::new(&p).parent() {
                if let Some(dir_name) = parent.file_name().and_then(|f| f.to_str()) {
                    if !dir_name.is_empty() && dir_name != "." && dir_name != "src" {
                        *dir_counts.entry(dir_name.to_string()).or_default() += 1;
                    }
                }
            }
        }
    }

    let dominant_tag = tag_counts
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.as_str().cmp(a.0.as_str())))
        .map(|(t, _)| t);
    let dominant_dir = dir_counts
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.as_str().cmp(a.0.as_str())))
        .map(|(d, _)| d);

    let raw_name = match (dominant_tag, dominant_dir) {
        (Some(t), _) => t,
        (None, Some(d)) => d,
        (None, None) => "proposed-schema".to_string(),
    };

    Ok(sanitize_kebab(&raw_name))
}

/// Propose a kebab-case schema name from members' dominant tags and anchor directories.
pub(crate) fn suggest_schema_name(conn: &Connection, members: &[String]) -> Result<String> {
    let kebab_base = suggest_base_schema_name(conn, members)?;
    resolve_unique_name(conn, &kebab_base)
}

pub(crate) fn sanitize_kebab(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_dash = true;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    if out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "schema".to_string()
    } else {
        out
    }
}

fn resolve_unique_name(conn: &Connection, base: &str) -> Result<String> {
    let mut candidate = base.to_string();
    let mut counter = 2;
    loop {
        let exists: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM schemas WHERE name = ?1",
            params![&candidate],
            |r| r.get(0),
        )?;
        if !exists {
            return Ok(candidate);
        }
        candidate = format!("{base}-{counter}");
        counter += 1;
    }
}

#[allow(clippy::too_many_arguments)]
pub fn propose(
    conn: &Connection,
    statement: &str,
    summary: Option<&str>,
    trigger: Option<&str>,
    rule: Option<&str>,
    raw_members: &[String],
    from: Option<&str>,
    thread: Option<&str>,
    lens: Option<&str>,
    name: Option<&str>,
    scope_tags: Option<&[String]>,
    scope_paths: Option<&[String]>,
    raw_contrary: &[String],
    session: Option<&str>,
    force: bool,
) -> Result<Value> {
    if from.is_some() && thread.is_some() {
        bail!("cannot combine --from and --thread");
    }
    let mut dropped_code_members: Vec<String> = Vec::new();
    let mut seeded_members: Vec<String> = Vec::new();
    let (source_kind, source_ref) = match (from, thread) {
        (Some(prefix), None) => {
            let cand = super::confirm::try_resolve_candidate(conn, prefix)?.ok_or_else(|| {
                anyhow::anyhow!(
                    "no staged candidate matches '{prefix}' (run `engrams schema scan`)"
                )
            })?;
            if raw_members.is_empty() {
                for m in &cand.members {
                    let Some((kind, _)) = m.split_once(':') else {
                        continue;
                    };
                    match normalize_note_kind(kind) {
                        Ok(_) => seeded_members.push(m.clone()),
                        Err(Some("code")) => dropped_code_members.push(m.clone()),
                        Err(_) => {}
                    }
                }
                dropped_code_members.sort_unstable();
                dropped_code_members.dedup();
            }
            ("candidate", Some(cand.cluster_sig))
        }
        (None, Some(raw_tag)) => {
            let tag = raw_tag.trim();
            if tag.is_empty() {
                bail!("thread tag cannot be empty");
            }
            ("thread", Some(tag.to_string()))
        }
        (None, None) => ("manual", None),
        (Some(_), Some(_)) => unreachable!(),
    };
    let resolved_lens = lens.map(str::trim).filter(|s| !s.is_empty());
    let effective_members: &[String] = if !raw_members.is_empty() {
        raw_members
    } else {
        &seeded_members
    };

    let validated = validate_proposal(
        conn,
        statement,
        summary,
        trigger,
        rule,
        effective_members,
        raw_contrary,
        scope_tags,
        scope_paths,
        name,
    )?;

    let member_strs: Vec<&str> = validated.members.iter().map(String::as_str).collect();

    // Checks 7 and 8: pending and rejected proposal overlap (bypassed by --force).
    if !force {
        // Check 7: pending proposal overlap >= JACCARD_IDENTITY.
        let mut pending_stmt = conn.prepare(
            "SELECT id, members_json FROM schema_proposals WHERE status = 'proposed' ORDER BY id ASC",
        )?;
        let pending_rows =
            pending_stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
        let mut similar: Vec<Value> = Vec::new();
        for row in pending_rows {
            let (id, members_json) = row?;
            let stored: Vec<String> = serde_json::from_str(&members_json).unwrap_or_default();
            let stored_strs: Vec<&str> = stored.iter().map(String::as_str).collect();
            let overlap = jaccard(&member_strs, &stored_strs);
            if overlap >= JACCARD_IDENTITY {
                similar.push(json!({
                    "id": id,
                    "overlap": overlap,
                }));
            }
        }
        if !similar.is_empty() {
            return Ok(json!({
                "inserted": false,
                "similar": similar,
            }));
        }

        // Check 8: rejected proposal overlap >= JACCARD_IDENTITY with no newer cited note.
        let mut rej_stmt = conn.prepare(
            "SELECT id, members_json, decided_at, decision_note \
             FROM schema_proposals WHERE status = 'rejected' ORDER BY id ASC",
        )?;
        let rej_rows = rej_stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        })?;
        let mut rejected_before: Vec<Value> = Vec::new();
        for row in rej_rows {
            let (id, members_json, decided_at, reason) = row?;
            let stored: Vec<String> = serde_json::from_str(&members_json).unwrap_or_default();
            let stored_strs: Vec<&str> = stored.iter().map(String::as_str).collect();
            let overlap = jaccard(&member_strs, &stored_strs);
            if overlap >= JACCARD_IDENTITY {
                let has_newer = match decided_at.as_deref() {
                    Some(dec_ts) => validated
                        .member_notes
                        .iter()
                        .any(|n| is_timestamp_newer(&n.timestamp, dec_ts)),
                    None => false,
                };
                if !has_newer {
                    rejected_before.push(json!({
                        "id": id,
                        "decided_at": decided_at,
                        "reason": reason,
                        "overlap": overlap,
                    }));
                }
            }
        }
        if !rejected_before.is_empty() {
            return Ok(json!({
                "inserted": false,
                "rejected_before": rejected_before,
            }));
        }
    }

    // Check 9: active-schema territory check (never bypassed by --force).
    if let Some((schema_id, schema_name)) = claimed_territory(conn, &validated.members)? {
        bail!("notes already covered by schema {schema_id} ({schema_name})");
    }

    let name_conflict = detect_name_conflict(conn, &validated.name)?;
    let uuid = Uuid::new_v4().to_string();
    let created_at = now();
    let resolved_session = match session.map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => Some(s.to_string()),
        None => std::env::var("ENGRAMS_SESSION")
            .ok()
            .filter(|s| !s.is_empty()),
    };

    let scope_json = serde_json::to_string(&validated.scope)?;
    let members_json = serde_json::to_string(&validated.members)?;
    let contrary_json = serde_json::to_string(&validated.contrary)?;
    let evidence_json = serde_json::to_string(&validated.evidence)?;

    conn.execute(
        "INSERT INTO schema_proposals \
         (uuid, name, statement, summary, scope_json, members_json, contrary_json, \
          evidence_json, source_kind, source_ref, lens, session, status, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        params![
            uuid,
            validated.name,
            validated.statement,
            validated.summary,
            scope_json,
            members_json,
            contrary_json,
            evidence_json,
            source_kind,
            source_ref,
            resolved_lens,
            resolved_session,
            ProposalStatus::Proposed.as_str(),
            created_at,
        ],
    )?;
    let id = conn.last_insert_rowid();

    let mut out = json!({
        "status": "success",
        "inserted": true,
        "proposal": {
            "id": id,
            "name": validated.name,
            "statement": validated.statement,
            "summary": validated.summary,
            "scope": validated.scope,
            "members": validated.members,
            "contrary": validated.contrary,
            "evidence": validated.evidence,
            "source": {
                "kind": source_kind,
                "ref": source_ref,
            },
            "lens": resolved_lens,
            "status": ProposalStatus::Proposed.as_str(),
        },
        "confirm_command": format!("engrams schema confirm proposal:{id}"),
        "reject_command": format!("engrams schema reject proposal:{id} --reason \"<why>\""),
    });
    if let Some(conflict) = name_conflict {
        out["name_conflict"] = conflict;
    }
    if !dropped_code_members.is_empty() {
        out["dropped_code_members"] = json!(dropped_code_members);
    }
    Ok(out)
}

pub fn reject(conn: &Connection, target: &str, reason: Option<&str>) -> Result<Value> {
    let Some(id) = parse_proposal_target(target) else {
        bail!("invalid proposal target '{target}' (expected proposal:<id>)");
    };
    let status: Option<String> = conn
        .query_row(
            "SELECT status FROM schema_proposals WHERE id = ?1",
            params![id],
            |r| r.get(0),
        )
        .optional()?;
    let Some(status) = status else {
        bail!("proposal {id} does not exist");
    };
    if status != ProposalStatus::Proposed.as_str() {
        bail!("proposal {id} is already {status}");
    }
    let decided_at = now();
    let decision_note = reason.map(str::trim).filter(|s| !s.is_empty());
    conn.execute(
        "UPDATE schema_proposals \
         SET status = ?1, decision_note = ?2, decided_at = ?3 \
         WHERE id = ?4",
        params![
            ProposalStatus::Rejected.as_str(),
            decision_note,
            decided_at,
            id,
        ],
    )?;
    Ok(json!({
        "status": "success",
        "proposal": {
            "id": id,
            "status": ProposalStatus::Rejected.as_str(),
            "decision_note": decision_note,
            "decided_at": decided_at,
        },
    }))
}

struct ProposalRow {
    id: i64,
    name: String,
    statement: String,
    summary: String,
    scope_json: String,
    members_json: String,
    contrary_json: String,
    evidence_json: String,
    source_kind: String,
    source_ref: Option<String>,
    lens: Option<String>,
    session: Option<String>,
    status: String,
    decision_note: Option<String>,
    schema_id: Option<i64>,
    created_at: String,
    decided_at: Option<String>,
}

pub fn proposals(conn: &Connection, status_filter: Option<&str>) -> Result<Value> {
    let filter = status_filter.map(str::trim).unwrap_or("proposed");
    let where_clause = match filter {
        "proposed" | "confirmed" | "rejected" => "WHERE status = ?1",
        "all" => "",
        other => bail!(
            "invalid proposal status '{other}' (expected proposed, confirmed, rejected, or all)"
        ),
    };
    let sql = format!(
        "SELECT id, name, statement, summary, scope_json, members_json, contrary_json, \
                evidence_json, source_kind, source_ref, lens, session, status, decision_note, \
                schema_id, created_at, decided_at \
         FROM schema_proposals {where_clause} ORDER BY id ASC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let map_row = |r: &rusqlite::Row<'_>| -> rusqlite::Result<ProposalRow> {
        Ok(ProposalRow {
            id: r.get(0)?,
            name: r.get(1)?,
            statement: r.get(2)?,
            summary: r.get(3)?,
            scope_json: r.get(4)?,
            members_json: r.get(5)?,
            contrary_json: r.get(6)?,
            evidence_json: r.get(7)?,
            source_kind: r.get(8)?,
            source_ref: r.get(9)?,
            lens: r.get(10)?,
            session: r.get(11)?,
            status: r.get(12)?,
            decision_note: r.get(13)?,
            schema_id: r.get(14)?,
            created_at: r.get(15)?,
            decided_at: r.get(16)?,
        })
    };
    let rows: Vec<ProposalRow> = if where_clause.is_empty() {
        stmt.query_map([], map_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?
    } else {
        stmt.query_map(params![filter], map_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let scope: Scope = serde_json::from_str(&row.scope_json).unwrap_or_default();
        let member_keys: Vec<String> = serde_json::from_str(&row.members_json).unwrap_or_default();
        let contrary: Vec<String> = serde_json::from_str(&row.contrary_json).unwrap_or_default();
        let evidence: Value = serde_json::from_str(&row.evidence_json).unwrap_or(Value::Null);

        let total = member_keys.len();
        let cap = total.min(retrieval::MEMBER_ENRICHMENT_CAP);
        let mut pairs = Vec::with_capacity(cap);
        for key in &member_keys[..cap] {
            if let Some((k, i)) = key.split_once(':') {
                pairs.push((k.to_string(), i.to_string()));
            }
        }
        let enriched = retrieval::enrich_members(conn, &pairs)?;

        let mut item = json!({
            "id": row.id,
            "name": row.name,
            "statement": row.statement,
            "summary": row.summary,
            "scope": scope,
            "member_count": total,
            "members": enriched,
            "truncated": total > cap,
            "contrary": contrary,
            "evidence": evidence,
            "source": {
                "kind": row.source_kind,
                "ref": row.source_ref,
            },
            "lens": row.lens,
            "session": row.session,
            "status": row.status,
            "decision_note": row.decision_note,
            "schema_id": row.schema_id,
            "created_at": row.created_at,
            "decided_at": row.decided_at,
            "confirm_command": format!("engrams schema confirm proposal:{}", row.id),
            "reject_command": format!("engrams schema reject proposal:{} --reason \"<why>\"", row.id),
        });
        if total > cap {
            item["total_members"] = json!(total);
        }
        out.push(item);
    }

    Ok(json!({
        "status": "success",
        "proposals": out,
    }))
}

/// Up to [`PENDING_IN_PRIME`] pending proposals, oldest first (`created_at ASC, id ASC`),
/// surfaced in `engrams prime` (spec 0007 AC-11).
pub fn pending_for_prime(conn: &Connection) -> Result<Vec<Value>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, statement, evidence_json, created_at \
         FROM schema_proposals \
         WHERE status = 'proposed' \
         ORDER BY created_at ASC, id ASC \
         LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![PENDING_IN_PRIME as i64], |r| {
        let id: i64 = r.get(0)?;
        let name: String = r.get(1)?;
        let statement: String = r.get(2)?;
        let evidence_json: String = r.get(3)?;
        let created_at: String = r.get(4)?;
        let evidence: Value = serde_json::from_str(&evidence_json).unwrap_or(Value::Null);
        Ok(json!({
            "id": id,
            "name": name,
            "statement": statement,
            "evidence": evidence,
            "created_at": created_at,
            "confirm_command": format!("engrams schema confirm proposal:{id}"),
            "reject_command": format!("engrams schema reject proposal:{id} --reason \"<why>\""),
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

    fn seed_decision(
        conn: &Connection,
        id: i64,
        summary: &str,
        tags: &str,
        ts: &str,
        anchor: Option<&str>,
    ) {
        conn.execute(
            "INSERT INTO decisions (id, uuid, timestamp, summary, tags, status, archived) \
             VALUES (?1, ?2, ?3, ?4, ?5, 'active', 0)",
            params![id, format!("u-{id}"), ts, summary, tags],
        )
        .unwrap();
        if let Some(path) = anchor {
            conn.execute(
                "INSERT INTO item_anchors (item_type, item_id, path, timestamp) \
                 VALUES ('decision', ?1, ?2, ?3)",
                params![id, path, ts],
            )
            .unwrap();
        }
    }

    #[test]
    fn proposal_records_pending_with_default_scope_and_no_schema_created() {
        let conn = mem_db();
        seed_decision(
            &conn,
            1,
            "Mara fails the spire seal while academy peers succeed",
            r#"["mara","wound"]"#,
            "2026-09-01T10:00:00Z",
            Some("manuscript/ch01.md"),
        );
        seed_decision(
            &conn,
            2,
            "Mara deflects praise in council dialogue onto squadmates",
            r#"["mara","dialogue"]"#,
            "2026-09-02T10:00:00Z",
            Some("manuscript/ch03.md"),
        );
        seed_decision(
            &conn,
            3,
            "Mara hesitates before issuing her first command at the crossing",
            r#"["mara","leadership"]"#,
            "2026-09-03T10:00:00Z",
            Some("manuscript/ch05.md"),
        );

        let members = vec![
            "decision:1".to_string(),
            "decision:2".to_string(),
            "decision:3".to_string(),
        ];
        let out = propose(
            &conn,
            "Mara believes she is broken and carries that doubt into command",
            Some("When writing Mara under pressure: she doubts herself aloud while acting decisively"),
            None,
            None,
            &members,
            None,
            None,
            None,
            None,
            None,
            None,
            &[],
            Some("sess-1"),
            false,
        )
        .unwrap();

        assert_eq!(out["inserted"], true);
        assert_eq!(out["proposal"]["name"], "mara");
        assert_eq!(out["proposal"]["scope"]["tags"], json!(["mara"]));
        assert_eq!(out["proposal"]["scope"]["paths"], json!(["manuscript"]));
        assert_eq!(
            out["proposal"]["evidence"],
            json!({"notes": 3, "files": 3, "days": 3, "contrary": 0})
        );
        assert_eq!(out["confirm_command"], "engrams schema confirm proposal:1");

        let schema_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM schemas", [], |r| r.get(0))
            .unwrap();
        assert_eq!(schema_count, 0);

        let listed = proposals(&conn, None).unwrap();
        assert_eq!(listed["proposals"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn proposal_validation_rejects_each_failing_check() {
        let conn = mem_db();
        seed_decision(
            &conn,
            1,
            "Alpha note on first chapter",
            r#"["mara"]"#,
            "2026-09-01T10:00:00Z",
            Some("manuscript/ch01.md"),
        );
        seed_decision(
            &conn,
            2,
            "Beta note on second chapter",
            r#"["mara"]"#,
            "2026-09-01T11:00:00Z",
            Some("manuscript/ch02.md"),
        );
        seed_decision(
            &conn,
            3,
            "Gamma note on third chapter with extra specific tokens here",
            r#"["mara","extra","tags","diluting","nothing"]"#,
            "2026-09-01T12:00:00Z",
            Some("manuscript/ch03.md"),
        );

        // Check 1: code file cited as note
        let err = propose(
            &conn,
            "Distinct claim about Mara's arc",
            Some("When writing Mara: keep her voice understated"),
            None,
            None,
            &["decision:1".into(), "decision:2".into(), "code:14".into()],
            None,
            None,
            None,
            None,
            None,
            None,
            &[],
            None,
            false,
        )
        .unwrap_err()
        .to_string();
        assert_eq!(
            err,
            "code:14 is a file, not a note: cite notes and put files in --scope-paths"
        );

        // Check 2: missing note
        let err = propose(
            &conn,
            "Distinct claim about Mara's arc",
            Some("When writing Mara: keep her voice understated"),
            None,
            None,
            &[
                "decision:1".into(),
                "decision:2".into(),
                "decision:99".into(),
            ],
            None,
            None,
            None,
            None,
            None,
            None,
            &[],
            None,
            false,
        )
        .unwrap_err()
        .to_string();
        assert_eq!(err, "decision:99 does not exist");

        // Check 2: both evidence and contrary
        let err = propose(
            &conn,
            "Distinct claim about Mara's arc",
            Some("When writing Mara: keep her voice understated"),
            None,
            None,
            &[
                "decision:1".into(),
                "decision:2".into(),
                "decision:3".into(),
            ],
            None,
            None,
            None,
            None,
            None,
            None,
            &["decision:2".into()],
            None,
            false,
        )
        .unwrap_err()
        .to_string();
        assert_eq!(err, "decision:2 is cited as both evidence and contrary");

        // Check 3: fewer than 3 notes
        let err = propose(
            &conn,
            "Distinct claim about Mara's arc",
            Some("When writing Mara: keep her voice understated"),
            None,
            None,
            &["decision:1".into(), "decision:2".into()],
            None,
            None,
            None,
            None,
            None,
            None,
            &[],
            None,
            false,
        )
        .unwrap_err()
        .to_string();
        assert_eq!(err, "proposal needs at least 3 notes (got 2)");

        // Check 5: restatement gate (tags on decision:3 must not dilute note tokens)
        let err = propose(
            &conn,
            "Gamma note on third chapter with extra specific tokens here",
            Some("When writing Mara: keep her voice understated"),
            None,
            None,
            &[
                "decision:1".into(),
                "decision:2".into(),
                "decision:3".into(),
            ],
            None,
            None,
            None,
            None,
            None,
            None,
            &[],
            None,
            false,
        )
        .unwrap_err()
        .to_string();
        assert!(
            err.starts_with("statement repeats decision:3 (overlap 1.00 >= 0.8)"),
            "unexpected error: {err}"
        );

        // Check 6: instruction without trigger prefix
        let err = propose(
            &conn,
            "Distinct claim about Mara's arc",
            Some("Keep her voice understated"),
            None,
            None,
            &[
                "decision:1".into(),
                "decision:2".into(),
                "decision:3".into(),
            ],
            None,
            None,
            None,
            None,
            None,
            None,
            &[],
            None,
            false,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("summary must begin with a conditional trigger phrase"));
    }

    #[test]
    fn proposal_overlap_and_reject_lifecycle() {
        let conn = mem_db();
        seed_decision(
            &conn,
            1,
            "First note on chapter one",
            r#"["mara"]"#,
            "2026-09-01T10:00:00Z",
            Some("manuscript/ch01.md"),
        );
        seed_decision(
            &conn,
            2,
            "Second note on chapter two",
            r#"["mara"]"#,
            "2026-09-02T10:00:00Z",
            Some("manuscript/ch02.md"),
        );
        seed_decision(
            &conn,
            3,
            "Third note on chapter three",
            r#"["mara"]"#,
            "2026-09-03T10:00:00Z",
            Some("manuscript/ch03.md"),
        );
        let members = vec![
            "decision:1".to_string(),
            "decision:2".to_string(),
            "decision:3".to_string(),
        ];

        let first = propose(
            &conn,
            "Synthesized claim across three chapters",
            None,
            Some("writing Mara in command scenes"),
            Some("keep self-doubt in dialogue while actions hold firm"),
            &members,
            None,
            None,
            None,
            Some("mara-leader"),
            None,
            None,
            &[],
            None,
            false,
        )
        .unwrap();
        assert_eq!(first["inserted"], true);

        // Check 7: duplicate pending proposal returns similar without inserting
        let dup = propose(
            &conn,
            "Another phrasing of the synthesized claim",
            Some("When writing Mara: keep self-doubt in dialogue"),
            None,
            None,
            &members,
            None,
            None,
            None,
            Some("mara-leader-2"),
            None,
            None,
            &[],
            None,
            false,
        )
        .unwrap();
        assert_eq!(dup["inserted"], false);
        assert_eq!(dup["similar"][0]["id"], 1);

        // Reject proposal:1
        let rej = reject(&conn, "proposal:1", Some("too early in the draft")).unwrap();
        assert_eq!(rej["proposal"]["status"], "rejected");
        assert_eq!(rej["proposal"]["decision_note"], "too early in the draft");

        // Check 8: proposing same notes with no newer note returns rejected_before
        let blocked = propose(
            &conn,
            "Another phrasing of the synthesized claim",
            Some("When writing Mara: keep self-doubt in dialogue"),
            None,
            None,
            &members,
            None,
            None,
            None,
            Some("mara-leader"),
            None,
            None,
            &[],
            None,
            false,
        )
        .unwrap();
        assert_eq!(blocked["inserted"], false);
        assert_eq!(blocked["rejected_before"][0]["id"], 1);
        assert_eq!(
            blocked["rejected_before"][0]["reason"],
            "too early in the draft"
        );
    }

    #[test]
    fn pending_for_prime_orders_oldest_first_and_caps_at_three() {
        let conn = mem_db();
        assert!(pending_for_prime(&conn).unwrap().is_empty());

        for (id, name, status, ts) in [
            (1, "p-late", "proposed", "2026-10-04T00:00:00Z"),
            (2, "p-first", "proposed", "2026-10-01T00:00:00Z"),
            (3, "p-confirmed", "confirmed", "2026-09-01T00:00:00Z"),
            (4, "p-second", "proposed", "2026-10-02T00:00:00Z"),
            (5, "p-third", "proposed", "2026-10-03T00:00:00Z"),
        ] {
            conn.execute(
                "INSERT INTO schema_proposals \
                 (id, uuid, name, statement, summary, scope_json, members_json, contrary_json, \
                  evidence_json, source_kind, status, created_at) \
                 VALUES (?1, ?2, ?3, ?4, 'When x: y', '{\"tags\":[],\"paths\":[]}', '[]', '[]', \
                         '{\"notes\":3,\"files\":2,\"days\":2,\"contrary\":0}', 'manual', ?5, ?6)",
                params![
                    id,
                    format!("u-{id}"),
                    name,
                    format!("claim {id}"),
                    status,
                    ts
                ],
            )
            .unwrap();
        }

        let pending = pending_for_prime(&conn).unwrap();
        assert_eq!(pending.len(), 3);
        let ids: Vec<i64> = pending.iter().map(|p| p["id"].as_i64().unwrap()).collect();
        assert_eq!(ids, vec![2, 4, 5]);
        assert_eq!(
            pending[0]["confirm_command"],
            "engrams schema confirm proposal:2"
        );
        assert_eq!(
            pending[0]["reject_command"],
            "engrams schema reject proposal:2 --reason \"<why>\""
        );
    }

    #[test]
    fn test_suggest_schema_name_prefers_dominant_tag_falls_back_to_dir_and_suffixes() {
        let conn = mem_db();
        seed_decision(
            &conn,
            1,
            "first auth note",
            r#"["auth", "jwt"]"#,
            "2026-09-01T10:00:00Z",
            None,
        );
        seed_decision(
            &conn,
            2,
            "second auth note",
            r#"["auth", "crypto"]"#,
            "2026-09-02T10:00:00Z",
            None,
        );
        let members = vec!["decision:1".to_string(), "decision:2".to_string()];
        assert_eq!(suggest_schema_name(&conn, &members).unwrap(), "auth");

        conn.execute(
            "INSERT INTO schemas (id, uuid, name, summary, centroid_json, created_at, updated_at) \
             VALUES (1, 's-1', 'auth', 'When x: y', '{}', 't', 't')",
            [],
        )
        .unwrap();
        assert_eq!(suggest_schema_name(&conn, &members).unwrap(), "auth-2");
        conn.execute(
            "INSERT INTO schemas (id, uuid, name, summary, centroid_json, created_at, updated_at) \
             VALUES (2, 's-2', 'auth-2', 'When x: y', '{}', 't', 't')",
            [],
        )
        .unwrap();
        assert_eq!(suggest_schema_name(&conn, &members).unwrap(), "auth-3");

        seed_decision(
            &conn,
            3,
            "untagged note",
            "[]",
            "2026-09-03T10:00:00Z",
            Some("src/ops/auth.rs"),
        );
        assert_eq!(
            suggest_schema_name(&conn, &["decision:3".to_string()]).unwrap(),
            "ops"
        );
    }

    #[test]
    fn propose_records_thread_and_lens_provenance_and_supports_from_with_explicit_members_ac9() {
        let conn = mem_db();
        seed_decision(
            &conn,
            1,
            "Mara fails the spire seal while academy peers succeed",
            r#"["mara","wound"]"#,
            "2026-09-01T10:00:00Z",
            Some("manuscript/ch01.md"),
        );
        seed_decision(
            &conn,
            2,
            "Mara deflects praise in council dialogue onto squadmates",
            r#"["mara","dialogue"]"#,
            "2026-09-02T10:00:00Z",
            Some("manuscript/ch03.md"),
        );
        seed_decision(
            &conn,
            3,
            "Mara hesitates before issuing her first command at the crossing",
            r#"["mara","leadership"]"#,
            "2026-09-03T10:00:00Z",
            Some("manuscript/ch05.md"),
        );

        let members = vec![
            "decision:1".to_string(),
            "decision:2".to_string(),
            "decision:3".to_string(),
        ];
        let out = propose(
            &conn,
            "Mara believes she is broken and carries that doubt into command",
            Some("When writing Mara under pressure: she doubts herself aloud while acting decisively"),
            None,
            None,
            &members,
            None,
            Some("mara"),
            Some("fiction"),
            Some("mara-broken-leader"),
            None,
            None,
            &[],
            Some("sess-ac9"),
            false,
        )
        .unwrap();

        assert_eq!(out["inserted"], true);
        assert_eq!(
            out["proposal"]["source"],
            json!({"kind": "thread", "ref": "mara"})
        );
        assert_eq!(out["proposal"]["lens"], "fiction");

        let listed = proposals(&conn, None).unwrap();
        let props = listed["proposals"].as_array().unwrap();
        assert_eq!(props.len(), 1);
        assert_eq!(props[0]["source"], json!({"kind": "thread", "ref": "mara"}));
        assert_eq!(props[0]["lens"], "fiction");

        // Combining --from with explicit --members records candidate provenance while using explicit members (F-2)
        conn.execute(
            "INSERT INTO schema_candidates \
             (cluster_sig, member_keys_json, density, stability_count, reward_hits, \
              first_seen_at, last_seen_at, kind) \
             VALUES ('code:99,decision:1,decision:2,decision:3,decision:999', \
                     '[\"code:99\",\"decision:1\",\"decision:2\",\"decision:3\",\"decision:999\"]', \
                     0.9, 3, 2, '2026-09-01T00:00:00Z', '2026-09-04T00:00:00Z', 'unclear')",
            [],
        )
        .unwrap();
        let cand_out = propose(
            &conn,
            "Candidate-seeded claim over uncovered notes only",
            Some("When writing crossing scenes: keep tension grounded in hesitation"),
            None,
            None,
            &members,
            Some("code:99,decision:1"),
            None,
            Some("fiction"),
            Some("crossing-hesitation"),
            None,
            None,
            &[],
            None,
            true,
        )
        .unwrap();
        assert_eq!(cand_out["inserted"], true);
        assert_eq!(
            cand_out["proposal"]["source"],
            json!({
                "kind": "candidate",
                "ref": "code:99,decision:1,decision:2,decision:3,decision:999"
            })
        );
        assert_eq!(cand_out["proposal"]["lens"], "fiction");
        assert_eq!(cand_out["proposal"]["members"], json!(members));
        assert!(cand_out.get("dropped_code_members").is_none());
    }
}
