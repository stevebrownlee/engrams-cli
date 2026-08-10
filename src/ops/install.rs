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

pub fn handle(conn: &Connection, harness: &str, hooks: bool, db_path: &Path) -> Result<Value> {
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

    let mut result = json!({
        "harness": "omp",
        "rules_dir": dir,
        "rules": count,
        "written": written,
        "guidance": guidance,
    });

    if hooks {
        let hook_path = write_pre_commit_hook(db_path)?;
        if let serde_json::Value::Object(map) = &mut result {
            map.insert(
                "hook_installed".into(),
                json!(hook_path.display().to_string()),
            );
        }
    }

    Ok(result)
}

/// Write a git pre-commit hook that runs `engrams check --staged`.
/// Returns the path to the installed hook. Errors if no git repo is found.
fn write_pre_commit_hook(db_path: &Path) -> Result<std::path::PathBuf> {
    let workspace_root = crate::db::workspace_root()?;

    // Resolve the engrams binary path (use the built binary if it exists,
    // otherwise fall back to `engrams` on PATH).
    let engrams_bin = {
        let local = workspace_root.join("target/debug/engrams");
        if local.exists() {
            local.display().to_string()
        } else {
            let release = workspace_root.join("target/release/engrams");
            if release.exists() {
                release.display().to_string()
            } else {
                "engrams".to_string()
            }
        }
    };

    // Resolve the DB path relative to workspace root for the hook
    let db_arg = if db_path.is_absolute() {
        db_path.display().to_string()
    } else {
        db_path.display().to_string()
    };

    let hook_content = format!(
"#!/bin/sh
# engrams pre-commit hook: runs stored pattern checks against staged files.
# Installed by `engrams install --harness omp --hooks`.
# Remove this file to uninstall.

engrams=\"{engrams_bin}\"

# Fast-path: skip if no staged files
staged=$(git diff --cached --name-only --diff-filter=ACMR)
[ -z \"$staged\" ] && exit 0

# Run checks and parse JSON. The hook blocks ONLY on error-severity
# violations; info/warn are advisory and printed but don't block the commit.
# Do NOT pass --fields: that strips the 'violations' key before the exit-1
# check inside the binary, silently making the hook a no-op.
output=$(\"$engrams\" --db \"{db_arg}\" check --staged 2>/dev/null)

echo \"$output\" | python3 -c '
import sys, json
try:
    data = json.load(sys.stdin)
except Exception:
    sys.exit(0)  # fail-open on parse error

errors = [v for v in data.get(\"violations\", []) if v.get(\"severity\") == \"error\"]
if not errors:
    sys.exit(0)

print(\"engrams check found error-severity violations:\")
for v in data.get(\"violations\", []):
    sev = v.get(\"severity\", \"warn\")
    print(\"  [\" + sev + \"] \" + str(v.get(\"pattern\",\"?\")) + \": \" + str(v.get(\"file\",\"?\")) + \":\" + str(v.get(\"line\",\"?\")))
print(\"\")
print(\"To bypass: git commit --no-verify\")
sys.exit(1)
' 2>/dev/null

if [ $? -ne 0 ]; then
  exit 1
fi

exit 0
",
        engrams_bin = engrams_bin,
        db_arg = db_arg,
    );

    let hooks_dir = workspace_root.join(".git/hooks");
    if !hooks_dir.exists() {
        anyhow::bail!(".git/hooks directory not found at {}", hooks_dir.display());
    }

    let hook_path = hooks_dir.join("pre-commit");
    std::fs::write(&hook_path, &hook_content)?;

    // Make executable (unix)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&hook_path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&hook_path, perms)?;
    }

    Ok(hook_path)
}
