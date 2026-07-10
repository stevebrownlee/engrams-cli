use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;
use serde_json::Value;

fn engrams(db_path: &std::path::Path) -> Command {
    let mut cmd = Command::cargo_bin("engrams").unwrap();
    cmd.arg("--db").arg(db_path);
    cmd
}

#[test]
fn test_init() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    // First init
    engrams(&db).arg("init")
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""created":true"#).or(predicate::str::contains(r#""created": true"#)));

    // Second init
    engrams(&db).arg("init")
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""created":false"#).or(predicate::str::contains(r#""created": false"#)));
}

#[test]
fn test_decision_lifecycle() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    // Log
    let output = engrams(&db)
        .args(&["decision", "log", "--summary", "Use Rust", "--rationale", "Speed", "--tags", "lang,arch"])
        .assert()
        .success()
        .get_output()
        .stdout.clone();
    
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
        .args(&["decision", "update", &id.to_string(), "--summary", "Use Rust CLI"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Use Rust CLI"));

    // Delete
    engrams(&db)
        .args(&["decision", "delete", &id.to_string()])
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""deleted":true"#).or(predicate::str::contains(r#""deleted": true"#)));

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

    let out1 = engrams(&db).args(&["progress", "log", "--status", "Open", "--description", "Task 1"]).output().unwrap();
    let p1: Value = serde_json::from_slice(&out1.stdout).unwrap();
    let id1 = p1["id"].as_i64().unwrap();

    let out2 = engrams(&db).args(&["progress", "log", "--status", "Done", "--description", "Task 2", "--parent-id", &id1.to_string()]).output().unwrap();
    let p2: Value = serde_json::from_slice(&out2.stdout).unwrap();
    let id2 = p2["id"].as_i64().unwrap();

    assert_eq!(p2["parent_id"].as_i64().unwrap(), id1);

    // List by parent
    engrams(&db).args(&["progress", "list", "--parent-id", &id1.to_string()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Task 2"));
}

#[test]
fn test_pattern_upsert() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    let out1 = engrams(&db).args(&["pattern", "log", "--name", "MVC", "--description", "desc 1"]).output().unwrap();
    let p1: Value = serde_json::from_slice(&out1.stdout).unwrap();
    let id1 = p1["id"].as_i64().unwrap();

    let out2 = engrams(&db).args(&["pattern", "log", "--name", "MVC", "--description", "desc 2"]).output().unwrap();
    let p2: Value = serde_json::from_slice(&out2.stdout).unwrap();
    let id2 = p2["id"].as_i64().unwrap();

    assert_eq!(id1, id2);
    assert_eq!(p2["description"].as_str().unwrap(), "desc 2");
}

#[test]
fn test_custom_data() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    engrams(&db).args(&["custom", "set", "--category", "conf", "--key", "port", "--value", "8080"])
        .assert().success()
        .stdout(predicate::str::contains(r#""value":"8080""#).or(predicate::str::contains(r#""value": "8080""#)));

    engrams(&db).args(&["custom", "set", "--category", "conf", "--key", "port", "--value", "8080", "--json"])
        .assert().success()
        .stdout(predicate::str::contains(r#""value":8080"#).or(predicate::str::contains(r#""value": 8080"#)));

    engrams(&db).args(&["custom", "get", "--category", "conf", "--key", "port"])
        .assert().success()
        .stdout(predicate::str::contains("8080"));

    engrams(&db).args(&["custom", "search", "8080"])
        .assert().success()
        .stdout(predicate::str::contains("8080"));

    engrams(&db).args(&["custom", "delete", "--category", "conf", "--key", "port"])
        .assert().success()
        .stdout(predicate::str::contains("deleted"));
}

#[test]
fn test_product_context() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    engrams(&db).args(&["product-context", "update", "--content", "{\"stack\":\"rust\"}"])
        .assert().success();

    engrams(&db).args(&["product-context", "update", "--patch", "{\"stack\":\"__DELETE__\",\"db\":\"sqlite\"}"])
        .assert().success();

    let out = engrams(&db).args(&["product-context", "get"]).output().unwrap();
    let doc: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(doc["version"].as_i64().unwrap(), 2);
    assert_eq!(doc["content"]["db"].as_str().unwrap(), "sqlite");
    assert!(doc["content"]["stack"].is_null());

    let hist = engrams(&db).args(&["history", "product-context"]).output().unwrap();
    let h: Value = serde_json::from_slice(&hist.stdout).unwrap();
    assert_eq!(h[0]["version"].as_i64().unwrap(), 1);
    assert_eq!(h[0]["content"]["stack"].as_str().unwrap(), "rust");
}

#[test]
fn test_links() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    let out1 = engrams(&db).args(&["decision", "log", "--summary", "d1"]).output().unwrap();
    let id1 = serde_json::from_slice::<Value>(&out1.stdout).unwrap()["id"].as_i64().unwrap();

    let out2 = engrams(&db).args(&["decision", "log", "--summary", "d2"]).output().unwrap();
    let id2 = serde_json::from_slice::<Value>(&out2.stdout).unwrap()["id"].as_i64().unwrap();

    engrams(&db).args(&["link", "add", "--source-type", "decision", "--source-id", &id1.to_string(), "--target-type", "decision", "--target-id", &id2.to_string(), "--rel", "blocks"])
        .assert().success();

    engrams(&db).args(&["link", "list", "--item-type", "decision", "--item-id", &id1.to_string()])
        .assert().success()
        .stdout(predicate::str::contains("outgoing"));

    // delete id1, link should be removed
    let del = engrams(&db).args(&["decision", "delete", &id1.to_string()]).output().unwrap();
    let d: Value = serde_json::from_slice(&del.stdout).unwrap();
    assert_eq!(d["links_removed"].as_i64().unwrap(), 1);
}

#[test]
fn test_activity() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    engrams(&db).args(&["decision", "log", "--summary", "act d1"]).output().unwrap();
    engrams(&db).args(&["progress", "log", "--status", "O", "--description", "act p1"]).output().unwrap();

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

    engrams(&db1).args(&["decision", "log", "--summary", "exp_d1"]).output().unwrap();
    engrams(&db1).args(&["export", "--path", exp_dir.to_str().unwrap()]).assert().success();

    engrams(&db2).args(&["import", "--path", exp_dir.to_str().unwrap()]).assert().success();

    engrams(&db2).args(&["decision", "list"]).assert().success()
        .stdout(predicate::str::contains("exp_d1"));
}

#[test]
fn test_migrate_command() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    // Run migrate on non-existent db (creates it)
    engrams(&db).arg("migrate").assert().success()
        .stdout(predicate::str::contains(r#""status":"success""#).or(predicate::str::contains(r#""status": "success""#)));

    // Run migrate again (noop)
    engrams(&db).arg("migrate").assert().success()
        .stdout(predicate::str::contains(r#""status":"success""#).or(predicate::str::contains(r#""status": "success""#)));
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
    let out = engrams(&db).arg("decision").arg("list")
        .output().unwrap();
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
        let ver: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0)).unwrap();
        assert_eq!(ver, 1);
    }
}
