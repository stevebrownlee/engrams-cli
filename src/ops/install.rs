//! `engrams install --harness omp` — one-shot workspace setup for in-session
//! enforcement (S10 / decision #42).
//!
//! Thin orchestration over the rules-export machinery: it writes rule files +
//! the deterministic manifest into the workspace's `.omp/rules/` directory and
//! returns JSON listing every written path together with next-step guidance.
//! Distinct from `rules export` only in framing — `install` is the user-facing
//! "enable enforcement here" command with no `--out` knob.

use anyhow::Result;
use rusqlite::Connection;
use serde_json::{json, Value};
use std::path::Path;

use crate::ops::rules;

pub fn handle(conn: &Connection, harness: &str, db_path: &Path) -> Result<Value> {
    if harness != "omp" {
        anyhow::bail!("unsupported harness '{}'; only 'omp' is supported", harness);
    }

    // Always install into the canonical workspace rule dir — install is the
    // standard setup path, so it deliberately ignores any `--out` style knob.
    let dir = rules::resolve_rules_dir(None, db_path)?;
    let summary = rules::export::regenerate(conn, &dir)?;

    let written = summary.get("written").cloned().unwrap_or_else(|| json!([]));
    let count = summary.get("rules").and_then(|v| v.as_i64()).unwrap_or(0);

    let guidance = if count == 0 {
        "No checkable patterns found. Add patterns with `engrams pattern log --check-kind regex --check <expr>` then re-run `engrams install --harness omp`.".to_string()
    } else {
        format!(
            "Installed {} rule file(s) to {}. omp reads .omp/rules on session start; restart your omp session (or reload the rulebook) for the new rules to take effect.",
            count,
            dir.display()
        )
    };

    Ok(json!({
        "harness": "omp",
        "rules_dir": dir,
        "rules": count,
        "written": written,
        "guidance": guidance,
    }))
}
