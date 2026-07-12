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
    let id2 = p2["id"].as_i64().unwrap();

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

    // Run a command, it should succeed and auto-upgrade version to 1
    engrams(&db).arg("decision").arg("list").assert().success();

    // Verify it was upgraded back to 1
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        let ver: i32 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(ver, 1);
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
