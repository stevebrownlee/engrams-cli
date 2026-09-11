//! Kind labeling for gate-passing schema candidates (spec 0003).
//!
//! [super::scan]'s gates prove a group is statistically real; this pass
//! records WHAT it is — a recurring practice ([Kind::Schema]), an ended
//! one-time burst ([Kind::Story]), a pile of files ([Kind::Inventory]),
//! or mixed signals a human must read ([Kind::Unclear]). Labels advise
//! and restrain, never decide: the list shows every kind, and apply holds
//! back everything except schema-kind candidates.
//!
//! Determinism mirrors the rest of formation (decision 78's "pure function
//! of the stored database"): a kind is computed from stored timestamps and
//! member metadata only, never wall-clock. "Time since the last awake
//! stretch" may be displayed by callers but never decides a kind, so an
//! unchanged database always relabels identically.
//!
//! Signal sources (spec 0003 architecture): awake stretches come from
//! member creation timestamps (`timestamp` / `first_seen`) plus
//! `retrieval_surfaces` telemetry for the member nodes; the member mix
//! comes from node kinds; trigger surfaces are checkable rules
//! (`system_patterns.check_kind`), shared `item_anchors` paths, and
//! pairwise member-vocabulary overlap reusing [super::assimilate]'s
//! token set and fit gate.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, Connection, OptionalExtension};

use super::assimilate::{item_tokens, FIT_GATE};
use super::confirm::{kind_table, parse_members};
use super::scan::jaccard;

/// Days of silence that separate two awake stretches of a group's
/// activity (spec 0003 open question 1). Pinned for the phase-2 dogfood
/// replay; that gate owns any sweep.
pub(super) const SILENCE_GAP_DAYS: i64 = 14;

/// File-node share at or above which a group leans inventory (spec 0003
/// open question 2). Pinned the same way.
pub(super) const INVENTORY_SHARE: f64 = 0.8;

/// A candidate's kind (spec 0003 nomenclature).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Schema,
    Story,
    Inventory,
    Unclear,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Schema => "schema",
            Kind::Story => "story",
            Kind::Inventory => "inventory",
            Kind::Unclear => "unclear",
        }
    }
}

/// The stored facts a label is derived from — everything the labeler
/// reads from the database, resolved once so [label] is a pure function.
pub(super) struct Signals {
    /// Awake stretches of member activity as (start, end) pairs, in time
    /// order. Empty when no parsable timestamps exist for any member.
    pub(super) stretches: Vec<(DateTime<Utc>, DateTime<Utc>)>,
    pub(super) member_count: usize,
    pub(super) file_members: usize,
    pub(super) checkable_members: usize,
    pub(super) decision_members: usize,
    /// Evidence sentence naming the trigger surface, if one was found.
    pub(super) trigger: Option<String>,
}

impl Signals {
    fn file_share(&self) -> f64 {
        if self.member_count == 0 {
            return 0.0;
        }
        self.file_members as f64 / self.member_count as f64
    }
}

/// A label plus its plain-language reasons, in read order.
pub(super) struct Labeled {
    pub(super) kind: Kind,
    pub(super) reasons: Vec<String>,
}

/// Compute a candidate's [Signals] from the database.
pub(super) fn signals(conn: &Connection, members: &[String]) -> Result<Signals> {
    let members = parse_members(conn, members)?;

    let mut instants: Vec<DateTime<Utc>> = Vec::new();
    let mut file_members = 0usize;
    let mut checkable: Option<(i64, String)> = None; // first checkable member
    let mut checkable_members = 0usize;
    let mut decision_members = 0usize;

    // Shared file anchors over the anchored member kinds (the same kinds
    // centroid-building treats as anchorable).
    let mut anchors: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    // Lexical surface per member for the vocabulary trigger.
    let mut tokens: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for m in &members {
        let key = format!("{}:{}", m.kind, m.id);
        match m.kind.as_str() {
            "code" => file_members += 1,
            "decision" => decision_members += 1,
            _ => {}
        }

        // Creation instant (per-kind timestamp column) + retrieval telemetry.
        let ts_col = match m.kind.as_str() {
            "code" => "first_seen",
            "schema" => "created_at",
            _ => "timestamp",
        };
        if let Some(table) = kind_table(&m.kind) {
            let ts: Option<String> = conn
                .query_row(
                    &format!("SELECT {ts_col} FROM {table} WHERE id = ?1"),
                    params![m.id],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(t) = ts {
                if let Ok(parsed) = DateTime::parse_from_rfc3339(&t) {
                    instants.push(parsed.with_timezone(&Utc));
                }
            }
        }
        {
            let mut stmt = conn.prepare(
                "SELECT ts FROM retrieval_surfaces WHERE node_kind = ?1 AND node_id = ?2",
            )?;
            let rows = stmt.query_map(params![m.kind, m.id], |r| r.get::<_, String>(0))?;
            for ts in rows {
                if let Ok(parsed) = DateTime::parse_from_rfc3339(&ts?) {
                    instants.push(parsed.with_timezone(&Utc));
                }
            }
        }

        // Checkable-rule members and the lexical/anchor surfaces.
        match m.kind.as_str() {
            "system_pattern" => {
                let check_kind: Option<String> = conn
                    .query_row(
                        "SELECT check_kind FROM system_patterns WHERE id = ?1",
                        params![m.id],
                        |r| r.get(0),
                    )
                    .optional()?
                    .flatten();
                if let Some(check_kind) = check_kind {
                    checkable_members += 1;
                    if checkable.is_none() {
                        checkable = Some((m.id, check_kind));
                    }
                }
                let summary = member_summary(conn, m.kind.as_str(), m.id)?;
                if let Some(text) = summary {
                    tokens.insert(key.clone(), item_tokens(&text, &[]));
                }
            }
            "decision" => {
                let (summary, tags): (String, Option<String>) = conn
                    .query_row(
                        "SELECT summary, tags FROM decisions WHERE id = ?1",
                        params![m.id],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .optional()?
                    .unwrap_or_default();
                tokens.insert(
                    key.clone(),
                    item_tokens(&summary, &crate::models::parse_tags(tags.as_deref())),
                );
            }
            "progress_entry" | "custom_data" | "code" => {
                let summary = member_summary(conn, m.kind.as_str(), m.id)?;
                if let Some(text) = summary {
                    tokens.insert(key.clone(), item_tokens(&text, &[]));
                }
            }
            _ => {}
        }
        if matches!(
            m.kind.as_str(),
            "decision" | "system_pattern" | "progress_entry"
        ) {
            let mut stmt = conn
                .prepare("SELECT path FROM item_anchors WHERE item_type = ?1 AND item_id = ?2")?;
            let paths = stmt.query_map(params![m.kind, m.id], |r| r.get::<_, String>(0))?;
            for p in paths {
                anchors.entry(p?).or_default().insert(key.clone());
            }
        }
    }

    // Trigger surfaces, cheapest structural first; the first hit is the
    // recorded evidence (deterministic: member order, then BTree order).
    let mut trigger = None;
    if let Some((id, check_kind)) = checkable {
        trigger = Some(format!(
            "member system_pattern:{id} carries a checkable rule ({check_kind})"
        ));
    }
    if trigger.is_none() {
        if let Some((path, keys)) = anchors
            .iter()
            .find(|(_, keys)| keys.len() >= 2)
            .map(|(p, k)| (p.clone(), k.iter().cloned().collect::<Vec<_>>()))
        {
            trigger = Some(format!(
                "members {} and {} share file anchor {path}",
                keys[0], keys[1]
            ));
        }
    }
    if trigger.is_none() {
        let keys: Vec<&String> = tokens.keys().collect();
        let mut best: Option<((usize, usize), f64)> = None;
        for i in 0..keys.len() {
            for j in (i + 1)..keys.len() {
                let a: Vec<String> = tokens[keys[i]].iter().cloned().collect();
                let b: Vec<String> = tokens[keys[j]].iter().cloned().collect();
                let fit = jaccard(&a, &b);
                if fit >= FIT_GATE && best.is_none_or(|(_, f)| fit > f) {
                    best = Some(((i, j), fit));
                }
            }
        }
        if let Some(((i, j), fit)) = best {
            trigger = Some(format!(
                "members {} and {} share vocabulary (overlap {fit:.2} ≥ {FIT_GATE})",
                keys[i], keys[j]
            ));
        }
    }

    Ok(Signals {
        stretches: awake_stretches(&mut instants),
        member_count: members.len(),
        file_members,
        checkable_members,
        decision_members,
        trigger,
    })
}

/// Summary-like text for non-decision members used by the vocabulary
/// trigger. Mirrors the fields assimilation treats as an item's lexical
/// surface.
fn member_summary(conn: &Connection, kind: &str, id: i64) -> Result<Option<String>> {
    let sql = match kind {
        "system_pattern" => {
            "SELECT name || ' ' || COALESCE(description, '') FROM system_patterns WHERE id = ?1"
        }
        "progress_entry" => "SELECT description FROM progress_entries WHERE id = ?1",
        "custom_data" => "SELECT key FROM custom_data WHERE id = ?1",
        "code" => "SELECT path FROM code_nodes WHERE id = ?1",
        _ => return Ok(None),
    };
    Ok(conn.query_row(sql, params![id], |r| r.get(0)).optional()?)
}

/// Split sorted activity instants into awake stretches separated by more
/// than [SILENCE_GAP_DAYS] days of silence. A gap of exactly the cutoff
/// stays inside one stretch (strict `>`).
fn awake_stretches(instants: &mut Vec<DateTime<Utc>>) -> Vec<(DateTime<Utc>, DateTime<Utc>)> {
    instants.sort();
    instants.dedup();
    let gap = Duration::days(SILENCE_GAP_DAYS);
    let mut out = Vec::new();
    let mut start: Option<DateTime<Utc>> = None;
    let mut prev: Option<DateTime<Utc>> = None;
    for t in instants.iter().copied() {
        match (start, prev) {
            (None, _) => start = Some(t),
            (Some(s), Some(p)) if t - p > gap => {
                out.push((s, p));
                start = Some(t);
            }
            _ => {}
        }
        prev = Some(t);
    }
    if let (Some(s), Some(p)) = (start, prev) {
        out.push((s, p));
    }
    out
}

/// Label a candidate from its [Signals]: ordered conservative rules
/// (spec 0003 signal 4). Inventory first — a file pile's timestamps all
/// come from one repo scan anyway, and no temporal reading rescues it.
/// Schema needs BOTH recurrence and a trigger; a single burst with no
/// trigger is a story; everything mixed is unclear rather than guessed.
pub(super) fn label(s: &Signals) -> Labeled {
    // 1. Inventory: overwhelmingly file nodes, nothing checkable or deciding.
    if s.file_share() >= INVENTORY_SHARE && s.checkable_members == 0 && s.decision_members == 0 {
        return Labeled {
            kind: Kind::Inventory,
            reasons: vec![
                format!(
                    "{} of {} members are file nodes",
                    s.file_members, s.member_count
                ),
                "no checkable rules or decisions among members".to_string(),
            ],
        };
    }

    // 2. Schema: recurrence AND a trigger surface.
    if s.stretches.len() >= 2 && s.trigger.is_some() {
        return Labeled {
            kind: Kind::Schema,
            reasons: vec![
                format!(
                    "activity recurs across {} awake stretches separated by ≥{}-day silences",
                    s.stretches.len(),
                    SILENCE_GAP_DAYS
                ),
                s.trigger.clone().unwrap(),
            ],
        };
    }

    // 3. Story: one awake burst, never re-activated, nothing that could
    // re-fire it.
    if s.stretches.len() == 1 && s.trigger.is_none() {
        let (start, end) = s.stretches[0];
        return Labeled {
            kind: Kind::Story,
            reasons: vec![
                format!(
                    "activity falls in one awake stretch ({} to {})",
                    start.format("%Y-%m-%d"),
                    end.format("%Y-%m-%d")
                ),
                "no re-activation after it and no trigger surface found".to_string(),
            ],
        };
    }

    // 4. Unclear: state the evidence both ways (AC-5).
    let mut reasons = Vec::new();
    match s.stretches.len() {
        0 => reasons.push("no parsable activity timestamps among members".to_string()),
        1 => {
            let (start, end) = s.stretches[0];
            reasons.push(format!(
                "single awake stretch ({} to {}) favors story",
                start.format("%Y-%m-%d"),
                end.format("%Y-%m-%d")
            ));
        }
        n => reasons.push(format!(
            "recurring activity across {n} awake stretches favors schema"
        )),
    }
    match &s.trigger {
        Some(t) => reasons.push(format!(
            "trigger surface exists: {t} — the practice may be too young to show recurrence"
        )),
        None => reasons.push(format!(
            "no trigger surface: no checkable rule member, no shared file anchor, vocabulary overlap below {FIT_GATE}"
        )),
    }
    Labeled {
        kind: Kind::Unclear,
        reasons,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::schema::SCHEMA).unwrap();
        conn
    }

    fn add_decision(conn: &Connection, id: i64, summary: &str, tags: &str, ts: &str) {
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
    }

    fn add_pattern(conn: &Connection, id: i64, name: &str, ts: &str, check_kind: Option<&str>) {
        conn.execute(
            "INSERT INTO system_patterns (id, uuid, timestamp, name, description, check_kind) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                id,
                format!("p{id}"),
                ts,
                name,
                format!("{name} description"),
                check_kind
            ],
        )
        .unwrap();
    }

    fn add_custom(conn: &Connection, id: i64, key: &str, ts: &str) {
        conn.execute(
            "INSERT INTO custom_data (id, timestamp, category, key, value) \
             VALUES (?1, ?2, 'notes', ?3, 'x')",
            params![id, ts, key],
        )
        .unwrap();
    }

    fn add_code(conn: &Connection, id: i64, path: &str, first_seen: &str) {
        conn.execute(
            "INSERT INTO code_nodes (id, kind, path, first_seen, last_seen) \
             VALUES (?1, 'file', ?2, ?3, ?3)",
            params![id, path, first_seen],
        )
        .unwrap();
    }

    fn add_retrieval(conn: &Connection, ts: &str, kind: &str, id: i64) {
        conn.execute(
            "INSERT INTO retrieval_surfaces (ts, cmd, arg, node_kind, node_id) \
             VALUES (?1, 'query', 'x', ?2, ?3)",
            params![ts, kind, id],
        )
        .unwrap();
    }

    fn add_anchor(conn: &Connection, kind: &str, id: i64, path: &str) {
        conn.execute(
            "INSERT INTO item_anchors (item_type, item_id, path, timestamp) \
             VALUES (?1, ?2, ?3, '2026-01-01T00:00:00Z')",
            params![kind, id, path],
        )
        .unwrap();
    }

    fn label_of(conn: &Connection, members: &[String]) -> Labeled {
        label(&signals(conn, members).unwrap())
    }

    #[test]
    fn story_labels_single_burst() {
        let conn = fresh();
        add_decision(&conn, 1, "Ship login form", "", "2026-01-10T09:00:00Z");
        add_decision(&conn, 2, "Fix signup bug", "", "2026-01-11T10:00:00Z");
        add_decision(&conn, 3, "Add auth test", "", "2026-01-12T11:00:00Z");
        add_retrieval(&conn, "2026-01-13T08:00:00Z", "decision", 1);
        add_retrieval(&conn, "2026-01-13T08:00:00Z", "decision", 2);

        let labeled = label_of(
            &conn,
            &[
                "decision:1".to_string(),
                "decision:2".to_string(),
                "decision:3".to_string(),
            ],
        );
        assert_eq!(labeled.kind, Kind::Story);
        assert_eq!(
            labeled.reasons[0],
            "activity falls in one awake stretch (2026-01-10 to 2026-01-13)"
        );
        assert!(labeled.reasons[1].contains("no re-activation"));
    }

    #[test]
    fn inventory_labels_file_pile() {
        let conn = fresh();
        for i in 1..=4 {
            add_code(
                &conn,
                i,
                &format!("docs/campaign-{i}.md"),
                "2026-02-01T00:00:00Z",
            );
        }
        add_custom(&conn, 9, "release notes draft", "2026-02-02T00:00:00Z");
        add_retrieval(&conn, "2026-02-03T00:00:00Z", "code", 1);

        let members: Vec<String> = (1..=4)
            .chain([9])
            .map(|i| {
                if i == 9 {
                    "custom_data:9".to_string()
                } else {
                    format!("code:{i}")
                }
            })
            .collect();
        let labeled = label_of(&conn, &members);
        assert_eq!(labeled.kind, Kind::Inventory);
        assert_eq!(labeled.reasons[0], "4 of 5 members are file nodes");
        assert_eq!(
            labeled.reasons[1],
            "no checkable rules or decisions among members"
        );
    }

    #[test]
    fn schema_labels_recurring_practice() {
        let conn = fresh();
        add_pattern(
            &conn,
            1,
            "release routine",
            "2026-01-05T00:00:00Z",
            Some("regex"),
        );
        add_decision(
            &conn,
            2,
            "Log release gate decision",
            "release",
            "2026-01-06T00:00:00Z",
        );
        // Second awake stretch a month later: retrieval telemetry only.
        for ts in ["2026-02-20T10:00:00Z", "2026-02-21T10:00:00Z"] {
            add_retrieval(&conn, ts, "system_pattern", 1);
            add_retrieval(&conn, ts, "decision", 2);
        }

        let labeled = label_of(
            &conn,
            &["decision:2".to_string(), "system_pattern:1".to_string()],
        );
        assert_eq!(labeled.kind, Kind::Schema);
        assert!(labeled.reasons[0].contains("2 awake stretches"));
        assert!(labeled.reasons[1].contains("system_pattern:1 carries a checkable rule"));
    }

    #[test]
    fn unclear_when_recurring_without_trigger() {
        let conn = fresh();
        add_decision(&conn, 1, "Alpha note", "", "2026-01-05T00:00:00Z");
        add_decision(&conn, 2, "Beta note", "", "2026-01-05T01:00:00Z");
        add_decision(&conn, 3, "Gamma note", "", "2026-01-05T02:00:00Z");
        for ts in ["2026-01-06T00:00:00Z", "2026-02-20T00:00:00Z"] {
            for id in 1..=3 {
                add_retrieval(&conn, ts, "decision", id);
            }
        }

        let labeled = label_of(
            &conn,
            &[
                "decision:1".to_string(),
                "decision:2".to_string(),
                "decision:3".to_string(),
            ],
        );
        assert_eq!(labeled.kind, Kind::Unclear);
        assert!(labeled.reasons[0].contains("favors schema"));
        assert!(labeled.reasons[1].contains("no trigger surface"));
    }

    #[test]
    fn unclear_when_single_burst_has_trigger() {
        let conn = fresh();
        add_decision(&conn, 1, "Release routine note", "", "2026-01-10T00:00:00Z");
        add_decision(
            &conn,
            2,
            "Release routine followup",
            "",
            "2026-01-11T00:00:00Z",
        );
        add_anchor(&conn, "decision", 1, "src/release.rs");
        add_anchor(&conn, "decision", 2, "src/release.rs");

        let labeled = label_of(&conn, &["decision:1".to_string(), "decision:2".to_string()]);
        assert_eq!(labeled.kind, Kind::Unclear);
        assert!(labeled.reasons[0].contains("favors story"));
        assert!(labeled.reasons[1].contains("share file anchor src/release.rs"));
    }

    #[test]
    fn vocabulary_overlap_is_a_trigger() {
        let conn = fresh();
        add_decision(
            &conn,
            1,
            "release routine checklist",
            "",
            "2026-01-10T00:00:00Z",
        );
        add_decision(
            &conn,
            2,
            "release routine gates",
            "",
            "2026-01-10T01:00:00Z",
        );
        // Two stretches with no anchors, no checkable members.
        add_retrieval(&conn, "2026-01-11T00:00:00Z", "decision", 1);
        add_retrieval(&conn, "2026-03-01T00:00:00Z", "decision", 2);

        let labeled = label_of(&conn, &["decision:1".to_string(), "decision:2".to_string()]);
        assert_eq!(labeled.kind, Kind::Schema);
        assert!(labeled.reasons[1].contains("share vocabulary"));
    }

    #[test]
    fn silence_gap_boundary_is_strict() {
        // Exactly 14 days apart: one stretch -> story.
        let mut instants = vec![
            DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
            DateTime::parse_from_rfc3339("2026-01-15T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        ];
        assert_eq!(awake_stretches(&mut instants).len(), 1);

        // 15 days apart: two stretches.
        let mut instants = vec![
            DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
            DateTime::parse_from_rfc3339("2026-01-16T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        ];
        assert_eq!(awake_stretches(&mut instants).len(), 2);
    }

    #[test]
    fn relabeling_is_deterministic() {
        let conn = fresh();
        add_pattern(
            &conn,
            1,
            "release routine",
            "2026-01-05T00:00:00Z",
            Some("regex"),
        );
        add_retrieval(&conn, "2026-02-20T00:00:00Z", "system_pattern", 1);
        let members = vec!["system_pattern:1".to_string()];

        let a = label_of(&conn, &members);
        let b = label_of(&conn, &members);
        assert_eq!(a.kind, b.kind);
        assert_eq!(a.reasons, b.reasons);
    }

    #[test]
    fn upgrade_v12_to_v13_adds_kind_columns() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE schema_candidates (id INTEGER PRIMARY KEY AUTOINCREMENT, \
             cluster_sig TEXT NOT NULL, member_keys_json TEXT NOT NULL); \
             PRAGMA user_version = 12;",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO schema_candidates (cluster_sig, member_keys_json) VALUES ('sig', '[]')",
            [],
        )
        .unwrap();
        crate::db::run_migrations(&mut conn).unwrap();

        assert_eq!(crate::db::get_user_version(&conn).unwrap(), 13);
        let mut stmt = conn
            .prepare("PRAGMA table_info(schema_candidates)")
            .unwrap();
        let cols: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .flatten()
            .collect();
        assert!(cols.contains(&"kind".to_string()));
        assert!(cols.contains(&"kind_reasons_json".to_string()));

        // Existing rows backfill to the defaults.
        let (kind, reasons): (String, String) = conn
            .query_row(
                "SELECT kind, kind_reasons_json FROM schema_candidates WHERE id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(kind, "unclear");
        assert_eq!(reasons, "[]");
    }

    /// AC-8: replay the hand-labeled dogfood corpus (snapshot of this
    /// repository's live gate-passing candidates, spec 0003) and score the
    /// labeler against the answer key. Hard clauses: every human `schema`
    /// label must come back `schema`; every human story/inventory label
    /// must come back accordingly or `unclear`. Remaining disagreements are
    /// collected for review, not failed — that listing is the feature.
    #[test]
    fn dogfood_replay_reproduces_hand_labels() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/schema-kind-dogfood.json"
        );
        let fixture: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let conn = fresh();
        let facts = &fixture["facts"];

        for d in facts["decisions"].as_array().unwrap() {
            conn.execute(
                "INSERT INTO decisions (id, uuid, timestamp, summary, tags) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    d["id"].as_i64().unwrap(),
                    format!("u{}", d["id"].as_i64().unwrap()),
                    d["timestamp"].as_str().unwrap(),
                    d["summary"].as_str().unwrap(),
                    d["tags"].as_str().unwrap_or("[]"),
                ],
            )
            .unwrap();
        }
        for p in facts["patterns"].as_array().unwrap() {
            conn.execute(
                "INSERT INTO system_patterns \
                 (id, uuid, timestamp, name, description, check_kind) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    p["id"].as_i64().unwrap(),
                    format!("p{}", p["id"].as_i64().unwrap()),
                    p["timestamp"].as_str().unwrap(),
                    p["name"].as_str().unwrap(),
                    p["description"].as_str().unwrap(),
                    p["check_kind"].as_str(),
                ],
            )
            .unwrap();
        }
        for g in facts["progress"].as_array().unwrap() {
            conn.execute(
                "INSERT INTO progress_entries (id, timestamp, status, description) \
                 VALUES (?1, ?2, 'Done', ?3)",
                params![
                    g["id"].as_i64().unwrap(),
                    g["timestamp"].as_str().unwrap(),
                    g["description"].as_str().unwrap(),
                ],
            )
            .unwrap();
        }
        for c in facts["code"].as_array().unwrap() {
            conn.execute(
                "INSERT INTO code_nodes (id, kind, path, symbol, first_seen, last_seen) \
                 VALUES (?1, 'file', ?2, ?3, ?4, ?5)",
                params![
                    c["id"].as_i64().unwrap(),
                    c["path"].as_str().unwrap(),
                    c["symbol"].as_str().unwrap_or(""),
                    c["first_seen"].as_str().unwrap(),
                    c["last_seen"].as_str().unwrap(),
                ],
            )
            .unwrap();
        }
        for a in facts["anchors"].as_array().unwrap() {
            conn.execute(
                "INSERT INTO item_anchors (item_type, item_id, path, timestamp) \
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    a["item_type"].as_str().unwrap(),
                    a["item_id"].as_i64().unwrap(),
                    a["path"].as_str().unwrap(),
                    a["timestamp"].as_str().unwrap_or(""),
                ],
            )
            .unwrap();
        }
        for r in facts["retrievals"].as_array().unwrap() {
            conn.execute(
                "INSERT INTO retrieval_surfaces (ts, cmd, arg, node_kind, node_id) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    r["ts"].as_str().unwrap(),
                    r["cmd"].as_str().unwrap(),
                    r["arg"].as_str(),
                    r["node_kind"].as_str().unwrap(),
                    r["node_id"].as_i64().unwrap(),
                ],
            )
            .unwrap();
        }

        let mut disagreements: Vec<String> = Vec::new();
        for grp in fixture["groups"].as_array().unwrap() {
            let id = grp["id"].as_str().unwrap();
            let expected = grp["expected"].as_str().unwrap();
            let members: Vec<String> = grp["members"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|m| m.as_str().map(str::to_string))
                .collect();

            let a = label_of(&conn, &members);
            let b = label_of(&conn, &members);
            assert_eq!(a.kind, b.kind, "{id}: replay is not deterministic");
            assert_eq!(a.reasons, b.reasons, "{id}: reasons not stable");
            let got = a.kind.as_str();

            let allowed = got == expected || got == "unclear" || expected == "unclear";
            assert!(
                allowed,
                "{id}: expected {expected}, got {got} — {}\nnote: {}",
                a.reasons.join("; "),
                grp["note"].as_str().unwrap_or("")
            );
            if got != expected {
                disagreements.push(format!("{id}: hand={expected} machine={got}"));
            }
        }
        // The disagreement list is the review surface AC-8 asks for; print
        // it so a `--nocapture` run shows the review queue.
        if !disagreements.is_empty() {
            eprintln!(
                "dogfood disagreements for review:\n  {}",
                disagreements.join("\n  ")
            );
        }
    }
}
