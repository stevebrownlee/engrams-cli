//! Integration tests for the policy engine spec — specs/policy-engine.md §7 (S1–S10).
//!
//! S11 (dogfood) — patterns 002–005 are registered in the DB and exported to
//! `.omp/rules/`; the rule files were validated against the omp `Rule` schema.
//! Live TTSR firing inside an omp session was not independently observed.

use assert_cmd::Command;
use serde_json::Value;
use std::fs;
use tempfile::TempDir;

fn engrams(db_path: &std::path::Path) -> Command {
    let mut cmd = Command::cargo_bin("engrams").unwrap();
    cmd.arg("--db").arg(db_path);
    cmd.env("ENGRAMS_NO_UPDATE_CHECK", "1");
    cmd
}

/// Set up a workspace whose db lives at `<temp>/engrams/context.db` so that
/// `workspace_root_from_db` resolves to `<temp>`. Returns (temp, db_path).
fn workspace() -> (TempDir, std::path::PathBuf) {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("engrams").join("context.db");
    (temp, db)
}

// ── S1: Migration / fresh schema ──────────────────────────────────────────

#[test]
fn s1_fresh_db_has_v5_schema_with_check_columns() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    engrams(&db).arg("init").assert().success();

    let conn = rusqlite::Connection::open(&db).unwrap();
    let v: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(v, 6);

    // The three new columns must exist.
    let cols: Vec<String> = conn
        .prepare("PRAGMA table_info(system_patterns)")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .map(|c| c.unwrap())
        .collect();
    assert!(cols.contains(&"check_kind".to_string()));
    assert!(cols.contains(&"check_expr".to_string()));
    assert!(cols.contains(&"severity".to_string()));
}

// ── S2: Log-time validation ───────────────────────────────────────────────

#[test]
fn s2_invalid_regex_rejected_no_row_inserted() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    let out = engrams(&db)
        .args([
            "pattern",
            "log",
            "--name",
            "Bad Regex",
            "--check-kind",
            "regex",
            "--check",
            "[unclosed",
            "--severity",
            "warn",
        ])
        .assert()
        .failure()
        .get_output()
        .clone();

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("check"),
        "error should name the invalid check: {stderr}"
    );

    // Fresh DB was never created (init not called), so no DB should exist.
    // If it does, the pattern count must be zero.
    if db.exists() {
        let conn = rusqlite::Connection::open(&db).unwrap();
        let n: i64 = conn
            .query_row("SELECT count(*) FROM system_patterns", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            n, 0,
            "no pattern row should be inserted on validation failure"
        );
    }
}

#[test]
fn s2_valid_check_surfaces_in_get() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    let output = engrams(&db)
        .args([
            "pattern",
            "log",
            "--name",
            "No Box params",
            "--check-kind",
            "regex",
            "--check",
            r"Vec<Box<dyn",
            "--severity",
            "warn",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let id = json["id"].as_i64().unwrap();

    let output = engrams(&db)
        .args(["pattern", "get", &id.to_string()])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let got: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(got["check_kind"].as_str().unwrap(), "regex");
    assert_eq!(got["check_expr"].as_str().unwrap(), "Vec<Box<dyn");
    assert_eq!(got["severity"].as_str().unwrap(), "warn");
}

#[test]
fn s2_default_severity_is_warn() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    let output = engrams(&db)
        .args([
            "pattern",
            "log",
            "--name",
            "Checkable Default",
            "--check-kind",
            "regex",
            "--check",
            "foo",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["severity"].as_str().unwrap(), "warn");
}

// ── S3: Field surfacing in list ───────────────────────────────────────────

#[test]
fn s3_list_includes_check_fields() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");

    // A checkable pattern.
    engrams(&db)
        .args([
            "pattern",
            "log",
            "--name",
            "Checkable",
            "--check-kind",
            "regex",
            "--check",
            r"Vec<Box<dyn",
            "--severity",
            "error",
        ])
        .assert()
        .success();

    // A prose-only pattern.
    engrams(&db)
        .args(["pattern", "log", "--name", "Prose Only"])
        .assert()
        .success();

    let output = engrams(&db)
        .args(["pattern", "list"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let patterns = json.as_array().unwrap();

    let checkable = patterns
        .iter()
        .find(|p| p["name"].as_str() == Some("Checkable"))
        .unwrap();
    assert_eq!(checkable["check_kind"].as_str(), Some("regex"));
    assert_eq!(checkable["severity"].as_str(), Some("error"));

    let prose = patterns
        .iter()
        .find(|p| p["name"].as_str() == Some("Prose Only"))
        .unwrap();
    // Prose-only patterns have no check fields surfaced.
    assert!(
        prose.get("check_kind").is_none() || prose["check_kind"].is_null(),
        "prose-only should have null check_kind"
    );
    assert_eq!(prose["severity"].as_str(), Some("warn"));
}

// ── S4/S5: Export ─────────────────────────────────────────────────────────

#[test]
fn s4_export_generates_rule_file_with_correct_frontmatter() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    let rules_dir = temp.path().join(".omp").join("rules");

    engrams(&db)
        .args([
            "pattern",
            "log",
            "--name",
            "No Boxed SQL Params",
            "--description",
            "Never box SQL params",
            "--check-kind",
            "regex",
            "--check",
            r"Vec<Box<dyn rusqlite::ToSql>>",
            "--severity",
            "warn",
            "--anchor",
            "src/ops",
        ])
        .assert()
        .success();

    let output = engrams(&db)
        .args([
            "rules",
            "export",
            "--harness",
            "omp",
            "--out",
            rules_dir.to_str().unwrap(),
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["rules"].as_i64(), Some(1));

    let rule_file = rules_dir.join("engrams-no-boxed-sql-params.md");
    assert!(rule_file.exists(), "rule file should exist");

    let content = fs::read_to_string(&rule_file).unwrap();
    // Frontmatter: condition verbatim.
    assert!(content.contains(r#"condition:"#));
    assert!(content.contains(r#"Vec<Box<dyn rusqlite::ToSql>>"#));
    // Scope covers edit and write surfaces.
    assert!(content.contains("tool:edit(src/ops/**)"));
    assert!(content.contains("tool:write(src/ops/**)"));
    // Body contains the description.
    assert!(content.contains("Never box SQL params"));

    // Manifest records pattern id and timestamp.
    let manifest = rules_dir.join(".engrams-manifest.json");
    assert!(manifest.exists());
    let mf: Value = serde_json::from_str(&fs::read_to_string(&manifest).unwrap()).unwrap();
    assert!(mf["rules"].is_array());
    assert_eq!(mf["rules"].as_array().unwrap().len(), 1);
    let entry = &mf["rules"][0];
    assert!(entry.get("pattern_id").is_some());
    assert!(entry.get("timestamp").is_some());
}

#[test]
fn s5_byte_identical_reexport() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    let rules_dir = temp.path().join("rules");

    engrams(&db)
        .args([
            "pattern",
            "log",
            "--name",
            "Stable Export",
            "--check-kind",
            "regex",
            "--check",
            "test",
            "--severity",
            "error",
        ])
        .assert()
        .success();

    // First export.
    engrams(&db)
        .args([
            "rules",
            "export",
            "--harness",
            "omp",
            "--out",
            rules_dir.to_str().unwrap(),
        ])
        .assert()
        .success();

    let first = fs::read_to_string(rules_dir.join("engrams-stable-export.md")).unwrap();
    let first_mf = fs::read_to_string(rules_dir.join(".engrams-manifest.json")).unwrap();

    // Second export.
    engrams(&db)
        .args([
            "rules",
            "export",
            "--harness",
            "omp",
            "--out",
            rules_dir.to_str().unwrap(),
        ])
        .assert()
        .success();

    let second = fs::read_to_string(rules_dir.join("engrams-stable-export.md")).unwrap();
    let second_mf = fs::read_to_string(rules_dir.join(".engrams-manifest.json")).unwrap();

    assert_eq!(first, second, "rule file must be byte-identical");
    assert_eq!(first_mf, second_mf, "manifest must be byte-identical");
}

#[test]
fn s5_prose_only_not_exported() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    let rules_dir = temp.path().join("rules");

    engrams(&db)
        .args(["pattern", "log", "--name", "Prose Only"])
        .assert()
        .success();

    let output = engrams(&db)
        .args([
            "rules",
            "export",
            "--harness",
            "omp",
            "--out",
            rules_dir.to_str().unwrap(),
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["rules"].as_i64(), Some(0));

    // No engrams-*.md files should exist.
    let rule_files: Vec<_> = fs::read_dir(&rules_dir)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .filter(|n| {
            n.to_str().unwrap().starts_with("engrams-") && n.to_str().unwrap().ends_with(".md")
        })
        .collect();
    assert!(rule_files.is_empty(), "no rule file for prose-only pattern");
}

// ── S6/S7: Staleness & write-through ──────────────────────────────────────

#[test]
fn s7_write_through_on_log() {
    let (temp, db) = workspace();
    let rules_dir = temp.path().join(".omp").join("rules");

    // Initial export creates the manifest.
    engrams(&db)
        .args([
            "pattern",
            "log",
            "--name",
            "First",
            "--check-kind",
            "regex",
            "--check",
            "first",
            "--anchor",
            "src",
        ])
        .assert()
        .success();
    engrams(&db)
        .args([
            "rules",
            "export",
            "--harness",
            "omp",
            "--out",
            rules_dir.to_str().unwrap(),
        ])
        .assert()
        .success();
    assert!(rules_dir.join(".engrams-manifest.json").exists());
    assert!(rules_dir.join("engrams-first.md").exists());

    // Logging a new checkable pattern triggers write-through (manifest present).
    engrams(&db)
        .args([
            "pattern",
            "log",
            "--name",
            "Second",
            "--check-kind",
            "regex",
            "--check",
            "second",
            "--anchor",
            "src",
        ])
        .assert()
        .success();

    // The new rule file should appear without an explicit export.
    assert!(
        rules_dir.join("engrams-second.md").exists(),
        "write-through should regenerate rule files on pattern log"
    );

    // Doctor should report no staleness.
    let output = engrams(&db)
        .arg("doctor")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let doc: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(doc["rules"]["stale"].as_bool(), Some(false));
}

#[test]
fn s6_doctor_detects_staleness() {
    let (temp, db) = workspace();
    let rules_dir = temp.path().join(".omp").join("rules");

    engrams(&db)
        .args([
            "pattern",
            "log",
            "--name",
            "Drifted",
            "--check-kind",
            "regex",
            "--check",
            "drift",
            "--anchor",
            "src",
        ])
        .assert()
        .success();
    engrams(&db)
        .args([
            "rules",
            "export",
            "--harness",
            "omp",
            "--out",
            rules_dir.to_str().unwrap(),
        ])
        .assert()
        .success();

    // Corrupt the manifest timestamp to predate the DB pattern.
    let mf_path = rules_dir.join(".engrams-manifest.json");
    let mut mf: Value = serde_json::from_str(&fs::read_to_string(&mf_path).unwrap()).unwrap();
    mf["rules"][0]["timestamp"] = Value::String("2000-01-01T00:00:00Z".to_string());
    fs::write(&mf_path, serde_json::to_string_pretty(&mf).unwrap()).unwrap();

    let output = engrams(&db)
        .arg("doctor")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let doc: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(doc["rules"]["stale"].as_bool(), Some(true));
    assert!(
        !doc["rules"]["drifted"].as_array().unwrap().is_empty(),
        "drifted patterns should be named"
    );
}

// ── S8: Check runner (regex) ──────────────────────────────────────────────

#[test]
fn s8_check_regex_violation_exit_1() {
    let (temp, db) = workspace();
    // Create a file with the antipattern.
    fs::create_dir_all(temp.path().join("src").join("ops")).unwrap();
    fs::write(
        temp.path().join("src").join("ops").join("query.rs"),
        "fn main() {\n    let v: Vec<Box<dyn rusqlite::ToSql>> = vec![];\n}\n",
    )
    .unwrap();

    engrams(&db)
        .args([
            "pattern",
            "log",
            "--name",
            "No Boxed SQL",
            "--description",
            "boxed params bad",
            "--check-kind",
            "regex",
            "--check",
            r"Vec<Box<dyn rusqlite::ToSql>>",
            "--severity",
            "warn",
            "--anchor",
            "src/ops",
        ])
        .assert()
        .success();

    let output = engrams(&db)
        .args(["check", "--paths", "src/ops/query.rs"])
        .assert()
        .failure() // exit 1 on violations
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let violations = json["violations"].as_array().unwrap();
    assert_eq!(violations.len(), 1);
    let v = &violations[0];
    assert_eq!(v["pattern"].as_str(), Some("No Boxed SQL"));
    assert!(v["file"].as_str().unwrap().contains("query.rs"));
    assert_eq!(v["line"].as_i64(), Some(2));
    assert_eq!(v["severity"].as_str(), Some("warn"));
}

#[test]
fn s8_check_clean_file_exit_0() {
    let (temp, db) = workspace();
    fs::create_dir_all(temp.path().join("src").join("ops")).unwrap();
    fs::write(
        temp.path().join("src").join("ops").join("clean.rs"),
        "fn main() {\n    let v: Vec<&dyn rusqlite::ToSql> = vec![];\n}\n",
    )
    .unwrap();

    engrams(&db)
        .args([
            "pattern",
            "log",
            "--name",
            "No Boxed SQL",
            "--check-kind",
            "regex",
            "--check",
            r"Vec<Box<dyn rusqlite::ToSql>>",
            "--anchor",
            "src/ops",
        ])
        .assert()
        .success();

    engrams(&db)
        .args(["check", "--paths", "src/ops/clean.rs"])
        .assert()
        .success(); // exit 0 — no violations
}

#[test]
fn s8_check_reports_file_count() {
    let (temp, db) = workspace();
    fs::create_dir_all(temp.path().join("src")).unwrap();
    fs::write(temp.path().join("src").join("a.rs"), "let x = 1;\n").unwrap();
    fs::write(temp.path().join("src").join("b.rs"), "let y = 2;\n").unwrap();

    engrams(&db)
        .args([
            "pattern",
            "log",
            "--name",
            "Always Match",
            "--check-kind",
            "regex",
            "--check",
            "let",
            "--anchor",
            "src",
        ])
        .assert()
        .success();

    let output = engrams(&db)
        .args(["check", "--paths", "src"])
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["files_checked"].as_i64(), Some(2));
    assert!(json["violations"].as_array().unwrap().len() >= 2);
}

// ── S10: Install ──────────────────────────────────────────────────────────

#[test]
fn s10_install_creates_rule_files_and_manifest() {
    let (temp, db) = workspace();
    let rules_dir = temp.path().join(".omp").join("rules");

    // Two checkable, one prose-only.
    engrams(&db)
        .args([
            "pattern",
            "log",
            "--name",
            "Check One",
            "--check-kind",
            "regex",
            "--check",
            "alpha",
            "--severity",
            "error",
            "--anchor",
            "src",
        ])
        .assert()
        .success();
    engrams(&db)
        .args([
            "pattern",
            "log",
            "--name",
            "Check Two",
            "--check-kind",
            "regex",
            "--check",
            "beta",
            "--severity",
            "warn",
        ])
        .assert()
        .success();
    engrams(&db)
        .args(["pattern", "log", "--name", "Prose"])
        .assert()
        .success();

    let output = engrams(&db)
        .args(["install", "--harness", "omp"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["rules"].as_i64(), Some(2));

    // One file per checkable pattern + the manifest.
    assert!(rules_dir.join("engrams-check-one.md").exists());
    assert!(rules_dir.join("engrams-check-two.md").exists());
    assert!(rules_dir.join(".engrams-manifest.json").exists());

    // Prose-only pattern should not have a rule file.
    assert!(!rules_dir.join("engrams-prose.md").exists());

    // JSON output lists every written path.
    let written = json["written"].as_array().unwrap();
    assert!(written.len() >= 3); // 2 rules + manifest

    // Guidance string present.
    assert!(json.get("guidance").is_some());
}

#[test]
fn s10_install_rejects_unknown_harness() {
    let (_temp, db) = workspace();
    engrams(&db)
        .args(["install", "--harness", "cursor"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("omp"));
}
