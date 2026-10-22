//! v0.11.0 tier-2 acceptance tests — specs/agent-memory-0.11.0.md §8 (S1–S13).
//!
//! Scenarios are grouped per feature: ontology + causal retrieval (S1/S2),
//! migration (S3), confidence fields & decay (S4), the contradiction gate
//! (S5–S8), consolidation (S9–S11), confidence round-trip + ranking (S12),
//! and the doctor advisory (S13). Timestamp fixtures are backdated directly
//! through rusqlite so decay math is deterministic.

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

fn log_decision(db: &std::path::Path, summary: &str) -> Value {
    json(
        engrams(db)
            .args(["decision", "log", "--summary", summary])
            .env("NO_COLOR", "1"),
    )
}

fn log_progress(db: &std::path::Path, description: &str) -> Value {
    json(engrams(db).args([
        "progress",
        "log",
        "--status",
        "Done",
        "--description",
        description,
    ]))
}

fn add_anchor(db: &std::path::Path, item_type: &str, id: i64, path: &str) {
    engrams(db)
        .args([
            "anchor",
            "add",
            "--type",
            item_type,
            "--id",
            &id.to_string(),
            "--path",
            path,
        ])
        .assert()
        .success();
}
/// `link list --item-type decision --item-id <id>` rows as (rel, src, tgt).
fn decision_links(db: &std::path::Path, id: i64) -> Vec<(String, String, String)> {
    let rows = json(engrams(db).args([
        "link",
        "list",
        "--item-type",
        "decision",
        "--item-id",
        &id.to_string(),
    ]));
    rows.as_array()
        .unwrap()
        .iter()
        .map(|l| {
            (
                l["relationship_type"].as_str().unwrap().to_string(),
                l["source_item_id"].as_str().unwrap().to_string(),
                l["target_item_id"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

fn scalar_i64(db: &std::path::Path, sql: &str) -> i64 {
    let conn = Connection::open(db).unwrap();
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

// --- S1/S2: ontology + causal retrieval ---------------------------------

#[test]
fn s1_caused_by_stores_canonical_causes_swapped() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    log_decision(&db, "Root cause");
    log_decision(&db, "Effect");

    // Inverse name `caused_by` (source causes target) normalizes to canonical
    // `causes` stored target→source.
    engrams(&db)
        .args([
            "link",
            "add",
            "--source-type",
            "decision",
            "--source-id",
            "1",
            "--target-type",
            "decision",
            "--target-id",
            "2",
            "--rel",
            "caused_by",
        ])
        .assert()
        .success();

    let links = decision_links(&db, 2);
    assert!(
        links
            .iter()
            .any(|(rel, src, tgt)| rel == "causes" && src == "2" && tgt == "1"),
        "expected canonical causes 2→1, got {links:?}"
    );

    let stats = json(engrams(&db).args(["graph", "stats"]));
    assert_eq!(
        stats["edges"]["by_relationship"]["causes"].as_i64(),
        Some(1)
    );
}

#[test]
fn s2_graph_why_upstream_and_downstream() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    for name in ["A", "B", "C"] {
        log_decision(&db, name);
    }
    for (src, tgt) in [(1, 2), (2, 3)] {
        engrams(&db)
            .args([
                "link",
                "add",
                "--source-type",
                "decision",
                "--source-id",
                &src.to_string(),
                "--target-type",
                "decision",
                "--target-id",
                &tgt.to_string(),
                "--rel",
                "causes",
            ])
            .assert()
            .success();
    }

    // Upstream from C: B at depth 1, A at depth 2, roots [A].
    let up = json(engrams(&db).args(["graph", "why", "decision:3"]));
    assert_eq!(up["direction"].as_str(), Some("upstream"));
    let chain: Vec<(&str, i64)> = up["chain"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| (e["node"].as_str().unwrap(), e["depth"].as_i64().unwrap()))
        .collect();
    assert_eq!(chain, vec![("decision:2", 1), ("decision:1", 2)]);
    assert_eq!(
        up["roots"].as_array().unwrap(),
        &vec![Value::String("decision:1".into())]
    );

    // Downstream from A: B at depth 1, C at depth 2.
    let down = json(engrams(&db).args(["graph", "why", "decision:1", "--down"]));
    assert_eq!(down["direction"].as_str(), Some("downstream"));
    let chain: Vec<(&str, i64)> = down["chain"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| (e["node"].as_str().unwrap(), e["depth"].as_i64().unwrap()))
        .collect();
    assert_eq!(chain, vec![("decision:2", 1), ("decision:3", 2)]);
}

// --- S3: migration --------------------------------------------------------

#[test]
fn s3_migration_v6_to_v7_adds_confidence_columns() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    // Minimal v6 database: system_patterns without the tier-2 columns.
    {
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch(
            r#"CREATE TABLE system_patterns (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              uuid TEXT NOT NULL,
              timestamp TEXT NOT NULL,
              name TEXT NOT NULL,
              description TEXT,
              tags TEXT,
              check_kind TEXT,
              check_expr TEXT,
              severity TEXT NOT NULL DEFAULT 'warn',
              importance INTEGER NOT NULL DEFAULT 5,
              access_count INTEGER NOT NULL DEFAULT 0,
              last_accessed_at TEXT,
              archived INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE decisions (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              uuid TEXT NOT NULL,
              timestamp TEXT NOT NULL,
              summary TEXT NOT NULL,
              rationale TEXT,
              implementation_details TEXT,
              tags TEXT,
              status TEXT NOT NULL DEFAULT 'active',
              commit_sha TEXT,
              importance INTEGER NOT NULL DEFAULT 5,
              access_count INTEGER NOT NULL DEFAULT 0,
              last_accessed_at TEXT,
              archived INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE code_nodes (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              kind TEXT NOT NULL,
              path TEXT NOT NULL,
              symbol TEXT NOT NULL DEFAULT '',
              first_seen TEXT NOT NULL,
              last_seen TEXT NOT NULL,
              UNIQUE(kind, path, symbol)
            );
            CREATE TABLE graph_meta (
              id INTEGER PRIMARY KEY CHECK (id = 1),
              last_rebuild_at TEXT,
              last_ingest_sha TEXT
            );
            INSERT INTO system_patterns (uuid, timestamp, name) VALUES ('u1', '2026-01-01T00:00:00Z', 'legacy');
            PRAGMA user_version = 6;"#,
        )
        .unwrap();
    }

    // Pre-migration guard: commands refuse to run against a stale schema.
    engrams(&db)
        .args(["pattern", "list"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("out of date"));

    engrams(&db)
        .args(["migrate"])
        .assert()
        .success()
        .stdout(predicates::str::contains("success"));

    {
        // On-disk version must match migrate's self-reported latest (self-maintaining
        // pin — derived from command output, not a hardcoded number).
        let out = engrams(&db).arg("migrate").output().unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        let conn = Connection::open(&db).unwrap();
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, parsed["version"].as_i64().unwrap());
        let mut stmt = conn.prepare("PRAGMA table_info(system_patterns)").unwrap();
        let cols: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        drop(stmt);
        assert!(cols.iter().any(|c| c == "confidence"), "columns: {cols:?}");
        assert!(
            cols.iter().any(|c| c == "last_confirmed_at"),
            "columns: {cols:?}"
        );
        // Legacy rows get the documented defaults.
        let (confidence, last_confirmed): (f64, Option<String>) = conn
            .query_row(
                "SELECT confidence, last_confirmed_at FROM system_patterns WHERE id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(confidence, 1.0);
        assert_eq!(last_confirmed, None);
    }
}

// --- S4: confidence fields & decay ---------------------------------------

#[test]
fn s4_confidence_fields_and_decay_math() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    engrams(&db)
        .args(["pattern", "log", "--name", "p", "--description", "d"])
        .assert()
        .success();

    // Update writes both stored fields.
    engrams(&db)
        .args(["pattern", "update", "1", "--confidence", "0.5"])
        .assert()
        .success();
    let p = json(engrams(&db).args(["pattern", "get", "1"]));
    assert_eq!(p["confidence"].as_f64(), Some(0.5));
    assert!(
        p["last_confirmed_at"].as_str().is_some(),
        "confirm stamped: {p}"
    );

    // Out-of-range values are rejected.
    for bad in ["0.0", "1.5"] {
        engrams(&db)
            .args(["pattern", "update", "1", "--confidence", bad])
            .assert()
            .failure()
            .stderr(predicates::str::contains("(0, 1]"));
    }

    // Deterministic decay: 0.7 confirmed exactly 20 days ago.
    let ts_20d = rfc3339_days_ago(20);
    {
        let conn = Connection::open(&db).unwrap();
        conn.execute(
            "UPDATE system_patterns SET confidence = 0.7, last_confirmed_at = ?1 WHERE id = 1",
            rusqlite::params![ts_20d],
        )
        .unwrap();
    }
    let p = json(engrams(&db).args(["pattern", "get", "1"]));
    let eff = p["effective_confidence"].as_f64().unwrap();
    let expected = 0.7 * (-0.01155_f64 * 20.0).exp();
    assert!(
        (eff - expected).abs() < 1e-6,
        "effective_confidence {eff} != {expected}"
    );
}

// --- S5–S8: contradiction gate --------------------------------------------

#[test]
fn s5_gate_lists_active_and_omits_superseded() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    log_decision(&db, "Use Vec<&dyn ToSql> for dynamic SQL parameter lists");
    // Near-duplicate, force-inserted then superseded.
    json(
        engrams(&db)
            .args([
                "decision",
                "log",
                "--summary",
                "Use Vec<&dyn ToSql> for parameters",
                "--force",
            ])
            .env("NO_COLOR", "1"),
    );
    engrams(&db)
        .args(["decision", "supersede", "2", "--by", "1"])
        .assert()
        .success();

    let res = json(
        engrams(&db)
            .args([
                "decision",
                "log",
                "--summary",
                "Use Vec<&dyn ToSql> for SQL parameters",
            ])
            .env("NO_COLOR", "1"),
    );
    assert_eq!(
        res["inserted"].as_bool(),
        Some(false),
        "gate blocked: {res}"
    );
    let similar = res["similar"].as_array().unwrap();
    assert!(!similar.is_empty());
    let ids: Vec<i64> = similar.iter().map(|s| s["id"].as_i64().unwrap()).collect();
    assert!(ids.contains(&1), "active listed: {ids:?}");
    assert!(!ids.contains(&2), "superseded absent: {ids:?}");
    assert!(
        similar
            .iter()
            .all(|s| s["suggested_relation"].as_str() == Some("conflicts_with")),
        "suggested_relation: {similar:?}"
    );
}

#[test]
fn s6_supersedes_flag_inserts_flips_status_and_links() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    log_decision(&db, "Old approach for config loading");
    let res = json(
        engrams(&db)
            .args([
                "decision",
                "log",
                "--summary",
                "New approach for config loading",
                "--supersedes",
                "1",
            ])
            .env("NO_COLOR", "1"),
    );
    assert_eq!(res["inserted"].as_bool(), Some(true));
    let new_id = res["id"].as_i64().unwrap();
    assert_eq!(new_id, 2);

    let old = json(engrams(&db).args(["decision", "get", "1"]));
    assert_eq!(old["status"].as_str(), Some("superseded"));
    assert!(
        decision_links(&db, new_id)
            .iter()
            .any(|(rel, src, tgt)| rel == "supersedes" && src == &new_id.to_string() && tgt == "1"),
        "supersedes link missing"
    );
}

#[test]
fn s7_conflicts_with_flag_inserts_and_links() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    log_decision(&db, "Serve static assets via CDN");
    let res = json(
        engrams(&db)
            .args([
                "decision",
                "log",
                "--summary",
                "Bundle assets inline instead of CDN",
                "--conflicts-with",
                "1",
            ])
            .env("NO_COLOR", "1"),
    );
    assert_eq!(res["inserted"].as_bool(), Some(true));
    let stored = json(engrams(&db).args(["decision", "get", "2"]));
    assert!(
        stored.get("status").is_none(),
        "absent status = active by convention"
    );
    assert_eq!(
        res["conflicts_with"].as_array().unwrap(),
        &vec![Value::from(1)]
    );
    assert!(
        decision_links(&db, 2)
            .iter()
            .any(|(rel, src, tgt)| rel == "conflicts_with" && src == "2" && tgt == "1"),
        "conflicts_with link missing"
    );
}

#[test]
fn s8_force_bypasses_gate() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    log_decision(&db, "Prefer uuid v4 for identifiers");
    let blocked = json(
        engrams(&db)
            .args([
                "decision",
                "log",
                "--summary",
                "Prefer uuid v4 for new identifiers",
            ])
            .env("NO_COLOR", "1"),
    );
    assert_eq!(blocked["inserted"].as_bool(), Some(false));
    let forced = json(
        engrams(&db)
            .args([
                "decision",
                "log",
                "--summary",
                "Prefer uuid v4 for new identifiers",
                "--force",
            ])
            .env("NO_COLOR", "1"),
    );
    assert_eq!(forced["inserted"].as_bool(), Some(true));
}

// --- S9–S11: consolidation ------------------------------------------------

#[test]
fn s9_s10_s11_consolidate_propose_apply_confirm() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    for i in 1..=4 {
        log_progress(&db, &format!("tuned scoring weights run {i}"));
        add_anchor(&db, "progress-entry", i, "src/ops/scoring.rs");
    }
    // 4 entries across 3 distinct days.
    {
        let conn = Connection::open(&db).unwrap();
        for (id, days) in [(1, 9), (2, 9), (3, 5), (4, 1)] {
            conn.execute(
                "UPDATE progress_entries SET timestamp = ?1 WHERE id = ?2",
                rusqlite::params![rfc3339_days_ago(days), id],
            )
            .unwrap();
        }
    }

    // S9 — propose only: candidate reported, nothing written.
    let res = json(engrams(&db).args(["consolidate"]));
    let candidates = res["candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 1, "candidates: {candidates:?}");
    let c = &candidates[0];
    assert_eq!(c["name"].as_str(), Some("consolidated-scoring"));
    assert_eq!(
        c["evidence"].as_array().unwrap(),
        &vec![
            Value::from(1),
            Value::from(2),
            Value::from(3),
            Value::from(4)
        ]
    );
    assert_eq!(c["initial_confidence"].as_f64(), Some(0.65));
    assert!(c["anchors"]
        .as_array()
        .unwrap()
        .iter()
        .any(|a| a == "src/ops/scoring.rs"));
    assert_eq!(scalar_i64(&db, "SELECT count(*) FROM system_patterns"), 0);

    // S10 — apply: pattern + evidence links + anchor, confidence 0.65.
    let res = json(engrams(&db).args(["consolidate", "--apply"]));
    let applied = res["applied"].as_array().unwrap();
    assert_eq!(applied.len(), 1);
    let pattern_id = applied[0]["id"].as_i64().unwrap();
    assert_eq!(pattern_id, 1);
    {
        let conn = Connection::open(&db).unwrap();
        let (confidence, last_confirmed): (f64, Option<String>) = conn
            .query_row(
                "SELECT confidence, last_confirmed_at FROM system_patterns WHERE id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(confidence, 0.65);
        assert!(last_confirmed.is_some());
        let links: i64 = conn
            .query_row(
                "SELECT count(*) FROM context_links \
                 WHERE relationship_type = 'derived_from' \
                 AND source_item_type = 'system_pattern' AND source_item_id = '1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(links, 4, "derived_from evidence links");
        let anchors: i64 = conn
            .query_row(
                "SELECT count(*) FROM item_anchors \
                 WHERE item_type = 'system_pattern' AND item_id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(anchors, 1);
    }

    // S11 — new evidence confirms the existing pattern (propose mode writes it).
    std::thread::sleep(std::time::Duration::from_millis(1100));
    log_progress(&db, "tuned scoring weights run 5");
    add_anchor(&db, "progress-entry", 5, "src/ops/scoring.rs");

    let res = json(engrams(&db).args(["consolidate"]));
    let confirmed = res["confirmed"].as_array().unwrap();
    assert_eq!(confirmed.len(), 1, "confirmed: {confirmed:?}");
    assert_eq!(confirmed[0]["id"].as_i64(), Some(1));
    assert_eq!(
        confirmed[0]["new_evidence"].as_array().unwrap(),
        &vec![Value::from(5)]
    );
    assert_eq!(scalar_i64(&db, "SELECT count(*) FROM system_patterns"), 1);
    assert_eq!(
        scalar_i64(
            &db,
            "SELECT count(*) FROM context_links \
             WHERE relationship_type = 'derived_from' \
             AND source_item_type = 'system_pattern' AND source_item_id = '1'"
        ),
        5,
        "confirm attaches new evidence"
    );

    // Idempotent: re-running with no new evidence confirms nothing.
    let res = json(engrams(&db).args(["consolidate"]));
    assert!(
        res["confirmed"].as_array().unwrap().is_empty(),
        "res: {res}"
    );
    assert_eq!(
        scalar_i64(
            &db,
            "SELECT count(*) FROM context_links \
             WHERE relationship_type = 'derived_from' \
             AND source_item_type = 'system_pattern' AND source_item_id = '1'"
        ),
        5
    );
}

// --- S12: round-trip + ranking ---------------------------------------------

#[test]
fn s12_export_import_preserves_confidence_fields() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    for name in ["p1", "p2"] {
        engrams(&db)
            .args(["pattern", "log", "--name", name, "--description", "d"])
            .assert()
            .success();
    }
    let (ts_a, ts_b) = (rfc3339_days_ago(3), rfc3339_days_ago(60));
    {
        let conn = Connection::open(&db).unwrap();
        conn.execute(
            "UPDATE system_patterns SET confidence = 0.65, last_confirmed_at = ?1 WHERE id = 1",
            rusqlite::params![ts_a],
        )
        .unwrap();
        conn.execute(
            "UPDATE system_patterns SET confidence = 0.5, last_confirmed_at = ?1 WHERE id = 2",
            rusqlite::params![ts_b],
        )
        .unwrap();
    }

    let export_dir = temp.path().join("export");
    engrams(&db)
        .args(["export", "--path"])
        .arg(&export_dir)
        .assert()
        .success();

    let db2 = temp.path().join("e2.db");
    engrams(&db2)
        .args(["import", "--path"])
        .arg(&export_dir)
        .assert()
        .success();

    let conn = Connection::open(&db2).unwrap();
    let mut stmt = conn
        .prepare("SELECT id, confidence, last_confirmed_at FROM system_patterns ORDER BY id")
        .unwrap();
    let rows: Vec<(i64, f64, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        rows,
        vec![(1, 0.65, ts_a), (2, 0.5, ts_b)],
        "confidence fields preserved exactly"
    );
}

#[test]
fn s12_prime_ranks_recently_confirmed_above_stale() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    for name in ["p-stale", "p-fresh"] {
        engrams(&db)
            .args(["pattern", "log", "--name", name, "--description", "d"])
            .assert()
            .success();
    }
    let shared_ts = rfc3339_days_ago(30);
    {
        let conn = Connection::open(&db).unwrap();
        // Equal importance and timestamps; only confirmation recency differs.
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
    let names: Vec<&str> = prime["patterns"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["p-fresh", "p-stale"], "prime order: {names:?}");
}

// --- S13: doctor advisory ---------------------------------------------------

#[test]
fn s13_doctor_flags_stale_and_never_confirmed_patterns() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    for name in ["p-stale", "p-never"] {
        engrams(&db)
            .args(["pattern", "log", "--name", name, "--description", "d"])
            .assert()
            .success();
    }
    log_progress(&db, "evidence entry");
    // Mark both as consolidation products via derived_from evidence links.
    for pid in [1, 2] {
        engrams(&db)
            .args([
                "link",
                "add",
                "--source-type",
                "system-pattern",
                "--source-id",
                &pid.to_string(),
                "--target-type",
                "progress-entry",
                "--target-id",
                "1",
                "--rel",
                "derived_from",
            ])
            .assert()
            .success();
    }
    {
        let conn = Connection::open(&db).unwrap();
        conn.execute(
            "UPDATE system_patterns SET last_confirmed_at = ?1 WHERE id = 1",
            rusqlite::params![rfc3339_days_ago(200)],
        )
        .unwrap();
    }

    let doctor = json(engrams(&db).args(["doctor"]));
    let unconfirmed = doctor["unconfirmed_patterns"].as_array().unwrap();
    let ids: Vec<i64> = unconfirmed
        .iter()
        .map(|p| p["id"].as_i64().unwrap())
        .collect();
    assert!(ids.contains(&1), "200-day-old flagged: {ids:?}");
    assert!(
        unconfirmed
            .iter()
            .any(|p| p["id"].as_i64() == Some(1)
                && p["days_since_confirmation"].as_i64() == Some(200)),
        "advisory details: {unconfirmed:?}"
    );
    assert!(ids.contains(&2), "never-confirmed flagged: {ids:?}");
    assert_eq!(doctor["ok"].as_bool(), Some(false));
}

// --- S14: schema round-trip through export / import (spec 0002, AC-11) ------

#[test]
fn s14_export_import_preserves_schemas_with_identity_and_telemetry() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    engrams(&db).arg("init").assert().success();

    // Full schema lifecycle on one database: a dense fully-linked trio
    // seeded by SQL (same rationale as the CLI schema tests — `anchor add`
    // would materialize a code node into the cluster), scanned to stability,
    // promoted with the drafted name, then refined as agent-authored.
    {
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch(
            "INSERT INTO decisions (uuid, timestamp, summary, tags, commit_sha) VALUES
             ('u1','2026-01-01T00:00:00Z','alpha gateway routing','[\"core\",\"graph\"]','abc'),
             ('u2','2026-01-01T00:00:00Z','beta rendering pipeline','[\"core\",\"graph\"]','abc'),
             ('u3','2026-01-02T00:00:00Z','gamma policy engine','[\"core\",\"graph\"]','abc');
             INSERT INTO item_anchors (item_type, item_id, path, timestamp) VALUES
             ('decision','1','src/a.rs','2026-01-01T00:00:00Z'),
             ('decision','2','src/a.rs','2026-01-01T00:00:00Z'),
             ('decision','3','src/a.rs','2026-01-01T00:00:00Z');
             INSERT INTO context_links (source_item_type, source_item_id, target_item_type,
              target_item_id, relationship_type, timestamp, origin) VALUES
             ('decision','1','decision','2','relates_to','2026-01-01T00:00:00Z','manual'),
             ('decision','2','decision','3','relates_to','2026-01-01T00:00:00Z','manual'),
             ('decision','1','decision','3','relates_to','2026-01-01T00:00:00Z','manual');
             INSERT INTO retrieval_surfaces (ts, cmd, arg, node_kind, node_id) VALUES
             ('2026-02-15T00:00:00Z','query','core','decision',1),
             ('2026-02-15T00:00:00Z','query','core','decision',2),
             ('2026-02-15T00:00:00Z','query','core','decision',3);",
        )
        .unwrap();
    }
    engrams(&db).args(["schema", "scan"]).assert().success();
    engrams(&db).args(["schema", "scan"]).assert().success();
    engrams(&db).args(["schema", "scan"]).assert().success();
    engrams(&db)
        .args([
            "schema",
            "propose",
            "--from",
            "decision:1",
            "--name",
            "core",
            "--statement",
            "Core gateway routing and rendering conventions",
            "--summary",
            "When requests hit the gateway: route through the shared table",
        ])
        .assert()
        .success();
    engrams(&db)
        .args(["schema", "confirm", "proposal:1"])
        .assert()
        .success();
    engrams(&db)
        .args([
            "schema",
            "refine",
            "--summary",
            "When requests hit the gateway, they route through the shared routing table",
            "core",
        ])
        .assert()
        .success();
    // A fired suggestion with a user resolution: a lexically matching item
    // that explicitly declined (the decision-79 opt-out leaves an audit row).
    engrams(&db)
        .args([
            "decision",
            "log",
            "--summary",
            "core graph export conventions",
            "--schema",
            "none",
        ])
        .assert()
        .success();
    // Reward telemetry: surface the schema so retrieval_surfaces has a row.
    // "core" overlaps the centroid tag vocabulary, so the query fires a
    // surfacing event (an overlapping query is what records telemetry).
    engrams(&db).args(["query", "core"]).assert().success();

    let mut out = engrams(&db)
        .args(["export", "--path"])
        .arg(temp.path().join("export"))
        .output()
        .unwrap();
    assert!(out.status.success(), "export failed: {out:?}");

    let fresh = temp.path().join("fresh.db");
    engrams(&fresh).arg("init").assert().success();
    out = engrams(&fresh)
        .args(["import", "--path"])
        .arg(temp.path().join("export"))
        .output()
        .unwrap();
    assert!(out.status.success(), "import failed: {out:?}");

    // Identity, membership, summaries, and telemetry intact on the target.
    let show = json(engrams(&fresh).args(["schema", "show", "core"]));
    let schema = &show["schema"];
    assert_eq!(schema["name"], "core");
    assert_eq!(schema["summary_source"], "agent");
    assert_eq!(
        schema["summary"],
        "When requests hit the gateway, they route through the shared routing table"
    );
    assert_eq!(schema["member_count"], 3);
    let members = schema["members"].as_array().unwrap();
    let keys: Vec<String> = members
        .iter()
        .map(|m| format!("{}:{}", m["kind"].as_str().unwrap(), m["id"]))
        .collect();
    assert_eq!(
        keys,
        vec!["decision:1", "decision:2", "decision:3"],
        "members restored with original ids: {members:?}"
    );

    let first_surfaces;
    {
        let conn = Connection::open(&fresh).unwrap();
        // The uuid is the identity anchor across the move.
        let (src_uuid, tgt_uuid): (String, String) = {
            let s = Connection::open(&db).unwrap();
            (
                s.query_row("SELECT uuid FROM schemas WHERE id = 1", [], |r| r.get(0))
                    .unwrap(),
                conn.query_row("SELECT uuid FROM schemas WHERE id = 1", [], |r| r.get(0))
                    .unwrap(),
            )
        };
        assert_eq!(src_uuid, tgt_uuid, "schema identity survives the move");

        // Fired-suggestion resolution traveled.
        let (status, n): (String, i64) = conn
            .query_row(
                "SELECT status, COUNT(*) FROM schema_suggestions \
                 WHERE item_kind = 'decision' AND item_id = 4",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((status.as_str(), n), ("declined", 1), "{status} x{n}");

        // Reward telemetry (retrieval_surfaces) traveled.
        let surfaces: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM retrieval_surfaces WHERE node_kind = 'schema'",
                [],
                |r| r.get(0),
            )
            .unwrap();

        assert!(surfaces >= 1, "surfaces traveled: {surfaces}");
        first_surfaces = surfaces;
        let fts: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM schemas_fts WHERE schemas_fts MATCH 'core'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(fts, 1, "schemas_fts hit after import");

        // No surprise writes on the source (AC-4 still holds): scan on the
        // target does not create a second schema.
    }
    // Pin (retry-1 review): re-importing the same export must be
    // idempotent — no duplicate links, no duplicate telemetry.
    engrams(&fresh)
        .args(["import", "--path"])
        .arg(temp.path().join("export"))
        .assert()
        .success();
    {
        let conn = Connection::open(&fresh).unwrap();
        let surfaces: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM retrieval_surfaces WHERE node_kind = 'schema'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            surfaces, first_surfaces,
            "double import duplicated telemetry"
        );
        let member_edges: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM context_links WHERE relationship_type = 'member_of'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(member_edges, 3, "double import duplicated member_of edges");
    }
    engrams(&fresh).args(["schema", "scan"]).assert().success();
    {
        let conn = Connection::open(&fresh).unwrap();
        let schemas: i64 = conn
            .query_row("SELECT COUNT(*) FROM schemas", [], |r| r.get(0))
            .unwrap();
        assert_eq!(schemas, 1, "post-import scan must not duplicate");
    }
}

// --- S15: read-path kind surfacing ------------------------------------------

#[test]
fn s15_prime_list_show_surface_kind() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    engrams(&db).arg("init").assert().success();

    // The dense fully-linked trio, seeded by SQL (s14's rationale): the
    // scan's kind pass labels the candidate `schema`, confirm copies that
    // snapshot into the schemas row, and every read path must surface it
    // (spec 0004 AC-2/4/5).
    {
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch(
            "INSERT INTO decisions (uuid, timestamp, summary, tags, commit_sha) VALUES
             ('u1','2026-01-01T00:00:00Z','alpha gateway routing','[\"core\",\"graph\"]','abc'),
             ('u2','2026-01-01T00:00:00Z','beta rendering pipeline','[\"core\",\"graph\"]','abc'),
             ('u3','2026-01-02T00:00:00Z','gamma policy engine','[\"core\",\"graph\"]','abc');
             INSERT INTO item_anchors (item_type, item_id, path, timestamp) VALUES
             ('decision','1','src/a.rs','2026-01-01T00:00:00Z'),
             ('decision','2','src/a.rs','2026-01-01T00:00:00Z'),
             ('decision','3','src/a.rs','2026-01-01T00:00:00Z');
             INSERT INTO context_links (source_item_type, source_item_id, target_item_type,
              target_item_id, relationship_type, timestamp, origin) VALUES
             ('decision','1','decision','2','relates_to','2026-01-01T00:00:00Z','manual'),
             ('decision','2','decision','3','relates_to','2026-01-01T00:00:00Z','manual'),
             ('decision','1','decision','3','relates_to','2026-01-01T00:00:00Z','manual');
             INSERT INTO retrieval_surfaces (ts, cmd, arg, node_kind, node_id) VALUES
             ('2026-02-15T00:00:00Z','query','core','decision',1),
             ('2026-02-15T00:00:00Z','query','core','decision',2),
             ('2026-02-15T00:00:00Z','query','core','decision',3);",
        )
        .unwrap();
    }
    engrams(&db).args(["schema", "scan"]).assert().success();
    engrams(&db).args(["schema", "scan"]).assert().success();
    engrams(&db).args(["schema", "scan"]).assert().success();
    engrams(&db)
        .args([
            "schema",
            "propose",
            "--from",
            "decision:1",
            "--name",
            "core",
            "--statement",
            "Core gateway routing and rendering conventions",
            "--summary",
            "When requests hit the gateway: route through the shared table",
        ])
        .assert()
        .success();
    engrams(&db)
        .args(["schema", "confirm", "proposal:1"])
        .assert()
        .success();

    // AC-2: prime schemas[] entries carry kind.
    engrams(&db)
        .args(["active-context", "update", "--content", "{\"tasks\": []}"])
        .assert()
        .success();
    let res = json(engrams(&db).arg("prime"));
    let schemas = res["schemas"].as_array().unwrap();
    assert_eq!(schemas.len(), 1, "prime leads with the schema: {res}");
    assert_eq!(schemas[0]["kind"], "schema");

    // AC-4: list shows kind next to status.
    let res = json(engrams(&db).args(["schema", "list"]));
    let entry = &res["schemas"][0];
    assert_eq!(entry["kind"], "schema", "list kind: {res}");
    assert_eq!(entry["status"], "active");

    // AC-5: show parses the confirm-time reasons into sentence strings.
    let res = json(engrams(&db).args(["schema", "show", "core"]));
    let schema = &res["schema"];
    assert_eq!(schema["kind"], "schema");
    let reasons = schema["reasons"].as_array().unwrap();
    assert!(!reasons.is_empty(), "reasons rendered: {schema}");
    assert!(
        reasons.iter().all(|r| r.is_string()),
        "reasons are sentences: {reasons:?}"
    );

    // Pre-0004 rows (kind `unclear`, reasons `[]`) display as-is on every
    // path — no error, no filtering.
    {
        let conn = Connection::open(&db).unwrap();
        conn.execute(
            "UPDATE schemas SET kind = 'unclear', kind_reasons_json = '[]'",
            [],
        )
        .unwrap();
    }
    let res = json(engrams(&db).arg("prime"));
    assert_eq!(res["schemas"][0]["kind"], "unclear");
    let res = json(engrams(&db).args(["schema", "list"]));
    assert_eq!(res["schemas"][0]["kind"], "unclear");
    let res = json(engrams(&db).args(["schema", "show", "core"]));
    assert_eq!(res["schema"]["kind"], "unclear");
    assert_eq!(
        res["schema"]["reasons"].as_array().unwrap().len(),
        0,
        "empty reasons render as []: {res}"
    );
}

// --- S16: export/import round-trips schema kind (spec 0004 AC-6) -----------

#[test]
fn s16_export_import_round_trips_schema_kind() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    engrams(&db).arg("init").assert().success();

    // A dense fully-linked trio (s14's seed) beside a legacy pre-0004 schema
    // row that stays on the column defaults (`unclear` / `[]`).
    {
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch(
            "INSERT INTO decisions (uuid, timestamp, summary, tags, commit_sha) VALUES
             ('u1','2026-01-01T00:00:00Z','alpha gateway routing','[\"core\",\"graph\"]','abc'),
             ('u2','2026-01-01T00:00:00Z','beta rendering pipeline','[\"core\",\"graph\"]','abc'),
             ('u3','2026-01-02T00:00:00Z','gamma policy engine','[\"core\",\"graph\"]','abc');
             INSERT INTO item_anchors (item_type, item_id, path, timestamp) VALUES
             ('decision','1','src/a.rs','2026-01-01T00:00:00Z'),
             ('decision','2','src/a.rs','2026-01-01T00:00:00Z'),
             ('decision','3','src/a.rs','2026-01-01T00:00:00Z');
             INSERT INTO context_links (source_item_type, source_item_id,
              target_item_type, target_item_id, relationship_type, timestamp, origin) VALUES
             ('decision','1','decision','2','relates_to','2026-01-01T00:00:00Z','manual'),
             ('decision','2','decision','3','relates_to','2026-01-01T00:00:00Z','manual'),
             ('decision','1','decision','3','relates_to','2026-01-01T00:00:00Z','manual');
             INSERT INTO retrieval_surfaces (ts, cmd, arg, node_kind, node_id) VALUES
             ('2026-02-15T00:00:00Z','query','core','decision',1),
             ('2026-02-15T00:00:00Z','query','core','decision',2),
             ('2026-02-15T00:00:00Z','query','core','decision',3);
             -- Legacy pre-0004 row: never labeled, on the column defaults.
             INSERT INTO schemas (uuid, name, summary, centroid_json, created_at, updated_at) VALUES
             ('legacy-uuid','legacy-pack','pre-0004 pack never labeled','{}',
              '2025-06-01T00:00:00Z','2025-06-01T00:00:00Z');",
        )
        .unwrap();
    }
    engrams(&db).args(["schema", "scan"]).assert().success();
    engrams(&db).args(["schema", "scan"]).assert().success();
    engrams(&db).args(["schema", "scan"]).assert().success();
    engrams(&db)
        .args([
            "schema",
            "propose",
            "--from",
            "decision:1",
            "--name",
            "core",
            "--statement",
            "Core gateway routing and rendering conventions",
            "--summary",
            "When requests hit the gateway: route through the shared table",
        ])
        .assert()
        .success();
    engrams(&db)
        .args(["schema", "confirm", "proposal:1"])
        .assert()
        .success();
    {
        let conn = Connection::open(&db).unwrap();
        let (kind, reasons): (String, String) = conn
            .query_row(
                "SELECT kind, kind_reasons_json FROM schemas WHERE name = 'core'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(kind, "schema", "confirm-time snapshot: {kind}");
        let parsed: Vec<String> = serde_json::from_str(&reasons).unwrap();
        assert!(!parsed.is_empty(), "reasons snapshot copied: {reasons}");
        let (kind, reasons): (String, String) = conn
            .query_row(
                "SELECT kind, kind_reasons_json FROM schemas WHERE name = 'legacy-pack'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            (kind.as_str(), reasons.as_str()),
            ("unclear", "[]"),
            "legacy row untouched by the labeler"
        );
    }

    let exp_dir = temp.path().join("export");
    engrams(&db)
        .args(["export", "--path", exp_dir.to_str().unwrap()])
        .assert()
        .success();

    // The export files themselves carry both columns verbatim (AC-6 emission).
    let mut exported: Vec<String> = std::fs::read_dir(exp_dir.join("schemas"))
        .unwrap()
        .map(|e| e.unwrap().path())
        // retrieval_surfaces.json shares the dir but is not a schema row;
        // import's process_dir likewise reads only .md files.
        .filter(|p| p.extension().is_some_and(|x| x == "md"))
        .map(|p| std::fs::read_to_string(p).unwrap())
        .collect();
    exported.sort();
    assert_eq!(exported.len(), 2, "both schema rows exported: {exported:?}");
    assert!(
        exported.iter().any(|c| c.contains("\"kind\": \"schema\"")),
        "labeled row exports its kind: {exported:?}"
    );
    assert!(
        exported
            .iter()
            .any(|c| c.contains("\"kind\": \"unclear\"")
                && c.contains("\"kind_reasons_json\": \"[]\"")),
        "legacy row exports its defaults verbatim: {exported:?}"
    );

    let fresh = temp.path().join("fresh.db");
    engrams(&fresh).arg("init").assert().success();
    engrams(&fresh)
        .args(["import", "--path", exp_dir.to_str().unwrap()])
        .assert()
        .success();

    // Round-trip identity: every row's (kind, kind_reasons_json) pair is
    // reproduced identically — verbatim snapshot, nothing re-derived.
    {
        let dump = |conn: &Connection| -> Vec<(String, String)> {
            let mut stmt = conn
                .prepare("SELECT kind, kind_reasons_json FROM schemas ORDER BY id")
                .unwrap();
            let rows = stmt
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
                .unwrap();
            rows.map(|r| r.unwrap()).collect()
        };
        let source_pairs = dump(&Connection::open(&db).unwrap());
        let target_pairs = dump(&Connection::open(&fresh).unwrap());
        assert_eq!(
            source_pairs.len(),
            2,
            "both rows exported: {source_pairs:?}"
        );
        assert_eq!(
            source_pairs, target_pairs,
            "kind snapshots reproduced identically"
        );
        assert!(
            target_pairs
                .iter()
                .any(|(k, rs)| k == "schema" && rs != "[]"),
            "labeled snapshot survives the move: {target_pairs:?}"
        );
    }

    // Read path surfaces the transported label (AC-2 payload intact).
    let res = json(engrams(&fresh).args(["schema", "show", "core"]));
    assert_eq!(res["schema"]["kind"], "schema");
    assert!(
        !res["schema"]["reasons"].as_array().unwrap().is_empty(),
        "reasons snapshot arrived: {res}"
    );
}

// --- Spec 0006 AC-9: export and restore code references by path -------------

#[test]
fn import_restores_code_references_by_path_ac9() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("source.db");
    engrams(&db).arg("init").assert().success();

    {
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch(
            "INSERT INTO code_nodes (id, kind, path, symbol, first_seen, last_seen) VALUES
             (31975, 'file', 'src/ops', '', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
             INSERT INTO decisions (id, uuid, timestamp, summary, tags) VALUES
             (1, 'u-d1', '2026-01-01T00:00:00Z', 'borrow sql params', '[\"performance\"]');
             INSERT INTO schemas (id, uuid, name, summary, summary_source, status, centroid_json, created_at, updated_at, kind, kind_reasons_json) VALUES
             (1, 'u-s1', 'performance-rules', 'Fires when editing Rust code under src/ops', 'agent', 'active', '{\"tags\":{\"performance\":1},\"anchors\":{\"src/ops\":1}}', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 'schema', '[]');
             INSERT INTO context_links (id, source_item_type, source_item_id, target_item_type, target_item_id, relationship_type, timestamp, origin, source) VALUES
             (10, 'code', '31975', 'schema', '1', 'member_of', '2026-01-01T00:00:00Z', 'manual', 'schema_confirm'),
             (11, 'decision', '1', 'schema', '1', 'member_of', '2026-01-01T00:00:00Z', 'manual', 'schema_confirm'),
             (12, 'decision', '1', 'code', '31975', 'relates_to', '2026-01-01T00:00:00Z', 'manual', NULL);
             INSERT INTO retrieval_surfaces (ts, cmd, arg, node_kind, node_id) VALUES
             ('2026-02-15T00:00:00Z', 'prime', NULL, 'code', 31975);",
        )
        .unwrap();
    }

    let exp_dir = temp.path().join("export");
    engrams(&db)
        .args(["export", "--path"])
        .arg(&exp_dir)
        .assert()
        .success();

    // Verify exported schema member, manual links, and retrieval surface carry path fields.
    let schema_md = std::fs::read_to_string(exp_dir.join("schemas/1.md")).unwrap();
    assert!(
        schema_md.contains("\"path\": \"src/ops\""),
        "exported schema member must carry path: {schema_md}"
    );
    let link_source_md = std::fs::read_to_string(exp_dir.join("links/10.md")).unwrap();
    assert!(
        link_source_md.contains("\"source_path\": \"src/ops\""),
        "exported source-code link must carry source_path: {link_source_md}"
    );
    let link_target_md = std::fs::read_to_string(exp_dir.join("links/12.md")).unwrap();
    assert!(
        link_target_md.contains("\"target_path\": \"src/ops\""),
        "exported target-code link must carry target_path: {link_target_md}"
    );
    let surfaces_json =
        std::fs::read_to_string(exp_dir.join("schemas/retrieval_surfaces.json")).unwrap();
    assert!(
        surfaces_json.contains("\"path\": \"src/ops\""),
        "exported code retrieval surface must carry path: {surfaces_json}"
    );

    // Seed a fresh database with pre-existing dummy code_nodes so the imported
    // code file gets a different code_nodes.id than 31975.
    let fresh = temp.path().join("fresh.db");
    engrams(&fresh).arg("init").assert().success();
    {
        let conn = Connection::open(&fresh).unwrap();
        conn.execute_batch(
            "INSERT INTO code_nodes (id, kind, path, symbol, first_seen, last_seen) VALUES
             (1, 'file', 'src/main.rs', '', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
             (2, 'file', 'src/lib.rs', '', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');",
        )
        .unwrap();
    }

    engrams(&fresh)
        .args(["import", "--path"])
        .arg(&exp_dir)
        .assert()
        .success();

    let new_code_id: i64 = {
        let conn = Connection::open(&fresh).unwrap();
        conn.query_row(
            "SELECT id FROM code_nodes WHERE kind = 'file' AND path = 'src/ops' AND symbol = ''",
            [],
            |r| r.get(0),
        )
        .unwrap()
    };
    assert_ne!(new_code_id, 31975, "fresh DB assigns a new code_nodes.id");

    // AC-9: inspecting the schema shows the code member with its path.
    let show = json(engrams(&fresh).args(["schema", "show", "performance-rules"]));
    let members = show["schema"]["members"].as_array().unwrap();
    let code_member = members
        .iter()
        .find(|m| m["kind"] == "code")
        .expect("schema show must include the restored code member");
    assert_eq!(code_member["id"], new_code_id);
    assert_eq!(code_member["path"], "src/ops");

    // Verify links and retrieval_surfaces point to new_code_id and not 31975,
    // and that re-importing is idempotent.
    engrams(&fresh)
        .args(["import", "--path"])
        .arg(&exp_dir)
        .assert()
        .success();
    {
        let conn = Connection::open(&fresh).unwrap();
        let code_member_edges: Vec<String> = {
            let mut stmt = conn
                .prepare(
                    "SELECT source_item_id FROM context_links \
                     WHERE relationship_type = 'member_of' AND source_item_type = 'code'",
                )
                .unwrap();
            stmt.query_map([], |r| r.get(0))
                .unwrap()
                .map(|r| r.unwrap())
                .collect()
        };
        assert_eq!(
            code_member_edges,
            vec![new_code_id.to_string()],
            "code member_of edge must be remapped to new_code_id without duplication"
        );

        let target_code_id: String = conn
            .query_row(
                "SELECT target_item_id FROM context_links WHERE id = 12",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(target_code_id, new_code_id.to_string());

        let surface_ids: Vec<i64> = {
            let mut stmt = conn
                .prepare("SELECT node_id FROM retrieval_surfaces WHERE node_kind = 'code'")
                .unwrap();
            stmt.query_map([], |r| r.get(0))
                .unwrap()
                .map(|r| r.unwrap())
                .collect()
        };
        assert_eq!(surface_ids, vec![new_code_id]);
    }
}

#[test]
fn import_skips_missing_references_preserves_existing_rows_and_scans_ac10_ac11() {
    let temp = TempDir::new().unwrap();
    let exp_dir = temp.path().join("export");
    for sub in [
        "decisions",
        "progress",
        "patterns",
        "links",
        "anchors",
        "schemas",
    ] {
        std::fs::create_dir_all(exp_dir.join(sub)).unwrap();
    }

    let write_md = |rel: &str, value: serde_json::Value| {
        let body = format!(
            "---\nidentifier: \"1\"\n---\n\n# item\n\n```json\n{}\n```\n",
            serde_json::to_string_pretty(&value).unwrap()
        );
        std::fs::write(exp_dir.join(rel), body).unwrap();
    };

    // Valid decisions forming a dense cluster for AC-11 schema scan.
    for (id, summary) in [
        (1, "alpha gateway routing"),
        (2, "beta rendering pipeline"),
        (3, "gamma policy engine"),
    ] {
        write_md(
            &format!("decisions/{id}.md"),
            serde_json::json!({
                "id": id,
                "uuid": format!("u-d{id}"),
                "summary": summary,
                "tags": ["core", "graph"],
                "timestamp": "2026-01-01T00:00:00Z",
                "commit_sha": "abc1234"
            }),
        );
    }

    // Progress entries: 10 (root), 11 (valid parent 10), 12 (missing parent 99).
    write_md(
        "progress/10.md",
        serde_json::json!({
            "id": 10,
            "timestamp": "2026-01-01T00:00:00Z",
            "status": "Done",
            "description": "root progress",
            "parent_id": null
        }),
    );
    write_md(
        "progress/11.md",
        serde_json::json!({
            "id": 11,
            "timestamp": "2026-01-01T00:00:00Z",
            "status": "Done",
            "description": "child progress with valid parent",
            "parent_id": 10
        }),
    );
    write_md(
        "progress/12.md",
        serde_json::json!({
            "id": 12,
            "timestamp": "2026-01-01T00:00:00Z",
            "status": "Done",
            "description": "orphan progress with missing parent",
            "parent_id": 99
        }),
    );

    // Patterns: id 1 is new; id 50 collides by name with pre-existing id 5 in target DB (F-2).
    write_md(
        "patterns/1.md",
        serde_json::json!({
            "id": 1,
            "uuid": "u-p1",
            "name": "valid-pattern",
            "description": "valid pattern",
            "timestamp": "2026-01-01T00:00:00Z"
        }),
    );
    write_md(
        "patterns/2.md",
        serde_json::json!({
            "id": 50,
            "uuid": "u-p50",
            "name": "colliding-pattern",
            "description": "updated description on conflict",
            "timestamp": "2026-01-02T00:00:00Z"
        }),
    );

    // Links: 1..=3 valid decision triangle, 4 valid PR link, 42 dangling -> decision:99.
    for (id, a, b) in [(1, "1", "2"), (2, "2", "3"), (3, "1", "3")] {
        write_md(
            &format!("links/{id}.md"),
            serde_json::json!({
                "id": id,
                "source_item_type": "decision",
                "source_item_id": a,
                "target_item_type": "decision",
                "target_item_id": b,
                "relationship_type": "relates_to",
                "timestamp": "2026-01-01T00:00:00Z",
                "origin": "manual"
            }),
        );
    }
    write_md(
        "links/4.md",
        serde_json::json!({
            "id": 4,
            "source_item_type": "decision",
            "source_item_id": "1",
            "target_item_type": "pr",
            "target_item_id": "https://github.com/example/repo/pull/7",
            "relationship_type": "implemented_in",
            "timestamp": "2026-01-01T00:00:00Z",
            "origin": "manual"
        }),
    );
    write_md(
        "links/42.md",
        serde_json::json!({
            "id": 42,
            "source_item_type": "decision",
            "source_item_id": "1",
            "target_item_type": "decision",
            "target_item_id": "99",
            "relationship_type": "relates_to",
            "timestamp": "2026-01-01T00:00:00Z",
            "origin": "manual"
        }),
    );

    // Anchors: 1 valid (decision:1), 41 dangling (decision:99), 50 dangling due to name conflict (system_pattern:50).
    write_md(
        "anchors/1.md",
        serde_json::json!({
            "id": 1,
            "item_type": "decision",
            "item_id": 1,
            "path": "src/a.rs",
            "timestamp": "2026-01-01T00:00:00Z"
        }),
    );
    write_md(
        "anchors/41.md",
        serde_json::json!({
            "id": 41,
            "item_type": "decision",
            "item_id": 99,
            "path": "src/dangling.rs",
            "timestamp": "2026-01-01T00:00:00Z"
        }),
    );
    write_md(
        "anchors/50.md",
        serde_json::json!({
            "id": 50,
            "item_type": "system_pattern",
            "item_id": 50,
            "path": "src/conflict.rs",
            "timestamp": "2026-01-01T00:00:00Z"
        }),
    );

    // Schema 1 with valid and missing members + valid and missing suggestions (F-1).
    write_md(
        "schemas/1.md",
        serde_json::json!({
            "id": 1,
            "uuid": "u-s1",
            "name": "existing-pack",
            "summary": "Pack with mixed valid and dangling references",
            "summary_source": "agent",
            "status": "active",
            "centroid_json": "{}",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:00Z",
            "kind": "schema",
            "kind_reasons_json": "[]",
            "members": [
                {"kind": "decision", "id": "1", "origin": "manual", "source": "schema_confirm"},
                {"kind": "code", "id": "31975", "path": "src/ops", "origin": "manual", "source": "schema_confirm"},
                {"kind": "decision", "id": "99", "origin": "manual", "source": "schema_confirm"},
                {"kind": "code", "id": "77777", "origin": "manual", "source": "schema_confirm"}
            ],
            "suggestions": [
                {"ts": "2026-01-02T00:00:00Z", "item_kind": "pattern", "item_id": 1, "fit": 0.8, "status": "accepted"},
                {"ts": "2026-01-02T00:00:00Z", "item_kind": "progress-entry", "item_id": 10, "fit": 0.7, "status": "declined"},
                {"ts": "2026-01-02T00:00:00Z", "item_kind": "pattern", "item_id": 88, "fit": 0.6, "status": "declined"}
            ]
        }),
    );

    std::fs::write(
        exp_dir.join("schemas/suggestions.json"),
        serde_json::to_string_pretty(&serde_json::json!([
            {"ts": "2026-01-02T00:00:00Z", "schema_id": 1, "item_kind": "custom-data", "item_id": 77, "fit": 0.5, "status": "declined"}
        ]))
        .unwrap(),
    )
    .unwrap();

    // Retrieval surfaces: valid decision 1..=3, valid code with path, dangling decision:99.
    std::fs::write(
        exp_dir.join("schemas/retrieval_surfaces.json"),
        serde_json::to_string_pretty(&serde_json::json!([
            {"ts": "2026-02-15T00:00:00Z", "cmd": "query", "arg": "core", "node_kind": "decision", "node_id": 1},
            {"ts": "2026-02-15T00:00:00Z", "cmd": "query", "arg": "core", "node_kind": "decision", "node_id": 2},
            {"ts": "2026-02-15T00:00:00Z", "cmd": "query", "arg": "core", "node_kind": "decision", "node_id": 3},
            {"ts": "2026-02-15T00:00:00Z", "cmd": "prime", "arg": null, "node_kind": "code", "node_id": 31975, "path": "src/ops"},
            {"ts": "2026-02-15T00:00:00Z", "cmd": "query", "arg": "core", "node_kind": "decision", "node_id": 99}
        ]))
        .unwrap(),
    )
    .unwrap();

    // Target DB with pre-existing rows at anchor id 41, link id 42, and pattern id 5 ("colliding-pattern").
    let db = temp.path().join("target.db");
    engrams(&db).arg("init").assert().success();
    {
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch(
            "INSERT INTO decisions (id, uuid, timestamp, summary, tags) VALUES
             (1, 'u-d1', '2026-01-01T00:00:00Z', 'pre-existing d1', '[\"core\"]');
             INSERT INTO system_patterns (id, uuid, timestamp, name, description) VALUES
             (5, 'u-p5', '2026-01-01T00:00:00Z', 'colliding-pattern', 'original description');
             INSERT INTO item_anchors (id, item_type, item_id, path, timestamp) VALUES
             (41, 'decision', 1, 'src/pre_existing_anchor.rs', '2026-01-01T00:00:00Z');
             INSERT INTO context_links (id, source_item_type, source_item_id, target_item_type, target_item_id, relationship_type, description, timestamp, origin) VALUES
             (42, 'decision', '1', 'system_pattern', '5', 'relates_to', 'pre-existing link 42', '2026-01-01T00:00:00Z', 'manual');",
        )
        .unwrap();
    }

    let out = json(engrams(&db).args(["import", "--path"]).arg(&exp_dir));
    assert_eq!(out["errors"], serde_json::json!([]));
    assert_eq!(out["imported"]["decisions"], 3);
    assert_eq!(out["imported"]["progress"], 3);
    assert_eq!(out["imported"]["patterns"], 2);
    assert_eq!(out["imported"]["links"], 4);
    assert_eq!(out["imported"]["anchors"], 1);
    assert_eq!(out["imported"]["schemas"], 1);

    let skipped = out["skipped_references"].as_array().unwrap();
    let expected_skipped = [
        serde_json::json!({
            "file": "progress/12.md",
            "reference": "progress_entry:99",
            "reason": "parent missing; entry imported without its parent link"
        }),
        serde_json::json!({
            "file": "links/42.md",
            "reference": "decision:99",
            "reason": "no decision 99"
        }),
        serde_json::json!({
            "file": "anchors/41.md",
            "reference": "decision:99",
            "reason": "no decision 99"
        }),
        serde_json::json!({
            "file": "anchors/50.md",
            "reference": "system_pattern:50",
            "reason": "no system_pattern 50"
        }),
        serde_json::json!({
            "file": "schemas/1.md",
            "reference": "decision:99",
            "reason": "no decision 99"
        }),
        serde_json::json!({
            "file": "schemas/1.md",
            "reference": "code:77777",
            "reason": "no code 77777"
        }),
        serde_json::json!({
            "file": "schemas/1.md",
            "reference": "pattern:88",
            "reason": "no pattern 88"
        }),
        serde_json::json!({
            "file": "schemas/suggestions.json",
            "reference": "custom-data:77",
            "reason": "no custom-data 77"
        }),
        serde_json::json!({
            "file": "schemas/retrieval_surfaces.json",
            "reference": "decision:99",
            "reason": "no decision 99"
        }),
    ];
    assert_eq!(skipped.as_slice(), &expected_skipped);

    // Verify DB state: pre-existing rows 41 and 42 untouched, no dangling rows written,
    // progress 12 imported with parent_id = NULL, progress 11 imported with parent_id = 10.
    {
        let conn = Connection::open(&db).unwrap();
        let anchor_41: (String, i64, String) = conn
            .query_row(
                "SELECT item_type, item_id, path FROM item_anchors WHERE id = 41",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            anchor_41,
            (
                "decision".to_string(),
                1,
                "src/pre_existing_anchor.rs".to_string()
            ),
            "skipped anchor 41 must not displace pre-existing anchor 41"
        );

        let link_42: (String, String, Option<String>) = conn
            .query_row(
                "SELECT target_item_type, target_item_id, description FROM context_links WHERE id = 42",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            link_42,
            (
                "system_pattern".to_string(),
                "5".to_string(),
                Some("pre-existing link 42".to_string())
            ),
            "skipped link 42 must not displace pre-existing link 42"
        );

        let p11_parent: Option<i64> = conn
            .query_row(
                "SELECT parent_id FROM progress_entries WHERE id = 11",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(p11_parent, Some(10));

        let p12_parent: Option<i64> = conn
            .query_row(
                "SELECT parent_id FROM progress_entries WHERE id = 12",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(p12_parent, None);

        let dangling_members: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM context_links WHERE relationship_type = 'member_of' AND source_item_id IN ('99', '77777')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(dangling_members, 0);

        let suggestions_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_suggestions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            suggestions_count, 2,
            "valid pattern and progress-entry suggestions imported; missing pattern:88 skipped"
        );
    }

    // AC-11: schema scan completes and reports its candidates on the imported database.
    for _ in 0..3 {
        let scan_out = json(engrams(&db).args(["schema", "scan"]));
        assert_eq!(scan_out["status"], "success");
        let cands = scan_out["candidates"].as_array().unwrap();
        assert!(
            !cands.is_empty(),
            "schema scan must report candidates after import with skipped references"
        );
    }
}

// --- S17: export/import round-trips proposals and schema claims (spec 0007 AC-10) ---

#[test]
fn s17_export_import_round_trips_schema_proposals_statement_scope_and_origin_ac10() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("source.db");
    engrams(&db).arg("init").assert().success();

    {
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch(
            "INSERT INTO decisions (id, uuid, timestamp, summary, tags) VALUES
             (1, 'u1', '2026-10-01T00:00:00Z', 'Mara hesitates before her first command', '[\"mara\",\"wound\"]'),
             (2, 'u2', '2026-10-02T00:00:00Z', 'Mara calls herself just the one left standing', '[\"mara\",\"wound\"]'),
             (3, 'u3', '2026-10-03T00:00:00Z', 'Mara doubts her tactical judgment under siege', '[\"mara\",\"wound\"]'),
             (4, 'u4', '2026-10-04T00:00:00Z', 'Mara rallies the rear guard without hesitation', '[\"mara\",\"courage\"]'),
             (5, 'u5', '2026-10-05T00:00:00Z', 'Soren waits in silence beside the parapet', '[\"soren\",\"trust\"]'),
             (6, 'u6', '2026-10-06T00:00:00Z', 'Soren shares rations without speaking', '[\"soren\",\"trust\"]'),
             (7, 'u7', '2026-10-07T00:00:00Z', 'Soren guards Mara while she sleeps', '[\"soren\",\"trust\"]'),
             (8, 'u8', '2026-10-08T00:00:00Z', 'Aether draws chill iron and frost', '[\"aether\",\"magic\"]'),
             (9, 'u9', '2026-10-09T00:00:00Z', 'Salt-iron rods ground excess aether', '[\"aether\",\"magic\"]'),
             (10, 'u10', '2026-10-10T00:00:00Z', 'Unshielded aether casting leaves frostbite', '[\"aether\",\"magic\"]');
             INSERT INTO item_anchors (item_type, item_id, path, timestamp) VALUES
             ('decision', 1, 'manuscript/ch01.md', '2026-10-01T00:00:00Z'),
             ('decision', 2, 'manuscript/ch02.md', '2026-10-02T00:00:00Z'),
             ('decision', 3, 'manuscript/ch03.md', '2026-10-03T00:00:00Z'),
             ('decision', 4, 'manuscript/ch04.md', '2026-10-04T00:00:00Z'),
             ('decision', 5, 'manuscript/ch05.md', '2026-10-05T00:00:00Z'),
             ('decision', 6, 'manuscript/ch06.md', '2026-10-06T00:00:00Z'),
             ('decision', 7, 'manuscript/ch07.md', '2026-10-07T00:00:00Z'),
             ('decision', 8, 'world/magic-01.md', '2026-10-08T00:00:00Z'),
             ('decision', 9, 'world/magic-02.md', '2026-10-09T00:00:00Z'),
             ('decision', 10, 'world/magic-03.md', '2026-10-10T00:00:00Z');
             -- Pre-v16 schema refined with a claim and scope
             INSERT INTO schemas (id, uuid, name, summary, summary_source, status, kind, centroid_json, created_at, updated_at) VALUES
             (10, 'u-legacy', 'prose-style', 'When editing prose chapters: cut ornamental adverbs', 'agent', 'active', 'schema', '{}', '2026-09-01T00:00:00Z', '2026-09-01T00:00:00Z');",
        )
        .unwrap();
    }

    engrams(&db)
        .args([
            "schema",
            "refine",
            "prose-style",
            "--statement",
            "Prose chapters stay spare and concrete under siege tension",
            "--scope-tags",
            "style,prose",
            "--scope-paths",
            "manuscript",
        ])
        .assert()
        .success();

    // Proposal 1: confirmed into schema
    engrams(&db)
        .args([
            "schema",
            "propose",
            "--name",
            "mara-broken-leader",
            "--statement",
            "Mara believes she is broken; that hurt carries with her as she grows into a leader",
            "--summary",
            "When writing Mara under pressure: she doubts herself aloud and credits others",
            "--members",
            "decision:1,decision:2,decision:3",
            "--contrary",
            "decision:4",
            "--scope-tags",
            "mara,wound",
            "--scope-paths",
            "manuscript",
            "--session",
            "sess-ac10",
        ])
        .assert()
        .success();
    engrams(&db)
        .args(["schema", "confirm", "proposal:1"])
        .assert()
        .success();

    // Proposal 2: rejected with decision_note
    engrams(&db)
        .args([
            "schema",
            "propose",
            "--name",
            "soren-silence",
            "--statement",
            "Soren's loyalty shows through silent action rather than speeches",
            "--summary",
            "When writing Soren in group scenes: keep his dialogue minimal and physical",
            "--members",
            "decision:5,decision:6,decision:7",
            "--scope-tags",
            "soren",
            "--scope-paths",
            "manuscript",
            "--session",
            "sess-ac10",
        ])
        .assert()
        .success();
    engrams(&db)
        .args([
            "schema",
            "reject",
            "proposal:2",
            "--reason",
            "Hold until act two betrayal notes land",
        ])
        .assert()
        .success();

    // Proposal 3: left pending
    engrams(&db)
        .args([
            "schema",
            "propose",
            "--name",
            "aether-cost",
            "--statement",
            "Aether magic exacts a physical cold cost unless grounded through salt-iron",
            "--summary",
            "When writing aether casting: show frost buildup and salt-iron grounding",
            "--members",
            "decision:8,decision:9,decision:10",
            "--scope-tags",
            "aether,magic",
            "--scope-paths",
            "world",
            "--session",
            "sess-ac10",
        ])
        .assert()
        .success();

    let src_proposals = json(engrams(&db).args(["schema", "proposals", "--status", "all"]));
    let src_mara = json(engrams(&db).args(["schema", "show", "mara-broken-leader"]));
    let src_style = json(engrams(&db).args(["schema", "show", "prose-style"]));

    let exp_dir = temp.path().join("export");
    let exp_out = json(engrams(&db).args(["export", "--path"]).arg(&exp_dir));
    assert_eq!(exp_out["counts"]["schemas"], 2);
    assert_eq!(exp_out["counts"]["schema_proposals"], 3);
    for pid in 1..=3 {
        assert!(
            exp_dir.join(format!("schema_proposals/{pid}.md")).exists(),
            "schema_proposals/{pid}.md must be exported"
        );
    }

    let fresh = temp.path().join("fresh.db");
    engrams(&fresh).arg("init").assert().success();
    let imp_out = json(engrams(&fresh).args(["import", "--path"]).arg(&exp_dir));
    assert_eq!(imp_out["errors"], serde_json::json!([]));
    assert_eq!(imp_out["imported"]["schemas"], 2);
    assert_eq!(imp_out["imported"]["schema_proposals"], 3);

    let dst_proposals = json(engrams(&fresh).args(["schema", "proposals", "--status", "all"]));
    assert_eq!(
        src_proposals, dst_proposals,
        "every proposal must round-trip with status, statement, summary, scope, members, evidence, and decision_note"
    );

    let dst_mara = json(engrams(&fresh).args(["schema", "show", "mara-broken-leader"]));
    assert_eq!(
        dst_mara["schema"]["statement"],
        src_mara["schema"]["statement"]
    );
    assert_eq!(dst_mara["schema"]["scope"], src_mara["schema"]["scope"]);
    assert_eq!(dst_mara["schema"]["origin"], src_mara["schema"]["origin"]);
    assert_eq!(dst_mara["schema"]["origin"]["proposal_id"], 1);

    let dst_style = json(engrams(&fresh).args(["schema", "show", "prose-style"]));
    assert_eq!(
        dst_style["schema"]["statement"],
        src_style["schema"]["statement"]
    );
    assert_eq!(dst_style["schema"]["scope"], src_style["schema"]["scope"]);
    assert_eq!(dst_style["schema"]["origin"], serde_json::Value::Null);

    // Brief also surfaces the round-tripped statement, scope, and origin.
    let brief_mara = json(engrams(&fresh).args(["brief", "schema:mara-broken-leader"]));
    assert_eq!(
        brief_mara["brief"]["statement"],
        src_mara["schema"]["statement"]
    );
    assert_eq!(brief_mara["brief"]["scope"], src_mara["schema"]["scope"]);
    assert_eq!(brief_mara["brief"]["origin"], src_mara["schema"]["origin"]);

    // Rejected proposal territory is remembered after import.
    let re_propose = json(engrams(&fresh).args([
        "schema",
        "propose",
        "--statement",
        "Soren's loyalty shows through silent action",
        "--summary",
        "When writing Soren: keep dialogue minimal",
        "--members",
        "decision:5,decision:6,decision:7",
    ]));
    assert_eq!(re_propose["inserted"], false);
    assert_eq!(re_propose["rejected_before"][0]["id"], 2);
    assert_eq!(
        re_propose["rejected_before"][0]["reason"],
        "Hold until act two betrayal notes land"
    );
}
