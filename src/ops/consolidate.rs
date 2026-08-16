//! `engrams consolidate` — promote repeated progress entries into candidate
//! system patterns (v0.11.0 tier-2, spec §4.1).
//!
//! Propose-only by default: clusters progress entries by shared anchor path,
//! reports candidates plus near-duplicate decision merge suggestions, and
//! mutates nothing except consolidation *confirmations* (existing
//! consolidated patterns whose anchor signature gained new evidence get
//! `last_confirmed_at` bumped, S11). `--apply` additionally inserts
//! candidates as patterns tagged `consolidated` with `derived_from` evidence
//! links (S10). Merge suggestions are never applied.
//!
//! Spec deviation (grounded): §4.1 sketches tag clusters over
//! `json_each(progress_entries.tags)`, but `progress_entries` carries no
//! `tags` column (id, timestamp, status, description, parent_id, commit_sha —
//! verified against schema and live DB). Clustering therefore keys on anchor
//! paths, the dimension the S9 acceptance scenarios exercise. Tag-based
//! prefiltering still applies to decision merge suggestions, where tags exist.

use anyhow::Result;
use chrono::{SecondsFormat, Utc};
use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Value};
use std::collections::{BTreeSet, HashSet};
use uuid::Uuid;

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// Only actual activity counts as evidence: planned (Todo) and abandoned
/// (Dropped) entries never confirm or seed a pattern.
const EVIDENCE_FILTER: &str = "pe.status NOT IN ('Todo', 'Dropped')";

/// Max merge suggestions reported per run (spec §4.1.5).
const MAX_MERGE_SUGGESTIONS: usize = 10;

struct Candidate {
    name: String,
    description: String,
    anchors: Vec<String>,
    evidence: Vec<i64>,
    first_seen: String,
    last_seen: String,
    initial_confidence: f64,
}

struct Existing {
    id: i64,
    name: String,
    confirm_ts: String,
    anchors: Vec<String>,
    evidence: Vec<i64>,
}

pub fn handle(conn: &Connection, apply: bool, min_repeats: i64, min_days: i64) -> Result<Value> {
    if min_repeats < 1 {
        anyhow::bail!("--min-repeats must be >= 1");
    }
    if min_days < 1 {
        anyhow::bail!("--min-days must be >= 1");
    }

    // 1. Existing consolidated patterns: anchor/tag signature + evidence.
    let mut pattern_anchors = crate::ops::anchor::anchors_map(conn, "system_pattern")?;
    let mut existing: Vec<Existing> = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT id, name, coalesce(last_confirmed_at, timestamp) FROM system_patterns \
             WHERE archived = 0 AND EXISTS (\
                 SELECT 1 FROM json_each(system_patterns.tags) \
                 WHERE json_each.value = 'consolidated'\
             )",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        for r in rows {
            let (id, name, confirm_ts) = r?;
            existing.push(Existing {
                id,
                name,
                confirm_ts,
                anchors: pattern_anchors.remove(&id).unwrap_or_default(),
                evidence: evidence_ids(conn, id)?,
            });
        }
    }

    // 2. Confirmation (S11): evidence on an existing pattern's anchors that
    //    was logged after its confirm anchor and is not already linked.
    let ts_now = now();
    let mut confirmed = Vec::new();
    for ex in &existing {
        let new_evidence = new_evidence_for(conn, &ex.anchors, &ex.confirm_ts, &ex.evidence)?;
        if !new_evidence.is_empty() {
            for ev in &new_evidence {
                crate::ops::decision::insert_link_if_absent(
                    conn,
                    "system_pattern",
                    ex.id,
                    "progress_entry",
                    *ev,
                    "derived_from",
                )?;
            }
            conn.execute(
                "UPDATE system_patterns SET last_confirmed_at = ?1 WHERE id = ?2",
                rusqlite::params![ts_now, ex.id],
            )?;
            confirmed.push(json!({
                "id": ex.id,
                "name": ex.name,
                "new_evidence": new_evidence,
            }));
        }
    }

    // 3. Anchor clusters (S9): paths with enough distinct entries spanning
    //    enough distinct calendar days. Evidence already claimed by an
    //    existing consolidated pattern is not re-proposed.
    let taken: HashSet<i64> = existing
        .iter()
        .flat_map(|e| e.evidence.iter().copied())
        .collect();
    let mut candidates: Vec<Candidate> = Vec::new();
    let mut used_names: HashSet<String> = {
        let mut names = HashSet::new();
        let mut stmt = conn.prepare("SELECT name FROM system_patterns")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        for r in rows {
            names.insert(r?);
        }
        names
    };
    {
        let mut stmt = conn.prepare(&format!(
            "SELECT ia.path, COUNT(DISTINCT pe.id), COUNT(DISTINCT date(pe.timestamp)) \
             FROM item_anchors ia \
             JOIN progress_entries pe ON pe.id = ia.item_id AND ia.item_type = 'progress_entry' \
             WHERE {EVIDENCE_FILTER} \
             GROUP BY ia.path \
             HAVING COUNT(DISTINCT pe.id) >= ?1 AND COUNT(DISTINCT date(pe.timestamp)) >= ?2 \
             ORDER BY COUNT(DISTINCT pe.id) DESC, MIN(pe.id) ASC"
        ))?;
        let cluster_paths: Vec<String> = {
            let rows = stmt
                .query_map(rusqlite::params![min_repeats, min_days], |r| {
                    r.get::<_, String>(0)
                })?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        };
        for path in cluster_paths {
            // Evidence detail for this path, minus already-claimed entries.
            let mut detail_stmt = conn.prepare(&format!(
                "SELECT pe.id, pe.timestamp, pe.description \
                 FROM progress_entries pe \
                 JOIN item_anchors ia ON ia.item_type = 'progress_entry' AND ia.item_id = pe.id \
                 WHERE ia.path = ?1 AND {EVIDENCE_FILTER} \
                 ORDER BY pe.timestamp ASC, pe.id ASC"
            ))?;
            let rows = detail_stmt
                .query_map(rusqlite::params![path], |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            let entries: Vec<_> = rows
                .into_iter()
                .filter(|(id, _, _)| !taken.contains(id))
                .collect();
            if (entries.len() as i64) < min_repeats {
                continue;
            }
            let distinct_days = entries
                .iter()
                .map(|(_, ts, _)| ts.get(..10).unwrap_or(ts).to_string())
                .collect::<HashSet<_>>()
                .len();
            if (distinct_days as i64) < min_days {
                continue;
            }

            let n = entries.len();
            let base = format!("consolidated-{}", slug(&path));
            let name = unique_name(&base, &mut used_names);
            let description =
                format!(
                "Consolidated from {n} progress entries ({}..{}) touching {path}; most recent: {}",
                entries[0].1, entries[n - 1].1, entries[n - 1].2
            );
            candidates.push(Candidate {
                name,
                description,
                anchors: vec![path],
                evidence: entries.iter().map(|(id, _, _)| *id).collect(),
                first_seen: entries[0].1.clone(),
                last_seen: entries[n - 1].1.clone(),
                initial_confidence: (0.5 + 0.15 * (n as i64 - min_repeats) as f64).min(1.0),
            });
        }
    }

    // 4. Merge suggestions (S9): near-duplicate active decision pairs,
    //    prefiltered by shared tag or anchor, via the FTS similarity gate.
    let merge_suggestions = merge_suggestions(conn)?;

    // 5. Apply (S10): insert candidates + evidence links. Suggestions never merge.
    let mut applied = Vec::new();
    if apply {
        let tx = conn.unchecked_transaction()?;
        for cand in &candidates {
            tx.execute(
                "INSERT INTO system_patterns (uuid, timestamp, name, description, tags, importance, confidence, last_confirmed_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, 5, ?6, ?7)",
                rusqlite::params![
                    Uuid::new_v4().to_string(),
                    ts_now,
                    cand.name,
                    cand.description,
                    r#"["consolidated"]"#,
                    cand.initial_confidence,
                    ts_now
                ],
            )?;
            let pid = tx.last_insert_rowid();
            crate::ops::anchor::attach(&tx, "system_pattern", pid, &cand.anchors)?;
            for eid in &cand.evidence {
                crate::ops::decision::insert_link_if_absent(
                    &tx,
                    "system_pattern",
                    pid,
                    "progress_entry",
                    *eid,
                    "derived_from",
                )?;
            }
            crate::ops::graph::rebuild::touch_item(&tx, "system_pattern", pid)?;
            applied.push(json!({
                "id": pid,
                "name": cand.name,
                "evidence": cand.evidence,
            }));
        }
        tx.commit()?;
    }

    let candidates_json: Vec<Value> = candidates
        .iter()
        .map(|c| {
            json!({
                "name": c.name,
                "description": c.description,
                "tags": ["consolidated"],
                "anchors": c.anchors,
                "evidence": c.evidence,
                "first_seen": c.first_seen,
                "last_seen": c.last_seen,
                "initial_confidence": c.initial_confidence,
            })
        })
        .collect();

    let mut out = serde_json::Map::new();
    out.insert("confirmed".into(), json!(confirmed));
    out.insert("candidates".into(), json!(candidates_json));
    out.insert("merge_suggestions".into(), json!(merge_suggestions));
    if apply {
        out.insert("applied".into(), json!(applied));
    }
    Ok(Value::Object(out))
}

/// Evidence ids of an existing consolidated pattern (`derived_from` targets).
fn evidence_ids(conn: &Connection, pattern_id: i64) -> Result<Vec<i64>> {
    let mut stmt = conn.prepare(
        "SELECT target_item_id FROM context_links \
         WHERE source_item_type = 'system_pattern' AND source_item_id = ?1 \
         AND relationship_type = 'derived_from' \
         ORDER BY target_item_id ASC",
    )?;
    let rows = stmt.query_map(rusqlite::params![pattern_id.to_string()], |r| {
        r.get::<_, String>(0)
    })?;
    let mut out = Vec::new();
    for r in rows {
        if let Ok(id) = r?.parse::<i64>() {
            out.push(id);
        }
    }
    Ok(out)
}

/// Progress entries on the given anchor paths logged after `confirm_ts`
/// that are not already evidence of the pattern.
fn new_evidence_for(
    conn: &Connection,
    paths: &[String],
    confirm_ts: &str,
    own_evidence: &[i64],
) -> Result<Vec<i64>> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = paths.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT pe.id FROM progress_entries pe \
         JOIN item_anchors ia ON ia.item_type = 'progress_entry' AND ia.item_id = pe.id \
         WHERE ia.path IN ({placeholders}) AND {EVIDENCE_FILTER} AND pe.timestamp > ? \
         ORDER BY pe.id ASC"
    );
    let mut params_vec: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(paths.len() + 1);
    for path in paths {
        params_vec.push(path);
    }
    params_vec.push(&confirm_ts);
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(params_vec), |r| {
        r.get::<_, i64>(0)
    })?;
    let own: HashSet<i64> = own_evidence.iter().copied().collect();
    let mut out = Vec::new();
    for r in rows {
        let id = r?;
        if !own.contains(&id) {
            out.push(id);
        }
    }
    Ok(out)
}

/// Near-duplicate decision pairs: shared tag or anchor prefilter, then the
/// FTS similarity gate within each pair. Output capped at
/// `MAX_MERGE_SUGGESTIONS`; never auto-merged.
fn merge_suggestions(conn: &Connection) -> Result<Vec<Value>> {
    let mut pairs: BTreeSet<(i64, i64)> = BTreeSet::new();
    {
        let mut stmt = conn.prepare(
            "SELECT d1.id, d2.id FROM decisions d1 JOIN decisions d2 ON d1.id < d2.id \
             WHERE d1.status = 'active' AND d1.archived = 0 \
             AND d2.status = 'active' AND d2.archived = 0 \
             AND EXISTS (SELECT 1 FROM json_each(d1.tags) t1 \
                         JOIN json_each(d2.tags) t2 ON t2.value = t1.value)",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?;
        for r in rows {
            pairs.insert(r?);
        }
    }
    {
        let mut stmt = conn.prepare(
            "SELECT DISTINCT d1.id, d2.id FROM decisions d1 \
             JOIN item_anchors a1 ON a1.item_type = 'decision' AND a1.item_id = d1.id \
             JOIN decisions d2 ON d1.id < d2.id \
             JOIN item_anchors a2 ON a2.item_type = 'decision' AND a2.item_id = d2.id \
             WHERE d1.status = 'active' AND d1.archived = 0 \
             AND d2.status = 'active' AND d2.archived = 0 \
             AND a1.path = a2.path",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?;
        for r in rows {
            pairs.insert(r?);
        }
    }

    let mut suggestions = Vec::new();
    for (a, b) in pairs {
        if suggestions.len() >= MAX_MERGE_SUGGESTIONS {
            break;
        }
        let sum_a: Option<String> = conn
            .query_row(
                "SELECT summary FROM decisions WHERE id = ?",
                rusqlite::params![a],
                |r| r.get(0),
            )
            .optional()?;
        let sum_b: Option<String> = conn
            .query_row(
                "SELECT summary FROM decisions WHERE id = ?",
                rusqlite::params![b],
                |r| r.get(0),
            )
            .optional()?;
        let (Some(sum_a), Some(sum_b)) = (sum_a, sum_b) else {
            continue;
        };
        // Similarity within the pair: search with each summary, require the
        // other side among the hits (either direction counts).
        let hits_b = crate::ops::decision::find_similar(conn, &sum_a, 5)?;
        if hits_b.iter().any(|h| h.id == b) {
            suggestions.push(json!({
                "source": a,
                "target": b,
                "shared_terms": crate::ops::decision::shared_terms(&sum_a, &sum_b),
            }));
            continue;
        }
        let hits_a = crate::ops::decision::find_similar(conn, &sum_b, 5)?;
        if hits_a.iter().any(|h| h.id == a) {
            suggestions.push(json!({
                "source": a,
                "target": b,
                "shared_terms": crate::ops::decision::shared_terms(&sum_a, &sum_b),
            }));
        }
    }
    Ok(suggestions)
}

/// `src/ops/scoring.rs` → `scoring`: last path component, extension stripped,
/// non-alphanumerics folded to `-`.
fn slug(path: &str) -> String {
    let stem = path
        .rsplit('/')
        .next()
        .unwrap_or(path)
        .split('.')
        .next()
        .unwrap_or("");
    let slug: String = stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    slug.trim_matches('-').to_string()
}

/// Collision-free pattern name: base, then `-2`, `-3`, ...
fn unique_name(base: &str, used: &mut HashSet<String>) -> String {
    if used.insert(base.to_string()) {
        return base.to_string();
    }
    for n in 2.. {
        let candidate = format!("{base}-{n}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
    }
    unreachable!("bounded by usize counter")
}
