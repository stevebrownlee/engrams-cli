//! Code nodes: files (and later symbols) as first-class graph nodes.

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};

use crate::ops::anchor::clean_path;

/// Upsert a file code node, refreshing `last_seen`, and return its row id.
pub fn upsert_file(conn: &Connection, path: &str, ts: &str) -> Result<i64> {
    let cleaned = clean_path(path);
    conn.execute(
        "INSERT INTO code_nodes (kind, path, symbol, first_seen, last_seen) \
         VALUES ('file', ?1, '', ?2, ?2) \
         ON CONFLICT(kind, path, symbol) DO UPDATE SET last_seen = excluded.last_seen",
        params![cleaned, ts],
    )?;
    let id: i64 = conn.query_row(
        "SELECT id FROM code_nodes WHERE kind = 'file' AND path = ?1 AND symbol = ''",
        params![cleaned],
        |row| row.get(0),
    )?;
    Ok(id)
}

/// Look up a file code node id by path.
#[allow(dead_code)] // lookup seam; ingest uses upsert_file
pub fn file_id(conn: &Connection, path: &str) -> Result<Option<i64>> {
    let cleaned = clean_path(path);
    let id = conn
        .query_row(
            "SELECT id FROM code_nodes WHERE kind = 'file' AND path = ?1 AND symbol = ''",
            params![cleaned],
            |row| row.get(0),
        )
        .optional()?;
    Ok(id)
}

/// True for vendored, generated, or engrams-internal paths that must never
/// become code nodes via bulk git ingest (commit history is full of them:
/// committed `node_modules`, build output, lockfiles). Deliberate user
/// anchors bypass this — only `CoChangeSource` filters.
pub fn is_generated(path: &str) -> bool {
    // Any path component matching a well-known vendored/build dir.
    const BLOCKED_DIRS: &[&str] = &[
        "node_modules",
        "bower_components",
        "vendor",
        "dist",
        "target",
        "build",
        "coverage",
        ".next",
        ".nuxt",
        ".turbo",
        ".astro",
        ".cache",
        "__pycache__",
        ".venv",
        "venv",
        "site-packages",
        ".gradle",
        // engrams' own export tree is tool state, not codebase.
        "engrams_export",
    ];
    if path.split('/').any(|part| BLOCKED_DIRS.contains(&part)) {
        return true;
    }
    // engrams' own database file.
    if path == "engrams/context.db" || path == ".engrams/context.db" {
        return true;
    }
    let name = path.rsplit('/').next().unwrap_or(path);
    name.ends_with(".map")
        || name.ends_with(".min.js")
        || name.ends_with(".min.css")
        || name.ends_with(".lock")
        || matches!(name, "package-lock.json" | "yarn.lock" | "pnpm-lock.yaml")
}
