//! `engrams check` — run stored checks against files for CI / session-end review.
//!
//! Loads every pattern carrying a machine-checkable expression (`check_kind` in
//! `regex` | `ast`), determines each pattern's scope from its anchors, collects
//! the files to scan (`--staged` / `--paths` / full workspace), and emits JSON
//! violations `{pattern, pattern_id, file, line, severity, message}`. The caller
//! (`main.rs`) turns a non-empty violation list into exit code 1.
//!
//! Regex checks are self-contained (the `regex` crate). AST checks shell out to
//! the `ast-grep` binary (`sg`); when `sg` is absent they are skipped with a
//! stderr note, so the omp TTSR engine (which the export produces the rule for)
//! remains the authoritative matcher for structural patterns.

use anyhow::Result;
use ignore::WalkBuilder;
use regex::Regex;
use rusqlite::Connection;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::db;
use crate::models::Pattern;
use crate::ops::pattern::parse_pattern_row;

struct Checkable {
    pattern: Pattern,
    anchors: Vec<String>,
    regex: Option<Regex>,
}

pub fn handle(conn: &Connection, staged: bool, paths: &[String], db_path: &Path) -> Result<Value> {
    let root = db::workspace_root_from_db(db_path).or_else(|| db::workspace_root().ok());
    let checkables = load_checkables(conn)?;
    let files = collect_files(staged, paths, root.as_deref())?;
    let violations = run(&checkables, &files, root.as_deref())?;
    Ok(json!({
        "violations": violations,
        "files_checked": files.len(),
        "checks": checkables.len(),
    }))
}

/// Load every pattern with a check expression, oldest id first (stable ordering).
/// Anchors are attached so each check can be scoped.
fn load_checkables(conn: &Connection) -> Result<Vec<Checkable>> {
    let mut stmt = conn.prepare(
        "SELECT id, uuid, name, description, tags, timestamp, check_kind, check_expr, severity \
         FROM system_patterns WHERE check_kind IS NOT NULL AND check_expr IS NOT NULL \
         ORDER BY id ASC",
    )?;
    let patterns: Vec<Pattern> = stmt
        .query_map([], parse_pattern_row)?
        .collect::<rusqlite::Result<_>>()?;
    let mut anchors_map = crate::ops::anchor::anchors_map(conn, "system_pattern")?;
    let mut out = Vec::with_capacity(patterns.len());
    for p in patterns {
        let anchors = anchors_map.remove(&p.id).unwrap_or_default();
        let regex = if p.check_kind.as_deref() == Some("regex") {
            Regex::new(p.check_expr.as_deref().unwrap_or("")).ok()
        } else {
            None
        };
        out.push(Checkable {
            pattern: p,
            anchors,
            regex,
        });
    }
    Ok(out)
}

/// Collect the set of files (repo-relative path strings) to scan.
/// Uses a `BTreeSet` so dedup is O(n log n) and output order is deterministic.
fn collect_files(staged: bool, paths: &[String], root: Option<&Path>) -> Result<Vec<String>> {
    use std::collections::BTreeSet;
    let mut files = BTreeSet::new();

    if staged {
        for f in crate::ops::git::staged_files()? {
            files.insert(f);
        }
    }

    for p in paths {
        for f in collect_path(p, root) {
            files.insert(f);
        }
    }

    if !staged && paths.is_empty() {
        let base = root.unwrap_or_else(|| Path::new("."));
        for result in WalkBuilder::new(base).build() {
            let Ok(entry) = result else { continue };
            if entry.file_type().is_some_and(|t| t.is_file()) {
                files.insert(relativize(entry.path(), base));
            }
        }
    }

    Ok(files.into_iter().collect())
}

/// Expand a single `--paths` entry (file or directory) to repo-relative paths.
fn collect_path(p: &str, root: Option<&Path>) -> Vec<String> {
    let base = root.unwrap_or_else(|| Path::new("."));
    let abs = base.join(p);
    let meta = match fs::metadata(&abs) {
        Ok(m) => m,
        Err(_) => {
            // May be a deleted-in-worktree staged file or a typo; pass it
            // through so a missing staged file is reported by scope, not lost.
            return vec![p.to_string()];
        }
    };
    if meta.is_file() {
        return vec![relativize(&abs, base)];
    }
    if meta.is_dir() {
        let mut out = Vec::new();
        for result in WalkBuilder::new(&abs).build() {
            let Ok(entry) = result else { continue };
            if entry.file_type().is_some_and(|t| t.is_file()) {
                out.push(relativize(entry.path(), base));
            }
        }
        return out;
    }
    Vec::new()
}

fn run(checkables: &[Checkable], files: &[String], root: Option<&Path>) -> Result<Vec<Value>> {
    let mut violations = Vec::new();
    let mut sg_checked = false;
    let sg_present = sg_available();

    for rel in files {
        let abs = root
            .map(|r| r.join(rel))
            .unwrap_or_else(|| PathBuf::from(rel));
        let content = match fs::read_to_string(&abs) {
            Ok(c) => c,
            Err(_) => continue, // deleted / binary / unreadable
        };

        for c in checkables {
            if !in_scope(rel, &c.anchors) {
                continue;
            }
            match c.pattern.check_kind.as_deref() {
                Some("regex") => {
                    let Some(re) = &c.regex else { continue };
                    for (i, line) in content.lines().enumerate() {
                        if re.is_match(line) {
                            violations.push(violation(&c.pattern, rel, i + 1));
                        }
                    }
                }
                Some("ast") => {
                    sg_checked = true;
                    if !sg_present {
                        continue;
                    }
                    run_ast(&c.pattern, rel, &abs, &mut violations)?;
                }
                _ => {}
            }
        }
    }

    if sg_checked && !sg_present {
        eprintln!("note: ast-grep (`sg`) not found on PATH; ast checks were skipped. Install ast-grep to run structural checks locally; omp TTSR rules remain authoritative.");
    }

    Ok(violations)
}

/// Shell out to `sg run --pattern <expr> --lang <lang> <file> --json` and turn
/// each match into a violation at the match's start line.
fn run_ast(pattern: &Pattern, rel: &str, abs: &Path, violations: &mut Vec<Value>) -> Result<()> {
    let Some(expr) = pattern.check_expr.as_deref() else {
        return Ok(());
    };
    let Some(lang) = lang_for(abs) else {
        return Ok(()); // unknown language; sg cannot match without it
    };
    let output = Command::new("sg")
        .args(["run", "--pattern", expr, "--lang", lang, "--json"])
        .arg(abs)
        .output();
    let Ok(out) = output else {
        return Ok(());
    };
    if !out.status.success() {
        return Ok(());
    }
    let parsed: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap_or_default();
    for m in parsed {
        let line = m
            .get("range")
            .and_then(|r| r.get("start"))
            .and_then(|s| s.get("line"))
            .and_then(|l| l.as_u64())
            .map(|n| n as usize + 1) // sg lines are 0-indexed
            .unwrap_or(1);
        violations.push(violation(pattern, rel, line));
    }
    Ok(())
}

#[inline]
fn violation(pattern: &Pattern, file: &str, line: usize) -> Value {
    json!({
        "pattern": &pattern.name,
        "pattern_id": pattern.id,
        "file": file,
        "line": line,
        "severity": &pattern.severity,
        "message": pattern.description.as_deref().unwrap_or(""),
    })
}

/// A file is in scope for a pattern if the pattern has no anchors (all files) or
/// the file lives under one of the anchor paths. Mirrors the export's
/// `tool:edit(<anchor>/**)` globbing.
#[inline]
fn in_scope(file: &str, anchors: &[String]) -> bool {
    if anchors.is_empty() {
        return true;
    }
    let norm = file.trim_start_matches("./");
    anchors.iter().any(|a| {
        let a = a.trim_end_matches('/');
        norm == a
            || norm
                .strip_prefix(a)
                .is_some_and(|rest| rest.starts_with('/'))
    })
}

#[inline]
fn relativize(p: &Path, base: &Path) -> String {
    let rel = p.strip_prefix(base).unwrap_or(p);
    rel.to_string_lossy().trim_start_matches("./").to_string()
}

/// Map a file extension to the ast-grep `--lang` identifier.
fn lang_for(path: &Path) -> Option<&'static str> {
    match path.extension().and_then(|e| e.to_str()) {
        Some("rs") => Some("rust"),
        Some("ts") | Some("tsx") => Some("tsx"),
        Some("js") | Some("jsx") | Some("mjs") | Some("cjs") => Some("js"),
        Some("py") => Some("python"),
        Some("go") => Some("Go"),
        Some("c") => Some("c"),
        Some("cpp") | Some("cc") | Some("cxx") => Some("cpp"),
        Some("java") => Some("Java"),
        Some("rb") => Some("Ruby"),
        _ => None,
    }
}

fn sg_available() -> bool {
    Command::new("sg")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
