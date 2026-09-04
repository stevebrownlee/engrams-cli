//! Tier-3 Memory Quality tests — arXiv:2603.07670 (agentic memory quality
//! evaluation), Decision #53, custom-data `t3-memory-quality-tests`.
//!
//! Where tier-2 suites pin individual features, these tests evaluate
//! *end-to-end memory quality properties* through the real CLI:
//!
//! 1. Superseded decisions are demoted and annotated in `prime` output —
//!    superseded context never silently mixes with active context
//!    (v0.11.0 contract: annotated, ordered last, never hidden).
//! 2. Prune-decay removes decayed knowledge from the read paths (`prime`,
//!    `relevant`), not just from the database row.
//! 3. Anchor-path retrieval is precise: querying one anchor must not
//!    return a decision anchored elsewhere.
//! 4. `prime --budget` output is token-tight: the compact serialization
//!    (bytes / 4, the same quantity `tok_cost` measures) never exceeds
//!    the limit.
//! 5. Recency and importance jointly rank `prime` decisions.
//! 6. Effective confidence decays with confirmation age (Ebbinghaus
//!    math), so fresh knowledge outranks stale knowledge at equal
//!    stored confidence.
//!
//! Timestamp fixtures are backdated directly via `rusqlite::Connection`
//! (the CLI never exposes historical timestamps).

use assert_cmd::Command;
use chrono::{Duration as ChronoDuration, SecondsFormat, Utc};
use rusqlite::Connection;
use serde_json::Value;
use tempfile::TempDir;

fn engrams(db: &std::path::Path) -> Command {
    let mut cmd = Command::cargo_bin("engrams").unwrap();
    cmd.arg("--db").arg(db);
    cmd
}

/// Run a command, assert success, parse stdout as JSON.
fn json(cmd: &mut Command) -> Value {
    let output = cmd.assert().success().get_output().stdout.clone();
    serde_json::from_slice(&output).unwrap()
}

/// RFC3339 timestamp `days` days before now, second precision (the CLI's
/// own format), so SQL day-diff arithmetic truncates to exactly `days`.
fn rfc3339_days_ago(days: i64) -> String {
    (Utc::now() - ChronoDuration::days(days)).to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// Backdate a decision's `timestamp` directly in the DB (the CLI never
/// exposes historical timestamps).
fn backdate_decision(db: &std::path::Path, id: i64, days: i64) {
    let conn = Connection::open(db).unwrap();
    conn.execute(
        "UPDATE decisions SET timestamp = ?1 WHERE id = ?2",
        rusqlite::params![rfc3339_days_ago(days), id],
    )
    .unwrap();
}

fn decision_ids(prime: &Value) -> Vec<i64> {
    prime["decisions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["id"].as_i64().unwrap())
        .collect()
}

fn pattern_names(prime: &Value) -> Vec<&str> {
    prime["patterns"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect()
}

// --- 1. Superseded decisions are demoted + annotated in prime ----------------

/// After `decision log --supersedes`, the old decision must not silently mix
/// with active context: `prime` keeps it in the list (v0.11.0 explicitly
/// annotates rather than hides superseded knowledge) but demotes it to the
/// last position and stamps `status: "superseded"` + `superseded_by`.
#[test]
fn test_superseded_decision_demoted_and_annotated_in_prime() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    let old = json(engrams(&db).args([
        "decision",
        "log",
        "--summary",
        "Original architecture: monolithic deploy",
        "--importance",
        "9",
    ]));
    let old_id = old["id"].as_i64().unwrap();
    let new = json(engrams(&db).args([
        "decision",
        "log",
        "--summary",
        "New architecture: cell-based deploy",
        "--supersedes",
        &old_id.to_string(),
    ]));
    let new_id = new["id"].as_i64().unwrap();

    let prime = json(engrams(&db).args(["prime"]));
    let decisions = prime["decisions"].as_array().unwrap();
    let ids = decision_ids(&prime);
    assert!(ids.contains(&new_id), "active decision surfaced: {ids:?}");
    assert!(
        ids.contains(&old_id),
        "superseded decision tracked: {ids:?}"
    );
    assert_eq!(
        ids.last().copied(),
        Some(old_id),
        "superseded decision is demoted to last position: {ids:?}"
    );

    let old_row = decisions
        .iter()
        .find(|d| d["id"].as_i64() == Some(old_id))
        .unwrap();
    assert_eq!(old_row["status"].as_str(), Some("superseded"));
    assert_eq!(old_row["superseded_by"].as_i64(), Some(new_id));

    let new_row = decisions
        .iter()
        .find(|d| d["id"].as_i64() == Some(new_id))
        .unwrap();
    assert!(
        new_row["status"].is_null(),
        "active decision carries no status annotation"
    );
    assert!(new_row["superseded_by"].is_null());
}

// --- 2. Prune-decay removes decayed knowledge from read paths ----------------

/// A low-importance decision backdated 100 days has Ebbinghaus retention
/// `exp(-100 / strength)`; with two pre-prune control reads its strength is
/// `(1 + 2) * 30 = 90` days, so retention ≈ 0.33 < 0.5 and `prune` must
/// archive it. After pruning it must disappear from BOTH read paths
/// (`prime` and `relevant`) while an untouched control pattern survives.
#[test]
fn test_pruned_decay_absent_from_reads() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    json(engrams(&db).args([
        "decision",
        "log",
        "--summary",
        "Retire legacy XML config parser",
        "--importance",
        "1",
        "--anchor",
        "src/legacy/config.rs",
    ]));
    engrams(&db)
        .args([
            "pattern",
            "log",
            "--name",
            "keep-fresh",
            "--description",
            "d",
        ])
        .assert()
        .success();
    backdate_decision(&db, 1, 100);

    // Control: before pruning, the decision is served by both read paths.
    let before = json(engrams(&db).args(["prime"]));
    assert!(
        decision_ids(&before).contains(&1),
        "pre-prune prime: {before:?}"
    );
    let rel_before = json(engrams(&db).args(["relevant", "src/legacy/config.rs"]));
    let rel_ids: Vec<i64> = rel_before["decisions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["id"].as_i64().unwrap())
        .collect();
    assert!(rel_ids.contains(&1), "pre-prune relevant: {rel_before:?}");

    let pruned = json(engrams(&db).args(["prune", "--threshold", "0.5"]));
    assert_eq!(
        pruned["count"].as_i64(),
        Some(1),
        "one decayed row: {pruned:?}"
    );
    assert_eq!(pruned["archived"].as_i64(), Some(1));

    let after = json(engrams(&db).args(["prime"]));
    let ids = decision_ids(&after);
    assert!(
        !ids.contains(&1),
        "pruned decision must leave prime: {ids:?}"
    );
    assert_eq!(
        pattern_names(&after),
        vec!["keep-fresh"],
        "control pattern survives pruning"
    );
    let rel_after = json(engrams(&db).args(["relevant", "src/legacy/config.rs"]));
    assert!(
        rel_after["decisions"].as_array().unwrap().is_empty(),
        "pruned decision must leave relevant: {rel_after:?}"
    );

    let conn = Connection::open(&db).unwrap();
    let archived: i64 = conn
        .query_row("SELECT archived FROM decisions WHERE id = 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(archived, 1, "decision archived in DB after prune");
}

// --- 3. Anchor-path retrieval precision --------------------------------------

/// `relevant` for one anchor path must return exactly the decision anchored
/// there — never the decision anchored elsewhere, and nothing for a path
/// nobody anchored.
#[test]
fn test_anchor_path_relevance() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    json(engrams(&db).args([
        "decision",
        "log",
        "--summary",
        "Sessions use rotating refresh tokens",
        "--anchor",
        "src/auth/session.rs",
    ]));
    json(engrams(&db).args([
        "decision",
        "log",
        "--summary",
        "Payments settle through ledger double-entry",
        "--anchor",
        "src/payment/ledger.rs",
    ]));
    json(engrams(&db).args([
        "decision",
        "log",
        "--summary",
        "Theme tokens live in the design package",
    ]));

    let hits = json(engrams(&db).args(["relevant", "src/auth/session.rs"]));
    let decisions = hits["decisions"].as_array().unwrap();
    assert_eq!(
        decisions.len(),
        1,
        "exactly the session decision matches: {hits:?}"
    );
    assert_eq!(decisions[0]["id"].as_i64(), Some(1));
    assert!(decisions[0]["score"].as_f64().is_some(), "scored hit");
    assert!(hits["patterns"].as_array().unwrap().is_empty());

    let hits = json(engrams(&db).args(["relevant", "src/payment/ledger.rs"]));
    let decisions = hits["decisions"].as_array().unwrap();
    assert_eq!(decisions.len(), 1, "exactly the ledger decision matches");
    assert_eq!(decisions[0]["id"].as_i64(), Some(2));

    let hits = json(engrams(&db).args(["relevant", "docs/unrelated.md"]));
    assert!(
        hits["decisions"].as_array().unwrap().is_empty(),
        "no false positives for unanchored paths: {hits:?}"
    );
}

// --- 4. Budget-token tightness ------------------------------------------------

/// `prime --budget` must trim to the allowance: the compact serialization of
/// the payload (minus the self-describing budget block) divided by 4 — the
/// same quantity `tok_cost` measures — must not exceed the limit, while
/// still serving a non-empty memory.
#[test]
fn test_prime_budget_respected() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    for i in 1..=30 {
        engrams(&db)
            .args([
                "decision",
                "log",
                "--force",
                "--summary",
                &format!("Decision {i}: adopt caching strategy for service module {i}"),
                "--rationale",
                &format!("Latency profiling showed p99 regressions under load scenario {i}"),
            ])
            .assert()
            .success();
    }

    let prime = json(engrams(&db).args(["prime", "--budget", "500"]));
    assert_eq!(prime["budget"]["limit"].as_i64(), Some(500));
    let estimated = prime["budget"]["estimated_tokens"].as_i64().unwrap();
    assert!(
        estimated > 0 && estimated <= 500,
        "estimated tokens within budget: {estimated}"
    );
    let served = prime["decisions"].as_array().unwrap().len();
    assert!(
        served > 0 && served < 30,
        "budget trims but never empties the payload: {served} served"
    );

    let mut payload = prime.clone();
    payload.as_object_mut().unwrap().remove("budget");
    let bytes = serde_json::to_vec(&payload).unwrap().len();
    assert!(
        bytes / 4 <= 500,
        "compact payload is {bytes}B = {} tok > 500 budget",
        bytes / 4
    );
}

// --- 5. Importance + recency joint ranking -----------------------------------

/// At prime time, a maximally-important fresh decision outranks a
/// low-importance stale one — neither factor alone: the score is
/// `0.6 * exp(-λ·age) + 0.4 * importance/10` with λ ≈ 0.01155/day.
/// Expected here: fresh-important ≈ 0.99, stale-unimportant ≈ 0.54.
#[test]
fn test_importance_recency_ranking() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    json(engrams(&db).args([
        "decision",
        "log",
        "--summary",
        "Prefer polling over webhooks for internal sync",
        "--importance",
        "3",
    ]));
    json(engrams(&db).args([
        "decision",
        "log",
        "--summary",
        "Adopt event-driven webhook bridge for partner APIs",
        "--importance",
        "10",
    ]));
    backdate_decision(&db, 1, 30);
    backdate_decision(&db, 2, 1);

    let prime = json(engrams(&db).args(["prime"]));
    let decisions = prime["decisions"].as_array().unwrap();
    let ids = decision_ids(&prime);
    assert_eq!(ids, vec![2, 1], "fresh+important first: {ids:?}");
    let s_fresh = decisions[0]["score"].as_f64().unwrap();
    let s_stale = decisions[1]["score"].as_f64().unwrap();
    assert!(
        s_fresh > 0.9 && s_fresh <= 1.0,
        "fresh importance-10 decision ≈ 0.99: {s_fresh}"
    );
    assert!(
        (0.50..0.58).contains(&s_stale),
        "30-day-old importance-3 decision ≈ 0.54: {s_stale}"
    );
}

// --- 6. Effective-confidence decay ranking -----------------------------------

/// Equal stored confidence (1.0) and timestamps, only confirmation recency
/// differs: effective confidence decays as `exp(-λ · days_since_confirm)`
/// (λ ≈ 0.01155 → 120 days ≈ 0.25), so the freshly confirmed pattern ranks
/// first AND the decay is quantitatively visible in the payload.
/// Extends tier-2 `s12_prime_ranks_recently_confirmed_above_stale` with the
/// decay-math value assertions.
#[test]
fn test_confidence_pattern_ranking() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    for name in ["p-stale-120d", "p-fresh"] {
        engrams(&db)
            .args(["pattern", "log", "--name", name, "--description", "d"])
            .assert()
            .success();
    }
    let shared_ts = rfc3339_days_ago(30);
    {
        let conn = Connection::open(&db).unwrap();
        conn.execute(
            "UPDATE system_patterns SET timestamp = ?1, confidence = 1.0, \
             last_confirmed_at = ?2 WHERE id = 1",
            rusqlite::params![shared_ts, rfc3339_days_ago(120)],
        )
        .unwrap();
        conn.execute(
            "UPDATE system_patterns SET timestamp = ?1, confidence = 1.0, \
             last_confirmed_at = ?2 WHERE id = 2",
            rusqlite::params![shared_ts, rfc3339_days_ago(0)],
        )
        .unwrap();
    }

    let prime = json(engrams(&db).args(["prime"]));
    assert_eq!(
        pattern_names(&prime),
        vec!["p-fresh", "p-stale-120d"],
        "fresh confirmation ranks first: {prime:?}"
    );
    let patterns = prime["patterns"].as_array().unwrap();
    let fresh = patterns[0]["effective_confidence"].as_f64().unwrap();
    let stale = patterns[1]["effective_confidence"].as_f64().unwrap();
    assert!(
        (0.99..=1.0).contains(&fresh),
        "fresh effective confidence ≈ 1.0: {fresh}"
    );
    assert!(
        (0.24..0.26).contains(&stale),
        "120-day-old effective confidence ≈ exp(-0.01155·120) ≈ 0.25: {stale}"
    );
}
