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
        .args(&[
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
        .args(&["decision", "get", &id.to_string()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Use Rust"));

    // List
    engrams(&db)
        .args(&["decision", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Use Rust"));

    // Search
    engrams(&db)
        .args(&["decision", "search", "Speed"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Use Rust"));

    // Update
    engrams(&db)
        .args(&[
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
        .args(&["decision", "delete", &id.to_string()])
        .assert()
        .success()
        .stdout(
            predicate::str::contains(r#""deleted":true"#)
                .or(predicate::str::contains(r#""deleted": true"#)),
        );

    // Get (not found)
    engrams(&db)
        .args(&["decision", "get", &id.to_string()])
        .assert()
        .failure();
}

#[test]
fn test_progress_linkage() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    let out1 = engrams(&db)
        .args(&[
            "progress",
            "log",
            "--status",
            "Open",
            "--description",
            "Task 1",
        ])
        .output()
        .unwrap();
    let p1: Value = serde_json::from_slice(&out1.stdout).unwrap();
    let id1 = p1["id"].as_i64().unwrap();

    let out2 = engrams(&db)
        .args(&[
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
        .args(&["progress", "list", "--parent-id", &id1.to_string()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Task 2"));
}

#[test]
fn test_pattern_upsert() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    let out1 = engrams(&db)
        .args(&["pattern", "log", "--name", "MVC", "--description", "desc 1"])
        .output()
        .unwrap();
    let p1: Value = serde_json::from_slice(&out1.stdout).unwrap();
    let id1 = p1["id"].as_i64().unwrap();

    let out2 = engrams(&db)
        .args(&["pattern", "log", "--name", "MVC", "--description", "desc 2"])
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
        .args(&[
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
        .args(&[
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
        .args(&["custom", "get", "--category", "conf", "--key", "port"])
        .assert()
        .success()
        .stdout(predicate::str::contains("8080"));

    engrams(&db)
        .args(&["custom", "search", "8080"])
        .assert()
        .success()
        .stdout(predicate::str::contains("8080"));

    engrams(&db)
        .args(&["custom", "delete", "--category", "conf", "--key", "port"])
        .assert()
        .success()
        .stdout(predicate::str::contains("deleted"));
}

#[test]
fn test_product_context() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    engrams(&db)
        .args(&[
            "product-context",
            "update",
            "--content",
            "{\"stack\":\"rust\"}",
        ])
        .assert()
        .success();

    engrams(&db)
        .args(&[
            "product-context",
            "update",
            "--patch",
            "{\"stack\":\"__DELETE__\",\"db\":\"sqlite\"}",
        ])
        .assert()
        .success();

    let out = engrams(&db)
        .args(&["product-context", "get"])
        .output()
        .unwrap();
    let doc: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(doc["version"].as_i64().unwrap(), 2);
    assert_eq!(doc["content"]["db"].as_str().unwrap(), "sqlite");
    assert!(doc["content"]["stack"].is_null());

    let hist = engrams(&db)
        .args(&["history", "product-context"])
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
        .args(&["decision", "log", "--summary", "d1"])
        .output()
        .unwrap();
    let id1 = serde_json::from_slice::<Value>(&out1.stdout).unwrap()["id"]
        .as_i64()
        .unwrap();

    let out2 = engrams(&db)
        .args(&["decision", "log", "--summary", "d2"])
        .output()
        .unwrap();
    let id2 = serde_json::from_slice::<Value>(&out2.stdout).unwrap()["id"]
        .as_i64()
        .unwrap();

    engrams(&db)
        .args(&[
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
        .args(&[
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
        .args(&["decision", "delete", &id1.to_string()])
        .output()
        .unwrap();
    let d: Value = serde_json::from_slice(&del.stdout).unwrap();
    assert_eq!(d["links_removed"].as_i64().unwrap(), 1);
}

#[test]
fn test_activity() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    engrams(&db)
        .args(&["decision", "log", "--summary", "act d1"])
        .output()
        .unwrap();
    engrams(&db)
        .args(&[
            "progress",
            "log",
            "--status",
            "O",
            "--description",
            "act p1",
        ])
        .output()
        .unwrap();

    let out = engrams(&db).args(&["activity"]).output().unwrap();
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
        .args(&["decision", "log", "--summary", "exp_d1"])
        .output()
        .unwrap();
    engrams(&db1)
        .args(&["export", "--path", exp_dir.to_str().unwrap()])
        .assert()
        .success();

    engrams(&db2)
        .args(&["import", "--path", exp_dir.to_str().unwrap()])
        .assert()
        .success();

    engrams(&db2)
        .args(&["decision", "list"])
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

    // Verify it was upgraded back to 2
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        let ver: i32 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(ver, 2);
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

    // Verify columns exist by checking PRAGMA user_version is 2
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        let ver: i32 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(ver, 2);
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
        .args(&["decision", "log", "--summary", "Use SQLite"])
        .output()
        .unwrap();
    engrams(&db)
        .args(&[
            "progress",
            "log",
            "--status",
            "D",
            "--description",
            "Set up DB",
        ])
        .output()
        .unwrap();
    engrams(&db)
        .args(&[
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
    let out = engrams(&db).args(&["report"]).output().unwrap();
    let r: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(r["active_context"].is_null() || r["active_context"].is_object());
    assert_eq!(r["decisions"].as_array().unwrap().len(), 1);
    assert_eq!(r["progress"].as_array().unwrap().len(), 1);
    assert_eq!(r["patterns"].as_array().unwrap().len(), 1);
    assert!(r["links"].as_array().unwrap().is_empty());

    // Topic report (JSON)
    let out = engrams(&db)
        .args(&["report", "decisions"])
        .output()
        .unwrap();
    let r: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(r.is_array());
    assert_eq!(r.as_array().unwrap().len(), 1);

    // Human formatting flag should now fail since it was removed
    engrams(&db)
        .args(&["--format", "human", "report"])
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
        .args(&["decision", "log", "--summary", "Use SQLite"])
        .output()
        .unwrap();
    engrams(&db)
        .args(&[
            "progress",
            "log",
            "--status",
            "D",
            "--description",
            "Set up DB",
        ])
        .output()
        .unwrap();
    engrams(&db)
        .args(&[
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
        .args(&[
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
        .args(&[
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
        .args(&[
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
        .args(&[
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
        .args(&[
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
        .args(&[
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
        .args(&[
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
        .args(&[
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
        .args(&[
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
        .args(&[
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
        .args(&["decision", "get", &id2.to_string()])
        .assert()
        .failure();

    // Consolidating into self should fail
    engrams(&db)
        .args(&[
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
        .args(&[
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
        .args(&[
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
        .args(&[
            "progress",
            "log",
            "--status",
            "done",
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
        .args(&[
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
        .args(&[
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
        .args(&[
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
        .args(&[
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
        .args(&["pr", "list", "--type", "decision", "--id", &id.to_string()])
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
        .args(&[
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
        .args(&["decision", "get", &id.to_string()])
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
        .args(&["remote", "add", "origin", "git@github.com:acme/w.git"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    // Log a decision with a numeric PR ID, passing custom current_dir
    let mut cmd = engrams(&db);
    cmd.current_dir(repo_path).args(&[
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
        .args(&[
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
        .args(&["relevant", "src/db.rs"])
        .output()
        .unwrap();
    let j2: Value = serde_json::from_slice(&out2.stdout).unwrap();
    let matched_decisions = j2["decisions"].as_array().unwrap();
    assert_eq!(matched_decisions.len(), 1);
    assert_eq!(matched_decisions[0]["id"].as_i64().unwrap(), id);

    // Query relevant for src (parent dir of anchor)
    let out3 = engrams(&db).args(&["relevant", "src"]).output().unwrap();
    let j3: Value = serde_json::from_slice(&out3.stdout).unwrap();
    assert_eq!(j3["decisions"].as_array().unwrap().len(), 1);

    // Query relevant for other.rs (no match)
    let out4 = engrams(&db)
        .args(&["relevant", "other.rs"])
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
        .args(&["decision", "log", "--summary", "First Decision", "--force"])
        .output()
        .unwrap();
    let j1: Value = serde_json::from_slice(&out1.stdout).unwrap();
    let id1 = j1["id"].as_i64().unwrap();

    // Log decision 2
    let out2 = engrams(&db)
        .args(&["decision", "log", "--summary", "Second Decision", "--force"])
        .output()
        .unwrap();
    let j2: Value = serde_json::from_slice(&out2.stdout).unwrap();
    let id2 = j2["id"].as_i64().unwrap();

    // Superseding self should fail
    engrams(&db)
        .args(&[
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
        .args(&[
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
    let out_list1 = engrams(&db).args(&["decision", "list"]).output().unwrap();
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
        .args(&["decision", "list", "--all"])
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
        .args(&["decision", "get", &id1.to_string()])
        .output()
        .unwrap();
    let j_get: Value = serde_json::from_slice(&out_get.stdout).unwrap();
    assert_eq!(j_get["status"].as_str().unwrap(), "superseded");

    // link list shows the supersedes edge
    let out_link = engrams(&db)
        .args(&[
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
        .args(&["decision", "update", &id1.to_string(), "--status", "active"])
        .output()
        .unwrap();
    let j_update: Value = serde_json::from_slice(&out_update.stdout).unwrap();
    assert!(j_update.get("status").is_none());

    // decision list now includes #1 again
    let out_list3 = engrams(&db).args(&["decision", "list"]).output().unwrap();
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
        .args(&["active-context", "update", "--content", "{\"tasks\": []}"])
        .assert()
        .success();

    // Seed 12 decisions
    for i in 1..=12 {
        engrams(&db)
            .args(&[
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
            .args(&["pattern", "log", "--name", &format!("Pattern {}", i)])
            .assert()
            .success();
    }

    // Seed 2 progress entries
    for i in 1..=2 {
        engrams(&db)
            .args(&[
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
    let out = engrams(&db).args(&["prime"]).output().unwrap();
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
        .args(&["prime", "--budget", "10"])
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

    // 1. Clean DB
    let out = engrams(&db).arg("doctor").output().unwrap();
    let j: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(j["ok"].as_bool().unwrap());

    // 2. Initialize git repo, commit a file
    std::process::Command::new("git")
        .arg("init")
        .current_dir(repo_path)
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(&["config", "user.name", "Test User"])
        .current_dir(repo_path)
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(&["config", "user.email", "test@example.com"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    let file_path = repo_path.join("anchor.txt");
    std::fs::write(&file_path, "original content").unwrap();

    std::process::Command::new("git")
        .args(&["add", "anchor.txt"])
        .current_dir(repo_path)
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(&["commit", "-m", "initial commit"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    // Log decision with anchor inside the repo
    let mut cmd = engrams(&db);
    cmd.current_dir(repo_path).args(&[
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
    cmd_doc.current_dir(repo_path).arg("doctor");
    let out_doc = cmd_doc.output().unwrap();
    let j_doc: Value = serde_json::from_slice(&out_doc.stdout).unwrap();
    assert!(!j_doc["ok"].as_bool().unwrap());
    let unlinked = j_doc["unlinked_decisions"].as_array().unwrap();
    assert_eq!(unlinked.len(), 1);
    assert_eq!(unlinked[0]["id"].as_i64().unwrap(), id);

    // 4. Test missing_anchor_paths (anchor to non-existent path)
    let out_anchor = engrams(&db)
        .args(&[
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
    cmd_doc2.current_dir(repo_path).arg("doctor");
    let out_doc2 = cmd_doc2.output().unwrap();
    let j_doc2: Value = serde_json::from_slice(&out_doc2.stdout).unwrap();
    let missing = j_doc2["missing_anchor_paths"].as_array().unwrap();
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0]["path"].as_str().unwrap(), "nonexistent.txt");

    // 5. Test stale_decisions (modify+commit the anchored file)
    std::fs::write(&file_path, "modified content").unwrap();
    std::process::Command::new("git")
        .args(&["add", "anchor.txt"])
        .current_dir(repo_path)
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(&["commit", "-m", "modified anchor"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    let mut cmd_doc3 = engrams(&db);
    cmd_doc3.current_dir(repo_path).arg("doctor");
    let out_doc3 = cmd_doc3.output().unwrap();
    let j_doc3: Value = serde_json::from_slice(&out_doc3.stdout).unwrap();
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
        .args(&["decision", "log", "--summary", "Compact Test Decision"])
        .output()
        .unwrap();
    let j: Value = serde_json::from_slice(&out.stdout).unwrap();
    let id = j["id"].as_i64().unwrap();

    // 1. Test --compact decision get <id>
    let out_compact = engrams(&db)
        .args(&["--compact", "decision", "get", &id.to_string()])
        .output()
        .unwrap();
    let stdout_str = String::from_utf8(out_compact.stdout).unwrap();
    assert_eq!(stdout_str.trim().lines().count(), 1);
    assert!(!stdout_str.contains("null"));

    // 2. Test --fields id,summary decision list
    let out_fields = engrams(&db)
        .args(&["--fields", "id,summary", "decision", "list"])
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
        .args(&["decision", "search", "Compact", "--snippets"])
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
    assert!(stdout_str.starts_with("## Memory & Project Context (engrams)"));
    assert_eq!(out.status.code(), Some(0));

    assert!(!temp.path().join("nonexistent_dir").exists());
}

#[test]
fn test_query() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    // 1. Seed decision
    let out1 = engrams(&db)
        .args(&[
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
        .args(&[
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
        .args(&[
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
    let out_q = engrams(&db).args(&["query", "token"]).output().unwrap();
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
        .args(&["query", "token", "--types", "decision"])
        .output()
        .unwrap();
    let j_types: Value = serde_json::from_slice(&out_types.stdout).unwrap();
    let results_types = j_types.as_array().unwrap();
    assert_eq!(results_types.len(), 1);
    assert_eq!(results_types[0]["type"].as_str().unwrap(), "decision");

    // Test --tags sql
    let out_tags = engrams(&db)
        .args(&["query", "token", "--tags", "sql"])
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
        .args(&["decision", "supersede", &dec_id.to_string()])
        .assert()
        .success();

    let out_q2 = engrams(&db).args(&["query", "token"]).output().unwrap();
    let j_q2: Value = serde_json::from_slice(&out_q2.stdout).unwrap();
    let results2 = j_q2.as_array().unwrap();
    assert_eq!(results2.len(), 2);
    let has_dec2 = results2
        .iter()
        .any(|r| r["type"].as_str().unwrap() == "decision");
    assert!(!has_dec2);

    // With --all, it should find the superseded decision too
    let out_all = engrams(&db)
        .args(&["query", "token", "--all"])
        .output()
        .unwrap();
    let j_all: Value = serde_json::from_slice(&out_all.stdout).unwrap();
    let results_all = j_all.as_array().unwrap();
    assert_eq!(results_all.len(), 3);
}
