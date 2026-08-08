pub mod export;

use anyhow::Result;
use rusqlite::Connection;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::cli::RulesCmd;
use crate::db;
use crate::models::Pattern;

/// Manifest filename written into the rule output directory.
pub const MANIFEST_FILE: &str = ".engrams-manifest.json";

pub fn handle(conn: &Connection, cmd: RulesCmd, db_path: &Path) -> Result<Value> {
    match cmd {
        RulesCmd::Export { harness, out } => export::handle(conn, &harness, out, db_path),
    }
}

/// Resolve the output directory for rules.
/// Explicit `out` wins; otherwise `<workspace-root>/.omp/rules`.
pub fn resolve_rules_dir(out: Option<&Path>, db_path: &Path) -> Result<PathBuf> {
    if let Some(o) = out {
        return Ok(o.to_path_buf());
    }
    let root = db::workspace_root_from_db(db_path)
        .or_else(|| db::workspace_root().ok())
        .ok_or_else(|| {
            anyhow::anyhow!("could not determine workspace root; pass --out <DIR> explicitly")
        })?;
    Ok(root.join(".omp").join("rules"))
}

pub fn manifest_path(rules_dir: &Path) -> PathBuf {
    rules_dir.join(MANIFEST_FILE)
}

/// Write-through (S7): if a manifest already exists in the workspace's rule dir,
/// regenerate the rulebook so generated files never lag the database.
/// Opt-in by manifest presence; no-op (no surprise file writes) otherwise.
/// Best-effort: any error is swallowed so a stale export never breaks a `pattern log`.
pub fn write_through(conn: &Connection, db_path: &Path) {
    // Anchor strictly to the database's workspace so write-through never
    // crosses into a different workspace (e.g. when run from a test TempDir
    // or with an explicit --db outside the workspace layout).
    let Some(root) = db::workspace_root_from_db(db_path) else {
        return; // db path doesn't follow <root>/engrams/context.db — skip.
    };
    let dir = root.join(".omp").join("rules");
    if manifest_path(&dir).exists() {
        let _ = export::regenerate(conn, &dir);
    }
}

/// Doctor (S6): compare the on-disk rulebook manifest against the current
/// checkable patterns in the database and report drift. Advisory only: never
/// errors, never affects DB-integrity `ok`. Independent of `ok` on purpose —
/// rules sync is a workspace concern, not database health.
pub fn staleness(conn: &Connection, db_path: &Path) -> Value {
    // Anchor strictly to the database's workspace, never CWD, so staleness
    // never reports against an unrelated workspace (S6).
    let Some(root) = db::workspace_root_from_db(db_path) else {
        return json!({"exported": false, "stale": false});
    };
    let dir = root.join(".omp").join("rules");
    let dir_s = dir.display().to_string();
    let manifest_file = manifest_path(&dir);
    if !manifest_file.exists() {
        return json!({"exported": false, "stale": false, "dir": dir_s});
    }

    // Current checkable patterns from the database, keyed by id.
    let current: Vec<Pattern> = match export::load_checkable_patterns(conn) {
        Ok(v) => v,
        Err(_) => return json!({"exported": true, "stale": true, "dir": dir_s, "db_error": true}),
    };
    let mut by_id: BTreeMap<i64, &Pattern> = BTreeMap::new();
    for p in &current {
        by_id.insert(p.id, p);
    }

    // Load manifest.
    let manifest: Value = match fs::read_to_string(&manifest_file)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
    {
        Some(m) => m,
        None => {
            return json!({"exported": true, "stale": true, "dir": dir_s, "corrupt_manifest": true})
        }
    };

    let mut drifted = Vec::new();
    let mut missing = Vec::new();
    let mut manifest_ids = Vec::new();

    if let Some(rules) = manifest.get("rules").and_then(|r| r.as_array()) {
        for r in rules {
            let id = r.get("pattern_id").and_then(|v| v.as_i64()).unwrap_or(-1);
            manifest_ids.push(id);
            let name = r
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            match by_id.get(&id) {
                None => missing.push(json!({"id": id, "name": name})),
                Some(p) => {
                    let m_ts = r.get("timestamp").and_then(|v| v.as_str()).unwrap_or("");
                    let m_kind = r.get("check_kind").and_then(|v| v.as_str());
                    let m_expr = r.get("check_expr").and_then(|v| v.as_str());
                    let m_sev = r.get("severity").and_then(|v| v.as_str()).unwrap_or("warn");
                    if p.name != name
                        || p.timestamp != m_ts
                        || p.check_kind.as_deref() != m_kind
                        || p.check_expr.as_deref() != m_expr
                        || p.severity != m_sev
                    {
                        drifted.push(json!({"id": id, "name": p.name.clone()}));
                    }
                }
            }
        }
    }

    // Patterns in the DB but absent from the manifest (added since export).
    let mut unexported = Vec::new();
    for p in &current {
        if !manifest_ids.contains(&p.id) {
            unexported.push(json!({"id": p.id, "name": p.name.clone()}));
        }
    }

    let stale = !(drifted.is_empty() && missing.is_empty() && unexported.is_empty());
    json!({
        "exported": true,
        "dir": dir_s,
        "stale": stale,
        "drifted": drifted,
        "missing": missing,
        "unexported": unexported,
    })
}
