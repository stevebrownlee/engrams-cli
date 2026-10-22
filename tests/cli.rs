use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use tempfile::TempDir;

fn engrams(db_path: &std::path::Path) -> Command {
    let mut cmd = Command::cargo_bin("engrams").unwrap();
    cmd.arg("--db").arg(db_path);
    cmd.env("ENGRAMS_NO_UPDATE_CHECK", "1");
    cmd
}

#[test]
fn test_init() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    // First init
    engrams(&db).arg("init").assert().success().stdout(
        predicate::str::contains(r#""created":true"#)
            .or(predicate::str::contains(r#""created": true"#)),
    );

    // Second init
    engrams(&db).arg("init").assert().success().stdout(
        predicate::str::contains(r#""created":false"#)
            .or(predicate::str::contains(r#""created": false"#)),
    );
}

#[test]
fn test_decision_lifecycle() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    // Log
    let output = engrams(&db)
        .args([
            "decision",
            "log",
            "--summary",
            "Use Rust",
            "--rationale",
            "Speed",
            "--tags",
            "lang,arch",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&output).unwrap();
    let id = json["id"].as_i64().unwrap();
    assert_eq!(json["summary"].as_str().unwrap(), "Use Rust");
    assert_eq!(json["tags"][0].as_str().unwrap(), "lang");

    // Get
    engrams(&db)
        .args(["decision", "get", &id.to_string()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Use Rust"));

    // List
    engrams(&db)
        .args(["decision", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Use Rust"));

    // Search
    engrams(&db)
        .args(["decision", "search", "Speed"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Use Rust"));

    // Update
    engrams(&db)
        .args([
            "decision",
            "update",
            &id.to_string(),
            "--summary",
            "Use Rust CLI",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Use Rust CLI"));

    // Delete
    engrams(&db)
        .args(["decision", "delete", &id.to_string()])
        .assert()
        .success()
        .stdout(
            predicate::str::contains(r#""deleted":true"#)
                .or(predicate::str::contains(r#""deleted": true"#)),
        );

    // Get (not found)
    engrams(&db)
        .args(["decision", "get", &id.to_string()])
        .assert()
        .failure();
}

#[test]
fn test_progress_linkage() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    let out1 = engrams(&db)
        .args([
            "progress",
            "log",
            "--status",
            "InProgress",
            "--description",
            "Task 1",
        ])
        .output()
        .unwrap();
    let p1: Value = serde_json::from_slice(&out1.stdout).unwrap();
    let id1 = p1["id"].as_i64().unwrap();

    let out2 = engrams(&db)
        .args([
            "progress",
            "log",
            "--status",
            "Done",
            "--description",
            "Task 2",
            "--parent-id",
            &id1.to_string(),
        ])
        .output()
        .unwrap();
    let p2: Value = serde_json::from_slice(&out2.stdout).unwrap();
    let _id2 = p2["id"].as_i64().unwrap();

    assert_eq!(p2["parent_id"].as_i64().unwrap(), id1);

    // List by parent
    engrams(&db)
        .args(["progress", "list", "--parent-id", &id1.to_string()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Task 2"));
}

#[test]
fn test_pattern_upsert() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    let out1 = engrams(&db)
        .args(["pattern", "log", "--name", "MVC", "--description", "desc 1"])
        .output()
        .unwrap();
    let p1: Value = serde_json::from_slice(&out1.stdout).unwrap();
    let id1 = p1["id"].as_i64().unwrap();

    let out2 = engrams(&db)
        .args(["pattern", "log", "--name", "MVC", "--description", "desc 2"])
        .output()
        .unwrap();
    let p2: Value = serde_json::from_slice(&out2.stdout).unwrap();
    let id2 = p2["id"].as_i64().unwrap();

    assert_eq!(id1, id2);
    assert_eq!(p2["description"].as_str().unwrap(), "desc 2");
}

#[test]
fn test_custom_data() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    engrams(&db)
        .args([
            "custom",
            "set",
            "--category",
            "conf",
            "--key",
            "port",
            "--value",
            "8080",
        ])
        .assert()
        .success()
        .stdout(
            predicate::str::contains(r#""value":"8080""#)
                .or(predicate::str::contains(r#""value": "8080""#)),
        );

    engrams(&db)
        .args([
            "custom",
            "set",
            "--category",
            "conf",
            "--key",
            "port",
            "--value",
            "8080",
            "--json",
        ])
        .assert()
        .success()
        .stdout(
            predicate::str::contains(r#""value":8080"#)
                .or(predicate::str::contains(r#""value": 8080"#)),
        );

    engrams(&db)
        .args(["custom", "get", "--category", "conf", "--key", "port"])
        .assert()
        .success()
        .stdout(predicate::str::contains("8080"));

    engrams(&db)
        .args(["custom", "search", "8080"])
        .assert()
        .success()
        .stdout(predicate::str::contains("8080"));

    engrams(&db)
        .args(["custom", "delete", "--category", "conf", "--key", "port"])
        .assert()
        .success()
        .stdout(predicate::str::contains("deleted"));
}

#[test]
fn test_product_context() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    engrams(&db)
        .args([
            "product-context",
            "update",
            "--content",
            "{\"stack\":\"rust\"}",
        ])
        .assert()
        .success();

    engrams(&db)
        .args([
            "product-context",
            "update",
            "--patch",
            "{\"stack\":\"__DELETE__\",\"db\":\"sqlite\"}",
        ])
        .assert()
        .success();

    let out = engrams(&db)
        .args(["product-context", "get"])
        .output()
        .unwrap();
    let doc: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(doc["version"].as_i64().unwrap(), 2);
    assert_eq!(doc["content"]["db"].as_str().unwrap(), "sqlite");
    assert!(doc["content"]["stack"].is_null());

    let hist = engrams(&db)
        .args(["history", "product-context"])
        .output()
        .unwrap();
    let h: Value = serde_json::from_slice(&hist.stdout).unwrap();
    assert_eq!(h[0]["version"].as_i64().unwrap(), 1);
    assert_eq!(h[0]["content"]["stack"].as_str().unwrap(), "rust");
}

#[test]
fn test_links() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    let out1 = engrams(&db)
        .args(["decision", "log", "--summary", "d1"])
        .output()
        .unwrap();
    let id1 = serde_json::from_slice::<Value>(&out1.stdout).unwrap()["id"]
        .as_i64()
        .unwrap();

    let out2 = engrams(&db)
        .args(["decision", "log", "--summary", "d2"])
        .output()
        .unwrap();
    let id2 = serde_json::from_slice::<Value>(&out2.stdout).unwrap()["id"]
        .as_i64()
        .unwrap();

    engrams(&db)
        .args([
            "link",
            "add",
            "--source-type",
            "decision",
            "--source-id",
            &id1.to_string(),
            "--target-type",
            "decision",
            "--target-id",
            &id2.to_string(),
            "--rel",
            "blocks",
        ])
        .assert()
        .success();

    engrams(&db)
        .args([
            "link",
            "list",
            "--item-type",
            "decision",
            "--item-id",
            &id1.to_string(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("outgoing"));

    // delete id1, link should be removed
    let del = engrams(&db)
        .args(["decision", "delete", &id1.to_string()])
        .output()
        .unwrap();
    let d: Value = serde_json::from_slice(&del.stdout).unwrap();
    assert_eq!(d["links_removed"].as_i64().unwrap(), 1);
}

#[test]
fn test_link_rel_constraints() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    let mk_decision = |summary: &str| -> i64 {
        let out = engrams(&db)
            .args(["decision", "log", "--summary", summary, "--force"])
            .output()
            .unwrap();
        let v = serde_json::from_slice::<Value>(&out.stdout).unwrap();
        assert!(out.status.success(), "decision log failed: {v}");
        v["id"].as_i64().unwrap()
    };
    let d1 = mk_decision("rel d1");
    let d2 = mk_decision("rel d2");
    let d3 = mk_decision("rel d3");

    let out = engrams(&db)
        .args([
            "progress",
            "log",
            "--status",
            "Todo",
            "--description",
            "rel p1",
        ])
        .output()
        .unwrap();
    let p1 = serde_json::from_slice::<Value>(&out.stdout).unwrap()["id"]
        .as_i64()
        .unwrap();

    let link = |st: &str, sid: i64, tt: &str, tid: i64, rel: &str, extra: &[&str]| {
        let sid = sid.to_string();
        let tid = tid.to_string();
        let mut args = vec![
            "link",
            "add",
            "--source-type",
            st,
            "--source-id",
            sid.as_str(),
            "--target-type",
            tt,
            "--target-id",
            tid.as_str(),
            "--rel",
            rel,
        ];
        args.extend_from_slice(extra);
        engrams(&db).args(&args).output().unwrap()
    };

    // same_type: supersedes across item types is rejected
    let out = link("decision", d1, "progress-entry", p1, "supersedes", &[]);
    assert!(!out.status.success());
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(err.contains("same_type constraint"), "{}", err);

    // --force overrides and reports which constraints fired
    let out = link(
        "decision",
        d1,
        "progress-entry",
        p1,
        "supersedes",
        &["--force"],
    );
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["overrides"], serde_json::json!(["same_type"]));

    // range: implemented_in must target pr/commit
    let out = link("decision", d1, "decision", d2, "implemented_in", &[]);
    assert!(!out.status.success());
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(err.contains("range constraint"), "{}", err);

    // disjoint: depends_on then conflicts_with on the same pair is rejected
    let out = link("decision", d1, "decision", d2, "depends_on", &[]);
    assert!(out.status.success());
    let out = link("decision", d1, "decision", d2, "conflicts_with", &[]);
    assert!(!out.status.success());
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(err.contains("disjoint constraint"), "{}", err);

    // disjoint also fires via the reverse direction of a directed partner:
    // depends_on d3->d1 then conflicts_with d1-d3
    let out = link("decision", d3, "decision", d1, "depends_on", &[]);
    assert!(out.status.success());
    let out = link("decision", d1, "decision", d3, "conflicts_with", &[]);
    assert!(!out.status.success());
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(err.contains("disjoint constraint"), "{}", err);

    // functional_to: a second supersedes into the same target is rejected
    let out = link("decision", d1, "decision", d3, "supersedes", &[]);
    assert!(out.status.success());
    let out = link("decision", d2, "decision", d3, "supersedes", &[]);
    assert!(!out.status.success());
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(err.contains("functional_to constraint"), "{}", err);
    let out = link("decision", d2, "decision", d3, "supersedes", &["--force"]);
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["overrides"], serde_json::json!(["functional_to"]));

    // unknown rels pass through with no validation and no overrides
    let out = link("decision", d1, "progress-entry", p1, "blocks", &["--force"]);
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["unknown_rel"], Value::Bool(true));
    assert!(v.get("overrides").is_none());
}

#[test]
fn test_activity() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    engrams(&db)
        .args(["decision", "log", "--summary", "act d1"])
        .output()
        .unwrap();
    engrams(&db)
        .args([
            "progress",
            "log",
            "--status",
            "InProgress",
            "--description",
            "act p1",
        ])
        .output()
        .unwrap();

    let out = engrams(&db).args(["activity"]).output().unwrap();
    let a: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(a["decisions"].as_array().unwrap().len(), 1);
    assert_eq!(a["progress"].as_array().unwrap().len(), 1);
}

#[test]
fn test_export_import() {
    let temp = TempDir::new().unwrap();
    let db1 = temp.path().join("e1.db");
    let exp_dir = temp.path().join("exp");
    let db2 = temp.path().join("e2.db");

    engrams(&db1)
        .args(["decision", "log", "--summary", "exp_d1"])
        .output()
        .unwrap();
    engrams(&db1)
        .args(["export", "--path", exp_dir.to_str().unwrap()])
        .assert()
        .success();

    engrams(&db2)
        .args(["import", "--path", exp_dir.to_str().unwrap()])
        .assert()
        .success();

    engrams(&db2)
        .args(["decision", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("exp_d1"));
}

#[test]
fn test_migrate_command() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    // Run migrate on non-existent db (creates it)
    engrams(&db).arg("migrate").assert().success().stdout(
        predicate::str::contains(r#""status":"success""#)
            .or(predicate::str::contains(r#""status": "success""#)),
    );

    // Run migrate again (noop)
    engrams(&db).arg("migrate").assert().success().stdout(
        predicate::str::contains(r#""status":"success""#)
            .or(predicate::str::contains(r#""status": "success""#)),
    );
}

#[test]
fn test_version_validation() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    // Init db
    engrams(&db).arg("init").assert().success();

    // Manually set user_version = 99 (newer version)
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute("PRAGMA user_version = 99", []).unwrap();
    }

    // Run a command, it should fail
    let out = engrams(&db).arg("decision").arg("list").output().unwrap();
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("Database schema is newer than this CLI version"));
    assert_eq!(out.status.code(), Some(1));

    // Manually set user_version = 0 (simulating old version pre-migration framework)
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute("PRAGMA user_version = 0", []).unwrap();
    }

    // Run a command, it should succeed and auto-upgrade version to 2
    engrams(&db).arg("decision").arg("list").assert().success();

    // Verify it was upgraded to the latest schema version — derived from the
    // migrate output itself so adding a migration never breaks this pin.
    {
        let out = engrams(&db).arg("migrate").output().unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        let conn = rusqlite::Connection::open(&db).unwrap();
        let ver: i32 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(ver, parsed["version"].as_i64().unwrap() as i32);
    }
}

#[test]
fn test_migration_v1_to_v2() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    // Construct a v1 database manually
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS product_context (
              id INTEGER PRIMARY KEY CHECK (id = 1),
              content TEXT NOT NULL,
              version INTEGER NOT NULL DEFAULT 1,
              updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS active_context (
              id INTEGER PRIMARY KEY CHECK (id = 1),
              content TEXT NOT NULL,
              version INTEGER NOT NULL DEFAULT 1,
              updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS product_context_history (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              version INTEGER NOT NULL, content TEXT NOT NULL,
              timestamp TEXT NOT NULL, change_source TEXT
            );
            CREATE TABLE IF NOT EXISTS active_context_history (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              version INTEGER NOT NULL, content TEXT NOT NULL,
              timestamp TEXT NOT NULL, change_source TEXT
            );
            CREATE TABLE IF NOT EXISTS decisions (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              uuid TEXT UNIQUE NOT NULL, timestamp TEXT NOT NULL,
              summary TEXT NOT NULL, rationale TEXT,
              implementation_details TEXT, tags TEXT
            );
            CREATE TABLE IF NOT EXISTS progress_entries (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              timestamp TEXT NOT NULL, status TEXT NOT NULL, description TEXT NOT NULL,
              parent_id INTEGER REFERENCES progress_entries(id) ON DELETE SET NULL
            );
            CREATE TABLE IF NOT EXISTS system_patterns (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              uuid TEXT UNIQUE NOT NULL, timestamp TEXT NOT NULL,
              name TEXT UNIQUE NOT NULL, description TEXT, tags TEXT
            );
            CREATE TABLE IF NOT EXISTS custom_data (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              timestamp TEXT NOT NULL, category TEXT NOT NULL, key TEXT NOT NULL,
              value TEXT NOT NULL, UNIQUE(category, key)
            );
            CREATE TABLE IF NOT EXISTS context_links (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              source_item_type TEXT NOT NULL, source_item_id TEXT NOT NULL,
              target_item_type TEXT NOT NULL, target_item_id TEXT NOT NULL,
              relationship_type TEXT NOT NULL, description TEXT, timestamp TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS ix_links_source ON context_links(source_item_type, source_item_id);
            CREATE INDEX IF NOT EXISTS ix_links_target ON context_links(target_item_type, target_item_id);
            CREATE VIRTUAL TABLE IF NOT EXISTS decisions_fts USING fts5(
              summary, rationale, implementation_details, tags,
              content='decisions', content_rowid='id'
            );
            CREATE TRIGGER IF NOT EXISTS decisions_ai AFTER INSERT ON decisions BEGIN
              INSERT INTO decisions_fts(rowid, summary, rationale, implementation_details, tags)
              VALUES (new.id, new.summary, new.rationale, new.implementation_details, new.tags);
            END;
            CREATE TRIGGER IF NOT EXISTS decisions_ad AFTER DELETE ON decisions BEGIN
              INSERT INTO decisions_fts(decisions_fts, rowid, summary, rationale, implementation_details, tags)
              VALUES ('delete', old.id, old.summary, old.rationale, old.implementation_details, old.tags);
            END;
            CREATE TRIGGER IF NOT EXISTS decisions_au AFTER UPDATE ON decisions BEGIN
              INSERT INTO decisions_fts(decisions_fts, rowid, summary, rationale, implementation_details, tags)
              VALUES ('delete', old.id, old.summary, old.rationale, old.implementation_details, old.tags);
              INSERT INTO decisions_fts(rowid, summary, rationale, implementation_details, tags)
              VALUES (new.id, new.summary, new.rationale, new.implementation_details, new.tags);
            END;
            CREATE VIRTUAL TABLE IF NOT EXISTS custom_data_fts USING fts5(
              category, key, value, content='custom_data', content_rowid='id'
            );
            CREATE TRIGGER IF NOT EXISTS custom_data_ai AFTER INSERT ON custom_data BEGIN
              INSERT INTO custom_data_fts(rowid, category, key, value)
              VALUES (new.id, new.category, new.key, new.value);
            END;
            CREATE TRIGGER IF NOT EXISTS custom_data_ad AFTER DELETE ON custom_data BEGIN
              INSERT INTO custom_data_fts(custom_data_fts, rowid, category, key, value)
              VALUES ('delete', old.id, old.category, old.key, old.value);
            END;
            CREATE TRIGGER IF NOT EXISTS custom_data_au AFTER UPDATE ON custom_data BEGIN
              INSERT INTO custom_data_fts(custom_data_fts, rowid, category, key, value)
              VALUES ('delete', old.id, old.category, old.key, old.value);
              INSERT INTO custom_data_fts(rowid, category, key, value)
              VALUES (new.id, new.category, new.key, new.value);
            END;
            PRAGMA user_version = 1;"
        ).unwrap();
    }

    // Run a command, it should fail
    let out = engrams(&db).arg("decision").arg("list").output().unwrap();
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("Database schema is out of date"));
    assert_eq!(out.status.code(), Some(1));

    // Run migrate
    engrams(&db).arg("migrate").assert().success();

    // Now it should succeed
    engrams(&db).arg("decision").arg("list").assert().success();

    // Verify the on-disk version matches migrate's self-reported latest —
    // derived from the command output so new migrations never break this pin.
    {
        let out = engrams(&db).arg("migrate").output().unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        let conn = rusqlite::Connection::open(&db).unwrap();
        let ver: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(ver, parsed["version"].as_i64().unwrap());
    }
}

#[test]
fn test_report() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    // Init first
    engrams(&db).arg("init").assert().success();

    // Seed data
    engrams(&db)
        .args(["decision", "log", "--summary", "Use SQLite"])
        .output()
        .unwrap();
    engrams(&db)
        .args([
            "progress",
            "log",
            "--status",
            "Done",
            "--description",
            "Set up DB",
        ])
        .output()
        .unwrap();
    engrams(&db)
        .args([
            "pattern",
            "log",
            "--name",
            "Singleton docs",
            "--description",
            "Upsert pattern",
        ])
        .output()
        .unwrap();

    // Full report (JSON)
    let out = engrams(&db).args(["report"]).output().unwrap();
    let r: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(r["active_context"].is_null() || r["active_context"].is_object());
    assert_eq!(r["decisions"].as_array().unwrap().len(), 1);
    assert_eq!(r["progress"].as_array().unwrap().len(), 1);
    assert_eq!(r["patterns"].as_array().unwrap().len(), 1);
    assert!(r["links"].as_array().unwrap().is_empty());

    // Topic report (JSON)
    let out = engrams(&db).args(["report", "decisions"]).output().unwrap();
    let r: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(r.is_array());
    assert_eq!(r.as_array().unwrap().len(), 1);

    // Human formatting flag should now fail since it was removed
    engrams(&db)
        .args(["--format", "human", "report"])
        .assert()
        .failure();
}

#[test]
fn test_report_open() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    // Init first
    engrams(&db).arg("init").assert().success();

    // Seed data
    engrams(&db)
        .args(["decision", "log", "--summary", "Use SQLite"])
        .output()
        .unwrap();
    engrams(&db)
        .args([
            "progress",
            "log",
            "--status",
            "Done",
            "--description",
            "Set up DB",
        ])
        .output()
        .unwrap();
    engrams(&db)
        .args([
            "pattern",
            "log",
            "--name",
            "Singleton docs",
            "--description",
            "Upsert pattern",
        ])
        .output()
        .unwrap();
    engrams(&db)
        .args([
            "link",
            "add",
            "--source-type",
            "decision",
            "--source-id",
            "1",
            "--target-type",
            "system-pattern",
            "--target-id",
            "1",
            "--rel",
            "implements",
        ])
        .output()
        .unwrap();

    let html_file = temp.path().join("dash.html");
    let out = engrams(&db)
        .args([
            "report",
            "open",
            "--no-browser",
            "--out",
            html_file.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let r: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(r["path"].as_str().unwrap(), html_file.to_str().unwrap());
    assert!(!r["opened"].as_bool().unwrap());
    assert_eq!(r["counts"]["decisions"].as_i64().unwrap(), 1);
    assert_eq!(r["counts"]["links"].as_i64().unwrap(), 1);

    assert!(html_file.exists());
    let html_content = std::fs::read_to_string(&html_file).unwrap();
    assert!(html_content.contains("ENGRAMS_DATA"));
    assert!(html_content.contains("cytoscape"));
    assert!(html_content.contains("Use SQLite"));
    assert!(html_content.contains("implements"));
}

#[test]
fn test_decision_similarity_check() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    // Log a decision with --force (bypasses check, ensures insert)
    let out1 = engrams(&db)
        .args([
            "decision",
            "log",
            "--summary",
            "Use Rust for the CLI tool",
            "--rationale",
            "Performance and safety",
            "--tags",
            "lang,arch",
            "--force",
        ])
        .output()
        .unwrap();
    let j1: Value = serde_json::from_slice(&out1.stdout).unwrap();
    assert!(j1["inserted"].as_bool().unwrap());
    let id1 = j1["id"].as_i64().unwrap();

    // Log a similar decision WITHOUT --force — should be blocked
    let out2 = engrams(&db)
        .args([
            "decision",
            "log",
            "--summary",
            "Use Rust for CLI",
            "--rationale",
            "Speed",
        ])
        .output()
        .unwrap();
    let j2: Value = serde_json::from_slice(&out2.stdout).unwrap();
    assert!(!j2["inserted"].as_bool().unwrap());
    assert!(j2["similar"].is_array());
    let similar = j2["similar"].as_array().unwrap();
    assert!(!similar.is_empty());
    assert_eq!(similar[0]["id"].as_i64().unwrap(), id1);

    // Log the same decision WITH --force — should insert
    let out3 = engrams(&db)
        .args([
            "decision",
            "log",
            "--summary",
            "Use Rust for CLI",
            "--rationale",
            "Speed",
            "--force",
        ])
        .output()
        .unwrap();
    let j3: Value = serde_json::from_slice(&out3.stdout).unwrap();
    assert!(j3["inserted"].as_bool().unwrap());
    assert!(j3["id"].as_i64().unwrap() > id1);

    // A completely different decision should insert without --force
    let out4 = engrams(&db)
        .args([
            "decision",
            "log",
            "--summary",
            "Deploy to Kubernetes",
            "--rationale",
            "Scalability",
        ])
        .output()
        .unwrap();
    let j4: Value = serde_json::from_slice(&out4.stdout).unwrap();
    assert!(j4["inserted"].as_bool().unwrap());
}

#[test]
fn test_decision_consolidate() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    // Create two decisions
    let out1 = engrams(&db)
        .args([
            "decision",
            "log",
            "--summary",
            "Use SQLite for storage",
            "--rationale",
            "Embedded, zero config",
            "--tags",
            "db,arch",
            "--force",
        ])
        .output()
        .unwrap();
    let j1: Value = serde_json::from_slice(&out1.stdout).unwrap();
    let id1 = j1["id"].as_i64().unwrap();

    let out2 = engrams(&db)
        .args([
            "decision",
            "log",
            "--summary",
            "SQLite with FTS5",
            "--rationale",
            "Full-text search capability",
            "--details",
            "Use bundled feature",
            "--tags",
            "db,search",
            "--force",
        ])
        .output()
        .unwrap();
    let j2: Value = serde_json::from_slice(&out2.stdout).unwrap();
    let id2 = j2["id"].as_i64().unwrap();

    // Add a link from id2 to id1 to verify repointing
    engrams(&db)
        .args([
            "link",
            "add",
            "--source-type",
            "decision",
            "--source-id",
            &id2.to_string(),
            "--target-type",
            "decision",
            "--target-id",
            &id1.to_string(),
            "--rel",
            "refines",
        ])
        .assert()
        .success();

    // Consolidate id2 into id1
    let out3 = engrams(&db)
        .args([
            "decision",
            "consolidate",
            &id2.to_string(),
            &id1.to_string(),
        ])
        .output()
        .unwrap();
    let j3: Value = serde_json::from_slice(&out3.stdout).unwrap();
    assert_eq!(j3["id"].as_i64().unwrap(), id1);
    assert_eq!(j3["consolidated_from"].as_i64().unwrap(), id2);

    // Rationale should be merged
    let rationale = j3["rationale"].as_str().unwrap();
    assert!(rationale.contains("Embedded, zero config"));
    assert!(rationale.contains("Full-text search capability"));

    // Details from source should be present
    let details = j3["implementation_details"].as_str().unwrap();
    assert!(details.contains("Use bundled feature"));

    // Tags should be unioned
    let tags = j3["tags"].as_array().unwrap();
    let tag_strs: Vec<&str> = tags.iter().map(|t| t.as_str().unwrap()).collect();
    assert!(tag_strs.contains(&"db"));
    assert!(tag_strs.contains(&"arch"));
    assert!(tag_strs.contains(&"search"));

    // Source decision should be gone
    engrams(&db)
        .args(["decision", "get", &id2.to_string()])
        .assert()
        .failure();

    // Consolidating into self should fail
    engrams(&db)
        .args([
            "decision",
            "consolidate",
            &id1.to_string(),
            &id1.to_string(),
        ])
        .assert()
        .failure();
}

#[test]
fn test_progress_check_similar() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    // Log a progress entry
    let out1 = engrams(&db)
        .args([
            "progress",
            "log",
            "--status",
            "Done",
            "--description",
            "Implemented auth module",
        ])
        .output()
        .unwrap();
    let j1: Value = serde_json::from_slice(&out1.stdout).unwrap();
    let id1 = j1["id"].as_i64().unwrap();

    // Log an identical entry with --check-similar — should be blocked
    let out2 = engrams(&db)
        .args([
            "progress",
            "log",
            "--status",
            "Done",
            "--description",
            "Implemented auth module",
            "--check-similar",
        ])
        .output()
        .unwrap();
    let j2: Value = serde_json::from_slice(&out2.stdout).unwrap();
    assert!(!j2["inserted"].as_bool().unwrap());
    assert_eq!(j2["existing"]["id"].as_i64().unwrap(), id1);

    // Case-insensitive match
    let out3 = engrams(&db)
        .args([
            "progress",
            "log",
            "--status",
            "Done",
            "--description",
            "implemented auth module",
            "--check-similar",
        ])
        .output()
        .unwrap();
    let j3: Value = serde_json::from_slice(&out3.stdout).unwrap();
    assert!(!j3["inserted"].as_bool().unwrap());

    // Different description should insert
    let out4 = engrams(&db)
        .args([
            "progress",
            "log",
            "--status",
            "Done",
            "--description",
            "Implemented billing module",
            "--check-similar",
        ])
        .output()
        .unwrap();
    let j4: Value = serde_json::from_slice(&out4.stdout).unwrap();
    assert!(j4["inserted"].as_bool().unwrap());

    // Without --check-similar, duplicates insert freely
    let out5 = engrams(&db)
        .args([
            "progress",
            "log",
            "--status",
            "Done",
            "--description",
            "Implemented auth module",
        ])
        .output()
        .unwrap();
    let j5: Value = serde_json::from_slice(&out5.stdout).unwrap();
    // No "inserted" key when check_similar is false (original behavior)
    assert!(j5["id"].as_i64().unwrap() > id1);
}

#[test]
fn test_pr_provenance() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    // Log decision with PR URL
    let out = engrams(&db)
        .args([
            "decision",
            "log",
            "--summary",
            "Use standard layout",
            "--pr",
            "https://github.com/acme/w/pull/7",
        ])
        .output()
        .unwrap();
    let j: Value = serde_json::from_slice(&out.stdout).unwrap();
    let id = j["id"].as_i64().unwrap();
    let pr_urls = j["pr_urls"].as_array().unwrap();
    assert_eq!(pr_urls.len(), 1);
    assert_eq!(
        pr_urls[0].as_str().unwrap(),
        "https://github.com/acme/w/pull/7"
    );

    // Add another PR using engrams pr add
    let out2 = engrams(&db)
        .args([
            "pr",
            "add",
            "--type",
            "decision",
            "--id",
            &id.to_string(),
            "--pr",
            "https://github.com/acme/w/pull/8",
        ])
        .output()
        .unwrap();
    let pr_urls2: Value = serde_json::from_slice(&out2.stdout).unwrap();
    assert_eq!(pr_urls2.as_array().unwrap().len(), 2);

    // List PRs
    let out3 = engrams(&db)
        .args(["pr", "list", "--type", "decision", "--id", &id.to_string()])
        .output()
        .unwrap();
    let pr_list: Value = serde_json::from_slice(&out3.stdout).unwrap();
    assert_eq!(pr_list.as_array().unwrap().len(), 2);
    assert_eq!(
        pr_list[0]["url"].as_str().unwrap(),
        "https://github.com/acme/w/pull/7"
    );
    assert_eq!(
        pr_list[1]["url"].as_str().unwrap(),
        "https://github.com/acme/w/pull/8"
    );

    // Remove PR
    engrams(&db)
        .args([
            "pr",
            "remove",
            "--type",
            "decision",
            "--id",
            &id.to_string(),
            "--url",
            "https://github.com/acme/w/pull/7",
        ])
        .assert()
        .success();

    // Verify removed
    let out4 = engrams(&db)
        .args(["decision", "get", &id.to_string()])
        .output()
        .unwrap();
    let j4: Value = serde_json::from_slice(&out4.stdout).unwrap();
    let pr_urls4 = j4["pr_urls"].as_array().unwrap();
    assert_eq!(pr_urls4.len(), 1);
    assert_eq!(
        pr_urls4[0].as_str().unwrap(),
        "https://github.com/acme/w/pull/8"
    );
}

#[test]
fn test_pr_provenance_git_derivation() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    let repo_temp = TempDir::new().unwrap();
    let repo_path = repo_temp.path();

    // Initialize git repo and set remote origin
    std::process::Command::new("git")
        .arg("init")
        .current_dir(repo_path)
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["remote", "add", "origin", "git@github.com:acme/w.git"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    // Log a decision with a numeric PR ID, passing explicit workspace
    let mut cmd = engrams(&db);
    cmd.arg("--workspace").arg(repo_path).args([
        "decision",
        "log",
        "--summary",
        "Derive PR URL",
        "--pr",
        "42",
    ]);
    let out = cmd.output().unwrap();
    let j: Value = serde_json::from_slice(&out.stdout).unwrap();
    let pr_urls = j["pr_urls"].as_array().unwrap();
    assert_eq!(pr_urls.len(), 1);
    assert_eq!(
        pr_urls[0].as_str().unwrap(),
        "https://github.com/acme/w/pull/42"
    );
}

#[test]
fn test_anchors_and_relevant() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    // Log decision with anchor
    let out = engrams(&db)
        .args([
            "decision",
            "log",
            "--summary",
            "Layout Decision",
            "--anchor",
            "./src/db.rs",
        ])
        .output()
        .unwrap();
    let j: Value = serde_json::from_slice(&out.stdout).unwrap();
    let id = j["id"].as_i64().unwrap();
    let anchors = j["anchors"].as_array().unwrap();
    assert_eq!(anchors.len(), 1);
    assert_eq!(anchors[0].as_str().unwrap(), "src/db.rs");

    // Query relevant for src/db.rs
    let out2 = engrams(&db)
        .args(["relevant", "src/db.rs"])
        .output()
        .unwrap();
    let j2: Value = serde_json::from_slice(&out2.stdout).unwrap();
    let matched_decisions = j2["decisions"].as_array().unwrap();
    assert_eq!(matched_decisions.len(), 1);
    assert_eq!(matched_decisions[0]["id"].as_i64().unwrap(), id);

    // Query relevant for src (parent dir of anchor)
    let out3 = engrams(&db).args(["relevant", "src"]).output().unwrap();
    let j3: Value = serde_json::from_slice(&out3.stdout).unwrap();
    assert_eq!(j3["decisions"].as_array().unwrap().len(), 1);

    // Query relevant for other.rs (no match)
    let out4 = engrams(&db)
        .args(["relevant", "other.rs"])
        .output()
        .unwrap();
    let j4: Value = serde_json::from_slice(&out4.stdout).unwrap();
    assert_eq!(j4["decisions"].as_array().unwrap().len(), 0);
}

#[test]
fn test_decision_supersede() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    // Log decision 1
    let out1 = engrams(&db)
        .args(["decision", "log", "--summary", "First Decision", "--force"])
        .output()
        .unwrap();
    let j1: Value = serde_json::from_slice(&out1.stdout).unwrap();
    let id1 = j1["id"].as_i64().unwrap();

    // Log decision 2
    let out2 = engrams(&db)
        .args(["decision", "log", "--summary", "Second Decision", "--force"])
        .output()
        .unwrap();
    let j2: Value = serde_json::from_slice(&out2.stdout).unwrap();
    let id2 = j2["id"].as_i64().unwrap();

    // Superseding self should fail
    engrams(&db)
        .args([
            "decision",
            "supersede",
            &id1.to_string(),
            "--by",
            &id1.to_string(),
        ])
        .assert()
        .failure();

    // Supersede 1 by 2
    let out_sup = engrams(&db)
        .args([
            "decision",
            "supersede",
            &id1.to_string(),
            "--by",
            &id2.to_string(),
        ])
        .output()
        .unwrap();
    let j_sup: Value = serde_json::from_slice(&out_sup.stdout).unwrap();
    assert_eq!(j_sup["status"].as_str().unwrap(), "superseded");
    assert_eq!(j_sup["superseded_by"].as_i64().unwrap(), id2);

    // decision list should omit #1
    let out_list1 = engrams(&db).args(["decision", "list"]).output().unwrap();
    let j_list1: Value = serde_json::from_slice(&out_list1.stdout).unwrap();
    let ids_list1: Vec<i64> = j_list1
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["id"].as_i64().unwrap())
        .collect();
    assert!(!ids_list1.contains(&id1));
    assert!(ids_list1.contains(&id2));

    // decision list --all should include #1 with status superseded
    let out_list2 = engrams(&db)
        .args(["decision", "list", "--all"])
        .output()
        .unwrap();
    let j_list2: Value = serde_json::from_slice(&out_list2.stdout).unwrap();
    let dec1_opt = j_list2
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["id"].as_i64().unwrap() == id1);
    assert!(dec1_opt.is_some());
    assert_eq!(dec1_opt.unwrap()["status"].as_str().unwrap(), "superseded");

    // decision get 1 still works
    let out_get = engrams(&db)
        .args(["decision", "get", &id1.to_string()])
        .output()
        .unwrap();
    let j_get: Value = serde_json::from_slice(&out_get.stdout).unwrap();
    assert_eq!(j_get["status"].as_str().unwrap(), "superseded");

    // link list shows the supersedes edge
    let out_link = engrams(&db)
        .args([
            "link",
            "list",
            "--item-type",
            "decision",
            "--item-id",
            &id2.to_string(),
        ])
        .output()
        .unwrap();
    let j_link: Value = serde_json::from_slice(&out_link.stdout).unwrap();
    let has_supersedes = j_link
        .as_array()
        .unwrap()
        .iter()
        .any(|l| l["relationship_type"].as_str().unwrap() == "supersedes");
    assert!(has_supersedes);

    // Reversal path: update status active
    let out_update = engrams(&db)
        .args(["decision", "update", &id1.to_string(), "--status", "active"])
        .output()
        .unwrap();
    let j_update: Value = serde_json::from_slice(&out_update.stdout).unwrap();
    assert!(j_update.get("status").is_none());

    // decision list now includes #1 again
    let out_list3 = engrams(&db).args(["decision", "list"]).output().unwrap();
    let j_list3: Value = serde_json::from_slice(&out_list3.stdout).unwrap();
    let ids_list3: Vec<i64> = j_list3
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["id"].as_i64().unwrap())
        .collect();
    assert!(ids_list3.contains(&id1));
}

#[test]
fn test_prime() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    // Seed context
    engrams(&db)
        .args(["active-context", "update", "--content", "{\"tasks\": []}"])
        .assert()
        .success();

    // Seed 12 decisions
    for i in 1..=12 {
        engrams(&db)
            .args([
                "decision",
                "log",
                "--summary",
                &format!("Decision {}", i),
                "--force",
            ])
            .assert()
            .success();
    }

    // Seed 2 patterns
    for i in 1..=2 {
        engrams(&db)
            .args(["pattern", "log", "--name", &format!("Pattern {}", i)])
            .assert()
            .success();
    }

    // Seed 2 progress entries
    for i in 1..=2 {
        engrams(&db)
            .args([
                "progress",
                "log",
                "--status",
                "Done",
                "--description",
                &format!("Progress {}", i),
            ])
            .assert()
            .success();
    }

    // Call prime without budget
    let out = engrams(&db).args(["prime"]).output().unwrap();
    let j: Value = serde_json::from_slice(&out.stdout).unwrap();

    // Decisions should be capped at 10
    let decs = j["decisions"].as_array().unwrap();
    assert_eq!(decs.len(), 10);

    let pats = j["patterns"].as_array().unwrap();
    assert_eq!(pats.len(), 2);

    let prog = j["progress"].as_array().unwrap();
    assert_eq!(prog.len(), 2);

    assert!(j.get("budget").is_none());

    // Call prime with budget 10
    let out2 = engrams(&db)
        .args(["prime", "--budget", "10"])
        .output()
        .unwrap();
    let j2: Value = serde_json::from_slice(&out2.stdout).unwrap();

    let decs2 = j2["decisions"].as_array().unwrap();
    let pats2 = j2["patterns"].as_array().unwrap();
    let prog2 = j2["progress"].as_array().unwrap();

    assert!(decs2.is_empty());
    assert!(pats2.is_empty());
    assert!(prog2.is_empty());
    assert!(j2.get("active_context").is_some());
    assert!(j2.get("budget").is_some());
    assert!(j2["budget"]["estimated_tokens"].as_i64().is_some());
    assert!(j2["product_context"].is_null());
}

#[test]
fn test_doctor() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    let repo_temp = TempDir::new().unwrap();
    let repo_path = repo_temp.path();

    // 1. Clean DB (nonstandard --db without --workspace skips stale_decisions)
    let out = engrams(&db).arg("doctor").output().unwrap();
    let j: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(j["ok"].as_bool().unwrap());
    assert_eq!(
        j["skipped_checks"],
        serde_json::json!(["stale_decisions: no workspace"])
    );
    // 2. Initialize git repo, commit a file
    std::process::Command::new("git")
        .arg("init")
        .current_dir(repo_path)
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(repo_path)
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    let file_path = repo_path.join("anchor.txt");
    std::fs::write(&file_path, "original content").unwrap();

    std::process::Command::new("git")
        .args(["add", "anchor.txt"])
        .current_dir(repo_path)
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["commit", "-m", "initial commit"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    // Log decision with anchor inside the repo
    let mut cmd = engrams(&db);
    cmd.current_dir(repo_path)
        .arg("--workspace")
        .arg(repo_path)
        .args([
            "decision",
            "log",
            "--summary",
            "Doctor Decision",
            "--anchor",
            "anchor.txt",
            "--force",
        ]);
    let out_dec = cmd.output().unwrap();
    let j_dec: Value = serde_json::from_slice(&out_dec.stdout).unwrap();
    let id = j_dec["id"].as_i64().unwrap();
    assert!(j_dec["commit_sha"].as_str().is_some());

    // 3. Test unlinked_decisions (has commit_sha but no PR links)
    let mut cmd_doc = engrams(&db);
    cmd_doc
        .current_dir(repo_path)
        .arg("--workspace")
        .arg(repo_path)
        .arg("doctor");
    let out_doc = cmd_doc.output().unwrap();
    let j_doc: Value = serde_json::from_slice(&out_doc.stdout).unwrap();
    assert!(!j_doc["ok"].as_bool().unwrap());
    let unlinked = j_doc["unlinked_decisions"].as_array().unwrap();
    assert_eq!(unlinked.len(), 1);
    assert_eq!(unlinked[0]["id"].as_i64().unwrap(), id);

    // 4. Test missing_anchor_paths (anchor to non-existent path)
    let out_anchor = engrams(&db)
        .arg("--workspace")
        .arg(repo_path)
        .args([
            "anchor",
            "add",
            "--type",
            "decision",
            "--id",
            &id.to_string(),
            "--path",
            "nonexistent.txt",
        ])
        .output()
        .unwrap();
    assert!(out_anchor.status.success());

    let mut cmd_doc2 = engrams(&db);
    cmd_doc2
        .current_dir(repo_path)
        .arg("--workspace")
        .arg(repo_path)
        .arg("doctor");
    let out_doc2 = cmd_doc2.output().unwrap();
    let j_doc2: Value = serde_json::from_slice(&out_doc2.stdout).unwrap();
    let missing = j_doc2["missing_anchor_paths"].as_array().unwrap();
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0]["path"].as_str().unwrap(), "nonexistent.txt");

    // 5. Test stale_decisions (modify+commit the anchored file)
    std::fs::write(&file_path, "modified content").unwrap();
    std::process::Command::new("git")
        .args(["add", "anchor.txt"])
        .current_dir(repo_path)
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["commit", "-m", "modified anchor"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    let mut cmd_doc3 = engrams(&db);
    cmd_doc3
        .current_dir(repo_path)
        .arg("--workspace")
        .arg(repo_path)
        .arg("doctor");
    let out_doc3 = cmd_doc3.output().unwrap();
    let j_doc3: Value = serde_json::from_slice(&out_doc3.stdout).unwrap();
    assert!(j_doc3.get("skipped_checks").is_none());
    let stale = j_doc3["stale_decisions"].as_array().unwrap();
    assert_eq!(stale.len(), 1);
    assert_eq!(stale[0]["id"].as_i64().unwrap(), id);
    assert_eq!(
        stale[0]["changed_paths"].as_array().unwrap()[0]
            .as_str()
            .unwrap(),
        "anchor.txt"
    );
}

#[test]
fn test_compact_fields_snippets() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    // Log a decision without rationale
    let out = engrams(&db)
        .args(["decision", "log", "--summary", "Compact Test Decision"])
        .output()
        .unwrap();
    let j: Value = serde_json::from_slice(&out.stdout).unwrap();
    let id = j["id"].as_i64().unwrap();

    // 1. Test --compact decision get <id>
    let out_compact = engrams(&db)
        .args(["--compact", "decision", "get", &id.to_string()])
        .output()
        .unwrap();
    let stdout_str = String::from_utf8(out_compact.stdout).unwrap();
    assert_eq!(stdout_str.trim().lines().count(), 1);
    assert!(!stdout_str.contains("null"));

    // 2. Test --fields id,summary decision list
    let out_fields = engrams(&db)
        .args(["--fields", "id,summary", "decision", "list"])
        .output()
        .unwrap();
    let j_fields: Value = serde_json::from_slice(&out_fields.stdout).unwrap();
    let arr = j_fields.as_array().unwrap();
    assert!(!arr.is_empty());
    let first = arr[0].as_object().unwrap();
    assert!(first.contains_key("id"));
    assert!(first.contains_key("summary"));
    assert!(!first.contains_key("uuid"));
    assert!(!first.contains_key("timestamp"));

    // 3. Test decision search --snippets
    let out_snippets = engrams(&db)
        .args(["decision", "search", "Compact", "--snippets"])
        .output()
        .unwrap();
    let j_snippets: Value = serde_json::from_slice(&out_snippets.stdout).unwrap();
    let arr_snippets = j_snippets.as_array().unwrap();
    assert_eq!(arr_snippets.len(), 1);
    let item = arr_snippets[0].as_object().unwrap();
    assert!(item.contains_key("id"));
    assert!(item.contains_key("summary"));
    assert!(item.contains_key("snippet"));
    let snippet_str = item["snippet"].as_str().unwrap();
    assert!(snippet_str.contains(">>") && snippet_str.contains("<<"));
}

#[test]
fn test_instructions() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("nonexistent_dir").join("e.db");

    let out = engrams(&db).arg("instructions").output().unwrap();

    let stdout_str = String::from_utf8(out.stdout).unwrap();
    assert!(stdout_str.starts_with("## Engrams — Memory & Active Code Advisor"));
    assert_eq!(out.status.code(), Some(0));

    assert!(!temp.path().join("nonexistent_dir").exists());
}

#[test]
fn test_query() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    // 1. Seed decision
    let out1 = engrams(&db)
        .args([
            "decision",
            "log",
            "--summary",
            "Unified query test",
            "--rationale",
            "This decision matches the query keyword token.",
            "--tags",
            "sql,db",
            "--force",
        ])
        .output()
        .unwrap();
    let j1: Value = serde_json::from_slice(&out1.stdout).unwrap();
    let dec_id = j1["id"].as_i64().unwrap();

    // 2. Seed pattern
    let out2 = engrams(&db)
        .args([
            "pattern",
            "log",
            "--name",
            "TokenPattern",
            "--description",
            "This pattern also matches the query keyword token.",
            "--tags",
            "sql,conv",
        ])
        .output()
        .unwrap();
    let j2: Value = serde_json::from_slice(&out2.stdout).unwrap();
    let pat_id = j2["id"].as_i64().unwrap();

    // 3. Seed custom row
    let out3 = engrams(&db)
        .args([
            "custom",
            "set",
            "--category",
            "settings",
            "--key",
            "keyword",
            "--value",
            "This custom value matches the token.",
        ])
        .output()
        .unwrap();
    let j3: Value = serde_json::from_slice(&out3.stdout).unwrap();
    let cust_id = j3["id"].as_i64().unwrap();

    // Query for "token"
    let out_q = engrams(&db).args(["query", "token"]).output().unwrap();
    let j_q: Value = serde_json::from_slice(&out_q.stdout).unwrap();
    let results = j_q.as_array().unwrap();

    assert_eq!(results.len(), 3);

    let has_dec = results
        .iter()
        .any(|r| r["type"].as_str().unwrap() == "decision" && r["id"].as_i64().unwrap() == dec_id);
    let has_pat = results.iter().any(|r| {
        r["type"].as_str().unwrap() == "system_pattern" && r["id"].as_i64().unwrap() == pat_id
    });
    let has_cust = results.iter().any(|r| {
        r["type"].as_str().unwrap() == "custom_data" && r["id"].as_i64().unwrap() == cust_id
    });
    assert!(has_dec);
    assert!(has_pat);
    assert!(has_cust);

    // Test --types decision
    let out_types = engrams(&db)
        .args(["query", "token", "--types", "decision"])
        .output()
        .unwrap();
    let j_types: Value = serde_json::from_slice(&out_types.stdout).unwrap();
    let results_types = j_types.as_array().unwrap();
    assert_eq!(results_types.len(), 1);
    assert_eq!(results_types[0]["type"].as_str().unwrap(), "decision");

    // Test --tags sql
    let out_tags = engrams(&db)
        .args(["query", "token", "--tags", "sql"])
        .output()
        .unwrap();
    let j_tags: Value = serde_json::from_slice(&out_tags.stdout).unwrap();
    let results_tags = j_tags.as_array().unwrap();
    assert_eq!(results_tags.len(), 2);
    let has_cust_tag = results_tags
        .iter()
        .any(|r| r["type"].as_str().unwrap() == "custom_data");
    assert!(!has_cust_tag);

    // Test superseded decision exclusion
    engrams(&db)
        .args(["decision", "supersede", &dec_id.to_string()])
        .assert()
        .success();

    let out_q2 = engrams(&db).args(["query", "token"]).output().unwrap();
    let j_q2: Value = serde_json::from_slice(&out_q2.stdout).unwrap();
    let results2 = j_q2.as_array().unwrap();
    assert_eq!(results2.len(), 2);
    let has_dec2 = results2
        .iter()
        .any(|r| r["type"].as_str().unwrap() == "decision");
    assert!(!has_dec2);

    // With --all, it should find the superseded decision too
    let out_all = engrams(&db)
        .args(["query", "token", "--all"])
        .output()
        .unwrap();
    let j_all: Value = serde_json::from_slice(&out_all.stdout).unwrap();
    let results_all = j_all.as_array().unwrap();
    assert_eq!(results_all.len(), 3);
}

#[test]
fn test_worktree_db_resolution() {
    let temp = TempDir::new().unwrap();
    let primary_dir = temp.path().join("primary_repo");
    let worktree_dir = temp.path().join("worktree_repo");

    std::fs::create_dir_all(&primary_dir).unwrap();
    std::fs::create_dir_all(&worktree_dir).unwrap();

    // Create .git directory in primary repo
    let primary_git_dir = primary_dir.join(".git");
    std::fs::create_dir_all(&primary_git_dir).unwrap();

    // Create the mock worktree info inside primary's .git/worktrees/mock-worktree
    let mock_worktree_meta_dir = primary_git_dir.join("worktrees").join("mock-worktree");
    std::fs::create_dir_all(&mock_worktree_meta_dir).unwrap();

    // Create .git file in worktree_repo pointing to primary's mock-worktree directory
    let worktree_git_file = worktree_dir.join(".git");
    let mock_worktree_meta_dir_abs = std::fs::canonicalize(&mock_worktree_meta_dir).unwrap();
    std::fs::write(
        &worktree_git_file,
        format!("gitdir: {}", mock_worktree_meta_dir_abs.to_str().unwrap()),
    )
    .unwrap();

    // Now, run init in the worktree directory
    let mut cmd_init = Command::cargo_bin("engrams").unwrap();
    cmd_init.current_dir(&worktree_dir);
    cmd_init.arg("init");
    cmd_init.env("ENGRAMS_NO_UPDATE_CHECK", "1");
    cmd_init.assert().success();

    // The database should have been created in the primary repo: primary_dir/engrams/context.db
    let db_path = primary_dir.join("engrams").join("context.db");
    assert!(db_path.exists(), "DB should be created in the primary repo");

    // Let's log a decision using CWD as worktree_dir (so it should write to primary's db)
    let mut cmd_log = Command::cargo_bin("engrams").unwrap();
    cmd_log.current_dir(&worktree_dir);
    cmd_log.args(["decision", "log", "--summary", "Worktree Decision"]);
    cmd_log.env("ENGRAMS_NO_UPDATE_CHECK", "1");
    cmd_log.assert().success();

    // Verify it is in the database by querying it directly using engrams command with --db in primary
    let mut cmd_list = Command::cargo_bin("engrams").unwrap();
    cmd_list.current_dir(&worktree_dir);
    cmd_list.args(["decision", "list"]);
    cmd_list.env("ENGRAMS_NO_UPDATE_CHECK", "1");
    cmd_list
        .assert()
        .success()
        .stdout(predicate::str::contains("Worktree Decision"));
}

#[test]
fn test_scoped_prime() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    // Init db
    engrams(&db).arg("init").assert().success();

    // Log decision 1
    let out1 = engrams(&db)
        .args([
            "decision",
            "log",
            "--summary",
            "Scoped Decision One",
            "--tags",
            "scoped-tag",
            "--anchor",
            "scoped/file.rs",
            "--force",
        ])
        .output()
        .unwrap();
    let dec1: Value = serde_json::from_slice(&out1.stdout).unwrap();
    let dec1_id = dec1["id"].as_i64().unwrap();

    // Log decision 2
    let out2 = engrams(&db)
        .args([
            "decision",
            "log",
            "--summary",
            "Other Decision Two",
            "--tags",
            "other-tag",
            "--anchor",
            "other/file.rs",
            "--force",
        ])
        .output()
        .unwrap();
    let dec2: Value = serde_json::from_slice(&out2.stdout).unwrap();
    let _dec2_id = dec2["id"].as_i64().unwrap();
    // Log a progress record so we can verify progress is excluded when scoped
    engrams(&db)
        .args([
            "progress",
            "log",
            "--status",
            "InProgress",
            "--description",
            "Some progress",
        ])
        .assert()
        .success();

    // Prime with no filters (unscoped) -> should contain both decisions and progress
    let out_unscoped = engrams(&db).arg("prime").output().unwrap();
    let prime_unscoped: Value = serde_json::from_slice(&out_unscoped.stdout).unwrap();
    let decisions_unscoped = prime_unscoped["decisions"].as_array().unwrap();
    let progress_unscoped = prime_unscoped["progress"].as_array().unwrap();
    assert_eq!(decisions_unscoped.len(), 2);
    assert!(!progress_unscoped.is_empty());

    // Prime with --tags scoped-tag -> should contain only Decision 1, and progress should be empty
    let out_tag = engrams(&db)
        .args(["prime", "--tags", "scoped-tag"])
        .output()
        .unwrap();
    let prime_tag: Value = serde_json::from_slice(&out_tag.stdout).unwrap();
    let decisions_tag = prime_tag["decisions"].as_array().unwrap();
    let progress_tag = prime_tag["progress"].as_array().unwrap();
    assert_eq!(decisions_tag.len(), 1);
    assert_eq!(decisions_tag[0]["id"].as_i64().unwrap(), dec1_id);
    assert!(progress_tag.is_empty());

    // Prime with --paths scoped/file.rs -> should contain only Decision 1, and progress should be empty
    let out_path = engrams(&db)
        .args(["prime", "--paths", "scoped/file.rs"])
        .output()
        .unwrap();
    let prime_path: Value = serde_json::from_slice(&out_path.stdout).unwrap();
    let decisions_path = prime_path["decisions"].as_array().unwrap();
    let progress_path = prime_path["progress"].as_array().unwrap();
    assert_eq!(decisions_path.len(), 1);
    assert_eq!(decisions_path[0]["id"].as_i64().unwrap(), dec1_id);
    assert!(progress_path.is_empty());
}

// ---------------------------------------------------------------------------
// Graph topology (schema v3)
// ---------------------------------------------------------------------------

#[test]
fn test_migration_v2_to_v3() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    // Construct a v2 database manually: v1 shape + v2 columns (status/commit_sha).
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS product_context (
              id INTEGER PRIMARY KEY CHECK (id = 1),
              content TEXT NOT NULL,
              version INTEGER NOT NULL DEFAULT 1,
              updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS active_context (
              id INTEGER PRIMARY KEY CHECK (id = 1),
              content TEXT NOT NULL,
              version INTEGER NOT NULL DEFAULT 1,
              updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS product_context_history (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              version INTEGER NOT NULL, content TEXT NOT NULL,
              timestamp TEXT NOT NULL, change_source TEXT
            );
            CREATE TABLE IF NOT EXISTS active_context_history (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              version INTEGER NOT NULL, content TEXT NOT NULL,
              timestamp TEXT NOT NULL, change_source TEXT
            );
            CREATE TABLE IF NOT EXISTS decisions (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              uuid TEXT UNIQUE NOT NULL, timestamp TEXT NOT NULL,
              summary TEXT NOT NULL, rationale TEXT,
              implementation_details TEXT, tags TEXT,
              status TEXT NOT NULL DEFAULT 'active', commit_sha TEXT
            );
            CREATE TABLE IF NOT EXISTS progress_entries (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              timestamp TEXT NOT NULL, status TEXT NOT NULL, description TEXT NOT NULL,
              parent_id INTEGER REFERENCES progress_entries(id) ON DELETE SET NULL,
              commit_sha TEXT
            );
            CREATE TABLE IF NOT EXISTS system_patterns (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              uuid TEXT UNIQUE NOT NULL, timestamp TEXT NOT NULL,
              name TEXT UNIQUE NOT NULL, description TEXT, tags TEXT
            );
            CREATE TABLE IF NOT EXISTS custom_data (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              timestamp TEXT NOT NULL, category TEXT NOT NULL, key TEXT NOT NULL,
              value TEXT NOT NULL, UNIQUE(category, key)
            );
            CREATE TABLE IF NOT EXISTS context_links (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              source_item_type TEXT NOT NULL, source_item_id TEXT NOT NULL,
              target_item_type TEXT NOT NULL, target_item_id TEXT NOT NULL,
              relationship_type TEXT NOT NULL, description TEXT, timestamp TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS ix_links_source ON context_links(source_item_type, source_item_id);
            CREATE INDEX IF NOT EXISTS ix_links_target ON context_links(target_item_type, target_item_id);
            CREATE TABLE IF NOT EXISTS item_anchors (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              item_type TEXT NOT NULL, item_id INTEGER NOT NULL,
              path TEXT NOT NULL, timestamp TEXT NOT NULL,
              UNIQUE(item_type, item_id, path)
            );
            INSERT INTO decisions (uuid, timestamp, summary) VALUES ('u1', '2026-01-01T00:00:00Z', 'v2 decision');
            INSERT INTO context_links (source_item_type, source_item_id, target_item_type, target_item_id, relationship_type, timestamp)
              VALUES ('decision', '1', 'decision', '1', 'related_to', '2026-01-01T00:00:00Z');
            PRAGMA user_version = 2;",
        )
        .unwrap();
    }
    // On-disk version must match migrate's self-reported latest (self-maintaining
    // pin — derived from command output, not a hardcoded number).
    let out = engrams(&db).arg("migrate").output().unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let conn = rusqlite::Connection::open(&db).unwrap();
    let version: i32 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, parsed["version"].as_i64().unwrap() as i32);

    // context_links gained origin/source/weight.
    let mut stmt = conn.prepare("PRAGMA table_info(context_links)").unwrap();
    let cols: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    for c in ["origin", "source", "weight"] {
        assert!(cols.iter().any(|x| x == c), "missing column {}", c);
    }

    // code_nodes + graph_meta exist.
    let code_nodes: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='code_nodes'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(code_nodes, 1);

    // Pre-existing link preserved and defaulted to manual origin.
    let (rel, origin, weight): (String, String, f64) = conn
        .query_row(
            "SELECT relationship_type, origin, weight FROM context_links WHERE id = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(rel, "related_to");
    assert_eq!(origin, "manual");
    assert_eq!(weight, 1.0);
}

#[test]
fn test_fresh_db_has_schema_formation_objects() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    engrams(&db).arg("init").assert().success();

    // AC-11: fresh databases only run SCHEMA, never migrations, so every
    // object the schema-formation feature defines must already exist there.
    let out = engrams(&db).arg("migrate").output().unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let conn = rusqlite::Connection::open(&db).unwrap();
    let version: i32 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, parsed["version"].as_i64().unwrap() as i32);
    assert!(
        version >= 12,
        "expected schema-formation schema, got v{}",
        version
    );

    for name in [
        "schemas",
        "schema_candidates",
        "retrieval_surfaces",
        "schema_suggestions",
        "schemas_fts",
        "idx_retrieval_surfaces_node",
        "idx_retrieval_surfaces_ts",
    ] {
        let n: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name = ?1",
                rusqlite::params![name],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1, "fresh db missing object {}", name);
    }
}

#[test]
fn test_migration_v11_to_v12() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    engrams(&db).arg("init").assert().success();

    // Rewind to the pre-v12 shape: MIGRATION_V12 is purely additive, so a
    // v11 database is exactly the current shape minus the schema-formation
    // objects. Seed a row the upgrade must preserve.
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch(
            "DROP TRIGGER IF EXISTS schemas_ai;
             DROP TRIGGER IF EXISTS schemas_au;
             DROP TRIGGER IF EXISTS schemas_ad;
             DROP TABLE IF EXISTS schemas_fts;
             DROP TABLE IF EXISTS retrieval_surfaces;
             DROP TABLE IF EXISTS schema_candidates;
             DROP TABLE IF EXISTS schema_suggestions;
             DROP TABLE IF EXISTS schemas;",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO decisions (uuid, timestamp, summary)
             VALUES ('u1', '2026-01-01T00:00:00Z', 'v11 decision')",
            [],
        )
        .unwrap();
        conn.execute("PRAGMA user_version = 11", []).unwrap();
    }

    // On-disk version must match migrate's self-reported latest (self-maintaining
    // pin — derived from command output, not a hardcoded number).
    let out = engrams(&db).arg("migrate").output().unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let conn = rusqlite::Connection::open(&db).unwrap();
    let version: i32 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, parsed["version"].as_i64().unwrap() as i32);

    // The schema-formation objects exist again post-migration.
    for name in [
        "schemas",
        "schema_candidates",
        "retrieval_surfaces",
        "schema_suggestions",
        "schemas_fts",
        "idx_retrieval_surfaces_node",
        "idx_retrieval_surfaces_ts",
    ] {
        let n: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name = ?1",
                rusqlite::params![name],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1, "migrated db missing object {}", name);
    }

    // Pre-existing row survived the upgrade.
    let summary: String = conn
        .query_row("SELECT summary FROM decisions WHERE uuid = 'u1'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(summary, "v11 decision");
}

#[test]
fn test_migration_v13_to_v14() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    engrams(&db).arg("init").assert().success();

    // Rewind to the pre-v14 shape: MIGRATION_V14 and MIGRATION_V15 are purely
    // additive, so a v13 database is exactly the current shape minus the
    // schemas kind columns and schema_candidates.notified_at. Seed a
    // confirmed schema the upgrade must preserve.
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch(
            "ALTER TABLE schemas DROP COLUMN kind;
             ALTER TABLE schemas DROP COLUMN kind_reasons_json;
             ALTER TABLE schemas DROP COLUMN statement;
             ALTER TABLE schemas DROP COLUMN scope_json;
             ALTER TABLE schema_candidates DROP COLUMN notified_at;
             DROP TABLE schema_proposals;",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO schemas (uuid, name, summary, centroid_json, created_at, updated_at) \
             VALUES ('u-schema', 'release routine', 'shipping checklist', '{}', \
                     '2026-01-01T00:00:00Z', '2026-01-02T00:00:00Z')",
            [],
        )
        .unwrap();
        conn.execute("PRAGMA user_version = 13", []).unwrap();
    }

    // On-disk version must match migrate's self-reported latest (self-maintaining
    // pin — derived from command output), which this contract pins at 15.
    let out = engrams(&db).arg("migrate").output().unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(parsed["version"].as_i64().unwrap(), 16);
    let conn = rusqlite::Connection::open(&db).unwrap();
    let version: i32 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, 16);

    // The kind columns exist post-migration.
    let mut stmt = conn.prepare("PRAGMA table_info(schemas)").unwrap();
    let cols: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .flatten()
        .collect();
    assert!(cols.contains(&"kind".to_string()));
    assert!(cols.contains(&"kind_reasons_json".to_string()));

    // MIGRATION_V15 ran as part of the 13->15 climb.
    let cand_cols: Vec<String> = conn
        .prepare("PRAGMA table_info(schema_candidates)")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .flatten()
        .collect();
    assert!(cand_cols.contains(&"notified_at".to_string()));

    // The pre-existing row backfilled via the column defaults (a successful
    // NOT NULL backfill proves the DEFAULT clauses landed), and every prior
    // column kept its data.
    let (kind, reasons, name, summary, status, created_at, updated_at): (
        String,
        String,
        String,
        String,
        String,
        String,
        String,
    ) = conn
        .query_row(
            "SELECT kind, kind_reasons_json, name, summary, status, created_at, updated_at \
             FROM schemas WHERE uuid = 'u-schema'",
            [],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(kind, "unclear");
    assert_eq!(reasons, "[]");
    assert_eq!(name, "release routine");
    assert_eq!(summary, "shipping checklist");
    assert_eq!(status, "active");
    assert_eq!(created_at, "2026-01-01T00:00:00Z");
    assert_eq!(updated_at, "2026-01-02T00:00:00Z");
}

#[test]
fn test_schema_scan_stages_and_writes_nothing_else() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    engrams(&db).arg("init").assert().success();

    // Seed a dense triangle through the CLI: three decisions anchored to one
    // path and fully linked, so detection has a real cluster to find.
    for summary in [
        "Schema formation detection alpha",
        "Schema formation detection beta",
        "Schema formation detection gamma",
    ] {
        engrams(&db)
            .args(["decision", "log", "--summary", summary, "--force"])
            .assert()
            .success();
    }
    // Anchor and link fixture rows go in by SQL: `anchor add` also
    // materializes a code node plus derived anchored_to edges (pre-existing
    // enrichment), which would join a fourth member to the cluster. The
    // fixture under test is exactly the decision triangle.
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        for id in 1..=3 {
            conn.execute(
                "INSERT INTO item_anchors (item_type, item_id, path, timestamp) \
                 VALUES ('decision', ?1, 'src/det.rs', '2026-01-01T00:00:00Z')",
                rusqlite::params![id],
            )
            .unwrap();
        }
        for (a, b) in [(1, 2), (2, 3), (1, 3)] {
            conn.execute(
                "INSERT INTO context_links (source_item_type, source_item_id, \
                 target_item_type, target_item_id, relationship_type, timestamp, origin) \
                 VALUES ('decision', ?1, 'decision', ?2, 'relates_to', \
                 '2026-01-01T00:00:00Z', 'manual')",
                rusqlite::params![a, b],
            )
            .unwrap();
        }
    }

    // Repeated scans surface one candidate with stable identity; stability
    // advances per sighting and the gates only pass on the third.
    let mut sigs = Vec::new();
    for expected_stability in [1, 2, 3] {
        let out = engrams(&db).args(["schema", "scan"]).output().unwrap();
        assert!(out.status.success());
        let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        let candidates = json["candidates"].as_array().unwrap();
        assert_eq!(candidates.len(), 1, "expected one candidate: {json}");
        let c = &candidates[0];
        assert_eq!(c["member_count"], 3);
        assert_eq!(c["stability_count"], expected_stability);
        assert_eq!(c["gates_pass"], expected_stability == 3);
        sigs.push(c["cluster_sig"].as_str().unwrap().to_string());
    }
    assert!(sigs.iter().all(|s| s == &sigs[0]), "sigs drifted: {sigs:?}");

    // Scan wrote only schema_candidates: the seeded rows are all there is.
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        let counts: Vec<(String, i64)> = [
            ("schema_candidates", 1),
            ("decisions", 3),
            ("context_links", 3),
            ("item_anchors", 3),
        ]
        .into_iter()
        .map(|(t, want)| {
            let n: i64 = conn
                .query_row(&format!("SELECT COUNT(*) FROM {t}"), [], |r| r.get(0))
                .unwrap();
            (t.to_string(), n - want)
        })
        .collect();
        assert!(
            counts.iter().all(|(_, delta)| *delta == 0),
            "unexpected row deltas: {counts:?}"
        );
    }
}

#[test]
fn test_schema_list_show_refine_and_confirm_bump() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    engrams(&db).arg("init").assert().success();

    // Seed a dense fully-linked trio by SQL: `anchor add` would materialize
    // a code node into the cluster, and a real CLI decision log would hit
    // the similarity gate on near-identical summaries.
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch(
            "INSERT INTO decisions (uuid, timestamp, summary, tags, commit_sha) VALUES
             ('u1','2026-01-01T00:00:00Z','alpha gateway routing','[\"core\",\"graph\"]','abc'),
             ('u2','2026-01-01T00:00:00Z','beta rendering pipeline','[\"core\",\"graph\"]','abc'),
             ('u3','2026-01-01T00:00:00Z','gamma policy engine','[\"core\",\"graph\"]','abc');
             INSERT INTO context_links (source_item_type, source_item_id, \
              target_item_type, target_item_id, relationship_type, timestamp, origin) VALUES
             ('decision','1','decision','2','relates_to','2026-01-01T00:00:00Z','manual'),
             ('decision','2','decision','3','relates_to','2026-01-01T00:00:00Z','manual'),
             ('decision','1','decision','3','relates_to','2026-01-01T00:00:00Z','manual');
             -- Spec 0003: apply now promotes only schema-kind candidates, so
             -- the trio gets a shared anchor (trigger) and a re-activation
             -- 45 days later (second awake stretch).
             INSERT INTO item_anchors (item_type, item_id, path, timestamp) VALUES
             ('decision',1,'src/gateway.rs','2026-01-02T00:00:00Z'),
             ('decision',2,'src/gateway.rs','2026-01-02T00:00:00Z'),
             ('decision',3,'src/gateway.rs','2026-01-02T00:00:00Z');
             INSERT INTO retrieval_surfaces (ts, cmd, arg, node_kind, node_id) VALUES
             ('2026-02-15T00:00:00Z','query','gateway','decision',1),
             ('2026-02-15T00:00:00Z','query','gateway','decision',2),
             ('2026-02-15T00:00:00Z','query','gateway','decision',3);",
        )
        .unwrap();
    }

    // Stability ramps per sighting; gates pass on the third, so --apply
    // promotes exactly then.
    engrams(&db).args(["schema", "scan"]).assert().success();
    engrams(&db).args(["schema", "scan"]).assert().success();
    let out = engrams(&db)
        .args(["schema", "scan", "--apply"])
        .output()
        .unwrap();
    let json: Value = serde_json::from_slice(&out.stdout).unwrap();
    let applied = json["applied"].as_array().unwrap();
    assert_eq!(applied.len(), 1, "expected one promotion: {json}");
    assert_eq!(applied[0], "core");

    // list: the draft exists with its member count.
    let out = engrams(&db).args(["schema", "list"]).output().unwrap();
    let json: Value = serde_json::from_slice(&out.stdout).unwrap();
    let schemas = json["schemas"].as_array().unwrap();
    assert_eq!(schemas.len(), 1);
    assert_eq!(schemas[0]["name"], "core");
    assert_eq!(schemas[0]["summary_source"], "drafted");
    assert_eq!(schemas[0]["member_count"], 3);
    assert_eq!(
        schemas[0]["kind"], "schema",
        "list shows kind next to status: {json}"
    );

    // show resolves by name and lists members.
    let out = engrams(&db)
        .args(["schema", "show", "core"])
        .output()
        .unwrap();
    let json: Value = serde_json::from_slice(&out.stdout).unwrap();
    let schema = &json["schema"];
    assert_eq!(schema["name"], "core");
    assert_eq!(schema["members"].as_array().unwrap().len(), 3);
    assert_eq!(
        schema["kind"], "schema",
        "show reports the confirm-time kind"
    );
    assert_eq!(
        schema["reasons"].as_array().map(Vec::len),
        Some(2),
        "show parses kind_reasons_json into sentence strings: {json}"
    );

    // refine rewrites the summary as agent-authored and re-ranks.
    let out = engrams(&db)
        .args([
            "schema",
            "refine",
            "core",
            "--summary",
            "Fires when gateway requests route: consult the routing decisions",
        ])
        .output()
        .unwrap();
    let json: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["schema"]["summary_source"], "agent");
    let out = engrams(&db).args(["schema", "list"]).output().unwrap();
    let json: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["schemas"][0]["summary_source"], "agent");

    // Confirm by name is a bump: timestamp advances, no new row or edges.
    // last_confirmed_at is second-granularity; wait out the rollover.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let conn = rusqlite::Connection::open(&db).unwrap();
    let before: String = conn
        .query_row(
            "SELECT last_confirmed_at FROM schemas WHERE name = 'core'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let member_of: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM context_links WHERE relationship_type = 'member_of'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    drop(conn);
    let out = engrams(&db)
        .args(["schema", "confirm", "core"])
        .output()
        .unwrap();
    let json: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["schema"]["bumped"], true, "expected bump: {json}");
    assert_eq!(
        json["schema"]["kind"], "schema",
        "bump payload also carries the row's kind: {json}"
    );
    let conn = rusqlite::Connection::open(&db).unwrap();
    let after: String = conn
        .query_row(
            "SELECT last_confirmed_at FROM schemas WHERE name = 'core'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let member_of_after: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM context_links WHERE relationship_type = 'member_of'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let rows: i64 = conn
        .query_row("SELECT COUNT(*) FROM schemas", [], |r| r.get(0))
        .unwrap();
    assert_ne!(before, after, "last_confirmed_at must advance");
    assert_eq!(rows, 1, "bump must not create a schema row");
    assert_eq!(
        member_of_after, member_of,
        "bump must not add member_of edges"
    );
    drop(conn);

    // Unknown id errors in the JSON error shape.
    let out = engrams(&db)
        .args(["schema", "show", "999"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "unknown id must fail");
    let json: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert!(
        json["error"]
            .as_str()
            .unwrap()
            .contains("no schema matches"),
        "unexpected error body: {json}"
    );
}

#[test]
fn test_schema_refine_trigger_and_rule_synthesize_agent_summary() {
    // AC-3: --trigger + --rule synthesize "Fires when <CONDITION>: <DIRECTIVE>"
    // and record summary_source as agent. Seeded by SQL because the contract
    // under test is the refine write path, not schema formation.
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    engrams(&db).arg("init").assert().success();
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch(
            "INSERT INTO schemas (uuid, name, summary, centroid_json, created_at, updated_at) VALUES
             ('u-core', 'core', 'Drafted summary', '{}',
              '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');",
        )
        .unwrap();
    }
    let out = engrams(&db)
        .args([
            "schema",
            "refine",
            "core",
            "--trigger",
            "editing public documentation",
            "--rule",
            "use problem-first risk messaging",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let json: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        json["schema"]["summary"],
        "Fires when editing public documentation: use problem-first risk messaging",
        "{json}"
    );
    assert_eq!(json["schema"]["summary_source"], "agent");
    let conn = rusqlite::Connection::open(&db).unwrap();
    let (summary, source): (String, String) = conn
        .query_row(
            "SELECT summary, summary_source FROM schemas WHERE name = 'core'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        summary,
        "Fires when editing public documentation: use problem-first risk messaging"
    );
    assert_eq!(source, "agent");
}

#[test]
fn test_schema_refine_rejects_manual_summary_without_trigger() {
    // AC-4: a manual summary that opens with neither conditional phrase is
    // rejected with guidance, and the stored summary stays untouched.
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    engrams(&db).arg("init").assert().success();
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch(
            "INSERT INTO schemas (uuid, name, summary, centroid_json, created_at, updated_at) VALUES
             ('u-core', 'core', 'Fires when deploying: run the smoke suite', '{}',
              '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');",
        )
        .unwrap();
    }
    let out = engrams(&db)
        .args([
            "schema",
            "refine",
            "core",
            "--summary",
            "Gateway architecture: how requests route",
        ])
        .output()
        .unwrap();
    assert!(!out.status.success(), "missing trigger phrase must fail");
    let json: Value = serde_json::from_slice(&out.stderr).unwrap();
    let err = json["error"].as_str().unwrap();
    assert!(err.contains("conditional trigger phrase"), "{json}");
    assert!(
        err.contains("Fires when"),
        "error names the accepted structure: {json}"
    );
    let conn = rusqlite::Connection::open(&db).unwrap();
    let (summary, source): (String, String) = conn
        .query_row(
            "SELECT summary, summary_source FROM schemas WHERE name = 'core'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        summary, "Fires when deploying: run the smoke suite",
        "failed refine must leave the summary unchanged"
    );
    assert_eq!(source, "drafted", "failed refine must not promote to agent");
}

#[test]
fn test_schema_refine_accepts_each_trigger_prefix() {
    // AC-4: every documented conditional phrase opens a manual summary.
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    engrams(&db).arg("init").assert().success();
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch(
            "INSERT INTO schemas (uuid, name, summary, centroid_json, created_at, updated_at) VALUES
             ('u-core', 'core', 'Drafted summary', '{}',
              '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');",
        )
        .unwrap();
    }
    for text in [
        "Fires when deploying: run the smoke suite",
        "When deploying: run the smoke suite",
        "Applies if deploying: run the smoke suite",
        "Activates when deploying: run the smoke suite",
    ] {
        let out = engrams(&db)
            .args(["schema", "refine", "core", "--summary", text])
            .output()
            .unwrap();
        assert!(out.status.success(), "{text} must be accepted");
        let json: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(json["schema"]["summary"], text);
    }
}

#[test]
fn test_schema_refine_requires_trigger_and_rule_together() {
    // Half a synthesized pair is rejected with guidance naming the missing
    // flag; the stored summary is untouched either way.
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    engrams(&db).arg("init").assert().success();
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch(
            "INSERT INTO schemas (uuid, name, summary, centroid_json, created_at, updated_at) VALUES
             ('u-core', 'core', 'Fires when deploying: run the smoke suite', '{}',
              '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');",
        )
        .unwrap();
    }
    for flags in [
        vec!["--trigger", "deploying"],
        vec!["--rule", "run the smoke suite"],
    ] {
        let out = engrams(&db)
            .args(["schema", "refine", "core"])
            .args(&flags)
            .output()
            .unwrap();
        assert!(!out.status.success(), "half a pair must fail: {flags:?}");
        let json: Value = serde_json::from_slice(&out.stderr).unwrap();
        let err = json["error"].as_str().unwrap();
        let missing = if flags[0] == "--trigger" {
            "--rule"
        } else {
            "--trigger"
        };
        assert!(err.contains(missing), "expected {missing} guidance: {json}");
    }
    let conn = rusqlite::Connection::open(&db).unwrap();
    let summary: String = conn
        .query_row("SELECT summary FROM schemas WHERE name = 'core'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(
        summary, "Fires when deploying: run the smoke suite",
        "failed refines must leave the summary unchanged"
    );
}

#[test]
fn test_prime_leads_with_schemas_block() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    engrams(&db).arg("init").assert().success();
    // prime requires an active-context track (same setup as test_prime).
    engrams(&db)
        .args(["active-context", "update", "--content", "{\"tasks\": []}"])
        .assert()
        .success();
    // Seed and confirm one schema (dense fully-linked trio; SQL seeding for
    // the same reasons as test_schema_list_show_refine_and_confirm_bump).
    // Spec 0003: shared anchor + re-activation make this trio a schema-kind
    // candidate so apply promotes it.
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch(
            "INSERT INTO decisions (uuid, timestamp, summary, tags, commit_sha) VALUES
             ('u1','2026-01-01T00:00:00Z','alpha gateway routing','[\"core\",\"graph\"]','abc'),
             ('u2','2026-01-01T00:00:00Z','beta rendering pipeline','[\"core\",\"graph\"]','abc'),
             ('u3','2026-01-01T00:00:00Z','gamma policy engine','[\"core\",\"graph\"]','abc');
             INSERT INTO context_links (source_item_type, source_item_id, \
              target_item_type, target_item_id, relationship_type, timestamp, origin) VALUES
             ('decision','1','decision','2','relates_to','2026-01-01T00:00:00Z','manual'),
             ('decision','2','decision','3','relates_to','2026-01-01T00:00:00Z','manual'),
             ('decision','1','decision','3','relates_to','2026-01-01T00:00:00Z','manual');
             INSERT INTO item_anchors (item_type, item_id, path, timestamp) VALUES
             ('decision',1,'src/gateway.rs','2026-01-02T00:00:00Z'),
             ('decision',2,'src/gateway.rs','2026-01-02T00:00:00Z'),
             ('decision',3,'src/gateway.rs','2026-01-02T00:00:00Z');
             INSERT INTO retrieval_surfaces (ts, cmd, arg, node_kind, node_id) VALUES
             ('2026-02-15T00:00:00Z','query','gateway','decision',1),
             ('2026-02-15T00:00:00Z','query','gateway','decision',2),
             ('2026-02-15T00:00:00Z','query','gateway','decision',3);",
        )
        .unwrap();
    }
    engrams(&db).args(["schema", "scan"]).assert().success();
    engrams(&db).args(["schema", "scan"]).assert().success();
    engrams(&db)
        .args(["schema", "scan", "--apply"])
        .assert()
        .success();

    // The pin: `schemas` is the FIRST key of prime's payload, ahead of the
    // pre-existing blocks (serde_json preserve_order makes key order load-
    // bearing; a HashMap regression here reorders it).
    let out = engrams(&db).arg("prime").output().unwrap();
    let json: Value = serde_json::from_slice(&out.stdout).unwrap();
    let map = json.as_object().expect("prime payload is an object");
    assert_eq!(
        map.keys().next(),
        Some(&"schemas".to_string()),
        "schemas block must lead prime's payload; keys: {:?}",
        map.keys().collect::<Vec<_>>()
    );
    assert!(!map["schemas"].as_array().unwrap().is_empty());
    assert_eq!(
        map["schemas"][0]["kind"], "schema",
        "prime schemas entries carry the confirm-time kind"
    );
}

#[test]
fn test_prime_ranks_context_matching_schemas() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    engrams(&db).arg("init").assert().success();
    // prime requires an active-context track (same setup as test_prime).
    engrams(&db)
        .args(["active-context", "update", "--content", "{\"tasks\": []}"])
        .assert()
        .success();
    // Two confirmed schemas: gateway (anchored to src/gateway.rs, one
    // retrieval event) and policy (anchored to docs/policy.md, two events).
    // Classically the policy schema leads on reward; AC-6 flips that when
    // the target paths or tags point at gateway. SQL seeding for the same
    // reasons as test_prime_leads_with_schemas_block.
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch(
            "INSERT INTO decisions (uuid, timestamp, summary, tags, commit_sha) VALUES
             ('u1','2026-01-01T00:00:00Z','alpha gateway routing','[\"gateway\",\"core\"]','abc'),
             ('u2','2026-01-01T00:00:00Z','beta gateway routing','[\"gateway\",\"core\"]','abc'),
             ('u3','2026-01-01T00:00:00Z','gamma gateway routing','[\"gateway\",\"core\"]','abc'),
             ('u4','2026-01-01T00:00:00Z','delta policy engine','[\"policy\",\"core\"]','abc'),
             ('u5','2026-01-01T00:00:00Z','epsilon policy engine','[\"policy\",\"core\"]','abc'),
             ('u6','2026-01-01T00:00:00Z','zeta policy engine','[\"policy\",\"core\"]','abc');
             INSERT INTO context_links (source_item_type, source_item_id, \
              target_item_type, target_item_id, relationship_type, timestamp, origin) VALUES
             ('decision','1','decision','2','relates_to','2026-01-01T00:00:00Z','manual'),
             ('decision','2','decision','3','relates_to','2026-01-01T00:00:00Z','manual'),
             ('decision','1','decision','3','relates_to','2026-01-01T00:00:00Z','manual'),
             ('decision','4','decision','5','relates_to','2026-01-01T00:00:00Z','manual'),
             ('decision','5','decision','6','relates_to','2026-01-01T00:00:00Z','manual'),
             ('decision','4','decision','6','relates_to','2026-01-01T00:00:00Z','manual');
             INSERT INTO item_anchors (item_type, item_id, path, timestamp) VALUES
             ('decision',1,'src/gateway.rs','2026-01-02T00:00:00Z'),
             ('decision',2,'src/gateway.rs','2026-01-02T00:00:00Z'),
             ('decision',3,'src/gateway.rs','2026-01-02T00:00:00Z'),
             ('decision',4,'docs/policy.md','2026-01-02T00:00:00Z'),
             ('decision',5,'docs/policy.md','2026-01-02T00:00:00Z'),
             ('decision',6,'docs/policy.md','2026-01-02T00:00:00Z');
             INSERT INTO retrieval_surfaces (ts, cmd, arg, node_kind, node_id) VALUES
             ('2026-02-15T00:00:00Z','query','gateway','decision',1),
             ('2026-02-15T00:00:00Z','query','gateway','decision',2),
             ('2026-02-15T00:00:00Z','query','gateway','decision',3),
             ('2026-02-15T00:00:00Z','query','policy','decision',4),
             ('2026-02-15T00:00:00Z','query','policy','decision',5),
             ('2026-02-15T00:00:00Z','query','policy','decision',6),
             ('2026-02-16T00:00:00Z','query','policy','decision',4),
             ('2026-02-16T00:00:00Z','query','policy','decision',5),
             ('2026-02-16T00:00:00Z','query','policy','decision',6);",
        )
        .unwrap();
    }
    engrams(&db).args(["schema", "scan"]).assert().success();
    engrams(&db).args(["schema", "scan"]).assert().success();
    engrams(&db)
        .args(["schema", "scan", "--apply"])
        .assert()
        .success();
    // reward_hits counts schema-level surface rows (retrieval_surfaces with
    // node_kind 'schema'), so seed two events for policy and one for gateway:
    // unscoped prime must lead with the higher-reward policy schema.
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        let id_for = |like: &str| -> i64 {
            conn.query_row(
                "SELECT id FROM schemas WHERE summary LIKE ?1",
                rusqlite::params![format!("%{like}%")],
                |r| r.get(0),
            )
            .unwrap()
        };
        let (gateway, policy) = (id_for("gateway routing"), id_for("policy engine"));
        conn.execute_batch(&format!(
            "INSERT INTO retrieval_surfaces (ts, cmd, arg, node_kind, node_id) VALUES
             ('2026-03-01T00:00:00Z','prime',NULL,'schema',{gateway}),
             ('2026-03-01T00:00:00Z','prime',NULL,'schema',{policy}),
             ('2026-03-02T00:00:00Z','prime',NULL,'schema',{policy});"
        ))
        .unwrap();
    }

    let summaries = |out: &std::process::Output| -> Vec<String> {
        let json: Value = serde_json::from_slice(&out.stdout).unwrap();
        json["schemas"]
            .as_array()
            .expect("prime payload carries a schemas array")
            .iter()
            .map(|s| {
                let sum = s["summary"].as_str().unwrap();
                assert!(!sum.is_empty(), "each entry carries its rule summary");
                sum.to_string()
            })
            .collect()
    };

    // No flags: classic order — the higher-reward policy schema leads.
    let sums = summaries(&engrams(&db).arg("prime").output().unwrap());
    assert!(
        sums[0].contains("policy") && !sums[0].contains("gateway"),
        "unscoped prime keeps classic reward order; got: {sums:?}"
    );

    // --paths: the schema anchored under the target path leads.
    let sums = summaries(
        &engrams(&db)
            .args(["prime", "--paths", "src/gateway.rs"])
            .output()
            .unwrap(),
    );
    assert!(
        sums[0].contains("gateway"),
        "path-anchored schema must outrank the higher-reward one; got: {sums:?}"
    );

    // --tags: the schema whose centroid matches the tag leads.
    let sums = summaries(
        &engrams(&db)
            .args(["prime", "--tags", "gateway"])
            .output()
            .unwrap(),
    );
    assert!(
        sums[0].contains("gateway"),
        "tag-matching schema must outrank the higher-reward one; got: {sums:?}"
    );
}

#[test]
fn test_schema_confirm_payload_carries_kind() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    engrams(&db).arg("init").assert().success();
    // Dense fully-linked trio (same SQL seed as the list/show test): the
    // scan's kind pass stamps the candidate `schema` — shared trigger
    // anchor plus a second awake stretch — and confirm's promote payload
    // must echo that snapshot (spec 0004: confirm gains kind, no reasons).
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch(
            "INSERT INTO decisions (uuid, timestamp, summary, tags, commit_sha) VALUES
             ('u1','2026-01-01T00:00:00Z','alpha gateway routing','[\"core\",\"graph\"]','abc'),
             ('u2','2026-01-01T00:00:00Z','beta rendering pipeline','[\"core\",\"graph\"]','abc'),
             ('u3','2026-01-01T00:00:00Z','gamma policy engine','[\"core\",\"graph\"]','abc');
             INSERT INTO context_links (source_item_type, source_item_id, \
              target_item_type, target_item_id, relationship_type, timestamp, origin) VALUES
             ('decision','1','decision','2','relates_to','2026-01-01T00:00:00Z','manual'),
             ('decision','2','decision','3','relates_to','2026-01-01T00:00:00Z','manual'),
             ('decision','1','decision','3','relates_to','2026-01-01T00:00:00Z','manual');
             INSERT INTO item_anchors (item_type, item_id, path, timestamp) VALUES
             ('decision',1,'src/gateway.rs','2026-01-02T00:00:00Z'),
             ('decision',2,'src/gateway.rs','2026-01-02T00:00:00Z'),
             ('decision',3,'src/gateway.rs','2026-01-02T00:00:00Z');
             INSERT INTO retrieval_surfaces (ts, cmd, arg, node_kind, node_id) VALUES
             ('2026-02-15T00:00:00Z','query','gateway','decision',1),
             ('2026-02-15T00:00:00Z','query','gateway','decision',2),
             ('2026-02-15T00:00:00Z','query','gateway','decision',3);",
        )
        .unwrap();
    }
    // Three sightings: stability ramps per scan and the gates pass on the
    // third, stamping the staged candidate `schema` (plain scan, no --apply,
    // so nothing is auto-promoted). `confirm` then resolves it by signature
    // prefix and takes the promote arm.
    engrams(&db).args(["schema", "scan"]).assert().success();
    engrams(&db).args(["schema", "scan"]).assert().success();
    engrams(&db).args(["schema", "scan"]).assert().success();
    let sig: String = {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.query_row("SELECT cluster_sig FROM schema_candidates", [], |r| {
            r.get(0)
        })
        .unwrap()
    };
    let out = engrams(&db)
        .args(["schema", "confirm", &sig, "--name", "core"])
        .output()
        .unwrap();
    let json: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        json["schema"].get("bumped").is_none(),
        "expected the promote arm, got a bump: {json}"
    );
    assert_eq!(
        json["schema"]["kind"], "schema",
        "promote payload carries the copied kind: {json}"
    );
    assert!(
        json["schema"].get("reasons").is_none(),
        "confirm reports kind only; reason sentences live on schema show"
    );

    // The copied row feeds the read paths: show parses the reasons array.
    let out = engrams(&db)
        .args(["schema", "show", "core"])
        .output()
        .unwrap();
    let json: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["schema"]["kind"], "schema");
    assert!(
        !json["schema"]["reasons"].as_array().unwrap().is_empty(),
        "show renders the stored reason sentences: {json}"
    );
}

#[test]
fn test_engrams_db_env_override() {
    let temp = TempDir::new().unwrap();
    let env_db = temp.path().join("env.db");
    // A fake workspace: cwd discovery would target this DB, so any write
    // landing here means the override was ignored.
    let ws = TempDir::new().unwrap();
    std::fs::create_dir_all(ws.path().join(".engrams")).unwrap();

    // Precedence: --db beats ENGRAMS_DB. The --db path is created; the env
    // path is not.
    let flag_db = temp.path().join("flag.db");
    Command::cargo_bin("engrams")
        .unwrap()
        .env("ENGRAMS_DB", &env_db)
        .arg("--db")
        .arg(&flag_db)
        .arg("init")
        .assert()
        .success();
    assert!(flag_db.exists(), "--db must win over ENGRAMS_DB");
    assert!(
        !env_db.exists(),
        "ENGRAMS_DB must not be used when --db is set"
    );

    // ENGRAMS_DB beats workspace/CWD discovery, and a nonexistent env path
    // is created on first use exactly like --db.
    Command::cargo_bin("engrams")
        .unwrap()
        .env("ENGRAMS_DB", &env_db)
        .current_dir(ws.path())
        .arg("init")
        .assert()
        .success();
    assert!(env_db.exists(), "ENGRAMS_DB must receive the writes");
    assert!(
        !ws.path().join("engrams").join("context.db").exists(),
        "the discovered workspace DB must be untouched while ENGRAMS_DB is set"
    );

    // The override is real routing, not just file creation: a write through
    // ENGRAMS_DB is readable through it.
    Command::cargo_bin("engrams")
        .unwrap()
        .env("ENGRAMS_DB", &env_db)
        .current_dir(ws.path())
        .args(["decision", "log", "--summary", "env routed", "--force"])
        .assert()
        .success();
    let out = Command::cargo_bin("engrams")
        .unwrap()
        .env("ENGRAMS_DB", &env_db)
        .current_dir(ws.path())
        .args(["decision", "list"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let body: Value = serde_json::from_slice(&out.stdout).unwrap();
    let summaries: Vec<&str> = body
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|d| d["summary"].as_str())
        .collect();
    assert!(
        summaries.contains(&"env routed"),
        "decision must land in the ENGRAMS_DB database: {summaries:?}"
    );
}
#[test]
fn test_graph_densification_from_anchors() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    engrams(&db).arg("init").assert().success();
    engrams(&db)
        .args([
            "decision",
            "log",
            "--summary",
            "d1",
            "--anchor",
            "src/a.rs",
            "--anchor",
            "src/b.rs",
        ])
        .assert()
        .success();
    engrams(&db)
        .args([
            "decision",
            "log",
            "--summary",
            "d2",
            "--anchor",
            "src/b.rs",
            "--anchor",
            "src/c.rs",
        ])
        .assert()
        .success();

    engrams(&db)
        .args(["graph", "rebuild", "--no-git"])
        .assert()
        .success();

    let out = engrams(&db).args(["graph", "stats"]).output().unwrap();
    let stats: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(stats["edges"]["total"].as_i64().unwrap() >= 3);
    let relates = stats["edges"]["by_relationship"]["relates_to"]
        .as_i64()
        .unwrap_or(0);
    assert!(relates >= 1, "expected relates_to edges, got {:?}", stats);

    let out = engrams(&db)
        .args([
            "graph",
            "path",
            "--from",
            "decision:1",
            "--to",
            "decision:2",
        ])
        .output()
        .unwrap();
    let path: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(!path["path"].as_array().unwrap().is_empty());

    let out = engrams(&db).args(["graph", "orphans"]).output().unwrap();
    let orphans: Value = serde_json::from_slice(&out.stdout).unwrap();
    let list: Vec<&str> = orphans["orphans"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["node"].as_str().unwrap())
        .collect();
    assert!(!list.contains(&"decision:1"));
}

#[test]
fn test_graph_incremental_on_write() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    engrams(&db).arg("init").assert().success();
    engrams(&db)
        .args([
            "decision",
            "log",
            "--summary",
            "d1",
            "--anchor",
            "src/a.rs",
            "--anchor",
            "src/b.rs",
        ])
        .assert()
        .success();
    engrams(&db)
        .args([
            "decision",
            "log",
            "--summary",
            "d2",
            "--anchor",
            "src/b.rs",
            "--anchor",
            "src/c.rs",
        ])
        .assert()
        .success();

    // No rebuild yet: touch_item on the second log must already link d1 and d2.
    let out = engrams(&db)
        .args(["link", "list", "--item-type", "decision", "--item-id", "1"])
        .output()
        .unwrap();
    let links: Value = serde_json::from_slice(&out.stdout).unwrap();
    let found = links.as_array().unwrap().iter().any(|l| {
        l["relationship_type"].as_str() == Some("relates_to")
            && (l["target_item_id"].as_str() == Some("2")
                || l["source_item_id"].as_str() == Some("2"))
    });
    assert!(found, "expected incremental relates_to edge: {:?}", links);
}

#[test]
fn test_graph_centrality_stats_shape() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    engrams(&db).arg("init").assert().success();
    engrams(&db)
        .args(["decision", "log", "--summary", "d1", "--anchor", "src/a.rs"])
        .assert()
        .success();
    engrams(&db)
        .args(["decision", "log", "--summary", "d2", "--anchor", "src/a.rs"])
        .assert()
        .success();
    engrams(&db)
        .args(["graph", "rebuild", "--no-git"])
        .assert()
        .success();

    let out = engrams(&db)
        .args(["graph", "central", "--limit", "5"])
        .output()
        .unwrap();
    let central: Value = serde_json::from_slice(&out.stdout).unwrap();
    let ranked = central["ranked"].as_array().unwrap();
    assert!(!ranked.is_empty() && ranked.len() <= 5);
    for r in ranked {
        assert!(r["node"].is_string());
        assert!(r["score"].is_number());
    }
    // Scores are ranked (non-increasing).
    for w in ranked.windows(2) {
        assert!(w[0]["score"].as_f64().unwrap() >= w[1]["score"].as_f64().unwrap());
    }

    let out = engrams(&db).args(["graph", "stats"]).output().unwrap();
    let stats: Value = serde_json::from_slice(&out.stdout).unwrap();
    for key in [
        "nodes",
        "edges",
        "density",
        "components",
        "orphans",
        "degree",
    ] {
        assert!(stats.get(key).is_some(), "missing stats key {}", key);
    }
}

#[test]
fn test_graph_git_cochange() {
    let temp = TempDir::new().unwrap();
    let repo = temp.path();

    let git = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(repo)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {:?} failed", args);
    };

    git(&["init"]);
    git(&["config", "user.email", "test@example.com"]);
    git(&["config", "user.name", "Test"]);
    std::fs::write(repo.join("f1"), "one").unwrap();
    std::fs::write(repo.join("f2"), "one").unwrap();
    // Vendored / generated files committed alongside real sources: these
    // must never become code nodes.
    std::fs::create_dir_all(repo.join("docs/node_modules/junk")).unwrap();
    std::fs::create_dir_all(repo.join("docs/dist")).unwrap();
    std::fs::write(repo.join("docs/node_modules/junk/index.js"), "junk").unwrap();
    std::fs::write(repo.join("docs/dist/out.html"), "junk").unwrap();
    std::fs::write(repo.join("package-lock.json"), "{}").unwrap();
    std::fs::write(repo.join("app.min.js"), "junk").unwrap();
    git(&["add", "."]);
    git(&["commit", "-m", "c1"]);
    std::fs::write(repo.join("f1"), "two").unwrap();
    std::fs::write(repo.join("f2"), "two").unwrap();
    std::fs::write(repo.join("docs/node_modules/junk/index.js"), "junk2").unwrap();
    git(&["add", "."]);
    git(&["commit", "-m", "c2"]);

    let mut cmd = Command::cargo_bin("engrams").unwrap();
    cmd.env("ENGRAMS_NO_UPDATE_CHECK", "1");
    cmd.current_dir(repo)
        .args(["graph", "ingest"])
        .assert()
        .success();

    let conn = rusqlite::Connection::open(repo.join("engrams/context.db")).unwrap();
    let edge_count: i64 = conn
        .query_row(
            "SELECT count(*) FROM context_links \
             WHERE relationship_type = 'co_changes' AND source_item_type = 'code' AND target_item_type = 'code'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(edge_count, 1);

    let node_count: i64 = conn
        .query_row("SELECT count(*) FROM code_nodes", [], |r| r.get(0))
        .unwrap();
    assert_eq!(node_count, 2);
    drop(conn);

    // Second ingest with no new commits must be a no-op that preserves
    // the existing co_changes edges (guards the empty-window wipe).
    let out = Command::cargo_bin("engrams")
        .unwrap()
        .env("ENGRAMS_NO_UPDATE_CHECK", "1")
        .current_dir(repo)
        .args(["graph", "ingest"])
        .output()
        .unwrap();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(!v["ingested"].as_bool().unwrap());

    let conn = rusqlite::Connection::open(repo.join("engrams/context.db")).unwrap();
    let edge_count_after: i64 = conn
        .query_row(
            "SELECT count(*) FROM context_links \
             WHERE relationship_type = 'co_changes' AND source_item_type = 'code' AND target_item_type = 'code'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(edge_count_after, 1);
    drop(conn);

    // Rebuild prunes code nodes no edge references (junk ingested before
    // filtering existed), while keeping the real co_changes structure.
    let conn = rusqlite::Connection::open(repo.join("engrams/context.db")).unwrap();
    conn.execute(
        "INSERT INTO code_nodes (kind, path, symbol, first_seen, last_seen) \
         VALUES ('file', 'docs/node_modules/legacy/x.js', '', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        [],
    )
    .unwrap();
    drop(conn);

    let out = Command::cargo_bin("engrams")
        .unwrap()
        .env("ENGRAMS_NO_UPDATE_CHECK", "1")
        .current_dir(repo)
        .args(["graph", "rebuild"])
        .output()
        .unwrap();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v["rebuilt"].as_bool().unwrap());
    assert!(v["code_nodes_pruned"].as_i64().unwrap() >= 1);

    let conn = rusqlite::Connection::open(repo.join("engrams/context.db")).unwrap();
    let node_count: i64 = conn
        .query_row("SELECT count(*) FROM code_nodes", [], |r| r.get(0))
        .unwrap();
    assert_eq!(node_count, 2);
    let vendored: i64 = conn
        .query_row(
            "SELECT count(*) FROM code_nodes WHERE path LIKE '%node_modules%' OR path LIKE '%dist%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(vendored, 0);
    let edge_count_final: i64 = conn
        .query_row(
            "SELECT count(*) FROM context_links \
             WHERE relationship_type = 'co_changes' AND source_item_type = 'code' AND target_item_type = 'code'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(edge_count_final, 1);
}

#[test]
fn test_graph_export_excludes_derived() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    let exp_dir = temp.path().join("exp");

    engrams(&db).arg("init").assert().success();
    engrams(&db)
        .args([
            "decision",
            "log",
            "--summary",
            "d1",
            "--anchor",
            "src/a.rs",
            "--anchor",
            "src/b.rs",
        ])
        .assert()
        .success();
    engrams(&db)
        .args([
            "decision",
            "log",
            "--summary",
            "d2",
            "--anchor",
            "src/b.rs",
            "--anchor",
            "src/c.rs",
        ])
        .assert()
        .success();
    // One manual edge alongside the derived ones.
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
            "extends",
        ])
        .assert()
        .success();
    engrams(&db)
        .args(["graph", "rebuild", "--no-git"])
        .assert()
        .success();

    engrams(&db)
        .args(["export", "--path", exp_dir.to_str().unwrap()])
        .assert()
        .success();

    let links_dir = exp_dir.join("links");
    let mut files: Vec<_> = std::fs::read_dir(&links_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().map(|x| x == "md").unwrap_or(false))
        .collect();
    files.sort();
    assert_eq!(files.len(), 1, "only the manual edge should export");
    for f in files {
        let content = std::fs::read_to_string(&f).unwrap();
        assert!(content.contains("\"origin\": \"manual\""), "{:?}", f);
        assert!(!content.contains("co_changes"));
        assert!(!content.contains("anchored_to"));
    }
}

// ========== v0.10.0: retrieval scoring, prune-decay, read-observability ==========

#[test]
fn test_importance_on_log_and_update() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    // Log with explicit importance
    let out = engrams(&db)
        .args([
            "decision",
            "log",
            "--summary",
            "Important decision",
            "--importance",
            "9",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["importance"], 9);

    // Update importance
    let out = engrams(&db)
        .args(["decision", "update", "1", "--importance", "3"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["importance"], 3);
}

#[test]
fn test_importance_default_hidden() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    let out = engrams(&db)
        .args(["decision", "log", "--summary", "Default importance"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    // importance=5 is the default; skip_serializing_if hides it
    assert!(v.get("importance").is_none() || v["importance"] == 5);
}

#[test]
fn test_reinforce_on_read() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    engrams(&db)
        .args(["decision", "log", "--summary", "Decision to be read"])
        .assert()
        .success();

    // access_count starts at 0
    let conn = rusqlite::Connection::open(&db).unwrap();
    let ac: i64 = conn
        .query_row("SELECT access_count FROM decisions WHERE id = 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(ac, 0);

    // prime should reinforce
    engrams(&db).arg("prime").assert().success();

    let ac: i64 = conn
        .query_row("SELECT access_count FROM decisions WHERE id = 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert!(
        ac >= 1,
        "access_count should be >= 1 after prime, got {}",
        ac
    );

    // query should also reinforce
    engrams(&db).args(["query", "Decision"]).assert().success();

    let ac2: i64 = conn
        .query_row("SELECT access_count FROM decisions WHERE id = 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert!(
        ac2 > ac,
        "access_count should increase after query, got {} -> {}",
        ac,
        ac2
    );
}

#[test]
fn test_scoring_in_query_output() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    engrams(&db)
        .args([
            "decision",
            "log",
            "--summary",
            "Scoring test",
            "--importance",
            "8",
        ])
        .assert()
        .success();

    let out = engrams(&db).args(["query", "Scoring"]).output().unwrap();
    assert!(out.status.success());
    let results: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert!(!results.is_empty());
    assert!(
        results[0].get("score").is_some(),
        "query results must include a score field"
    );
}

#[test]
fn test_importance_affects_ranking() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    // Two decisions with same summary but different importance
    engrams(&db)
        .args([
            "decision",
            "log",
            "--summary",
            "Ranking alpha beta",
            "--importance",
            "2",
        ])
        .assert()
        .success();
    engrams(&db)
        .args([
            "decision",
            "log",
            "--summary",
            "Ranking alpha beta",
            "--importance",
            "10",
            "--force",
        ])
        .assert()
        .success();

    // query should rank the higher-importance one first
    let out = engrams(&db)
        .args(["query", "Ranking", "--limit", "5"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let results: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert!(results.len() >= 2, "should have at least 2 results");
    let first_id = results[0]["id"].as_i64().unwrap();
    assert_eq!(
        first_id, 2,
        "higher importance decision (id 2) should rank first"
    );
}

#[test]
fn test_prune_dry_run_no_match() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    engrams(&db)
        .args([
            "decision",
            "log",
            "--summary",
            "Fresh decision",
            "--importance",
            "7",
        ])
        .assert()
        .success();

    let out = engrams(&db).args(["prune", "--dry-run"]).output().unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["dry_run"], true);
    assert_eq!(
        v["count"], 0,
        "fresh high-importance decision should not be prunable"
    );
}

#[test]
fn test_prune_archives_low_importance() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    // importance 0, never accessed — always prunable
    engrams(&db)
        .args([
            "decision",
            "log",
            "--summary",
            "Forgettable",
            "--importance",
            "0",
        ])
        .assert()
        .success();

    // Dry run should show it as a candidate
    let out = engrams(&db).args(["prune", "--dry-run"]).output().unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        v["count"], 1,
        "importance-0 never-read decision should be prunable"
    );

    // Actual prune
    engrams(&db).args(["prune"]).assert().success();

    // Verify archived flag
    let conn = rusqlite::Connection::open(&db).unwrap();
    let archived: i64 = conn
        .query_row("SELECT archived FROM decisions WHERE id = 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(
        archived, 1,
        "decision should be archived after prune --apply"
    );
}

#[test]
fn test_archived_hidden_from_prime() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    engrams(&db)
        .args([
            "decision",
            "log",
            "--summary",
            "Will be archived",
            "--importance",
            "0",
        ])
        .assert()
        .success();
    engrams(&db).args(["prune"]).assert().success();

    let out = engrams(&db).arg("prime").output().unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    let decisions = v["decisions"].as_array().unwrap();
    assert!(
        decisions.is_empty(),
        "archived decisions should not appear in prime"
    );
}

#[test]
fn test_archived_visible_with_all_flag() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    // Log a low-importance pattern that will also be pruned
    engrams(&db)
        .args([
            "pattern",
            "log",
            "--name",
            "ForgettablePattern",
            "--importance",
            "0",
        ])
        .assert()
        .success();
    engrams(&db)
        .args([
            "decision",
            "log",
            "--summary",
            "Will be archived",
            "--importance",
            "0",
        ])
        .assert()
        .success();
    engrams(&db).args(["prune"]).assert().success();

    // relevant --all should include archived
    engrams(&db)
        .args(["relevant", "src/main.rs", "--all"])
        .assert()
        .success();

    // query --all should include archived decisions AND patterns
    let out = engrams(&db)
        .args(["query", "ForgettablePattern", "--all"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let results: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    let has_archived_pattern = results
        .iter()
        .any(|r| r["type"] == "system_pattern" && r["title"] == "ForgettablePattern");
    assert!(
        has_archived_pattern,
        "archived patterns should appear in query --all"
    );
}

#[test]
fn test_doctor_never_read() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    // Log a decision but don't read it
    engrams(&db)
        .args(["decision", "log", "--summary", "Never surfaced decision"])
        .assert()
        .success();

    let out = engrams(&db).arg("doctor").output().unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    let never_read = v["never_read"].as_array().unwrap();
    assert!(
        !never_read.is_empty(),
        "un-read decisions should be flagged"
    );

    // After prime, it should be read
    engrams(&db).arg("prime").assert().success();
    let out = engrams(&db).arg("doctor").output().unwrap();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    let never_read = v["never_read"].as_array().unwrap();
    assert!(
        never_read.is_empty(),
        "after prime, no decisions should be never-read"
    );
}

#[test]
fn test_doctor_archived_count() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    engrams(&db)
        .args([
            "decision",
            "log",
            "--summary",
            "Archived one",
            "--importance",
            "0",
        ])
        .assert()
        .success();
    engrams(&db).args(["prune"]).assert().success();

    let out = engrams(&db).arg("doctor").output().unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    let archived = v["archived"].as_object().unwrap();
    let dec_count = archived["decisions"].as_i64().unwrap();
    assert_eq!(dec_count, 1, "doctor should count 1 archived decision");
}

#[test]
fn test_v6_schema_columns() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    engrams(&db).arg("init").assert().success();

    let conn = rusqlite::Connection::open(&db).unwrap();

    // Check decisions columns
    let cols: Vec<String> = conn
        .prepare("PRAGMA table_info(decisions)")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .map(|c| c.unwrap())
        .collect();
    for col in &["importance", "access_count", "last_accessed_at", "archived"] {
        assert!(
            cols.contains(&col.to_string()),
            "decisions table missing column: {}",
            col
        );
    }

    // Check system_patterns columns
    let cols: Vec<String> = conn
        .prepare("PRAGMA table_info(system_patterns)")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .map(|c| c.unwrap())
        .collect();
    for col in &["importance", "access_count", "last_accessed_at", "archived"] {
        assert!(
            cols.contains(&col.to_string()),
            "system_patterns table missing column: {}",
            col
        );
    }
}

#[test]
fn test_pattern_importance_flag() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    let out = engrams(&db)
        .args([
            "pattern",
            "log",
            "--name",
            "test-pattern-imp",
            "--description",
            "test",
            "--importance",
            "8",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["importance"], 8);
}

#[test]
fn test_export_import_preserves_importance() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    engrams(&db)
        .args([
            "decision",
            "log",
            "--summary",
            "Export test",
            "--importance",
            "9",
        ])
        .assert()
        .success();

    let exp_dir = temp.path().join("exp");
    engrams(&db)
        .args(["export", "--path", exp_dir.to_str().unwrap()])
        .assert()
        .success();

    // Verify importance in exported file
    let decisions_dir = exp_dir.join("decisions");
    let files: Vec<_> = std::fs::read_dir(&decisions_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().map(|x| x == "md").unwrap_or(false))
        .collect();
    assert!(!files.is_empty(), "should have exported decisions");
    let content = std::fs::read_to_string(&files[0]).unwrap();
    assert!(
        content.contains("\"importance\": 9"),
        "exported decision must preserve importance"
    );

    // Import into a new DB
    let db2 = temp.path().join("e2.db");
    engrams(&db2)
        .args(["import", "--path", exp_dir.to_str().unwrap()])
        .assert()
        .success();

    let conn = rusqlite::Connection::open(&db2).unwrap();
    let imp: i64 = conn
        .query_row("SELECT importance FROM decisions WHERE id = 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(imp, 9, "imported decision must have importance 9");
}

#[test]
fn test_export_emits_schema_kind_columns() {
    // spec 0004 AC-6: every schema row exports kind and kind_reasons_json
    // verbatim, including legacy rows that never left the column defaults.
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    engrams(&db).arg("init").assert().success();

    // One labeled row and one pre-0004 row, seeded by SQL: the export
    // contract is about emitting stored columns, not about how confirm
    // produced them.
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch(
            "INSERT INTO schemas (uuid, name, summary, centroid_json, created_at, \
             updated_at, kind, kind_reasons_json) VALUES
             ('uuid-labeled', 'core', 'labeled pack', '{}',
              '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 'schema',
              '[\"dense cluster\",\"anchors agree\"]'),
             ('uuid-legacy', 'legacy-pack', 'pre-0004 pack', '{}',
              '2025-06-01T00:00:00Z', '2025-06-01T00:00:00Z', 'unclear', '[]');",
        )
        .unwrap();
    }

    let exp_dir = temp.path().join("exp");
    engrams(&db)
        .args(["export", "--path", exp_dir.to_str().unwrap()])
        .assert()
        .success();

    let labeled = std::fs::read_to_string(exp_dir.join("schemas/1.md")).unwrap();
    assert!(
        labeled.contains("\"kind\": \"schema\""),
        "labeled row exports its kind: {labeled}"
    );
    assert!(
        labeled
            .contains("\"kind_reasons_json\": \"[\\\"dense cluster\\\",\\\"anchors agree\\\"]\""),
        "reasons snapshot exported verbatim: {labeled}"
    );
    let legacy = std::fs::read_to_string(exp_dir.join("schemas/2.md")).unwrap();
    assert!(
        legacy.contains("\"kind\": \"unclear\"")
            && legacy.contains("\"kind_reasons_json\": \"[]\""),
        "legacy row exports its defaults verbatim: {legacy}"
    );
}

#[test]
fn test_import_schema_kind_falls_back_for_older_exports() {
    // An export predating the kind columns carries neither field; import
    // must land the row on the column defaults without error and without
    // re-labeling.
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    engrams(&db).arg("init").assert().success();

    let exp_dir = temp.path().join("old-export");
    std::fs::create_dir_all(exp_dir.join("schemas")).unwrap();
    std::fs::write(
        exp_dir.join("schemas/7.md"),
        "---\nidentifier: \"7\"\ntitle: \"legacy-pack\"\n---\n\n# legacy-pack\n\n```json\n{\n  \"id\": 7,\n  \"uuid\": \"uuid-old-7\",\n  \"name\": \"legacy-pack\",\n  \"summary\": \"written before schema v14\",\n  \"centroid_json\": \"{}\",\n  \"created_at\": \"2025-01-01T00:00:00Z\",\n  \"updated_at\": \"2025-01-01T00:00:00Z\"\n}\n```\n",
    )
    .unwrap();

    engrams(&db)
        .args(["import", "--path", exp_dir.to_str().unwrap()])
        .assert()
        .success();

    let conn = rusqlite::Connection::open(&db).unwrap();
    let (kind, reasons): (String, String) = conn
        .query_row(
            "SELECT kind, kind_reasons_json FROM schemas WHERE id = 7",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        (kind.as_str(), reasons.as_str()),
        ("unclear", "[]"),
        "missing kind fields fall back to the column defaults"
    );
}

#[test]
fn test_batch_decision() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    engrams(&db).arg("init").assert().success();

    let items_json = r#"[
        {"summary": "Batch decision one", "rationale": "Rationale 1"},
        {"summary": "Batch decision two", "rationale": "Rationale 2", "tags": ["tagA", "tagB"]}
    ]"#;
    let stdout = engrams(&db)
        .args(["batch", "--type", "decision", "--items", items_json])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let result: Value = serde_json::from_slice(&stdout).unwrap();
    assert_eq!(result["created"], 2, "batch must create 2 decisions");
    let ids = result["ids"].as_array().expect("ids array in batch output");
    assert_eq!(ids.len(), 2, "batch must return 2 ids");

    let id1 = ids[0].as_i64().unwrap();
    let id2 = ids[1].as_i64().unwrap();

    let stdout = engrams(&db)
        .args(["decision", "get", &id1.to_string()])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let one: Value = serde_json::from_slice(&stdout).unwrap();
    assert_eq!(one["summary"], "Batch decision one");
    assert_eq!(one["rationale"], "Rationale 1");

    let stdout = engrams(&db)
        .args(["decision", "get", &id2.to_string()])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let two: Value = serde_json::from_slice(&stdout).unwrap();
    assert_eq!(two["summary"], "Batch decision two");
    assert_eq!(two["rationale"], "Rationale 2");
    assert_eq!(
        two["tags"],
        serde_json::json!(["tagA", "tagB"]),
        "batched decision must keep its tags"
    );
}

#[test]
fn test_advise_surfaces_matching_schemas() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    engrams(&db).arg("init").assert().success();

    // Seeded by SQL because the contract under test is schema resolution
    // against anchors, not member production: an active schema over a
    // decision anchored to the advised path, an active schema anchored
    // elsewhere, and a proposed schema over the same decision.
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch(
            "INSERT INTO decisions (uuid, timestamp, summary, tags, commit_sha, status, contract) VALUES
             ('d1', '2026-01-01T00:00:00Z', 'Advise returns only actionable constraints', '[\"advise\"]', 'abc', 'active',
              'No scores, no progress, no reinforcement'),
             ('d2', '2026-01-01T00:00:00Z', 'Docs use problem-first messaging', '[\"docs\"]', 'def', 'active',
              'Lead with the reader risk');
            INSERT INTO schemas (uuid, name, summary, centroid_json, created_at, updated_at, kind) VALUES
             ('match-uuid', 'pre-edit-gating',
              'Fires when editing the advise pipeline: keep output constraints-only and machine-readable',
              '{}', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 'schema'),
             ('elsewhere-uuid', 'docs-voice',
              'Fires when writing docs: use problem-first messaging',
              '{}', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 'story');
            INSERT INTO schemas (uuid, name, summary, centroid_json, created_at, updated_at) VALUES
             ('proposed-uuid', 'not-yet-active',
              'Fires when editing the advise pipeline: still proposed',
              '{}', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
            UPDATE schemas SET status = 'proposed' WHERE uuid = 'proposed-uuid';
            INSERT INTO item_anchors (item_type, item_id, path, timestamp) VALUES
             ('decision', 1, 'src/ops/advise.rs', '2026-01-01T00:00:00Z'),
             ('decision', 2, 'docs/guide.md', '2026-01-01T00:00:00Z');
            INSERT INTO context_links (source_item_type, source_item_id, target_item_type, target_item_id,
                                       relationship_type, description, timestamp) VALUES
             ('decision', '1', 'schema', '1', 'member_of', NULL, '2026-01-01T00:00:00Z'),
             ('decision', '1', 'schema', '3', 'member_of', NULL, '2026-01-01T00:00:00Z'),
             ('decision', '2', 'schema', '2', 'member_of', NULL, '2026-01-01T00:00:00Z');",
        )
        .unwrap();
    }

    let out = engrams(&db)
        .args(["advise", "src/ops/advise.rs"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let parsed: Value = serde_json::from_str(&stdout).unwrap();

    // AC-5 clause 1: the schema whose member anchors to the advised path
    // appears with its trigger-and-rule summary; non-anchored and
    // non-active schemas stay out.
    let schemas = parsed["schemas"].as_array().unwrap();
    assert_eq!(
        schemas.len(),
        1,
        "exactly the anchored active schema: {:?}",
        schemas,
    );
    let entry = &schemas[0];
    assert_eq!(entry["id"], 1);
    assert_eq!(entry["name"], "pre-edit-gating");
    assert_eq!(entry["kind"], "schema");
    let summary = entry["summary"].as_str().unwrap();
    assert!(
        summary.starts_with("Fires when") && summary.contains(':'),
        "summary carries trigger and rule: {summary}"
    );

    // AC-5 clause 2: schemas sit above the low-level constraints in the
    // serialized output (serde_json preserve_order keeps insertion order).
    let s_pos = stdout.find("\"schemas\"").unwrap();
    let c_pos = stdout.find("\"constraints\"").unwrap();
    let v_pos = stdout.find("\"violations\"").unwrap();
    assert!(s_pos < c_pos && c_pos < v_pos);

    // The low-level machinery keeps working alongside: the anchored
    // decision still surfaces as a constraint.
    let constraints = parsed["constraints"].as_array().unwrap();
    assert_eq!(constraints.len(), 1);
    assert_eq!(constraints[0]["type"], "decision");
    assert_eq!(constraints[0]["id"], 1);
}

#[test]
fn test_advise_schemas_empty_without_anchored_schemas() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    engrams(&db).arg("init").assert().success();

    let out = engrams(&db)
        .args(["advise", "src/nothing/here.rs"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let parsed: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(parsed["schemas"], serde_json::json!([]));
    assert_eq!(parsed["constraints"], serde_json::json!([]));
    let s_pos = stdout.find("\"schemas\"").unwrap();
    let c_pos = stdout.find("\"constraints\"").unwrap();
    assert!(s_pos < c_pos, "schemas key stays above constraints");
}

// spec 0005 Architecture item 1: `schema show` members gain the per-kind
// details `brief` surfaces, capped at 20 with truncation metadata.
#[test]
fn test_schema_show_enriches_members_by_kind() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    engrams(&db).arg("init").assert().success();

    // One member of every enrichable kind, one nested schema, one
    // pass-through kind, one dangling reference — seeded by SQL because
    // the contract under test is the read path, not member production.
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch(
            "INSERT INTO decisions (uuid, timestamp, summary, tags, commit_sha, status, contract) VALUES
             ('d1', '2026-01-01T00:00:00Z', 'Ship the parser rewrite', '[\"parser\"]', 'abc', 'accepted',
              'Fail fast on malformed input');
            INSERT INTO system_patterns (uuid, timestamp, name, description, severity) VALUES
             ('p1', '2026-01-01T00:00:00Z', 'borrow-first', 'Consume lookup maps by move', 'high');
            INSERT INTO progress_entries (timestamp, status, description) VALUES
             ('2026-01-02T00:00:00Z', 'in_progress', 'Wire enriched show output');
            INSERT INTO custom_data (timestamp, category, key, value) VALUES
             ('2026-01-03T00:00:00Z', 'config', 'region', '{\"default\": \"us-east-1\"}');
            INSERT INTO code_nodes (kind, path, symbol, first_seen, last_seen, line_count) VALUES
             ('file', 'src/parse.rs', '', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 120);
            INSERT INTO schemas (uuid, name, summary, centroid_json, created_at, updated_at) VALUES
             ('nested-uuid', 'nested-pack', 'A schema used as a member', '{}',
              '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
            INSERT INTO schemas (uuid, name, summary, centroid_json, created_at, updated_at) VALUES
             ('multi-uuid', 'multi-kind', 'Host schema for every member kind', '{}',
              '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
            INSERT INTO item_anchors (item_type, item_id, path, timestamp) VALUES
             ('decision', 1, 'src/parse.rs', '2026-01-01T00:00:00Z'),
             ('system_pattern', 1, 'docs/patterns.md', '2026-01-01T00:00:00Z');
            INSERT INTO context_links (source_item_type, source_item_id, target_item_type, target_item_id,
                                       relationship_type, description, timestamp) VALUES
             ('decision', '1', 'schema', '2', 'member_of', NULL, '2026-01-01T00:00:00Z'),
             ('system_pattern', '1', 'schema', '2', 'member_of', NULL, '2026-01-01T00:00:00Z'),
             ('progress_entry', '1', 'schema', '2', 'member_of', NULL, '2026-01-01T00:00:00Z'),
             ('custom_data', '1', 'schema', '2', 'member_of', NULL, '2026-01-01T00:00:00Z'),
             ('code', '1', 'schema', '2', 'member_of', NULL, '2026-01-01T00:00:00Z'),
             ('schema', '1', 'schema', '2', 'member_of', NULL, '2026-01-01T00:00:00Z'),
             ('pr', '42', 'schema', '2', 'member_of', NULL, '2026-01-01T00:00:00Z'),
             ('decision', '9999', 'schema', '2', 'member_of', NULL, '2026-01-01T00:00:00Z');",
        )
        .unwrap();
    }

    let out = engrams(&db)
        .args(["schema", "show", "multi-kind"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let parsed: Value = serde_json::from_slice(&out.stdout).unwrap();
    let schema = &parsed["schema"];

    assert_eq!(
        schema["member_count"], 8,
        "member_count stays the true total"
    );
    assert_eq!(schema["truncated"], false);
    assert!(
        schema.get("total_members").is_none(),
        "total_members only appears when the cap bites"
    );
    let members = schema["members"].as_array().unwrap();
    assert_eq!(members.len(), 8);
    let find = |kind: &str, id: i64| {
        members
            .iter()
            .find(|m| m["kind"] == kind && m["id"] == id)
            .unwrap_or_else(|| panic!("member {kind}:{id} missing"))
    };

    let dec = find("decision", 1);
    assert_eq!(dec["summary"], "Ship the parser rewrite");
    assert_eq!(dec["status"], "accepted");
    assert_eq!(dec["contract"], "Fail fast on malformed input");
    assert_eq!(dec["anchors"], serde_json::json!(["src/parse.rs"]));

    let pat = find("system_pattern", 1);
    assert_eq!(pat["name"], "borrow-first");
    assert_eq!(pat["description"], "Consume lookup maps by move");
    assert_eq!(pat["severity"], "high");
    assert_eq!(pat["anchors"], serde_json::json!(["docs/patterns.md"]));

    let prog = find("progress_entry", 1);
    assert_eq!(prog["status"], "in_progress");
    assert_eq!(prog["description"], "Wire enriched show output");
    assert_eq!(prog["timestamp"], "2026-01-02T00:00:00Z");

    let data = find("custom_data", 1);
    assert_eq!(data["key"], "region");
    assert_eq!(
        data["value"],
        serde_json::json!({"default": "us-east-1"}),
        "stored TEXT value surfaces as JSON when it parses"
    );

    let code = find("code", 1);
    assert_eq!(code["path"], "src/parse.rs");
    assert_eq!(code["symbol"], "");
    assert_eq!(code["kind"], "code");
    assert_eq!(code["line_count"], 120);

    let nested = find("schema", 1);
    assert_eq!(nested["name"], "nested-pack");
    assert_eq!(nested["summary"], "A schema used as a member");

    let pr = find("pr", 42);
    assert!(
        pr.get("summary").is_none() && pr.get("path").is_none(),
        "unenrichable kinds stay bare references"
    );

    let dangling = find("decision", 9999);
    assert!(
        dangling.get("summary").is_none(),
        "dangling references stay bare, never fabricated"
    );
    assert_eq!(dangling["anchors"], serde_json::json!([]));
}

#[test]
fn test_schema_show_truncates_members_over_cap() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    engrams(&db).arg("init").assert().success();

    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        let mut batch = String::new();
        for i in 1..=25 {
            batch.push_str(&format!(
                "INSERT INTO decisions (uuid, timestamp, summary) VALUES \
                 ('big-{i}', '2026-01-01T00:00:00Z', 'Decision {i}');\n"
            ));
        }
        batch.push_str(
            "INSERT INTO schemas (uuid, name, summary, centroid_json, created_at, updated_at) VALUES \
             ('big-uuid', 'big-pack', 'Over the cap', '{}', \
              '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');\n",
        );
        for i in 1..=25 {
            batch.push_str(&format!(
                "INSERT INTO context_links (source_item_type, source_item_id, target_item_type, \
                 target_item_id, relationship_type, description, timestamp) VALUES \
                 ('decision', '{i}', 'schema', '1', 'member_of', NULL, '2026-01-01T00:00:00Z');\n"
            ));
        }
        conn.execute_batch(&batch).unwrap();
    }

    let out = engrams(&db)
        .args(["schema", "show", "big-pack"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let parsed: Value = serde_json::from_slice(&out.stdout).unwrap();
    let schema = &parsed["schema"];

    assert_eq!(
        schema["member_count"], 25,
        "member_count stays the true total"
    );
    assert_eq!(schema["truncated"], true);
    assert_eq!(schema["total_members"], 25);
    let members = schema["members"].as_array().unwrap();
    assert_eq!(members.len(), 20, "exactly the cap is enriched");

    // Members keep the link query's (kind, insertion) order — insertion is
    // the context_links rowid, here 1..=25 in seed order — so the first 20
    // shown are ids 1 through 20.
    let shown: Vec<String> = members
        .iter()
        .map(|m| m["id"].as_i64().unwrap().to_string())
        .collect();
    let expected: Vec<String> = (1..=20).map(|i| i.to_string()).collect();
    assert_eq!(shown, expected);
    for m in members {
        assert_eq!(m["summary"], format!("Decision {}", m["id"]));
    }
}

// spec 0005 Architecture item 2: `brief schema:<id>` returns the schema's
// immediate members enriched with the same per-kind details `schema show`
// surfaces (phase 1's shared code path), capped at 20 with truncation metadata.
#[test]
fn test_brief_schema_enriches_members_by_kind() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    engrams(&db).arg("init").assert().success();

    // Same multi-kind seeding as the `schema show` enrichment test: the
    // contract is that both read paths surface identical member details.
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch(
            "INSERT INTO decisions (uuid, timestamp, summary, tags, commit_sha, status, contract) VALUES
             ('d1', '2026-01-01T00:00:00Z', 'Ship the parser rewrite', '[\"parser\"]', 'abc', 'accepted',
              'Fail fast on malformed input');
            INSERT INTO system_patterns (uuid, timestamp, name, description, severity) VALUES
             ('p1', '2026-01-01T00:00:00Z', 'borrow-first', 'Consume lookup maps by move', 'high');
            INSERT INTO progress_entries (timestamp, status, description) VALUES
             ('2026-01-02T00:00:00Z', 'in_progress', 'Wire enriched show output');
            INSERT INTO custom_data (timestamp, category, key, value) VALUES
             ('2026-01-03T00:00:00Z', 'config', 'region', '{\"default\": \"us-east-1\"}');
            INSERT INTO code_nodes (kind, path, symbol, first_seen, last_seen, line_count) VALUES
             ('file', 'src/parse.rs', '', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 120);
            INSERT INTO schemas (uuid, name, summary, centroid_json, created_at, updated_at) VALUES
             ('nested-uuid', 'nested-pack', 'A schema used as a member', '{}',
              '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
            INSERT INTO schemas (uuid, name, summary, centroid_json, created_at, updated_at) VALUES
             ('multi-uuid', 'multi-kind', 'Host schema for every member kind', '{}',
              '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
            INSERT INTO item_anchors (item_type, item_id, path, timestamp) VALUES
             ('decision', 1, 'src/parse.rs', '2026-01-01T00:00:00Z'),
             ('system_pattern', 1, 'docs/patterns.md', '2026-01-01T00:00:00Z');
            INSERT INTO context_links (source_item_type, source_item_id, target_item_type, target_item_id,
                                       relationship_type, description, timestamp) VALUES
             ('decision', '1', 'schema', '2', 'member_of', NULL, '2026-01-01T00:00:00Z'),
             ('system_pattern', '1', 'schema', '2', 'member_of', NULL, '2026-01-01T00:00:00Z'),
             ('progress_entry', '1', 'schema', '2', 'member_of', NULL, '2026-01-01T00:00:00Z'),
             ('custom_data', '1', 'schema', '2', 'member_of', NULL, '2026-01-01T00:00:00Z'),
             ('code', '1', 'schema', '2', 'member_of', NULL, '2026-01-01T00:00:00Z'),
             ('schema', '1', 'schema', '2', 'member_of', NULL, '2026-01-01T00:00:00Z'),
             ('pr', '42', 'schema', '2', 'member_of', NULL, '2026-01-01T00:00:00Z'),
             ('decision', '9999', 'schema', '2', 'member_of', NULL, '2026-01-01T00:00:00Z');",
        )
        .unwrap();
    }

    let out = engrams(&db).args(["brief", "schema:2"]).output().unwrap();
    assert!(out.status.success());
    let parsed: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(parsed["node"], "schema:2");
    assert_eq!(parsed["kind"], "schema");

    let brief = &parsed["brief"];
    assert_eq!(brief["id"], 2);
    assert_eq!(brief["name"], "multi-kind");
    assert_eq!(brief["summary"], "Host schema for every member kind");
    assert_eq!(
        brief["member_count"], 8,
        "member_count stays the true total"
    );
    assert_eq!(brief["truncated"], false);
    assert!(
        brief.get("total_members").is_none(),
        "total_members only appears when the cap bites"
    );

    let members = brief["members"].as_array().unwrap();
    assert_eq!(members.len(), 8);
    let find = |kind: &str, id: i64| {
        members
            .iter()
            .find(|m| m["kind"] == kind && m["id"] == id)
            .unwrap_or_else(|| panic!("member {kind}:{id} missing"))
    };

    let dec = find("decision", 1);
    assert_eq!(dec["summary"], "Ship the parser rewrite");
    assert_eq!(dec["status"], "accepted");
    assert_eq!(dec["contract"], "Fail fast on malformed input");
    assert_eq!(dec["anchors"], serde_json::json!(["src/parse.rs"]));

    let pat = find("system_pattern", 1);
    assert_eq!(pat["name"], "borrow-first");
    assert_eq!(pat["description"], "Consume lookup maps by move");
    assert_eq!(pat["severity"], "high");
    assert_eq!(pat["anchors"], serde_json::json!(["docs/patterns.md"]));

    let prog = find("progress_entry", 1);
    assert_eq!(prog["status"], "in_progress");
    assert_eq!(prog["description"], "Wire enriched show output");
    assert_eq!(prog["timestamp"], "2026-01-02T00:00:00Z");

    let data = find("custom_data", 1);
    assert_eq!(data["key"], "region");
    assert_eq!(
        data["value"],
        serde_json::json!({"default": "us-east-1"}),
        "stored TEXT value surfaces as JSON when it parses"
    );

    let code = find("code", 1);
    assert_eq!(code["path"], "src/parse.rs");
    assert_eq!(code["symbol"], "");
    assert_eq!(code["line_count"], 120);

    let nested = find("schema", 1);
    assert_eq!(nested["name"], "nested-pack");
    assert_eq!(nested["summary"], "A schema used as a member");

    let pr = find("pr", 42);
    assert!(
        pr.get("summary").is_none() && pr.get("path").is_none(),
        "unenrichable kinds stay bare references"
    );

    let dangling = find("decision", 9999);
    assert!(
        dangling.get("summary").is_none(),
        "dangling references stay bare, never fabricated"
    );
}

#[test]
fn test_brief_schema_resolves_by_name_and_slug() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    engrams(&db).arg("init").assert().success();
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch(
            "INSERT INTO schemas (uuid, name, summary, centroid_json, created_at, updated_at) VALUES
             ('wp-uuid', 'Deploy Pipeline', 'Ships code to production', '{}',
              '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');",
        )
        .unwrap();
    }

    for target in [
        "schema:Deploy Pipeline",
        "schema:deploy-pipeline",
        "schema:1",
    ] {
        let out = engrams(&db).args(["brief", target]).output().unwrap();
        assert!(out.status.success(), "target '{target}' should resolve");
        let parsed: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(parsed["brief"]["id"], 1, "target '{target}'");
        assert_eq!(parsed["brief"]["name"], "Deploy Pipeline");
    }
}

#[test]
fn test_brief_schema_unknown_target_errors() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    engrams(&db).arg("init").assert().success();

    engrams(&db)
        .args(["brief", "schema:ghost"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no schema matches 'ghost'"));

    // Bare numbers stay FTS text: no implicit schema fallback.
    engrams(&db).args(["brief", "1"]).assert().failure();
}

#[test]
fn test_brief_schema_truncates_members_over_cap() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    engrams(&db).arg("init").assert().success();
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        let mut batch = String::new();
        for i in 1..=25 {
            batch.push_str(&format!(
                "INSERT INTO decisions (uuid, timestamp, summary) VALUES \
                 ('big-{i}', '2026-01-01T00:00:00Z', 'Decision {i}');\n"
            ));
        }
        batch.push_str(
            "INSERT INTO schemas (uuid, name, summary, centroid_json, created_at, updated_at) VALUES \
             ('big-uuid', 'big-pack', 'Over the cap', '{}', \
              '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');\n",
        );
        for i in 1..=25 {
            batch.push_str(&format!(
                "INSERT INTO context_links (source_item_type, source_item_id, target_item_type, \
                 target_item_id, relationship_type, description, timestamp) VALUES \
                 ('decision', '{i}', 'schema', '1', 'member_of', NULL, '2026-01-01T00:00:00Z');\n"
            ));
        }
        conn.execute_batch(&batch).unwrap();
    }

    let out = engrams(&db)
        .args(["brief", "schema:big-pack"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let parsed: Value = serde_json::from_slice(&out.stdout).unwrap();
    let brief = &parsed["brief"];

    assert_eq!(
        brief["member_count"], 25,
        "member_count stays the true total"
    );
    assert_eq!(brief["truncated"], true);
    assert_eq!(brief["total_members"], 25);
    let members = brief["members"].as_array().unwrap();
    assert_eq!(members.len(), 20, "exactly the cap is enriched");
    for m in members {
        assert_eq!(m["summary"], format!("Decision {}", m["id"]));
    }
}

// --- Session close: schema_value + schema_opportunities (spec 0005) ---

/// Seeds one `kind='schema'` candidate that clears the gates, plus retrieval
/// surfaces and a usage row for the given session. Returns the candidate's
/// signature so assertions can stay data-driven.
fn seed_schema_candidate(
    db: &std::path::Path,
    signature: &str,
    surface_ts: &[(&str, i64)],
    usage_ts: &str,
    session: &str,
) {
    let conn = rusqlite::Connection::open(db).unwrap();
    conn.execute(
        "INSERT INTO schema_candidates (cluster_sig, member_keys_json, density, stability_count, reward_hits, last_drift_removed, last_drift_added, first_seen_at, last_seen_at, kind) \
         VALUES (?1, ?2, 0.8, 3, 1, 0, 0, '2020-01-01T00:00:00Z', '2020-01-01T00:00:00Z', 'schema')",
        rusqlite::params![signature, r#"["decision:1","decision:2"]"#],
    )
    .unwrap();
    for (ts, node_id) in surface_ts {
        conn.execute(
            "INSERT INTO retrieval_surfaces (ts, cmd, arg, node_kind, node_id) VALUES (?1, 'advise', NULL, 'schema', ?2)",
            rusqlite::params![ts, node_id],
        )
        .unwrap();
    }
    conn.execute(
        "INSERT INTO usage_log (timestamp, command, arg, hits, miss, session) VALUES (?1, 'advise', NULL, 2, 0, ?2)",
        rusqlite::params![usage_ts, session],
    )
    .unwrap();
}

#[test]
fn test_session_close_surfaces_schema_value_and_opportunities() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    engrams(&db).arg("init").assert().success();

    // Session usage starts at 12:00; surfaces at/after it count — one per
    // distinct schema node (1 and 2) plus a repeat of node 1.
    seed_schema_candidate(
        &db,
        "decision:1|decision:2",
        &[
            ("2026-10-04T12:01:00Z", 1),
            ("2026-10-04T12:02:00Z", 2),
            ("2026-10-04T12:03:00Z", 1),
        ],
        "2026-10-04T12:00:00Z",
        "ses_test",
    );

    let out = engrams(&db)
        .args([
            "session",
            "close",
            "--session",
            "ses_test",
            "--note",
            "test",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let parsed: Value = serde_json::from_slice(&out.stdout).unwrap();

    // Windowed guided-retrieval counts for the named session.
    assert_eq!(parsed["schema_value"]["guided_retrievals"], 3);
    assert_eq!(parsed["schema_value"]["distinct_schemas"], 2);

    // The gated candidate is surfaced with a suggestion.
    let opps = parsed["schema_opportunities"].as_array().unwrap();
    assert_eq!(opps.len(), 1, "one gated candidate surfaces");
    assert_eq!(opps[0]["signature"], "decision:1|decision:2");
    assert!(opps[0]["suggested_name"].as_str().is_some());
    assert!(opps[0]["confirm_command"].as_str().is_some());

    // Surfaced == stamped: notified_at is set for exactly what was emitted.
    let conn = rusqlite::Connection::open(&db).unwrap();
    let notified: String = conn
        .query_row(
            "SELECT notified_at FROM schema_candidates WHERE cluster_sig = ?1",
            rusqlite::params!["decision:1|decision:2"],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!notified.is_empty(), "surfaced candidate must be stamped");
}

#[test]
fn test_session_close_manual_close_before_any_closes_is_zero_window() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    engrams(&db).arg("init").assert().success();

    // Surfaces exist but no session and no prior close: a manual close has no
    // window anchor, so counts stay zero instead of reporting all-time totals.
    seed_schema_candidate(
        &db,
        "decision:1|decision:2",
        &[("2026-10-04T12:01:00Z", 1)],
        "2026-10-04T12:00:00Z",
        "ses_test",
    );

    let out = engrams(&db)
        .args(["session", "close", "--note", "first manual close"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let parsed: Value = serde_json::from_slice(&out.stdout).unwrap();

    assert_eq!(parsed["schema_value"]["guided_retrievals"], 0);
    assert_eq!(parsed["schema_value"]["distinct_schemas"], 0);
}

#[test]
fn test_session_close_manual_close_windows_since_previous_close() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    engrams(&db).arg("init").assert().success();

    // One surface before the previous close, one after it: only the newer
    // surface falls inside this close's window.
    seed_schema_candidate(
        &db,
        "decision:1|decision:2",
        &[("2026-10-04T09:00:00Z", 1), ("2026-10-04T11:00:00Z", 2)],
        "2026-10-04T08:00:00Z",
        "ses_earlier",
    );
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute(
        "INSERT INTO session_closes (timestamp, session, reads_skipped, reads_required) VALUES ('2026-10-04T10:00:00Z', 'ses_earlier', 0, 0)",
        [],
    )
    .unwrap();
    drop(conn);

    let out = engrams(&db)
        .args(["session", "close", "--note", "second manual close"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let parsed: Value = serde_json::from_slice(&out.stdout).unwrap();

    assert_eq!(parsed["schema_value"]["guided_retrievals"], 1);
    assert_eq!(parsed["schema_value"]["distinct_schemas"], 1);
}

#[test]
fn test_session_close_second_close_does_not_resurface_stamped_candidates() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    engrams(&db).arg("init").assert().success();

    seed_schema_candidate(
        &db,
        "decision:1|decision:2",
        &[("2026-10-04T12:01:00Z", 1)],
        "2026-10-04T12:00:00Z",
        "ses_test",
    );

    let out = engrams(&db)
        .args(["session", "close", "--session", "ses_test"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let first: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(first["schema_opportunities"].as_array().unwrap().len(), 1);

    // The candidate is unchanged (notified_at was stamped at first close), so
    // the next close must not surface it again.
    let out = engrams(&db)
        .args(["session", "close", "--session", "ses_test"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let second: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        second["schema_opportunities"].as_array().unwrap().len(),
        0,
        "stamped candidates are not re-surfaced"
    );
}

#[test]
fn test_schema_list_includes_guided_count() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    engrams(&db).arg("init").assert().success();

    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch(
            "INSERT INTO schemas (id, uuid, name, summary, centroid_json, created_at, updated_at) VALUES
             (1, 'u1', 'active-schema', 'summary', '{}', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
             INSERT INTO retrieval_surfaces (ts, cmd, arg, node_kind, node_id) VALUES
             ('2026-01-01T00:00:00Z', 'advise', 'path', 'schema', 1),
             ('2026-01-01T00:01:00Z', 'advise', 'path', 'schema', 1),
             ('2026-01-01T00:02:00Z', 'advise', 'path', 'decision', 1);",
        )
        .unwrap();
    }

    let out = engrams(&db).args(["schema", "list"]).output().unwrap();
    assert!(out.status.success());
    let val: Value = serde_json::from_slice(&out.stdout).unwrap();
    let schemas = val["schemas"].as_array().unwrap();
    assert_eq!(schemas.len(), 1);
    assert_eq!(
        schemas[0]["guided"], 2,
        "two schema-kind surface rows count; the decision row does not: {val}"
    );

    // `schema show` reads the same counter so both views agree.
    let out = engrams(&db).args(["schema", "show", "1"]).output().unwrap();
    assert!(out.status.success());
    let val: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        val["schema"]["guided"], 2,
        "show mirrors the list count: {val}"
    );
}

#[test]
fn test_schema_list_guided_is_zero_without_surfaces() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    engrams(&db).arg("init").assert().success();

    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch(
            "INSERT INTO schemas (uuid, name, summary, centroid_json, created_at, updated_at) VALUES
             ('u-quiet', 'quiet-schema', 'never surfaced', '{}',
              '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');",
        )
        .unwrap();
    }

    let out = engrams(&db).args(["schema", "list"]).output().unwrap();
    assert!(out.status.success());
    let val: Value = serde_json::from_slice(&out.stdout).unwrap();
    let schemas = val["schemas"].as_array().unwrap();
    assert_eq!(schemas.len(), 1);
    assert_eq!(
        schemas[0]["guided"], 0,
        "a never-surfaced schema reads 0, not null: {val}"
    );
}

#[test]
fn test_session_close_opportunity_workflow_actionable() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    engrams(&db).arg("init").assert().success();

    // Seed real members and a gate-passing schema candidate whose territory no
    // confirmed schema claims. Gate values mirror seed_schema_candidate (density
    // 0.8 / stability 3 / reward 1 all pass).
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch(
            "INSERT INTO decisions (id, uuid, timestamp, summary, tags) VALUES
             (10, 'u10', '2026-01-01T00:00:00Z', 'Dec 10', '[\"ast\", \"compiler\"]'),
             (11, 'u11', '2026-01-01T00:00:00Z', 'Dec 11', '[\"ast\", \"syntax\"]');
             INSERT INTO item_anchors (item_type, item_id, path, timestamp) VALUES
             ('decision', 10, 'src/ast/lexer.rs', '2026-01-01T00:00:00Z'),
             ('decision', 11, 'src/ast/parser.rs', '2026-01-01T00:00:00Z');
             INSERT INTO schema_candidates (cluster_sig, member_keys_json, density, stability_count, reward_hits, first_seen_at, last_seen_at, kind) VALUES
             ('decision:10,decision:11', '[\"decision:10\", \"decision:11\"]', 0.8, 3, 1,
              '2026-01-01T00:00:00Z', '2026-01-01T01:00:00Z', 'schema');",
        )
        .unwrap();
    }

    // Close surfaces exactly one opportunity with an executable command.
    let out = engrams(&db)
        .args(["session", "close", "--note", "first"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let val: Value = serde_json::from_slice(&out.stdout).unwrap();
    let opps = val["schema_opportunities"].as_array().unwrap();
    assert_eq!(opps.len(), 1, "one gate-passing candidate surfaces: {val}");

    // The emitted command runs verbatim: drop the leading "engrams" and pass
    // the rest as argv (the suggested name is a single kebab-case token).
    let cmd_str = opps[0]["confirm_command"].as_str().unwrap();
    let args: Vec<&str> = cmd_str.split_whitespace().skip(1).collect();
    engrams(&db).args(&args).assert().success();

    // The schema now exists under the suggested name.
    let name = opps[0]["suggested_name"].as_str().unwrap();
    let show_out = engrams(&db)
        .args(["schema", "show", name])
        .output()
        .unwrap();
    assert!(
        show_out.status.success(),
        "emitted command must produce a resolvable schema"
    );

    // A later close neither re-nags (notified) nor re-offers (territory claimed).
    let out2 = engrams(&db)
        .args(["session", "close", "--note", "second"])
        .output()
        .unwrap();
    assert!(out2.status.success());
    let val2: Value = serde_json::from_slice(&out2.stdout).unwrap();
    let opps2 = val2["schema_opportunities"].as_array().unwrap();
    assert!(
        opps2.is_empty(),
        "promoted/notified candidate must not re-nag: {val2}"
    );
}

fn init_git_repo_with_commit(dir: &std::path::Path, filename: &str, content: &str) -> String {
    let run = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };
    run(&["init"]);
    run(&["config", "user.email", "test@example.com"]);
    run(&["config", "user.name", "Test"]);
    std::fs::write(dir.join(filename), content).unwrap();
    run(&["add", filename]);
    run(&["commit", "-m", "initial"]);
    run(&["rev-parse", "HEAD"])
}

#[test]
fn workspace_stamps_named_workspace_commit_not_shell_repo_ac1() {
    let repo_a = TempDir::new().unwrap();
    let repo_b = TempDir::new().unwrap();
    let sha_a = init_git_repo_with_commit(repo_a.path(), "a.rs", "pub fn a() {}\n");
    let sha_b = init_git_repo_with_commit(repo_b.path(), "b.rs", "pub fn b() {}\n");
    assert_ne!(sha_a, sha_b);

    let out = Command::cargo_bin("engrams")
        .unwrap()
        .env("ENGRAMS_NO_UPDATE_CHECK", "1")
        .current_dir(repo_b.path())
        .arg("--workspace")
        .arg(repo_a.path())
        .args([
            "decision",
            "log",
            "--summary",
            "Scoped decision",
            "--contract",
            "fn a()",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let dec: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(dec["commit_sha"].as_str(), Some(sha_a.as_str()));

    let out_prog = Command::cargo_bin("engrams")
        .unwrap()
        .env("ENGRAMS_NO_UPDATE_CHECK", "1")
        .current_dir(repo_b.path())
        .arg("--workspace")
        .arg(repo_a.path())
        .args([
            "progress",
            "log",
            "--status",
            "Done",
            "--description",
            "Scoped progress",
        ])
        .output()
        .unwrap();
    assert!(out_prog.status.success());
    let prog: Value = serde_json::from_slice(&out_prog.stdout).unwrap();
    assert_eq!(prog["commit_sha"].as_str(), Some(sha_a.as_str()));
}

#[test]
fn workspace_rebuild_uses_named_workspace_history_only_ac2() {
    let repo_a = TempDir::new().unwrap();
    let repo_b = TempDir::new().unwrap();
    let _ = init_git_repo_with_commit(repo_a.path(), "a1.rs", "pub fn a1() {}\n");
    let _ = init_git_repo_with_commit(repo_b.path(), "b1.rs", "pub fn b1() {}\n");

    for (repo, f1, f2) in [
        (repo_a.path(), "a1.rs", "a2.rs"),
        (repo_b.path(), "b1.rs", "b2.rs"),
    ] {
        for step in 1..=2 {
            std::fs::write(repo.join(f1), format!("// step {step}\npub fn x() {{}}\n")).unwrap();
            std::fs::write(repo.join(f2), format!("// step {step}\npub fn y() {{}}\n")).unwrap();
            std::process::Command::new("git")
                .args(["add", f1, f2])
                .current_dir(repo)
                .output()
                .unwrap();
            std::process::Command::new("git")
                .args(["commit", "-m", &format!("cochange {step}")])
                .current_dir(repo)
                .output()
                .unwrap();
        }
    }

    let out = Command::cargo_bin("engrams")
        .unwrap()
        .env("ENGRAMS_NO_UPDATE_CHECK", "1")
        .current_dir(repo_b.path())
        .arg("--workspace")
        .arg(repo_a.path())
        .args(["graph", "rebuild", "--min-cochange", "2"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let rebuilt: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(rebuilt["edges_by_source"]["cochange"], 1);
    assert!(rebuilt.get("cochange_skipped").is_none());

    let conn = rusqlite::Connection::open(repo_a.path().join("engrams/context.db")).unwrap();
    let mut stmt = conn
        .prepare("SELECT path FROM code_nodes ORDER BY path ASC")
        .unwrap();
    let paths: Vec<String> = stmt
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(paths, vec!["a1.rs".to_string(), "a2.rs".to_string()]);
}

#[test]
fn workspace_standard_db_layout_derives_root_ac4() {
    let repo_a = TempDir::new().unwrap();
    let repo_b = TempDir::new().unwrap();
    let sha_a = init_git_repo_with_commit(repo_a.path(), "a.rs", "pub fn a() {}\n");
    let _sha_b = init_git_repo_with_commit(repo_b.path(), "b.rs", "pub fn b() {}\n");

    let standard_db = repo_a.path().join("engrams").join("context.db");
    let out = engrams(&standard_db)
        .current_dir(repo_b.path())
        .args([
            "decision",
            "log",
            "--summary",
            "Standard DB layout",
            "--contract",
            "fn a()",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let dec: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(dec["commit_sha"].as_str(), Some(sha_a.as_str()));
}

#[test]
fn workspace_nonstandard_db_withholds_git_commit_ac5() {
    let repo_b = TempDir::new().unwrap();
    let _sha_b = init_git_repo_with_commit(repo_b.path(), "b.rs", "pub fn b() {}\n");
    let isolated = TempDir::new().unwrap();
    let db = isolated.path().join("copy.db");

    let out = engrams(&db)
        .current_dir(repo_b.path())
        .args([
            "decision",
            "log",
            "--summary",
            "Isolated DB decision",
            "--contract",
            "none",
            "--anchor",
            "b.rs",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let dec: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        dec["commit_sha"].is_null(),
        "nonstandard --db must not record shell repo commit: {dec}"
    );

    // Also verify code_nodes enrichment did not read b.rs from shell CWD.
    let conn = rusqlite::Connection::open(&db).unwrap();
    let line_count: Option<i64> = conn
        .query_row(
            "SELECT line_count FROM code_nodes WHERE path = 'b.rs'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        line_count.is_none(),
        "upsert_file must not read shell CWD when root is None"
    );

    let out_prog = engrams(&db)
        .current_dir(repo_b.path())
        .args([
            "progress",
            "log",
            "--status",
            "Done",
            "--description",
            "Isolated progress",
        ])
        .output()
        .unwrap();
    assert!(out_prog.status.success());
    let prog: Value = serde_json::from_slice(&out_prog.stdout).unwrap();
    assert!(prog["commit_sha"].is_null());
}

#[test]
fn workspace_refuses_pr_number_resolution_without_workspace_ac6() {
    let repo_b = TempDir::new().unwrap();
    let _ = init_git_repo_with_commit(repo_b.path(), "b.rs", "pub fn b() {}\n");
    std::process::Command::new("git")
        .args(["remote", "add", "origin", "git@github.com:acme/b.git"])
        .current_dir(repo_b.path())
        .output()
        .unwrap();

    let isolated = TempDir::new().unwrap();
    let db = isolated.path().join("copy.db");

    let out = engrams(&db)
        .current_dir(repo_b.path())
        .args([
            "decision",
            "log",
            "--summary",
            "Needs PR number",
            "--pr",
            "42",
        ])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let err: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(
        err["error"].as_str(),
        Some(
            "PR number resolution needs a workspace: pass --workspace <dir> or keep the database at <dir>/engrams/context.db"
        )
    );
}

#[test]
fn workspace_skips_cochange_without_workspace_ac7() {
    let repo_b = TempDir::new().unwrap();
    let _ = init_git_repo_with_commit(repo_b.path(), "b1.rs", "pub fn b1() {}\n");
    let isolated = TempDir::new().unwrap();
    let db = isolated.path().join("copy.db");

    let out_rebuild = engrams(&db)
        .current_dir(repo_b.path())
        .args(["graph", "rebuild"])
        .output()
        .unwrap();
    assert!(out_rebuild.status.success());
    let rebuilt: Value = serde_json::from_slice(&out_rebuild.stdout).unwrap();
    assert_eq!(rebuilt["rebuilt"], true);
    assert_eq!(rebuilt["cochange_skipped"], "no workspace");
    assert!(rebuilt["edges_by_source"].get("cochange").is_none());

    let out_ingest = engrams(&db)
        .current_dir(repo_b.path())
        .args(["graph", "ingest"])
        .output()
        .unwrap();
    assert!(out_ingest.status.success());
    let ingested: Value = serde_json::from_slice(&out_ingest.stdout).unwrap();
    assert_eq!(ingested["ingested"], false);
    assert_eq!(ingested["reason"], "no workspace named");
    assert_eq!(ingested["edges_by_source"]["cochange"], 0);
}

#[test]
fn workspace_staged_checks_only_named_workspace_files_ac3() {
    let repo_a = TempDir::new().unwrap();
    let repo_b = TempDir::new().unwrap();
    let _ = init_git_repo_with_commit(repo_a.path(), "init_a.rs", "pub fn init_a() {}\n");
    let _ = init_git_repo_with_commit(repo_b.path(), "init_b.rs", "pub fn init_b() {}\n");

    Command::cargo_bin("engrams")
        .unwrap()
        .env("ENGRAMS_NO_UPDATE_CHECK", "1")
        .current_dir(repo_b.path())
        .arg("--workspace")
        .arg(repo_a.path())
        .args([
            "pattern",
            "log",
            "--name",
            "No Forbidden Token",
            "--check-kind",
            "regex",
            "--check",
            "FORBIDDEN_TOKEN",
            "--severity",
            "error",
        ])
        .assert()
        .success();

    // Stage a violating file in repo A and a different violating file in repo B.
    std::fs::write(
        repo_a.path().join("staged_a.rs"),
        "pub fn bad_a() { let _ = FORBIDDEN_TOKEN; }\n",
    )
    .unwrap();
    std::process::Command::new("git")
        .args(["add", "staged_a.rs"])
        .current_dir(repo_a.path())
        .output()
        .unwrap();

    std::fs::write(
        repo_b.path().join("staged_b.rs"),
        "pub fn bad_b() { let _ = FORBIDDEN_TOKEN; }\n",
    )
    .unwrap();
    std::process::Command::new("git")
        .args(["add", "staged_b.rs"])
        .current_dir(repo_b.path())
        .output()
        .unwrap();

    let out = Command::cargo_bin("engrams")
        .unwrap()
        .env("ENGRAMS_NO_UPDATE_CHECK", "1")
        .current_dir(repo_b.path())
        .arg("--workspace")
        .arg(repo_a.path())
        .args(["check", "--staged"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "violation in repo A should exit 1");
    let res: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(res["files_checked"], 1);
    let violations = res["violations"].as_array().unwrap();
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0]["file"], "staged_a.rs");

    // Fix repo A's staged file while repo B still has staged_b.rs with the violation.
    std::fs::write(repo_a.path().join("staged_a.rs"), "pub fn clean_a() {}\n").unwrap();
    std::process::Command::new("git")
        .args(["add", "staged_a.rs"])
        .current_dir(repo_a.path())
        .output()
        .unwrap();

    let out_clean = Command::cargo_bin("engrams")
        .unwrap()
        .env("ENGRAMS_NO_UPDATE_CHECK", "1")
        .current_dir(repo_b.path())
        .arg("--workspace")
        .arg(repo_a.path())
        .args(["check", "--staged"])
        .output()
        .unwrap();
    assert!(
        out_clean.status.success(),
        "repo B's staged file must not be checked when repo A is named"
    );
    let res_clean: Value = serde_json::from_slice(&out_clean.stdout).unwrap();
    assert_eq!(res_clean["files_checked"], 1);
    assert_eq!(res_clean["violations"].as_array().unwrap().len(), 0);
}

#[test]
fn workspace_refuses_git_only_operations_without_workspace_ac6() {
    let repo_b = TempDir::new().unwrap();
    let _ = init_git_repo_with_commit(repo_b.path(), "b.rs", "pub fn b() {}\n");
    std::fs::write(repo_b.path().join("b.rs"), "pub fn b2() {}\n").unwrap();
    std::process::Command::new("git")
        .args(["add", "b.rs"])
        .current_dir(repo_b.path())
        .output()
        .unwrap();

    let isolated = TempDir::new().unwrap();
    let db = isolated.path().join("copy.db");
    engrams(&db).arg("init").assert().success();

    let cases: &[(&[&str], &str)] = &[
        (
            &["check", "--staged"],
            "check --staged needs a workspace: pass --workspace <dir> or keep the database at <dir>/engrams/context.db",
        ),
        (
            &["advise", "--staged"],
            "advise --staged needs a workspace: pass --workspace <dir> or keep the database at <dir>/engrams/context.db",
        ),
        (
            &["relevant", "--staged"],
            "relevant --staged needs a workspace: pass --workspace <dir> or keep the database at <dir>/engrams/context.db",
        ),
        (
            &["coverage", "--diff", "HEAD"],
            "coverage --diff needs a workspace: pass --workspace <dir> or keep the database at <dir>/engrams/context.db",
        ),
        (
            &["install", "--harness", "omp", "--hooks"],
            "install --hooks needs a workspace: pass --workspace <dir> or keep the database at <dir>/engrams/context.db",
        ),
    ];

    for (args, expected_err) in cases {
        let out = engrams(&db)
            .current_dir(repo_b.path())
            .args(*args)
            .output()
            .unwrap();
        assert!(
            !out.status.success(),
            "expected {:?} to fail without workspace",
            args
        );
        let err: Value = serde_json::from_slice(&out.stderr).unwrap();
        assert_eq!(err["error"].as_str(), Some(*expected_err));
    }

    assert!(
        !repo_b.path().join(".git/hooks/pre-commit").exists(),
        "shell repository hooks must be left unchanged"
    );
}

#[test]
fn workspace_installs_hooks_only_into_named_workspace_ac8() {
    let repo_a = TempDir::new().unwrap();
    let repo_b = TempDir::new().unwrap();
    let _ = init_git_repo_with_commit(repo_a.path(), "a.rs", "pub fn a() {}\n");
    let _ = init_git_repo_with_commit(repo_b.path(), "b.rs", "pub fn b() {}\n");

    let out = Command::cargo_bin("engrams")
        .unwrap()
        .env("ENGRAMS_NO_UPDATE_CHECK", "1")
        .current_dir(repo_b.path())
        .arg("--workspace")
        .arg(repo_a.path())
        .args(["install", "--harness", "omp", "--hooks"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(
        repo_a.path().join(".git/hooks/pre-commit").exists(),
        "pre-commit hook must be written into repo A"
    );
    assert!(
        repo_a
            .path()
            .join(".omp/extensions/engrams-advisor/index.ts")
            .exists(),
        "omp extension must be written into repo A"
    );
    assert!(
        !repo_b.path().join(".git/hooks/pre-commit").exists(),
        "repo B's hooks must be unchanged"
    );
    assert!(
        !repo_b.path().join(".omp").exists(),
        "repo B must not receive .omp directory"
    );
}

#[test]
fn workspace_doctor_skips_stale_decisions_without_workspace() {
    let repo_a = TempDir::new().unwrap();
    let _ = init_git_repo_with_commit(repo_a.path(), "anchor.txt", "v1\n");
    let isolated = TempDir::new().unwrap();
    let db = isolated.path().join("copy.db");

    // Log decision with --workspace repo_a and --db copy.db so commit_sha is recorded.
    let out_log = engrams(&db)
        .arg("--workspace")
        .arg(repo_a.path())
        .args([
            "decision",
            "log",
            "--summary",
            "Anchored decision",
            "--contract",
            "v1",
            "--anchor",
            "anchor.txt",
        ])
        .output()
        .unwrap();
    assert!(out_log.status.success());

    // Modify and commit anchor.txt in repo_a so it would be stale if git were checked.
    std::fs::write(repo_a.path().join("anchor.txt"), "v2\n").unwrap();
    std::process::Command::new("git")
        .args(["add", "anchor.txt"])
        .current_dir(repo_a.path())
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["commit", "-m", "v2"])
        .current_dir(repo_a.path())
        .output()
        .unwrap();

    // Run doctor with only --db copy.db (no --workspace) from inside repo_a.
    let out_doc = engrams(&db)
        .current_dir(repo_a.path())
        .arg("doctor")
        .output()
        .unwrap();
    assert!(out_doc.status.success());
    let doc: Value = serde_json::from_slice(&out_doc.stdout).unwrap();
    assert_eq!(doc["stale_decisions"], serde_json::json!([]));
    assert_eq!(
        doc["skipped_checks"],
        serde_json::json!(["stale_decisions: no workspace"])
    );
    assert_eq!(doc["missing_anchor_paths"], serde_json::json!([]));
}

// ── Spec 0007 Phase 1: Reviewed Schema Proposals (AC-1, AC-2, AC-4, AC-7) ──

fn load_manuscript_book(db: &std::path::Path) {
    engrams(db).arg("init").assert().success();
    let fixture_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/manuscript-book.json"
    );
    let fixture: Value =
        serde_json::from_str(&std::fs::read_to_string(fixture_path).unwrap()).unwrap();
    let facts = &fixture["facts"];
    let conn = rusqlite::Connection::open(db).unwrap();

    for d in facts["decisions"].as_array().unwrap() {
        let id = d["id"].as_i64().unwrap();
        conn.execute(
            "INSERT INTO decisions (id, uuid, timestamp, summary, tags, status, archived) \
             VALUES (?1, ?2, ?3, ?4, ?5, 'active', 0)",
            rusqlite::params![
                id,
                format!("u-dec-{id}"),
                d["timestamp"].as_str().unwrap(),
                d["summary"].as_str().unwrap(),
                d["tags"].as_str(),
            ],
        )
        .unwrap();
    }
    for p in facts["patterns"].as_array().unwrap() {
        let id = p["id"].as_i64().unwrap();
        conn.execute(
            "INSERT INTO system_patterns (id, uuid, timestamp, name, description, tags, check_kind, check_expr, severity, archived) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0)",
            rusqlite::params![
                id,
                format!("u-pat-{id}"),
                p["timestamp"].as_str().unwrap(),
                p["name"].as_str().unwrap(),
                p["description"].as_str(),
                p["tags"].as_str(),
                p["check_kind"].as_str(),
                p["check_expr"].as_str(),
                p["severity"].as_str().unwrap_or("warn"),
            ],
        )
        .unwrap();
    }
    for g in facts["progress"].as_array().unwrap() {
        let id = g["id"].as_i64().unwrap();
        conn.execute(
            "INSERT INTO progress_entries (id, timestamp, status, description) \
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                id,
                g["timestamp"].as_str().unwrap(),
                g["status"].as_str().unwrap_or("Done"),
                g["description"].as_str().unwrap(),
            ],
        )
        .unwrap();
    }
    for c in facts["custom"].as_array().unwrap() {
        let id = c["id"].as_i64().unwrap();
        conn.execute(
            "INSERT INTO custom_data (id, timestamp, category, key, value) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                id,
                c["timestamp"].as_str().unwrap(),
                c["category"].as_str().unwrap(),
                c["key"].as_str().unwrap(),
                c["value"].as_str().unwrap(),
            ],
        )
        .unwrap();
    }
    for code in facts["code"].as_array().unwrap() {
        let id = code["id"].as_i64().unwrap();
        conn.execute(
            "INSERT INTO code_nodes (id, kind, path, symbol, first_seen, last_seen) \
             VALUES (?1, 'file', ?2, ?3, ?4, ?5)",
            rusqlite::params![
                id,
                code["path"].as_str().unwrap(),
                code["symbol"].as_str().unwrap_or(""),
                code["first_seen"].as_str().unwrap(),
                code["last_seen"].as_str().unwrap(),
            ],
        )
        .unwrap();
    }
    for a in facts["anchors"].as_array().unwrap() {
        conn.execute(
            "INSERT INTO item_anchors (item_type, item_id, path, timestamp) \
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                a["item_type"].as_str().unwrap(),
                a["item_id"].as_i64().unwrap(),
                a["path"].as_str().unwrap(),
                a["timestamp"].as_str().unwrap_or(""),
            ],
        )
        .unwrap();
    }
    for l in facts["links"].as_array().unwrap() {
        conn.execute(
            "INSERT INTO context_links (source_item_type, source_item_id, target_item_type, target_item_id, relationship_type, timestamp, origin) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'manual')",
            rusqlite::params![
                l["source_type"].as_str().unwrap(),
                l["source_id"].as_str().unwrap(),
                l["target_type"].as_str().unwrap(),
                l["target_id"].as_str().unwrap(),
                l["rel"].as_str().unwrap(),
                l["timestamp"].as_str().unwrap_or(""),
            ],
        )
        .unwrap();
    }
}

#[test]
fn test_proposal_ac1_records_pending_without_surfacing_as_schema() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("engrams").join("context.db");
    load_manuscript_book(&db);

    let out = engrams(&db)
        .args([
            "schema",
            "propose",
            "--name",
            "mara-broken-leader",
            "--statement",
            "Mara believes she is broken; that hurt carries with her as she grows into a leader",
            "--summary",
            "When writing Mara under pressure: she doubts herself aloud and credits others, even as her actions grow stronger",
            "--members",
            "decision:1,decision:2,decision:3,decision:4,decision:5,decision:6,decision:7,decision:8",
            "--scope-tags",
            "mara",
            "--scope-paths",
            "manuscript",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "propose failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let prop_json: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(prop_json["status"], "success");
    assert_eq!(prop_json["inserted"], true);
    assert_eq!(prop_json["proposal"]["id"], 1);
    assert_eq!(prop_json["proposal"]["name"], "mara-broken-leader");
    assert_eq!(
        prop_json["proposal"]["statement"],
        "Mara believes she is broken; that hurt carries with her as she grows into a leader"
    );
    assert_eq!(
        prop_json["proposal"]["summary"],
        "When writing Mara under pressure: she doubts herself aloud and credits others, even as her actions grow stronger"
    );
    assert_eq!(
        prop_json["proposal"]["scope"]["tags"],
        serde_json::json!(["mara"])
    );
    assert_eq!(
        prop_json["proposal"]["scope"]["paths"],
        serde_json::json!(["manuscript"])
    );
    assert_eq!(
        prop_json["proposal"]["evidence"],
        serde_json::json!({"notes": 8, "files": 8, "days": 8, "contrary": 0})
    );
    assert_eq!(
        prop_json["confirm_command"],
        "engrams schema confirm proposal:1"
    );
    assert_eq!(
        prop_json["reject_command"],
        "engrams schema reject proposal:1 --reason \"<why>\""
    );

    // Listed as pending in `schema proposals` with enriched members.
    let list_out = engrams(&db).args(["schema", "proposals"]).output().unwrap();
    assert!(list_out.status.success());
    let listed: Value = serde_json::from_slice(&list_out.stdout).unwrap();
    let proposals = listed["proposals"].as_array().unwrap();
    assert_eq!(proposals.len(), 1);
    assert_eq!(proposals[0]["id"], 1);
    assert_eq!(proposals[0]["status"], "proposed");
    assert_eq!(proposals[0]["members"].as_array().unwrap().len(), 8);
    assert_eq!(proposals[0]["truncated"], false);
    assert_eq!(
        proposals[0]["evidence"],
        serde_json::json!({"notes": 8, "files": 8, "days": 8, "contrary": 0})
    );

    // No schema exists for it, and no note gains a schema membership.
    let conn = rusqlite::Connection::open(&db).unwrap();
    let schema_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM schemas", [], |r| r.get(0))
        .unwrap();
    assert_eq!(schema_count, 0);
    let member_link_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM context_links WHERE relationship_type = 'member_of'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(member_link_count, 0);

    // Pre-edit advice and search return no schemas.
    let advise_out = engrams(&db)
        .args(["advise", "manuscript/ch01.md"])
        .output()
        .unwrap();
    assert!(advise_out.status.success());
    let advise_json: Value = serde_json::from_slice(&advise_out.stdout).unwrap();
    assert_eq!(advise_json["schemas"].as_array().unwrap().len(), 0);

    let query_out = engrams(&db)
        .args(["query", "mara broken leader"])
        .output()
        .unwrap();
    assert!(query_out.status.success());
    let query_json: Value = serde_json::from_slice(&query_out.stdout).unwrap();
    assert!(
        query_json
            .get("schemas")
            .and_then(|s| s.as_array())
            .is_none_or(|a| a.is_empty()),
        "pending proposal must not surface in query schemas"
    );
}

#[test]
fn test_proposal_ac2_refuses_each_failing_evidence_or_wording_check() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("engrams").join("context.db");
    load_manuscript_book(&db);

    // Seed a single-file, single-day trio (decisions 101..103) plus a superseded
    // decision (104) and an archived pattern (10) to test every check in isolation.
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        for id in 101..=103 {
            conn.execute(
                "INSERT INTO decisions (id, uuid, timestamp, summary, tags, status, archived) \
                 VALUES (?1, ?2, '2026-09-20T10:00:00Z', ?3, '[\"single\"]', 'active', 0)",
                rusqlite::params![
                    id,
                    format!("u-{id}"),
                    format!("Distinct note {id} on same day")
                ],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO item_anchors (item_type, item_id, path, timestamp) \
                 VALUES ('decision', ?1, 'manuscript/ch01.md', '2026-09-20T10:00:00Z')",
                rusqlite::params![id],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO decisions (id, uuid, timestamp, summary, tags, status, archived) \
             VALUES (104, 'u-104', '2026-09-20T10:00:00Z', 'Superseded note', '[]', 'superseded', 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO system_patterns (id, uuid, timestamp, name, description, archived) \
             VALUES (10, 'u-p10', '2026-09-20T10:00:00Z', 'Archived rule', 'old', 1)",
            [],
        )
        .unwrap();
    }

    let cases: &[(&[&str], &str)] = &[
        // 1. Fewer than three notes
        (
            &[
                "schema",
                "propose",
                "--statement",
                "Synthesized claim about Mara",
                "--summary",
                "When writing Mara: keep her dialogue understated",
                "--members",
                "decision:1,decision:2",
            ],
            "proposal needs at least 3 notes (got 2)",
        ),
        // 2. Notes on one file and one day
        (
            &[
                "schema",
                "propose",
                "--statement",
                "Synthesized claim about same day notes",
                "--summary",
                "When writing chapter one: keep pacing tight",
                "--members",
                "decision:101,decision:102,decision:103",
            ],
            "cited notes must span 2 files or 2 days",
        ),
        // 3. Missing note
        (
            &[
                "schema",
                "propose",
                "--statement",
                "Synthesized claim about Mara",
                "--summary",
                "When writing Mara: keep her dialogue understated",
                "--members",
                "decision:1,decision:2,decision:99",
            ],
            "decision:99 does not exist",
        ),
        // 4. Retired / superseded note
        (
            &[
                "schema",
                "propose",
                "--statement",
                "Synthesized claim about Mara",
                "--summary",
                "When writing Mara: keep her dialogue understated",
                "--members",
                "decision:1,decision:2,decision:104",
            ],
            "decision:104 is not current",
        ),
        // 5. Code file cited as a note
        (
            &[
                "schema",
                "propose",
                "--statement",
                "Synthesized claim about Mara",
                "--summary",
                "When writing Mara: keep her dialogue understated",
                "--members",
                "decision:1,decision:2,code:14",
            ],
            "code:14 is a file, not a note: cite notes and put files in --scope-paths",
        ),
        // 6a. Counter-evidence note that is missing
        (
            &[
                "schema",
                "propose",
                "--statement",
                "Synthesized claim about Mara",
                "--summary",
                "When writing Mara: keep her dialogue understated",
                "--members",
                "decision:1,decision:2,decision:3",
                "--contrary",
                "decision:999",
            ],
            "decision:999 does not exist",
        ),
        // 6b. Counter-evidence note that is retired (archived pattern)
        (
            &[
                "schema",
                "propose",
                "--statement",
                "Synthesized claim about Mara",
                "--summary",
                "When writing Mara: keep her dialogue understated",
                "--members",
                "decision:1,decision:2,decision:3",
                "--contrary",
                "system-pattern:10",
            ],
            "system_pattern:10 is not current",
        ),
        // 6c. Counter-evidence note also cited as evidence
        (
            &[
                "schema",
                "propose",
                "--statement",
                "Synthesized claim about Mara",
                "--summary",
                "When writing Mara: keep her dialogue understated",
                "--members",
                "decision:1,decision:2,decision:3",
                "--contrary",
                "decision:2",
            ],
            "decision:2 is cited as both evidence and contrary",
        ),
        // 7. Claim that repeats one cited note
        (
            &[
                "schema",
                "propose",
                "--statement",
                "Mara steadies panicking recruits on the parapet while privately expecting her own nerve to give way",
                "--summary",
                "When writing Mara: keep her dialogue understated",
                "--members",
                "decision:1,decision:2,decision:7",
            ],
            "statement repeats decision:7 (overlap 1.00 >= 0.8)",
        ),
        // 8. Instruction without a trigger phrase
        (
            &[
                "schema",
                "propose",
                "--statement",
                "Synthesized claim about Mara",
                "--summary",
                "She doubts herself aloud and credits others",
                "--members",
                "decision:1,decision:2,decision:3",
            ],
            "summary must begin with a conditional trigger phrase",
        ),
    ];

    for (args, expected_err) in cases {
        let out = engrams(&db).args(*args).output().unwrap();
        assert!(
            !out.status.success(),
            "expected failure for {args:?}, got stdout: {}",
            String::from_utf8_lossy(&out.stdout)
        );
        let err_json: Value = serde_json::from_slice(&out.stderr).unwrap();
        let msg = err_json["error"].as_str().unwrap_or("");
        assert!(
            msg.contains(expected_err),
            "expected error containing '{expected_err}', got '{msg}'"
        );
    }

    let conn = rusqlite::Connection::open(&db).unwrap();
    let prop_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM schema_proposals", [], |r| r.get(0))
        .unwrap();
    assert_eq!(prop_count, 0, "no proposal row should be recorded");
}

#[test]
fn test_proposal_ac4_refuses_pending_rejected_and_confirmed_territory() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("engrams").join("context.db");
    load_manuscript_book(&db);

    // 1. Create pending proposal #1 over decisions 1,2,3,4
    let out1 = engrams(&db)
        .args([
            "schema",
            "propose",
            "--name",
            "mara-wound",
            "--statement",
            "Mara believes she is broken and hides behind her squad",
            "--summary",
            "When writing Mara in early chapters: show her deflecting credit",
            "--members",
            "decision:1,decision:2,decision:3,decision:4",
        ])
        .output()
        .unwrap();
    assert!(out1.status.success());

    // 2. Proposing mostly the same notes (decisions 1,2,3 -> overlap 3/4 = 0.75 >= 0.7)
    // returns inserted: false with `similar`.
    let dup_out = engrams(&db)
        .args([
            "schema",
            "propose",
            "--statement",
            "Another claim covering the same Mara notes",
            "--summary",
            "When writing Mara: show her deflecting credit",
            "--members",
            "decision:1,decision:2,decision:3",
        ])
        .output()
        .unwrap();
    assert!(dup_out.status.success());
    let dup_json: Value = serde_json::from_slice(&dup_out.stdout).unwrap();
    assert_eq!(dup_json["inserted"], false);
    assert_eq!(dup_json["similar"][0]["id"], 1);
    assert_eq!(dup_json["similar"][0]["overlap"], 0.75);

    // 3. Reject proposal:1 with a reason.
    let rej_out = engrams(&db)
        .args([
            "schema",
            "reject",
            "proposal:1",
            "--reason",
            "Wait until Part II is drafted",
        ])
        .output()
        .unwrap();
    assert!(rej_out.status.success());
    let rej_json: Value = serde_json::from_slice(&rej_out.stdout).unwrap();
    assert_eq!(rej_json["proposal"]["id"], 1);
    assert_eq!(rej_json["proposal"]["status"], "rejected");
    assert_eq!(
        rej_json["proposal"]["decision_note"],
        "Wait until Part II is drafted"
    );
    let decided_at = rej_json["proposal"]["decided_at"]
        .as_str()
        .unwrap()
        .to_string();

    // Rejecting a non-pending proposal is an error.
    let rej_again = engrams(&db)
        .args(["schema", "reject", "proposal:1", "--reason", "duplicate"])
        .output()
        .unwrap();
    assert!(!rej_again.status.success());
    let rej_err: Value = serde_json::from_slice(&rej_again.stderr).unwrap();
    assert!(rej_err["error"]
        .as_str()
        .unwrap()
        .contains("proposal 1 is already rejected"));

    // 4. Proposing the rejected territory with no note newer than decided_at
    // returns inserted: false with `rejected_before` naming id, decided_at, and reason.
    let blocked_out = engrams(&db)
        .args([
            "schema",
            "propose",
            "--statement",
            "Another claim covering the rejected Mara notes",
            "--summary",
            "When writing Mara: show her deflecting credit",
            "--members",
            "decision:1,decision:2,decision:3,decision:4",
        ])
        .output()
        .unwrap();
    assert!(blocked_out.status.success());
    let blocked_json: Value = serde_json::from_slice(&blocked_out.stdout).unwrap();
    assert_eq!(blocked_json["inserted"], false);
    assert_eq!(blocked_json["rejected_before"][0]["id"], 1);
    assert_eq!(blocked_json["rejected_before"][0]["decided_at"], decided_at);
    assert_eq!(
        blocked_json["rejected_before"][0]["reason"],
        "Wait until Part II is drafted"
    );
    assert_eq!(blocked_json["rejected_before"][0]["overlap"], 1.0);

    // 5. Seed an active confirmed schema (id 3, "mara-arc") covering decisions 13, 14, 15, 16
    // plus a pre-v16 code member (code:1), and verify Check 9 refuses the proposal even with --force.
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute(
            "INSERT INTO schemas (id, uuid, name, summary, summary_source, status, centroid_json, created_at, updated_at, kind) \
             VALUES (3, 'u-s3', 'mara-arc', 'When channeling aether: pay salt-iron cost', 'agent', 'active', '{}', '2026-09-01T00:00:00Z', '2026-09-01T00:00:00Z', 'schema')",
            [],
        )
        .unwrap();
        for (kind, id) in [
            ("decision", "13"),
            ("decision", "14"),
            ("decision", "15"),
            ("decision", "16"),
            ("code", "1"),
        ] {
            conn.execute(
                "INSERT INTO context_links (source_item_type, source_item_id, target_item_type, target_item_id, relationship_type, timestamp, origin) \
                 VALUES (?1, ?2, 'schema', '3', 'member_of', '2026-09-01T00:00:00Z', 'manual')",
                rusqlite::params![kind, id],
            )
            .unwrap();
        }
    }

    let territory_out = engrams(&db)
        .args([
            "schema",
            "propose",
            "--statement",
            "Aether draws chill iron and require salt-iron grounding rings",
            "--summary",
            "When writing aether casting: show frost and grounding",
            "--members",
            "decision:13,decision:14,decision:15",
            "--force",
        ])
        .output()
        .unwrap();
    assert!(!territory_out.status.success());
    let territory_err: Value = serde_json::from_slice(&territory_out.stderr).unwrap();
    assert_eq!(
        territory_err["error"],
        "notes already covered by schema 3 (mara-arc)"
    );

    // Verify only proposal:1 exists in schema_proposals.
    let conn = rusqlite::Connection::open(&db).unwrap();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM schema_proposals", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn test_proposal_ac7_seeds_from_detector_group_drops_code_and_snapshots_evidence() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("engrams").join("context.db");
    load_manuscript_book(&db);

    // Stage a detector candidate containing knowledge notes AND code files.
    let initial_sig = "code:6,code:9,decision:17,decision:18,decision:19";
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute(
            "INSERT INTO schema_candidates \
             (cluster_sig, member_keys_json, density, stability_count, reward_hits, first_seen_at, last_seen_at, kind) \
             VALUES (?1, ?2, 0.85, 3, 1, '2026-09-01T00:00:00Z', '2026-09-10T00:00:00Z', 'schema')",
            rusqlite::params![
                initial_sig,
                r#"["code:6","code:9","decision:17","decision:18","decision:19"]"#
            ],
        )
        .unwrap();
    }

    // Propose from a unique prefix of the candidate signature.
    let out = engrams(&db)
        .args([
            "schema",
            "propose",
            "--from",
            "code:6,code:9",
            "--statement",
            "Winter siege survival depends on strict barley, water, and axle-rest rations",
            "--trigger",
            "writing garrison supply scenes",
            "--rule",
            "check the forty-two-day grain math, third-night well brigades, and four-day mule rest",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "propose --from failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let prop: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(prop["inserted"], true);
    assert_eq!(
        prop["proposal"]["members"],
        serde_json::json!(["decision:17", "decision:18", "decision:19"])
    );
    assert_eq!(
        prop["dropped_code_members"],
        serde_json::json!(["code:6", "code:9"])
    );
    assert_eq!(prop["proposal"]["source"]["kind"], "candidate");
    assert_eq!(prop["proposal"]["source"]["ref"], initial_sig);

    // Later scans that mutate or replace schema_candidates leave the proposal's
    // evidence and source_ref snapshot untouched.
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute(
            "UPDATE schema_candidates SET cluster_sig = 'decision:17,decision:18,decision:19,decision:20', \
             member_keys_json = '[\"decision:17\",\"decision:18\",\"decision:19\",\"decision:20\"]' \
             WHERE cluster_sig = ?1",
            rusqlite::params![initial_sig],
        )
        .unwrap();
    }
    engrams(&db).args(["schema", "scan"]).assert().success();

    let list_out = engrams(&db).args(["schema", "proposals"]).output().unwrap();
    assert!(list_out.status.success());
    let listed: Value = serde_json::from_slice(&list_out.stdout).unwrap();
    let p = &listed["proposals"][0];
    assert_eq!(p["member_count"], 3);
    let member_ids: Vec<i64> = p["members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_i64().unwrap())
        .collect();
    assert_eq!(member_ids, vec![17, 18, 19]);
    assert_eq!(p["source"]["kind"], "candidate");
    assert_eq!(p["source"]["ref"], initial_sig);
}
