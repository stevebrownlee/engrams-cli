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
             ('u3','2026-01-01T00:00:00Z','gamma policy engine','[\"core\",\"graph\"]','abc');
             INSERT INTO item_anchors (item_type, item_id, path, timestamp) VALUES
             ('decision','1','src/a.rs','2026-01-01T00:00:00Z'),
             ('decision','2','src/a.rs','2026-01-01T00:00:00Z'),
             ('decision','3','src/a.rs','2026-01-01T00:00:00Z');
             INSERT INTO context_links (source_item_type, source_item_id, target_item_type,
              target_item_id, relationship_type, timestamp, origin) VALUES
             ('decision','1','decision','2','relates_to','2026-01-01T00:00:00Z','manual'),
             ('decision','2','decision','3','relates_to','2026-01-01T00:00:00Z','manual'),
             ('decision','1','decision','3','relates_to','2026-01-01T00:00:00Z','manual');",
        )
        .unwrap();
    }
    engrams(&db).args(["schema", "scan"]).assert().success();
    engrams(&db).args(["schema", "scan"]).assert().success();
    engrams(&db)
        .args(["schema", "scan", "--apply"])
        .assert()
        .success();
    engrams(&db)
        .args([
            "schema",
            "refine",
            "--summary",
            "Gateway architecture: how requests route",
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
        "Gateway architecture: how requests route"
    );
    assert_eq!(schema["member_count"], 3);
    let members = schema["members"].as_array().unwrap();
    let keys: Vec<&str> = members.iter().map(|m| m.as_str().unwrap()).collect();
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
