//! Session-close schema prompts and project lenses (spec 0008).
//!
//! Builds up to `PROMPTS_PER_CLOSE` deterministic prompts from two evidence
//! sources:
//! 1. Staged `schema_candidates` rows that pass `gates_pass`, of any `kind`.
//! 2. Subject-tag threads across active knowledge notes (`>= MIN_EVIDENCE`
//!    notes, not in the lens's `ignore_tags`, and `<= THREAD_MAX_SHARE` of all
//!    tagged active notes).
//!
//! Filtering enforces coverage against active schemas and pending proposals,
//! Spec 0007's propose evidence gates (at least 3 uncovered notes across at
//! least 2 files or 2 days, plus `claimed_territory`), and evidence-based
//! recurrence suppression against `schema_prompt_notices` and rejected
//! proposals before ranking and Jaccard 0.7 deduplication.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use anyhow::{bail, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::gates::{claimed_territory, gates_pass, jaccard, JACCARD_IDENTITY};
use super::propose::{
    is_timestamp_newer, normalize_note_kind, parse_note_key, sanitize_kebab, suggest_schema_name,
    ProposalStatus, MIN_EVIDENCE,
};
use super::retrieval::parse_scope;

pub const THREAD_MAX_SHARE: f64 = 0.8;
pub const PROMPTS_PER_CLOSE: usize = 3;
pub const COVERED_SHARE: f64 = 0.5;

const BUILTIN_GENERAL: &str = include_str!("../../assets/lenses/general.json");
const BUILTIN_FICTION: &str = include_str!("../../assets/lenses/fiction.json");
const BUILTIN_SOFTWARE: &str = include_str!("../../assets/lenses/software.json");

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lens {
    pub name: String,
    pub description: String,
    pub questions: Vec<String>,
    pub ignore_tags: Vec<String>,
    pub order: Order,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Order {
    Story,
    Time,
}

impl Order {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Story => "story",
            Self::Time => "time",
        }
    }
}

impl Serialize for Order {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptSourceKind {
    Thread,
    Candidate,
}

impl PromptSourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Thread => "thread",
            Self::Candidate => "candidate",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptSource {
    pub kind: String,
    #[serde(rename = "ref")]
    pub source_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptEvidence {
    pub kind: String,
    pub id: i64,
    pub summary: String,
    pub anchors: Vec<String>,
    pub timestamp: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptSpread {
    pub notes: usize,
    pub files: usize,
    pub days: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaPrompt {
    pub key: String,
    pub source: PromptSource,
    pub label: Option<String>,
    pub lens: String,
    pub questions: Vec<String>,
    pub evidence: Vec<PromptEvidence>,
    pub spread: PromptSpread,
    pub propose_command: String,
    #[serde(skip)]
    pub(crate) notice_key: String,
    #[serde(skip)]
    pub(crate) sorted_evidence_keys: Vec<String>,
}

/// Compare two strings in natural reading order: non-digit bytes compare
/// byte-by-byte, while contiguous ASCII digit runs compare by numeric value
/// (ignoring leading zeros, then breaking equal-value ties by shorter digit
/// run first so the ordering is a strict total order).
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let ab = a.as_bytes();
    let bb = b.as_bytes();
    let (mut ia, mut ib) = (0usize, 0usize);

    while ia < ab.len() && ib < bb.len() {
        let ca = ab[ia];
        let cb = bb[ib];
        if ca.is_ascii_digit() && cb.is_ascii_digit() {
            let a_start = ia;
            while ia < ab.len() && ab[ia].is_ascii_digit() {
                ia += 1;
            }
            let b_start = ib;
            while ib < bb.len() && bb[ib].is_ascii_digit() {
                ib += 1;
            }
            let a_digits = &ab[a_start..ia];
            let b_digits = &bb[b_start..ib];
            let a_nz = trim_leading_zeros(a_digits);
            let b_nz = trim_leading_zeros(b_digits);

            let num_cmp = a_nz.len().cmp(&b_nz.len()).then_with(|| a_nz.cmp(b_nz));
            if num_cmp != Ordering::Equal {
                return num_cmp;
            }
            let len_cmp = a_digits.len().cmp(&b_digits.len());
            if len_cmp != Ordering::Equal {
                return len_cmp;
            }
        } else {
            let byte_cmp = ca.cmp(&cb);
            if byte_cmp != Ordering::Equal {
                return byte_cmp;
            }
            ia += 1;
            ib += 1;
        }
    }

    (ab.len() - ia).cmp(&(bb.len() - ib))
}

fn trim_leading_zeros(bytes: &[u8]) -> &[u8] {
    let mut i = 0;
    while i < bytes.len() && bytes[i] == b'0' {
        i += 1;
    }
    &bytes[i..]
}

fn available_lenses(workspace_root: Option<&Path>) -> String {
    let mut names = vec![
        "general".to_string(),
        "fiction".to_string(),
        "software".to_string(),
    ];
    if let Some(root) = workspace_root {
        let lenses_dir = root.join("engrams").join("lenses");
        if let Ok(entries) = std::fs::read_dir(lenses_dir) {
            let mut extra = BTreeSet::new();
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("json") {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        if !stem.is_empty() && !names.iter().any(|n| n == stem) {
                            extra.insert(stem.to_string());
                        }
                    }
                }
            }
            names.extend(extra);
        }
    }
    names.join(", ")
}

fn is_valid_lens_name(name: &str) -> bool {
    !name.is_empty() && !name.contains('/') && !name.contains('\\') && !name.contains("..")
}

/// Load a lens by name (default `"general"`), preferring a workspace lens file
/// at `<root>/engrams/lenses/<name>.json` over compiled-in built-ins.
pub fn load_lens(name: Option<&str>, workspace_root: Option<&Path>) -> Result<Lens> {
    let requested = match name {
        Some(raw) => raw.trim(),
        None => "general",
    };
    if !is_valid_lens_name(requested) {
        bail!(
            "unknown lens '{requested}': available {}",
            available_lenses(workspace_root)
        );
    }

    if let Some(root) = workspace_root {
        let lens_path = root
            .join("engrams")
            .join("lenses")
            .join(format!("{requested}.json"));
        if lens_path.is_file() {
            let raw = std::fs::read_to_string(&lens_path)
                .map_err(|e| anyhow::anyhow!("lens file engrams/lenses/{requested}.json: {e}"))?;
            let lens: Lens = serde_json::from_str(&raw)
                .map_err(|e| anyhow::anyhow!("lens file engrams/lenses/{requested}.json: {e}"))?;
            return Ok(lens);
        }
    }

    let builtin_raw = match requested {
        "general" => Some(BUILTIN_GENERAL),
        "fiction" => Some(BUILTIN_FICTION),
        "software" => Some(BUILTIN_SOFTWARE),
        _ => None,
    };

    if let Some(raw) = builtin_raw {
        let lens: Lens = serde_json::from_str(raw)?;
        return Ok(lens);
    }

    bail!(
        "unknown lens '{requested}': available {}",
        available_lenses(workspace_root)
    )
}

struct ActiveNote {
    key: String,
    kind: &'static str,
    id: i64,
    summary: String,
    timestamp: String,
    tags: Vec<String>,
    anchors: Vec<String>,
}

type NoteIndexMap = HashMap<(&'static str, i64), usize>;

fn load_active_notes(conn: &Connection) -> Result<(Vec<ActiveNote>, NoteIndexMap)> {
    let mut anchor_map: HashMap<(&'static str, i64), Vec<String>> = HashMap::new();
    {
        let mut stmt =
            conn.prepare("SELECT item_type, item_id, path FROM item_anchors ORDER BY id ASC")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        for row in rows {
            let (raw_type, item_id, path) = row?;
            if let Ok(canon) = normalize_note_kind(&raw_type) {
                anchor_map.entry((canon, item_id)).or_default().push(path);
            }
        }
    }

    let mut notes: Vec<ActiveNote> = Vec::new();

    // 1. Active, unarchived decisions
    {
        let mut stmt = conn.prepare(
            "SELECT id, summary, timestamp, tags FROM decisions \
             WHERE status = 'active' AND archived = 0 ORDER BY id ASC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        })?;
        for row in rows {
            let (id, summary, timestamp, tags_raw) = row?;
            // Move anchors out of temporary lookup map (rule 004).
            let mut anchors = anchor_map.remove(&("decision", id)).unwrap_or_default();
            anchors.sort_by(|a, b| natural_cmp(a, b));
            anchors.dedup();
            notes.push(ActiveNote {
                key: format!("decision:{id}"),
                kind: "decision",
                id,
                summary,
                timestamp,
                tags: crate::models::parse_tags(tags_raw.as_deref()),
                anchors,
            });
        }
    }

    // 2. Unarchived system patterns
    {
        let mut stmt = conn.prepare(
            "SELECT id, name, description, timestamp, tags FROM system_patterns \
             WHERE archived = 0 ORDER BY id ASC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, Option<String>>(4)?,
            ))
        })?;
        for row in rows {
            let (id, name, description, timestamp, tags_raw) = row?;
            let summary = match description {
                Some(desc) if !desc.is_empty() => format!("{name}: {desc}"),
                _ => name,
            };
            let mut anchors = anchor_map
                .remove(&("system_pattern", id))
                .unwrap_or_default();
            anchors.sort_by(|a, b| natural_cmp(a, b));
            anchors.dedup();
            notes.push(ActiveNote {
                key: format!("system_pattern:{id}"),
                kind: "system_pattern",
                id,
                summary,
                timestamp,
                tags: crate::models::parse_tags(tags_raw.as_deref()),
                anchors,
            });
        }
    }

    // 3. Progress entries
    {
        let mut stmt = conn.prepare(
            "SELECT id, description, timestamp FROM progress_entries \
             WHERE status != 'Dropped' ORDER BY id ASC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        for row in rows {
            let (id, description, timestamp) = row?;
            let mut anchors = anchor_map
                .remove(&("progress_entry", id))
                .unwrap_or_default();
            anchors.sort_by(|a, b| natural_cmp(a, b));
            anchors.dedup();
            notes.push(ActiveNote {
                key: format!("progress_entry:{id}"),
                kind: "progress_entry",
                id,
                summary: description,
                timestamp,
                tags: Vec::new(),
                anchors,
            });
        }
    }

    // 4. Custom data
    {
        let mut stmt =
            conn.prepare("SELECT id, key, value, timestamp FROM custom_data ORDER BY id ASC")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?;
        for row in rows {
            let (id, cd_key, value, timestamp) = row?;
            let mut anchors = anchor_map.remove(&("custom_data", id)).unwrap_or_default();
            anchors.sort_by(|a, b| natural_cmp(a, b));
            anchors.dedup();
            notes.push(ActiveNote {
                key: format!("custom_data:{id}"),
                kind: "custom_data",
                id,
                summary: format!("{cd_key}: {value}"),
                timestamp,
                tags: Vec::new(),
                anchors,
            });
        }
    }

    // Sort notes by canonical key so ascending index order equals ascending key order.
    notes.sort_by(|a, b| a.key.cmp(&b.key));
    let mut key_to_idx = HashMap::with_capacity(notes.len());
    for (idx, note) in notes.iter().enumerate() {
        key_to_idx.insert((note.kind, note.id), idx);
    }

    Ok((notes, key_to_idx))
}

fn load_coverage(
    conn: &Connection,
    key_to_idx: &HashMap<(&'static str, i64), usize>,
) -> Result<(BTreeSet<usize>, BTreeSet<Vec<String>>)> {
    let mut covered_notes = BTreeSet::new();
    let mut handled_tag_parts = BTreeSet::new();

    // Active schemas: scope tags and member_of knowledge notes
    {
        let mut stmt = conn.prepare("SELECT scope_json FROM schemas WHERE status = 'active'")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        for row in rows {
            let scope_json = row?;
            for tag in parse_scope(&scope_json).tags {
                let parts = crate::ops::ident_parts(&tag);
                if !parts.is_empty() {
                    handled_tag_parts.insert(parts);
                }
            }
        }
    }
    {
        let mut stmt = conn.prepare(
            "SELECT l.source_item_type, l.source_item_id \
             FROM schemas s \
             JOIN context_links l ON l.relationship_type = 'member_of' \
              AND l.target_item_type = 'schema' AND l.target_item_id = s.id \
             WHERE s.status = 'active'",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        for row in rows {
            let (raw_type, raw_id) = row?;
            if let (Ok(canon_kind), Ok(id)) =
                (normalize_note_kind(&raw_type), raw_id.trim().parse::<i64>())
            {
                if let Some(idx) = key_to_idx.get(&(canon_kind, id)).copied() {
                    covered_notes.insert(idx);
                }
            }
        }
    }

    // Pending proposals: scope tags and cited members
    {
        let mut stmt = conn
            .prepare("SELECT scope_json, members_json FROM schema_proposals WHERE status = ?1")?;
        let rows = stmt.query_map(params![ProposalStatus::Proposed.as_str()], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (scope_json, members_json) = row?;
            for tag in parse_scope(&scope_json).tags {
                let parts = crate::ops::ident_parts(&tag);
                if !parts.is_empty() {
                    handled_tag_parts.insert(parts);
                }
            }
            let members: Vec<String> = serde_json::from_str(&members_json).unwrap_or_default();
            for m in members {
                if let Ok((canon_kind, id, _)) = parse_note_key(&m) {
                    if let Some(idx) = key_to_idx.get(&(canon_kind, id)).copied() {
                        covered_notes.insert(idx);
                    }
                }
            }
        }
    }

    Ok((covered_notes, handled_tag_parts))
}

struct PriorNotice {
    source_key: String,
    keys: Vec<String>,
    notified_at: String,
}

struct PriorRejected {
    keys: Vec<String>,
    decided_at: Option<String>,
}

fn parse_sorted_keys(json_str: &str) -> Vec<String> {
    let raw: Vec<String> = serde_json::from_str(json_str).unwrap_or_default();
    let mut keys = Vec::with_capacity(raw.len());
    for item in raw {
        if let Ok((_, _, canon_key)) = parse_note_key(&item) {
            keys.push(canon_key);
        }
    }
    keys.sort_unstable();
    keys.dedup();
    keys
}

fn load_recurrence_history(conn: &Connection) -> Result<(Vec<PriorNotice>, Vec<PriorRejected>)> {
    let mut notices = Vec::new();
    {
        let mut stmt = conn
            .prepare("SELECT source_key, evidence_json, notified_at FROM schema_prompt_notices")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        for row in rows {
            let (source_key, evidence_json, notified_at) = row?;
            let keys = parse_sorted_keys(&evidence_json);
            if !keys.is_empty() {
                notices.push(PriorNotice {
                    source_key,
                    keys,
                    notified_at,
                });
            }
        }
    }

    let mut rejected = Vec::new();
    {
        let mut stmt = conn
            .prepare("SELECT members_json, decided_at FROM schema_proposals WHERE status = ?1")?;
        let rows = stmt.query_map(params![ProposalStatus::Rejected.as_str()], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
        })?;
        for row in rows {
            let (members_json, decided_at) = row?;
            let keys = parse_sorted_keys(&members_json);
            if !keys.is_empty() {
                rejected.push(PriorRejected { keys, decided_at });
            }
        }
    }

    Ok((notices, rejected))
}

/// True if every element of ascending-sorted `sub` appears in ascending-sorted `sup`.
fn is_subset(sub: &[&str], sup: &[&str]) -> bool {
    if sub.is_empty() || sub.len() > sup.len() {
        return false;
    }
    let (mut i, mut j) = (0usize, 0usize);
    while i < sub.len() && j < sup.len() {
        match sub[i].cmp(sup[j]) {
            Ordering::Less => return false,
            Ordering::Greater => j += 1,
            Ordering::Equal => {
                i += 1;
                j += 1;
            }
        }
    }
    i == sub.len()
}

fn is_held_back(
    notice_key: &str,
    uncovered_indices: &[usize],
    uncovered_strs: &[&str],
    notes: &[ActiveNote],
    notices: &[PriorNotice],
    rejected: &[PriorRejected],
) -> bool {
    for n in notices {
        let prior_strs: Vec<&str> = n.keys.iter().map(String::as_str).collect();
        let overlaps = jaccard(uncovered_strs, &prior_strs) >= JACCARD_IDENTITY
            || is_subset(uncovered_strs, &prior_strs)
            || n.source_key == notice_key;
        if overlaps {
            let has_newer = uncovered_indices
                .iter()
                .any(|&idx| is_timestamp_newer(&notes[idx].timestamp, &n.notified_at));
            if !has_newer {
                return true;
            }
        }
    }

    for r in rejected {
        let prior_strs: Vec<&str> = r.keys.iter().map(String::as_str).collect();
        let overlaps = jaccard(uncovered_strs, &prior_strs) >= JACCARD_IDENTITY
            || is_subset(uncovered_strs, &prior_strs);
        if overlaps {
            let has_newer = match r.decided_at.as_deref() {
                Some(dec_ts) => uncovered_indices
                    .iter()
                    .any(|&idx| is_timestamp_newer(&notes[idx].timestamp, dec_ts)),
                None => false,
            };
            if !has_newer {
                return true;
            }
        }
    }

    false
}

fn compute_spread(indices: &[usize], notes: &[ActiveNote]) -> PromptSpread {
    let mut files: BTreeSet<&str> = BTreeSet::new();
    let mut days: BTreeSet<&str> = BTreeSet::new();
    for &idx in indices {
        let note = &notes[idx];
        for path in &note.anchors {
            files.insert(path.as_str());
        }
        let day = note.timestamp.get(..10).unwrap_or(note.timestamp.as_str());
        if !day.is_empty() {
            days.insert(day);
        }
    }
    PromptSpread {
        notes: indices.len(),
        files: files.len(),
        days: days.len(),
    }
}

/// Pick the shortest comma-delimited prefix of `sig` (starting at the first 2
/// items) that uniquely resolves `sig` among `all_sigs` under
/// `try_resolve_candidate` rules. Returns a borrowed `&str` slice (rule 003).
fn unique_candidate_prefix<'a>(sig: &'a str, all_sigs: &[String]) -> &'a str {
    for (n, (idx, _)) in sig.match_indices(',').enumerate() {
        if n < 1 {
            continue;
        }
        let prefix = &sig[..idx];
        let prefix_matches = all_sigs
            .iter()
            .filter(|other| other.as_str() == prefix || other.starts_with(prefix))
            .count();
        if prefix_matches == 1 {
            return prefix;
        }
    }
    sig
}

struct RankedCandidate {
    key: String,
    notice_key: String,
    source_kind: PromptSourceKind,
    source_ref: String,
    label: Option<String>,
    uncovered_indices: Vec<usize>,
    uncovered_keys: Vec<String>,
    spread: PromptSpread,
}

fn compare_notes_for_order(a: &ActiveNote, b: &ActiveNote, order: Order) -> Ordering {
    let anchor_cmp = match (a.anchors.first(), b.anchors.first()) {
        (Some(pa), Some(pb)) => natural_cmp(pa, pb),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    };
    match order {
        Order::Story => anchor_cmp
            .then_with(|| a.timestamp.cmp(&b.timestamp))
            .then_with(|| a.key.cmp(&b.key)),
        Order::Time => a
            .timestamp
            .cmp(&b.timestamp)
            .then_with(|| anchor_cmp)
            .then_with(|| a.key.cmp(&b.key)),
    }
}

fn join_note_keys(indices: &[usize], notes: &[ActiveNote]) -> String {
    if indices.is_empty() {
        return String::new();
    }
    let cap: usize = indices.iter().map(|&i| notes[i].key.len() + 1).sum();
    let mut out = String::with_capacity(cap);
    for (pos, &idx) in indices.iter().enumerate() {
        if pos > 0 {
            out.push(',');
        }
        out.push_str(&notes[idx].key);
    }
    out
}

/// Collect up to `limit` schema prompts for `lens` without writing notices.
pub fn collect_prompts(conn: &Connection, lens: &Lens, limit: usize) -> Result<Vec<SchemaPrompt>> {
    if limit == 0 {
        return Ok(Vec::new());
    }

    let (notes, key_to_idx) = load_active_notes(conn)?;
    if notes.is_empty() {
        return Ok(Vec::new());
    }

    let (covered_notes, handled_tag_parts) = load_coverage(conn, &key_to_idx)?;
    let (notices, rejected) = load_recurrence_history(conn)?;

    let mut ignored_tag_parts: BTreeSet<Vec<String>> = BTreeSet::new();
    for raw_tag in &lens.ignore_tags {
        let parts = crate::ops::ident_parts(raw_tag);
        if !parts.is_empty() {
            ignored_tag_parts.insert(parts);
        }
    }

    let mut candidates: Vec<RankedCandidate> = Vec::new();

    // Source 1: Staged schema_candidates passing formation gates, of any kind.
    {
        let mut stmt = conn.prepare(
            "SELECT cluster_sig, member_keys_json, density, stability_count, reward_hits, kind \
             FROM schema_candidates ORDER BY cluster_sig ASC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, f64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, String>(5)?,
            ))
        })?;
        let mut raw_rows = Vec::new();
        let mut all_sigs = Vec::new();
        for row in rows {
            let item = row?;
            all_sigs.push(item.0.clone());
            raw_rows.push(item);
        }

        for (cluster_sig, member_keys_json, density, stability, reward, kind) in raw_rows {
            if !gates_pass(density, stability, reward) {
                continue;
            }
            let raw_members: Vec<String> =
                serde_json::from_str(&member_keys_json).unwrap_or_default();
            let mut knowledge_indices: Vec<usize> = Vec::new();
            for raw in &raw_members {
                let Some((raw_kind, raw_id)) = raw.trim().split_once(':') else {
                    continue;
                };
                let Ok(canon_kind) = normalize_note_kind(raw_kind) else {
                    continue;
                };
                let Ok(id) = raw_id.parse::<i64>() else {
                    continue;
                };
                if let Some(idx) = key_to_idx.get(&(canon_kind, id)).copied() {
                    knowledge_indices.push(idx);
                }
            }
            knowledge_indices.sort_unstable();
            knowledge_indices.dedup();
            if knowledge_indices.is_empty() {
                continue;
            }

            let uncovered_indices: Vec<usize> = knowledge_indices
                .iter()
                .copied()
                .filter(|idx| !covered_notes.contains(idx))
                .collect();

            if (uncovered_indices.len() as f64) / (knowledge_indices.len() as f64) < COVERED_SHARE {
                continue;
            }

            let sig_prefix = unique_candidate_prefix(&cluster_sig, &all_sigs);
            let key = format!("candidate:{sig_prefix}");
            let notice_key = format!("candidate:{cluster_sig}");

            if let Some(rc) = try_qualify_prompt(
                conn,
                key,
                notice_key,
                PromptSourceKind::Candidate,
                sig_prefix.to_string(),
                Some(kind),
                uncovered_indices,
                &notes,
                &notices,
                &rejected,
            )? {
                candidates.push(rc);
            }
        }
    }

    // Source 2: Subject-tag threads across active notes.
    {
        let mut tag_groups: BTreeMap<Vec<String>, Vec<usize>> = BTreeMap::new();
        let mut tagged_active_notes = 0usize;

        for (idx, note) in notes.iter().enumerate() {
            let mut seen_on_note: BTreeSet<Vec<String>> = BTreeSet::new();
            for raw_tag in &note.tags {
                let parts = crate::ops::ident_parts(raw_tag);
                if !parts.is_empty() && seen_on_note.insert(parts.clone()) {
                    tag_groups.entry(parts).or_default().push(idx);
                }
            }
            if !seen_on_note.is_empty() {
                tagged_active_notes += 1;
            }
        }

        if tagged_active_notes > 0 {
            let non_ignored_tag_count = tag_groups
                .keys()
                .filter(|parts| !ignored_tag_parts.contains(*parts))
                .count();
            for (parts, all_indices) in tag_groups {
                if all_indices.len() < MIN_EVIDENCE {
                    continue;
                }
                if ignored_tag_parts.contains(&parts) {
                    continue;
                }
                if non_ignored_tag_count > 1
                    && (all_indices.len() as f64) / (tagged_active_notes as f64) > THREAD_MAX_SHARE
                {
                    continue;
                }
                if handled_tag_parts.contains(&parts) {
                    continue;
                }

                let uncovered_indices: Vec<usize> = all_indices
                    .into_iter()
                    .filter(|idx| !covered_notes.contains(idx))
                    .collect();

                let tag = parts.join("-");
                let key = format!("thread:{tag}");
                let notice_key = key.clone();

                if let Some(rc) = try_qualify_prompt(
                    conn,
                    key,
                    notice_key,
                    PromptSourceKind::Thread,
                    tag,
                    None,
                    uncovered_indices,
                    &notes,
                    &notices,
                    &rejected,
                )? {
                    candidates.push(rc);
                }
            }
        }
    }

    // Rank by uncovered notes DESC, files DESC, days DESC, key ASC.
    candidates.sort_by(|a, b| {
        b.spread
            .notes
            .cmp(&a.spread.notes)
            .then_with(|| b.spread.files.cmp(&a.spread.files))
            .then_with(|| b.spread.days.cmp(&a.spread.days))
            .then_with(|| a.key.cmp(&b.key))
    });

    // Inter-prompt Jaccard >= 0.7 deduplication and top-K truncation.
    let mut accepted: Vec<RankedCandidate> = Vec::with_capacity(limit.min(candidates.len()));
    for cand in candidates {
        let cand_strs: Vec<&str> = cand.uncovered_keys.iter().map(String::as_str).collect();
        let overlaps_higher = accepted.iter().any(|existing| {
            let existing_strs: Vec<&str> =
                existing.uncovered_keys.iter().map(String::as_str).collect();
            jaccard(&cand_strs, &existing_strs) >= JACCARD_IDENTITY
        });
        if overlaps_higher {
            continue;
        }
        accepted.push(cand);
        if accepted.len() == limit {
            break;
        }
    }

    // Materialize final SchemaPrompt entries in lens reading order.
    let mut out = Vec::with_capacity(accepted.len());
    for item in accepted {
        let mut ordered_indices = item.uncovered_indices;
        ordered_indices.sort_by(|&a, &b| compare_notes_for_order(&notes[a], &notes[b], lens.order));

        let mut evidence = Vec::with_capacity(ordered_indices.len());
        for &idx in &ordered_indices {
            let n = &notes[idx];
            evidence.push(PromptEvidence {
                kind: n.kind.to_string(),
                id: n.id,
                summary: n.summary.clone(),
                anchors: n.anchors.clone(),
                timestamp: n.timestamp.clone(),
            });
        }

        let subject = match item.source_kind {
            PromptSourceKind::Thread => item.source_ref.as_str(),
            PromptSourceKind::Candidate => "these notes",
        };
        let questions: Vec<String> = lens
            .questions
            .iter()
            .map(|q| q.replace("{subject}", subject))
            .collect();

        let members_csv = join_note_keys(&ordered_indices, &notes);
        let suggested_name = match item.source_kind {
            PromptSourceKind::Thread => sanitize_kebab(&item.source_ref),
            PromptSourceKind::Candidate => suggest_schema_name(conn, &item.uncovered_keys)?,
        };
        let propose_command = match item.source_kind {
            PromptSourceKind::Thread => format!(
                "engrams schema propose --thread {} --lens {} --members {} --name {} --statement \"<claim>\" --summary \"When <condition>: <directive>\"",
                item.source_ref, lens.name, members_csv, suggested_name
            ),
            PromptSourceKind::Candidate => format!(
                "engrams schema propose --from {} --lens {} --members {} --name {} --statement \"<claim>\" --summary \"When <condition>: <directive>\"",
                item.source_ref, lens.name, members_csv, suggested_name
            ),
        };

        out.push(SchemaPrompt {
            key: item.key,
            source: PromptSource {
                kind: item.source_kind.as_str().to_string(),
                source_ref: item.source_ref,
            },
            label: item.label,
            lens: lens.name.clone(),
            questions,
            evidence,
            spread: item.spread,
            propose_command,
            notice_key: item.notice_key,
            sorted_evidence_keys: item.uncovered_keys,
        });
    }

    Ok(out)
}

#[allow(clippy::too_many_arguments)]
fn try_qualify_prompt(
    conn: &Connection,
    key: String,
    notice_key: String,
    source_kind: PromptSourceKind,
    source_ref: String,
    label: Option<String>,
    uncovered_indices: Vec<usize>,
    notes: &[ActiveNote],
    notices: &[PriorNotice],
    rejected: &[PriorRejected],
) -> Result<Option<RankedCandidate>> {
    if uncovered_indices.len() < MIN_EVIDENCE {
        return Ok(None);
    }

    let spread = compute_spread(&uncovered_indices, notes);
    if spread.files < 2 && spread.days < 2 {
        return Ok(None);
    }

    let uncovered_keys: Vec<String> = uncovered_indices
        .iter()
        .map(|&idx| notes[idx].key.clone())
        .collect();
    let uncovered_strs: Vec<&str> = uncovered_keys.iter().map(String::as_str).collect();

    if claimed_territory(conn, &uncovered_keys)?.is_some() {
        return Ok(None);
    }

    if is_held_back(
        &notice_key,
        &uncovered_indices,
        &uncovered_strs,
        notes,
        notices,
        rejected,
    ) {
        return Ok(None);
    }

    Ok(Some(RankedCandidate {
        key,
        notice_key,
        source_kind,
        source_ref,
        label,
        uncovered_indices,
        uncovered_keys,
        spread,
    }))
}

/// Record shown prompts in `schema_prompt_notices` at session close.
/// Upserts on `source_key` so revived threads or candidates refresh their
/// evidence snapshot and `notified_at` timestamp (review finding F-1).
pub fn record_prompt_notices(
    conn: &Connection,
    prompts: &[SchemaPrompt],
    notified_at: &str,
) -> Result<()> {
    if prompts.is_empty() {
        return Ok(());
    }
    let mut stmt = conn.prepare(
        "INSERT INTO schema_prompt_notices (source_key, evidence_json, notified_at) \
         VALUES (?1, ?2, ?3) \
         ON CONFLICT(source_key) DO UPDATE SET \
           evidence_json = excluded.evidence_json, \
           notified_at = excluded.notified_at",
    )?;
    for p in prompts {
        let evidence_json = serde_json::to_string(&p.sorted_evidence_keys)?;
        stmt.execute(params![&p.notice_key, &evidence_json, notified_at])?;
    }
    Ok(())
}

/// Handler for `engrams schema prompts [--lens <name>] [--limit <n>]`.
/// Read-only preview: never writes to `schema_prompt_notices` (AC-8).
pub fn prompts_cmd(
    conn: &Connection,
    lens_name: Option<&str>,
    limit: usize,
    workspace_root: Option<&Path>,
) -> Result<Value> {
    let lens = load_lens(lens_name, workspace_root)?;
    let prompts = collect_prompts(conn, &lens, limit)?;
    Ok(json!({
        "status": "success",
        "lens": lens.name,
        "prompts": prompts,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_natural_cmp_orders_chapters_numerically() {
        let mut paths = vec![
            "manuscript/ch10.md",
            "manuscript/ch2.md",
            "manuscript/ch1.md",
            "manuscript/ch20.md",
            "manuscript/ch02.md",
        ];
        paths.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(
            paths,
            vec![
                "manuscript/ch1.md",
                "manuscript/ch2.md",
                "manuscript/ch02.md",
                "manuscript/ch10.md",
                "manuscript/ch20.md",
            ]
        );
    }

    #[test]
    fn test_builtin_lenses_parse_cleanly() {
        let general = load_lens(None, None).unwrap();
        assert_eq!(general.name, "general");
        assert_eq!(general.order, Order::Time);
        assert!(!general.questions.is_empty());

        let fiction = load_lens(Some("fiction"), None).unwrap();
        assert_eq!(fiction.name, "fiction");
        assert_eq!(fiction.order, Order::Story);
        assert_eq!(fiction.questions.len(), 4);
        assert!(fiction.ignore_tags.contains(&"plot".to_string()));
        assert!(fiction.ignore_tags.contains(&"chapter".to_string()));

        let software = load_lens(Some("software"), None).unwrap();
        assert_eq!(software.name, "software");
        assert_eq!(software.order, Order::Time);
        assert!(!software.questions.is_empty());
    }

    #[test]
    fn test_unknown_and_traversal_lens_rejected() {
        let err = load_lens(Some("fantasy"), None).unwrap_err().to_string();
        assert_eq!(
            err,
            "unknown lens 'fantasy': available general, fiction, software"
        );

        let traversal = load_lens(Some("../fiction"), None).unwrap_err().to_string();
        assert!(traversal.contains("unknown lens '../fiction'"));
    }

    #[test]
    fn test_record_prompt_notices_holds_back_until_newer_note_and_upserts() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::schema::SCHEMA).unwrap();

        for (id, ts, path) in [
            (1i64, "2026-08-01T10:00:00Z", "manuscript/ch01.md"),
            (2i64, "2026-08-02T10:00:00Z", "manuscript/ch02.md"),
            (3i64, "2026-08-03T10:00:00Z", "manuscript/ch03.md"),
            (4i64, "2026-08-04T10:00:00Z", "manuscript/ch04.md"),
        ] {
            conn.execute(
                "INSERT INTO decisions (id, uuid, timestamp, summary, tags, status, archived) \
                 VALUES (?1, ?2, ?3, ?4, '[\"mara\"]', 'active', 0)",
                params![id, format!("u-{id}"), ts, format!("Mara beat {id}")],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO item_anchors (item_type, item_id, path, timestamp) \
                 VALUES ('decision', ?1, ?2, ?3)",
                params![id, path, ts],
            )
            .unwrap();
        }

        let lens = load_lens(Some("fiction"), None).unwrap();
        let first = collect_prompts(&conn, &lens, PROMPTS_PER_CLOSE).unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].key, "thread:mara");

        // Record notice at 2026-08-10; next collection without a newer note returns empty.
        record_prompt_notices(&conn, &first, "2026-08-10T00:00:00Z").unwrap();
        assert!(collect_prompts(&conn, &lens, PROMPTS_PER_CLOSE)
            .unwrap()
            .is_empty());

        // Removing a note (archiving decision:4) does not revive the prompt.
        conn.execute("UPDATE decisions SET archived = 1 WHERE id = 4", [])
            .unwrap();
        assert!(collect_prompts(&conn, &lens, PROMPTS_PER_CLOSE)
            .unwrap()
            .is_empty());

        // Adding a newly created note (timestamp > notified_at) revives the prompt,
        // and recording notices again upserts the existing source_key cleanly.
        conn.execute(
            "INSERT INTO decisions (id, uuid, timestamp, summary, tags, status, archived) \
             VALUES (5, 'u-5', '2026-08-15T10:00:00Z', 'Mara beat 5', '[\"mara\"]', 'active', 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO item_anchors (item_type, item_id, path, timestamp) \
             VALUES ('decision', 5, 'manuscript/ch05.md', '2026-08-15T10:00:00Z')",
            [],
        )
        .unwrap();

        let revived = collect_prompts(&conn, &lens, PROMPTS_PER_CLOSE).unwrap();
        assert_eq!(revived.len(), 1);
        assert_eq!(revived[0].spread.notes, 4);

        record_prompt_notices(&conn, &revived, "2026-08-20T00:00:00Z").unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_prompt_notices", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(count, 1);
    }
}
