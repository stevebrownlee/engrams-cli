//! `engrams schema scan`: formation pipeline stages 3–4 (spec 0002).
//!
//! Detection (phase 2's deterministic Louvain over the union adjacency)
//! produces clusters; this module stages them into `schema_candidates`.
//! Identity across scans is Jaccard ≥ [`JACCARD_IDENTITY`] over member sets:
//! a match advances the existing row's stability and records drift counts,
//! a miss inserts a fresh candidate. Staging writes touch only
//! `schema_candidates` (AC-4: nothing is created without an explicit apply;
//! schemas, links, and telemetry are untouched). Gates are evaluated after
//! the upsert so a row's own refreshed stability counts toward readiness.

use anyhow::Result;
use chrono::{SecondsFormat, Utc};
use rusqlite::Connection;
use serde_json::json;
use std::cmp::Ordering;
use std::collections::HashMap;
#[cfg(test)]
use std::fmt::Write as _;

use super::super::graph::louvain::{self, OverlayWeights};
use super::super::graph::model::NodeKey;

/// Launch-default gate thresholds (spec 0002 Architecture step 4). Sweep
/// constants owned by the phase-8 dogfood gate; the reward floor of zero
/// exists because `retrieval_surfaces` ships empty and any higher floor
/// would block all formation until telemetry exists.
pub(super) const DENSITY_GATE: f64 = 0.5;
pub(super) const STABILITY_GATE: i64 = 3;
pub(super) const REWARD_GATE: i64 = 0;
/// Jaccard threshold above which a cluster and a staged row share identity.
pub(super) const JACCARD_IDENTITY: f64 = 0.7;

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// Canonical member string: `kind:id` in the database's kind spelling.
fn member_string(key: &NodeKey) -> String {
    format!("{}:{}", key.0, key.1)
}

/// Sorted member strings; the staging identity unit.
fn member_strings(members: &[NodeKey]) -> Vec<String> {
    let mut ms: Vec<String> = members.iter().map(member_string).collect();
    ms.sort();
    ms
}

/// Sorted-member signature: the exact-set key of a candidate row.
fn signature(members: &[String]) -> String {
    members.join(",")
}

/// Jaccard similarity of two sorted member sets.
pub(super) fn jaccard(a: &[String], b: &[String]) -> f64 {
    let inter = a.iter().filter(|m| b.contains(m)).count();
    let union = a.len() + b.len() - inter;
    if union == 0 {
        0.0
    } else {
        inter as f64 / union as f64
    }
}

/// A staged row as read from `schema_candidates`.
#[derive(Clone)]
struct Stored {
    sig: String,
    members: Vec<String>,
    stability_count: i64,
}

fn load_stored(conn: &Connection) -> Result<Vec<Stored>> {
    let mut stmt = conn.prepare(
        "SELECT cluster_sig, member_keys_json, stability_count \
         FROM schema_candidates",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
        ))
    })?;
    let mut out = Vec::new();
    for r in rows {
        let (sig, members_json, stability_count) = r?;
        let members: Vec<String> = serde_json::from_str(&members_json)
            .map_err(|e| anyhow::anyhow!("corrupt member_keys_json in {sig}: {e}"))?;
        out.push(Stored {
            sig,
            members,
            stability_count,
        });
    }
    Ok(out)
}

/// Rows of retrieval telemetry per touched node, read once: the store is
/// rolling-window pruned, so this is bounded. Reward hits for a candidate
/// are the sum over its members — read-only against telemetry.
fn telemetry_counts(conn: &Connection) -> Result<HashMap<NodeKey, i64>> {
    let mut stmt = conn.prepare("SELECT node_kind, node_id FROM retrieval_surfaces")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    let mut counts: HashMap<NodeKey, i64> = HashMap::new();
    for r in rows {
        let (kind, id) = r?;
        *counts.entry((kind, id.to_string())).or_insert(0) += 1;
    }
    Ok(counts)
}

fn reward_hits(members: &[String], telemetry: &HashMap<NodeKey, i64>) -> i64 {
    members
        .iter()
        .filter_map(|m| m.split_once(':'))
        .filter_map(|(kind, id)| telemetry.get(&(kind.to_string(), id.to_string())))
        .sum()
}

/// Deterministic assignment of current clusters to staged rows.
///
/// Every (cluster, stored) pair with Jaccard ≥ [`JACCARD_IDENTITY`] is a
/// match candidate; pairs are accepted greedily by (Jaccard desc, cluster
/// member set asc, stored signature asc) so that when one staged row
/// matches two current clusters — or one cluster matches two rows — the
/// winner is always the higher similarity, ties broken by the
/// lexicographically smaller member set. Each cluster pairs with at most
/// one row and each row with at most one cluster.
fn assign(clusters: &[Vec<String>], stored: &[Stored]) -> Vec<Option<usize>> {
    let mut pairs: Vec<(usize, usize, f64)> = Vec::new();
    for (ci, members) in clusters.iter().enumerate() {
        for (si, s) in stored.iter().enumerate() {
            let j = jaccard(members, &s.members);
            if j >= JACCARD_IDENTITY {
                pairs.push((ci, si, j));
            }
        }
    }
    pairs.sort_by(|a, b| {
        b.2.partial_cmp(&a.2)
            .unwrap_or(Ordering::Equal)
            .then_with(|| clusters[a.0].cmp(&clusters[b.0]))
            .then_with(|| stored[a.1].sig.cmp(&stored[b.1].sig))
    });
    let mut match_of: Vec<Option<usize>> = vec![None; clusters.len()];
    let mut taken = vec![false; stored.len()];
    for (ci, si, _) in pairs {
        if match_of[ci].is_none() && !taken[si] {
            match_of[ci] = Some(si);
            taken[si] = true;
        }
    }
    match_of
}

/// Staging upsert for one detected cluster. A match advances the row's
/// identity: stability +1, members and signature refreshed to the current
/// set, drift recorded as removed/added counts against the row's previous
/// members. A miss inserts a fresh row at stability 1. Unmatched stored
/// rows are left untouched — a cluster that skips a scan keeps its history
/// and may re-match later.
fn upsert(
    conn: &Connection,
    members: &[String],
    density: f64,
    reward: i64,
    matched: Option<&Stored>,
    ts: &str,
) -> Result<(i64, i64, i64)> {
    let sig = signature(members);
    let members_json = serde_json::to_string(members)?;
    match matched {
        Some(s) => {
            let removed = s.members.iter().filter(|m| !members.contains(m)).count() as i64;
            let added = members.iter().filter(|m| !s.members.contains(m)).count() as i64;
            let stability = s.stability_count + 1;
            conn.execute(
                "UPDATE schema_candidates SET cluster_sig = ?1, member_keys_json = ?2, \
                 density = ?3, stability_count = ?4, reward_hits = ?5, \
                 last_drift_removed = ?6, last_drift_added = ?7, last_seen_at = ?8 \
                 WHERE cluster_sig = ?9",
                rusqlite::params![
                    sig,
                    members_json,
                    density,
                    stability,
                    reward,
                    removed,
                    added,
                    ts,
                    s.sig
                ],
            )?;
            Ok((stability, removed, added))
        }
        None => {
            conn.execute(
                "INSERT INTO schema_candidates \
                 (cluster_sig, member_keys_json, density, stability_count, reward_hits, \
                  last_drift_removed, last_drift_added, first_seen_at, last_seen_at) \
                 VALUES (?1, ?2, ?3, 1, ?4, 0, 0, ?5, ?5)",
                rusqlite::params![sig, members_json, density, reward, ts],
            )?;
            Ok((1, 0, 0))
        }
    }
}

/// Run detection and staging; report candidates with gate detail. The
/// output is a report, not promotion: no schema rows, links, or telemetry
/// are written (AC-4).
pub fn scan(conn: &Connection) -> Result<serde_json::Value> {
    let detected = louvain::clusters(conn, &OverlayWeights::default())?;
    let stored = load_stored(conn)?;
    let telemetry = telemetry_counts(conn)?;
    let ts = now();

    let member_sets: Vec<Vec<String>> = detected
        .iter()
        .map(|c| member_strings(&c.members))
        .collect();
    let match_of = assign(&member_sets, &stored);

    // The whole staging phase is one transaction: a mid-scan crash leaves
    // stability counts un-inflated (all upserts land or none do).
    let tx = conn.unchecked_transaction()?;
    let mut candidates = Vec::new();
    for (ci, cluster) in detected.iter().enumerate() {
        let members = &member_sets[ci];
        let reward = reward_hits(members, &telemetry);
        let prev = match_of[ci].map(|si| &stored[si]);
        let (stability, removed, added) = upsert(&tx, members, cluster.density, reward, prev, &ts)?;
        let density_pass = cluster.density >= DENSITY_GATE;
        let stability_pass = stability >= STABILITY_GATE;
        let reward_pass = reward >= REWARD_GATE;
        let failed_gates: Vec<&str> = [
            ("density", density_pass),
            ("stability", stability_pass),
            ("reward", reward_pass),
        ]
        .iter()
        .filter(|(_, pass)| !pass)
        .map(|(name, _)| *name)
        .collect();
        candidates.push(json!({
            "cluster_sig": signature(members),
            "member_count": members.len(),
            "density": cluster.density,
            "stability_count": stability,
            "reward_hits": reward,
            "drift_removed": removed,
            "drift_added": added,
            "gates": {
                "density": {
                    "value": cluster.density,
                    "threshold": DENSITY_GATE,
                    "pass": density_pass,
                },
                "stability": {
                    "value": stability,
                    "threshold": STABILITY_GATE,
                    "pass": stability_pass,
                },
                "reward": {
                    "value": reward,
                    "threshold": REWARD_GATE,
                    "pass": reward_pass,
                },
            },
            "failed_gates": failed_gates,
            "gates_pass": density_pass && stability_pass && reward_pass,
        }));
    }
    tx.commit()?;

    Ok(json!({
        "status": "success",
        "candidates": candidates,
    }))
}

#[cfg(test)]
/// Full-content snapshot of every user table (all of them, including
/// FTS shadow tables, enumerated from sqlite_master): rows rendered to
/// strings in rowid order. The write-audit asserts these are identical
/// before and after a scan for every table except schema_candidates.
fn snapshot(conn: &Connection) -> Vec<(String, Vec<String>)> {
    let tables: Vec<String> = conn
        .prepare(
            "SELECT name FROM sqlite_master \
                 WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    tables
        .into_iter()
        .map(|t| {
            // WITHOUT ROWID tables (FTS5 `%_config`, `%_idx` shadows)
            // have no rowid to order by; natural scan order is their
            // b-tree key order, which is deterministic for a fixed state.
            let has_rowid = conn
                .prepare(&format!("SELECT rowid FROM \"{t}\" LIMIT 0"))
                .is_ok();
            let mut stmt = conn
                .prepare(&format!(
                    "SELECT * FROM \"{t}\"{}",
                    if has_rowid { " ORDER BY rowid" } else { "" }
                ))
                .unwrap();
            let ncols = stmt.column_count();
            let rows: Vec<String> = stmt
                .query_map([], |row| {
                    let mut parts = Vec::with_capacity(ncols);
                    for i in 0..ncols {
                        parts.push(match row.get_ref(i)? {
                            rusqlite::types::ValueRef::Null => "null".to_string(),
                            rusqlite::types::ValueRef::Integer(v) => v.to_string(),
                            rusqlite::types::ValueRef::Real(v) => v.to_string(),
                            rusqlite::types::ValueRef::Text(v) => {
                                String::from_utf8_lossy(v).into_owned()
                            }
                            rusqlite::types::ValueRef::Blob(v) => {
                                let mut hex = String::with_capacity(v.len() * 2);
                                for b in v {
                                    let _ = write!(hex, "{b:02x}");
                                }
                                hex
                            }
                        });
                    }
                    Ok(parts.join("\u{1f}"))
                })
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap();
            (t, rows)
        })
        .collect()
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

    fn add_decision(conn: &Connection, id: i64) {
        conn.execute(
            "INSERT INTO decisions (uuid, timestamp, summary) \
             VALUES (?1, '2026-01-01T00:00:00Z', 'd')",
            rusqlite::params![format!("u{id}")],
        )
        .unwrap();
    }

    fn link(conn: &Connection, a: i64, b: i64) {
        conn.execute(
            "INSERT INTO context_links (source_item_type, source_item_id, target_item_type, \
             target_item_id, relationship_type, timestamp) \
             VALUES ('decision', ?1, 'decision', ?2, 'relates_to', '2026-01-01T00:00:00Z')",
            rusqlite::params![a, b],
        )
        .unwrap();
    }

    fn anchor(conn: &Connection, id: i64, path: &str) {
        conn.execute(
            "INSERT INTO item_anchors (item_type, item_id, path, timestamp) \
             VALUES ('decision', ?1, ?2, '2026-01-01T00:00:00Z')",
            rusqlite::params![id, path],
        )
        .unwrap();
    }

    fn surface(conn: &Connection, id: i64, ts: &str) {
        conn.execute(
            "INSERT INTO retrieval_surfaces (ts, cmd, arg, node_kind, node_id) \
             VALUES (?1, 'query', 'x', 'decision', ?2)",
            rusqlite::params![ts, id],
        )
        .unwrap();
    }

    /// One dense trio: all pairs declared (weight 1.0) so density 1.0.
    fn dense_trio(conn: &Connection) {
        for id in 1..=3 {
            add_decision(conn, id);
            anchor(conn, id, "src/a.rs");
        }
        link(conn, 1, 2);
        link(conn, 1, 3);
        link(conn, 2, 3);
    }

    fn candidate(out: &serde_json::Value) -> &serde_json::Value {
        &out["candidates"][0]
    }

    #[test]
    fn scan_twice_keeps_identity_and_records_stability() {
        let conn = mem_db();
        dense_trio(&conn);
        surface(&conn, 1, "t1");
        surface(&conn, 1, "t2");
        surface(&conn, 2, "t3");

        let first = scan(&conn).unwrap();
        let c = candidate(&first);
        assert_eq!(c["member_count"], 3);
        assert_eq!(c["stability_count"], 1);
        assert_eq!(c["reward_hits"], 3);
        assert_eq!(c["drift_removed"], 0);
        assert_eq!(c["drift_added"], 0);
        // Stability gate needs 3 sightings: not yet on scan one.
        assert_eq!(c["gates_pass"], false);

        let second = scan(&conn).unwrap();
        let c2 = candidate(&second);
        assert_eq!(c2["cluster_sig"], c["cluster_sig"]);
        assert_eq!(c2["member_count"], c["member_count"]);
        assert_eq!(c2["density"], c["density"]);
        assert_eq!(c2["stability_count"], 2);
        assert_eq!(c2["reward_hits"], 3);
        assert_eq!(c2["drift_removed"], 0);
        assert_eq!(c2["drift_added"], 0);
        assert_eq!(c2["gates_pass"], false);
        let third = scan(&conn).unwrap();
        let c3 = candidate(&third);
        assert_eq!(c3["stability_count"], 3);
        assert_eq!(c3["gates_pass"], true);
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_candidates", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 1);
    }

    #[test]
    fn member_growth_within_jaccard_budget_stays_same_row() {
        let conn = mem_db();
        dense_trio(&conn);
        scan(&conn).unwrap();

        add_decision(&conn, 4);
        anchor(&conn, 4, "src/a.rs");
        link(&conn, 1, 4);
        link(&conn, 2, 4);
        link(&conn, 3, 4);

        let out = scan(&conn).unwrap();
        let c = candidate(&out);
        // Jaccard {1,2,3} → {1,2,3,4} is 3/4 = 0.75 ≥ 0.7: same identity,
        // refreshed membership, drift recorded.
        assert_eq!(c["member_count"], 4);
        assert_eq!(c["stability_count"], 2);
        assert_eq!(c["drift_removed"], 0);
        assert_eq!(c["drift_added"], 1);
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_candidates", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 1);
    }

    #[test]
    fn sparse_cluster_stages_but_fails_density_gate() {
        let conn = mem_db();
        add_decision(&conn, 1);
        add_decision(&conn, 2);
        conn.execute(
            "INSERT INTO context_links (source_item_type, source_item_id, target_item_type, \
             target_item_id, relationship_type, weight, timestamp) \
             VALUES ('decision', 1, 'decision', 2, 'relates_to', 0.2, '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();

        let out = scan(&conn).unwrap();
        let c = candidate(&out);
        assert_eq!(c["member_count"], 2);
        assert!((c["density"].as_f64().unwrap() - 0.2).abs() < 1e-9);
        assert_eq!(c["gates_pass"], false);
        // Density and stability both fail on a fresh single scan (stability
        // is 1 of the required 3); the report names them from JSON alone.
        assert_eq!(c["failed_gates"], json!(["density", "stability"]));
        let gates = &c["gates"];
        assert_eq!(gates["density"]["pass"], false);
        assert!((gates["density"]["value"].as_f64().unwrap() - 0.2).abs() < 1e-9);
        assert!((gates["density"]["threshold"].as_f64().unwrap() - DENSITY_GATE).abs() < 1e-9);
        assert_eq!(gates["stability"]["pass"], false);
        assert_eq!(gates["stability"]["value"], 1);
        assert_eq!(gates["stability"]["threshold"], STABILITY_GATE);
        assert_eq!(gates["reward"]["pass"], true);
    }

    #[test]
    fn scan_writes_only_schema_candidates() {
        let conn = mem_db();
        dense_trio(&conn);
        surface(&conn, 1, "t1");

        let before = snapshot(&conn);
        assert!(before.iter().any(|(t, _)| t == "schema_candidates"));

        scan(&conn).unwrap();
        scan(&conn).unwrap();

        let after = snapshot(&conn);
        for ((t, before_rows), (_, after_rows)) in before.iter().zip(after.iter()) {
            if t == "schema_candidates" {
                continue;
            }
            assert_eq!(before_rows, after_rows, "{t} was mutated by scan");
        }
        let candidates: usize = after
            .iter()
            .find(|(t, _)| t == "schema_candidates")
            .map(|(_, rows)| rows.len())
            .unwrap();
        assert_eq!(candidates, 1);
    }

    fn mset(ids: &[i64]) -> Vec<String> {
        ids.iter().map(|id| format!("decision:{id}")).collect()
    }

    fn stored_row(members: Vec<String>, stability: i64) -> Stored {
        Stored {
            sig: members.join(","),
            members,
            stability_count: stability,
        }
    }

    #[test]
    fn assign_prefers_higher_jaccard_order_independently() {
        let cluster = mset(&[1, 2, 3]);
        // Both rows qualify (J >= 0.7); `stronger` must win regardless of
        // the order the caller lists them in.
        let weaker = mset(&[1, 2, 3, 9]); // J = 0.75 vs cluster
        let stronger = mset(&[1, 2, 3]); // J = 1.0 vs cluster
        let forward = assign(
            std::slice::from_ref(&cluster),
            &[
                stored_row(weaker.clone(), 1),
                stored_row(stronger.clone(), 1),
            ],
        );
        let backward = assign(
            std::slice::from_ref(&cluster),
            &[
                stored_row(stronger.clone(), 1),
                stored_row(weaker.clone(), 1),
            ],
        );
        let again = assign(
            &[cluster],
            &[stored_row(weaker, 1), stored_row(stronger, 1)],
        );
        assert_eq!(forward, [Some(1)]);
        assert_eq!(backward, [Some(0)]);
        assert_eq!(forward, again, "assign must be stable across runs");
    }

    #[test]
    fn assign_is_one_to_one_when_two_clusters_match_one_row() {
        let exact = mset(&[1, 2, 3]); // jaccard 1.0 vs the stored row
        let superset = mset(&[1, 2, 3, 4]); // jaccard 0.75
        let staged = stored_row(mset(&[1, 2, 3]), 5);

        let forward = assign(
            &[superset.clone(), exact.clone()],
            std::slice::from_ref(&staged),
        );
        let backward = assign(&[exact, superset], &[staged]);
        assert_eq!(forward, [None, Some(0)]);
        assert_eq!(backward, [Some(0), None]);
    }

    #[test]
    fn assign_tiebreaks_equal_jaccard_by_member_set_then_sig() {
        // Every listed pair ties at J = 0.8 (the comparator's level 1), so
        // levels 2 and 3 decide: the lexicographically smaller member set
        // claims the lexicographically smaller signature, regardless of the
        // order the caller passed them in. c2 matches only row_a, so it
        // stays unmatched once the c1 x row_a pair consumes it.
        let c1 = mset(&[1, 2, 3, 4]);
        let c2 = mset(&[1, 2, 3, 5]);
        let row_a = stored_row(mset(&[1, 2, 3, 4, 5]), 9);
        let row_b = stored_row(mset(&[1, 2, 3, 4, 6]), 9);

        let got = assign(&[c1.clone(), c2.clone()], &[row_a.clone(), row_b.clone()]);
        assert_eq!(got, [Some(0), None]);

        let got = assign(&[c2, c1], &[row_b, row_a]);
        assert_eq!(got, [None, Some(1)]);
    }

    #[test]
    fn assign_tiebreaks_equal_jaccard_distinct_clusters_by_member_set() {
        // Two distinct clusters with genuinely equal Jaccard (5/6) against
        // one row: level 2 of the comparator (cluster member set asc)
        // decides, independent of caller order.
        let left = mset(&[1, 2, 3, 4, 5]); // J = 5/6 vs the row
        let right = mset(&[2, 3, 4, 5, 6]); // J = 5/6 vs the row
        let staged = stored_row(mset(&[1, 2, 3, 4, 5, 6]), 4);

        let forward = assign(
            &[left.clone(), right.clone()],
            std::slice::from_ref(&staged),
        );
        assert_eq!(forward, [Some(0), None]);

        let backward = assign(&[right, left], &[staged]);
        // left (index 1 here) still claims the row: smaller member set wins.
        assert_eq!(backward, [None, Some(0)]);
    }

    #[test]
    fn upsert_records_removed_drift_against_previous_members() {
        let conn = mem_db();
        let prev_members = mset(&[1, 2, 3, 4]);
        let prev_sig = prev_members.join(",");
        conn.execute(
            "INSERT INTO schema_candidates (cluster_sig, member_keys_json, density, \
             stability_count, first_seen_at, last_seen_at) \
             VALUES (?1, ?2, 1.0, 2, 't0', 't0')",
            rusqlite::params![prev_sig, serde_json::to_string(&prev_members).unwrap()],
        )
        .unwrap();
        let prev = stored_row(prev_members, 2);
        let current = mset(&[1, 2, 3, 5]);

        let (stability, removed, added) =
            upsert(&conn, &current, 1.0, 0, Some(&prev), "t").unwrap();
        assert_eq!((stability, removed, added), (3, 1, 1));

        let (sig, drift_removed, drift_added, row_stability): (String, i64, i64, i64) = conn
            .query_row(
                "SELECT cluster_sig, last_drift_removed, last_drift_added, \
                 stability_count FROM schema_candidates",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!(sig, "decision:1,decision:2,decision:3,decision:5");
        assert_eq!((drift_removed, drift_added, row_stability), (1, 1, 3));
    }
}
