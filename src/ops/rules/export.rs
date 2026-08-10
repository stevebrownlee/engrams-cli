use anyhow::Result;
use rusqlite::Connection;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::models::Pattern;
use crate::ops::anchor;

/// `engrams rules export --harness omp [--out DIR]`.
pub fn handle(
    conn: &Connection,
    harness: &str,
    out: Option<PathBuf>,
    db_path: &Path,
) -> Result<Value> {
    if harness != "omp" {
        anyhow::bail!("unsupported harness '{}'; only 'omp' is supported", harness);
    }
    let dir = super::resolve_rules_dir(out.as_deref(), db_path)?;
    regenerate(conn, &dir)
}

/// Regenerate the full rulebook into `dir`: one `engrams-<slug>.md` per checkable
/// pattern (prose-only patterns are skipped), a deterministic manifest, and pruning
/// of any previously-generated rule file whose pattern no longer exists.
pub fn regenerate(conn: &Connection, dir: &Path) -> Result<Value> {
    fs::create_dir_all(dir)?;
    let patterns = load_checkable_patterns(conn)?;

    let mut owned: Vec<String> = Vec::with_capacity(patterns.len());
    let mut manifest_rules: Vec<Value> = Vec::with_capacity(patterns.len());

    for pat in &patterns {
        let slug = slugify(&pat.name);
        let fname = format!("engrams-{}.md", slug);
        let anchors = anchor::anchors_for(conn, "system_pattern", pat.id).unwrap_or_default();
        let content = render_rule(pat, &slug, &anchors);
        let hash = hex_sha256(content.as_bytes());
        fs::write(dir.join(&fname), content)?;
        manifest_rules.push(json!({
            "pattern_id": pat.id,
            "slug": slug,
            "file": fname.as_str(),
            "name": pat.name,
            "timestamp": pat.timestamp,
            "check_kind": pat.check_kind,
            "check_expr": pat.check_expr,
            "severity": pat.severity,
            "sha256": hash,
        }));
        owned.push(fname);
    }

    prune_stale(dir, &owned)?;

    // Deterministic manifest: rule objects keyed alphabetically (serde_json BTreeMap),
    // array sorted by pattern id.
    manifest_rules.sort_by_key(|r| r["pattern_id"].as_i64().unwrap_or(0));
    let manifest = json!({
        "harness": "omp",
        "version": 1,
        "rules": manifest_rules,
    });
    let manifest_file = super::manifest_path(dir);
    fs::write(&manifest_file, serde_json::to_string_pretty(&manifest)?)?;

    let mut written: Vec<String> = owned
        .iter()
        .map(|f| dir.join(f).display().to_string())
        .collect();
    written.push(manifest_file.display().to_string());

    Ok(json!({
        "harness": "omp",
        "rules_dir": dir.display().to_string(),
        "rules": patterns.len(),
        "written": written,
    }))
}

/// Patterns carrying a machine-checkable expression, oldest id first (stable ordering).
pub(crate) fn load_checkable_patterns(conn: &Connection) -> Result<Vec<Pattern>> {
    let mut stmt = conn.prepare(
        "SELECT id, uuid, name, description, tags, timestamp, check_kind, check_expr, severity, importance, access_count, last_accessed_at, archived \
         FROM system_patterns WHERE check_kind IS NOT NULL ORDER BY id ASC",
    )?;
    let rows = stmt.query_map([], crate::ops::pattern::parse_pattern_row)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Remove previously-generated `engrams-*.md` files whose pattern no longer maps to a
/// rule this run. Leaves user-authored rules (non-`engrams-` prefix) untouched.
fn prune_stale(dir: &Path, owned: &[String]) -> Result<()> {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name.starts_with("engrams-")
                    && name.ends_with(".md")
                    && !owned.iter().any(|o| o == name)
                {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }
    }
    Ok(())
}

/// Render a single omp rule file. Deterministic: fixed field order, sorted scopes.
fn render_rule(pat: &Pattern, slug: &str, anchors: &[String]) -> String {
    let description = pat.description.as_deref().unwrap_or(&pat.name);
    let (interrupt, is_ttsr) = severity_to_interrupt(&pat.severity);

    let mut s = String::new();
    s.push_str("---\n");
    s.push_str(&format!("name: engrams-{}\n", slug));
    s.push_str(&format!("description: {}\n", yaml_scalar(description)));

    if is_ttsr {
        let expr = pat.check_expr.as_deref().unwrap_or("");
        match pat.check_kind.as_deref() {
            Some("ast") => {
                s.push_str("astCondition:\n");
                s.push_str(&format!("  - {}\n", yaml_scalar(expr)));
            }
            _ => {
                s.push_str("condition:\n");
                s.push_str(&format!("  - {}\n", yaml_scalar(expr)));
            }
        }
        let scopes = scopes_for_anchors(anchors);
        s.push_str("scope:\n");
        for sc in &scopes {
            s.push_str(&format!("  - {}\n", yaml_scalar(sc)));
        }
        s.push_str(&format!("interruptMode: {}\n", interrupt));
    }

    s.push_str("---\n\n");
    s.push_str(&format!("# {}\n\n", pat.name));
    s.push_str(description);
    s.push_str("\n\n---\n\n");
    s.push_str(&format!(
        "_Generated by engrams (pattern #{}). Edit the pattern, not this file. \
         Re-run `engrams rules export --harness omp` to refresh._\n",
        pat.id
    ));
    s
}

/// Severity → omp `interruptMode` routing (R5: isolated here so a correction is one-site).
/// `error` → hard interrupt; `warn` → advisory (non-interrupting) injection;
/// `info` → rulebook entry only (no TTSR fields exported). Returns `(mode, is_ttsr)`.
#[inline]
fn severity_to_interrupt(severity: &str) -> (&'static str, bool) {
    match severity {
        "error" => ("always", true),
        "warn" => ("never", true),
        "info" => ("", false),
        _ => ("never", true),
    }
}

/// Map a pattern's anchor paths to omp `scope` tokens.
/// A directory anchor (`src/ops`) becomes `src/ops/**`; a file anchor keeps its path.
/// No anchors → `**` (all edit/write surfaces). Output sorted + de-duplicated.
fn scopes_for_anchors(anchors: &[String]) -> Vec<String> {
    let mut globs: BTreeSet<String> = BTreeSet::new();
    if anchors.is_empty() {
        globs.insert("**".to_string());
    } else {
        for a in anchors {
            globs.insert(anchor_to_glob(a));
        }
    }
    let mut scopes = Vec::with_capacity(globs.len() * 2);
    for g in &globs {
        scopes.push(format!("tool:edit({})", g));
        scopes.push(format!("tool:write({})", g));
    }
    scopes
}

#[inline]
fn anchor_to_glob(anchor: &str) -> String {
    let trimmed = anchor.trim_end_matches('/');
    let last = trimmed.rsplit('/').next().unwrap_or(trimmed);
    if last.contains('.') {
        trimmed.to_string()
    } else {
        format!("{}/**", trimmed)
    }
}

/// Lowercase, hyphen-separated slug from a pattern name.
fn slugify(name: &str) -> String {
    let mut slug = String::with_capacity(name.len());
    let mut prev_dash = true;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            slug.push('-');
            prev_dash = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        "rule".to_string()
    } else {
        slug
    }
}

/// YAML double-quoted scalar: backslash and double-quote escaped. Always-valid,
/// deterministic, and round-trips verbatim through omp's frontmatter parser.
fn yaml_scalar(s: &str) -> String {
    let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{}\"", escaped)
}

fn hex_sha256(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut h = Sha256::new();
    h.update(bytes);
    let digest = h.finalize();
    let mut s = String::with_capacity(64);
    for b in digest {
        // SAFETY: `{:02x}` on a `u8` never errors on a String.
        let _ = write!(s, "{b:02x}");
    }
    s
}
