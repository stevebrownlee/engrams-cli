//! Code nodes: files (and later symbols) as first-class graph nodes.

use std::sync::LazyLock;

use anyhow::Result;
use regex::Regex;
use rusqlite::{params, Connection, OptionalExtension};

use crate::ops::anchor::clean_path;

const MAX_SYMBOLS: usize = 32;
const MAX_DOC_LINES: usize = 40;
const MAX_DOC_CHARS: usize = 1200;

/// Upsert a file code node, refreshing `last_seen` and re-scanning enrichment
/// (symbols, module doc, line count), then return its row id. Enrichment is
/// best-effort: unreadable or missing files store NULLs, never error.
pub fn upsert_file(conn: &Connection, path: &str, ts: &str) -> Result<i64> {
    let cleaned = clean_path(path);
    let (symbols, module_doc, line_count) = scan(&cleaned);
    conn.execute(
        "INSERT INTO code_nodes (kind, path, symbol, first_seen, last_seen, symbols, module_doc, line_count) \
         VALUES ('file', ?1, '', ?2, ?2, ?3, ?4, ?5) \
         ON CONFLICT(kind, path, symbol) DO UPDATE SET \
           last_seen = excluded.last_seen, \
           symbols = excluded.symbols, \
           module_doc = excluded.module_doc, \
           line_count = excluded.line_count",
        params![cleaned, ts, symbols, module_doc, line_count],
    )?;
    let id: i64 = conn.query_row(
        "SELECT id FROM code_nodes WHERE kind = 'file' AND path = ?1 AND symbol = ''",
        params![cleaned],
        |row| row.get(0),
    )?;
    Ok(id)
}

/// Scan one workspace-relative file for enrichment metadata.
/// Returns `(symbols_json, module_doc, line_count)`; any element is None when
/// unavailable (no workspace root, unreadable, or not a scannable source file
/// for symbols).
fn scan(rel: &str) -> (Option<String>, Option<String>, Option<i64>) {
    // Anchors are workspace-root-relative; resolve via the same climb
    // `resolve_db_path` uses. Missing root ⇒ no enrichment (NULLs).
    let root = match crate::db::workspace_root() {
        Ok(r) => r,
        Err(_) => return (None, None, None),
    };
    let full = root.join(rel);
    let Ok(content) = std::fs::read_to_string(&full) else {
        return (None, None, None);
    };
    let line_count = content.lines().count() as i64;
    let ext = full
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default();
    let symbols = scan_symbols(&content, ext);
    let module_doc = module_doc(&content);
    (symbols, module_doc, Some(line_count))
}

/// Extract up to `MAX_SYMBOLS` top-level declaration names as a JSON array.
fn scan_symbols(content: &str, ext: &str) -> Option<String> {
    let re = symbol_regex(ext)?;
    let mut names: Vec<&str> = Vec::with_capacity(16);
    for caps in re.captures_iter(content) {
        if names.len() >= MAX_SYMBOLS {
            break;
        }
        if let Some(m) = caps.iter().skip(1).flatten().next() {
            let name = m.as_str();
            if !names.contains(&name) {
                names.push(name);
            }
        }
    }
    if names.is_empty() {
        return None;
    }
    serde_json::to_string(&names).ok()
}

/// Leading `//!` block (Rust) or `/** … */` doc block (C-family) at the top
/// of the file, joined with newlines. None when the file has no doc block.
fn module_doc(content: &str) -> Option<String> {
    let mut lines = content.lines().skip_while(|l| l.trim().is_empty());
    let first = lines.next()?.trim();
    if let Some(rest) = first.strip_prefix("//!") {
        let mut out = Vec::new();
        if !rest.trim().is_empty() {
            out.push(rest.trim().to_string());
        }
        for line in lines {
            match line.trim().strip_prefix("//!") {
                Some(r) if out.len() < MAX_DOC_LINES => {
                    if !r.trim().is_empty() {
                        out.push(r.trim().to_string());
                    }
                }
                _ => break,
            }
        }
        return finish_doc(out);
    }
    if let Some(rest) = first.strip_prefix("/**") {
        let mut out = Vec::new();
        if let Some(end) = rest.find("*/") {
            let seg = rest[..end].trim().trim_start_matches('*').trim();
            if !seg.is_empty() {
                out.push(seg.to_string());
            }
        } else {
            let seg = rest.trim_start_matches('*').trim();
            if !seg.is_empty() {
                out.push(seg.to_string());
            }
            for line in lines {
                let t = line.trim();
                if let Some(pos) = t.find("*/") {
                    let seg = t[..pos].trim().trim_start_matches('*').trim();
                    if !seg.is_empty() && out.len() < MAX_DOC_LINES {
                        out.push(seg.to_string());
                    }
                    break;
                }
                if out.len() >= MAX_DOC_LINES {
                    break;
                }
                let seg = t.trim_start_matches('*').trim();
                if !seg.is_empty() {
                    out.push(seg.to_string());
                }
            }
        }
        return finish_doc(out);
    }
    None
}

/// Join collected doc lines and clamp to `MAX_DOC_CHARS`; empty input ⇒ None.
fn finish_doc(out: Vec<String>) -> Option<String> {
    if out.is_empty() {
        return None;
    }
    let joined = out.join("\n");
    let text = if joined.len() > MAX_DOC_CHARS {
        let mut end = MAX_DOC_CHARS;
        while !joined.is_char_boundary(end) {
            end -= 1;
        }
        joined[..end].to_string()
    } else {
        joined
    };
    Some(text)
}
static RS_SYMS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\b(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?(?:unsafe\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)|\b(?:pub\s+)?(?:struct|enum|trait|type)\s+([A-Za-z_][A-Za-z0-9_]*)",
    )
    .unwrap()
});
static JS_SYMS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\bfunction\s*\*?\s+([A-Za-z_$][A-Za-z0-9_$]*)|\b(?:class|interface)\s+([A-Za-z_$][A-Za-z0-9_$]*)|\btype\s+([A-Za-z_$][A-Za-z0-9_$]*)\s*=",
    )
    .unwrap()
});
static PY_SYMS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\bdef\s+([A-Za-z_][A-Za-z0-9_]*)|\bclass\s+([A-Za-z_][A-Za-z0-9_]*)").unwrap()
});
static GO_SYMS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\bfunc\s+(?:\([^)]*\)\s*)?([A-Za-z_][A-Za-z0-9_]*)|\btype\s+([A-Za-z_][A-Za-z0-9_]*)\s+(?:struct|interface)\b",
    )
    .unwrap()
});

fn symbol_regex(ext: &str) -> Option<&'static Regex> {
    match ext {
        "rs" => Some(&RS_SYMS),
        "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" => Some(&JS_SYMS),
        "py" => Some(&PY_SYMS),
        "go" => Some(&GO_SYMS),
        _ => None,
    }
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
