use crate::cli::{AnchorCmd, RefItemType};
use crate::models::{Decision, Pattern};
use crate::ops::git;
use anyhow::Result;
use chrono::{SecondsFormat, Utc};
use rusqlite::{params, Connection};
use serde_json::Value;
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Strip leading `./` and trailing `/`, and fold `<name>.worktrees/<branch>`
/// segments into `<name>`. Returns `Cow::Borrowed` when no rewrite is needed
/// (Rule 003: zero allocation on the already-canonical fast path).
#[inline]
pub fn fold_worktree_segments(path: &str) -> Cow<'_, str> {
    let mut s = path.trim();
    while let Some(rest) = s.strip_prefix("./") {
        s = rest;
    }
    if s.len() > 1 {
        s = s.trim_end_matches('/');
    }
    if !s.contains(".worktrees/") {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len());
    let leading_slash = s.starts_with('/');
    let mut iter = s.split('/').filter(|seg| !seg.is_empty()).peekable();
    let mut first = true;
    while let Some(seg) = iter.next() {
        let emitted = if let Some(stem) = seg.strip_suffix(".worktrees") {
            if !stem.is_empty() && iter.peek().is_some() {
                let _branch = iter.next();
                stem
            } else {
                seg
            }
        } else {
            seg
        };
        if leading_slash || !first {
            out.push('/');
        }
        out.push_str(emitted);
        first = false;
    }
    Cow::Owned(out)
}

#[inline]
pub fn clean_path(path: &str) -> String {
    fold_worktree_segments(path).into_owned()
}

/// Resolved path pair: `logical` is the canonical workspace-relative key for
/// SQLite storage and matching; `physical` is the concrete on-disk path (e.g.
/// inside an active git worktree or child repo) for file-content checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPath {
    pub logical: String,
    pub physical: Option<PathBuf>,
}

fn strip_workspace_prefix<'a>(target: &'a Path, ws_root: &Path) -> Option<&'a Path> {
    if let Ok(rel) = target.strip_prefix(ws_root) {
        return Some(rel);
    }
    if let Ok(canon_ws) = std::fs::canonicalize(ws_root) {
        if let Ok(rel) = target.strip_prefix(&canon_ws) {
            return Some(rel);
        }
    }
    None
}

fn fold_git_worktree_prefix(abs_path: &Path) -> PathBuf {
    // Check ancestors from deepest to shallowest for a `.git` worktree pointer file.
    let mut prefixes: Vec<&Path> = abs_path.ancestors().collect();
    prefixes.reverse();
    for anc in prefixes {
        if let Some(main_repo) = crate::db::resolve_git_worktree(anc) {
            if let Ok(rel) = abs_path.strip_prefix(anc) {
                return if rel.as_os_str().is_empty() {
                    main_repo
                } else {
                    main_repo.join(rel)
                };
            }
        }
    }
    abs_path.to_path_buf()
}

/// Resolve `raw` into its canonical workspace-relative `logical` path and its
/// on-disk `physical` path.
pub fn resolve_workspace_path(raw: &str, root: Option<&Path>) -> ResolvedPath {
    let cwd = std::env::current_dir().ok();
    resolve_workspace_path_with_cwd(raw, cwd.as_deref(), root)
}

pub fn resolve_workspace_path_with_cwd(
    raw: &str,
    cwd: Option<&Path>,
    root: Option<&Path>,
) -> ResolvedPath {
    let mut trimmed = raw.trim();
    while let Some(rest) = trimmed.strip_prefix("./") {
        trimmed = rest;
    }
    if trimmed.len() > 1 {
        trimmed = trimmed.trim_end_matches('/');
    }
    let Some(ws_root) = root else {
        let logical = fold_worktree_segments(trimmed).into_owned();
        let physical = Path::new(trimmed)
            .is_absolute()
            .then(|| PathBuf::from(trimmed));
        return ResolvedPath { logical, physical };
    };

    let raw_path = Path::new(trimmed);
    if raw_path.is_absolute() {
        let physical = Some(raw_path.to_path_buf());
        let git_folded = fold_git_worktree_prefix(raw_path);
        let git_folded_lossy = git_folded.to_string_lossy();
        let str_folded = fold_worktree_segments(&git_folded_lossy);
        let folded_path = Path::new(str_folded.as_ref());
        if let Some(rel) = strip_workspace_prefix(folded_path, ws_root) {
            let rel_lossy = rel.to_string_lossy();
            let rel_str = fold_worktree_segments(&rel_lossy).into_owned();
            if !rel_str.is_empty() {
                return ResolvedPath {
                    logical: rel_str,
                    physical,
                };
            }
        }
        return ResolvedPath {
            logical: str_folded.into_owned(),
            physical,
        };
    }

    let folded = fold_worktree_segments(trimmed);
    // Check if `cwd` is a child directory or worktree strictly inside `ws_root`.
    if let Some(c) = cwd {
        let c_git_folded = fold_git_worktree_prefix(c);
        let c_git_lossy = c_git_folded.to_string_lossy();
        let c_str_folded = fold_worktree_segments(&c_git_lossy);
        let c_folded_path = Path::new(c_str_folded.as_ref());
        if let Some(cwd_rel) = strip_workspace_prefix(c_folded_path, ws_root) {
            let cwd_rel_lossy = cwd_rel.to_string_lossy();
            let cwd_rel_str = fold_worktree_segments(&cwd_rel_lossy);
            if !cwd_rel_str.is_empty()
                && folded.as_ref() != cwd_rel_str.as_ref()
                && !folded.starts_with(&format!("{cwd_rel_str}/"))
            {
                let in_cwd = c.join(trimmed);
                let first_seg = folded.split('/').next().unwrap_or("");
                let is_workspace_container =
                    matches!(first_seg, "tenants" | "core-libs" | "packages" | ".pilot");
                if !is_workspace_container {
                    return ResolvedPath {
                        logical: format!("{cwd_rel_str}/{folded}"),
                        physical: Some(in_cwd),
                    };
                }
            }
        }
    }

    let physical = {
        let direct = ws_root.join(trimmed);
        if direct.exists() {
            Some(direct)
        } else {
            Some(ws_root.join(folded.as_ref()))
        }
    };
    ResolvedPath {
        logical: folded.into_owned(),
        physical,
    }
}

#[inline]
pub fn normalize_path(raw: &str, root: Option<&Path>) -> String {
    resolve_workspace_path(raw, root).logical
}

/// Read git-staged files for `--staged` commands, preferring the git repository
/// containing `cwd` when `cwd` sits inside `root` (e.g. a sub-repo or worktree
/// under an umbrella workspace) and mapping each staged file back to its
/// workspace-relative logical path.
pub fn staged_files_for_workspace(root: &Path) -> Result<Vec<ResolvedPath>> {
    if let Ok(cwd) = std::env::current_dir() {
        let cwd_folded = fold_git_worktree_prefix(&cwd);
        let cwd_lossy = cwd_folded.to_string_lossy();
        let cwd_str = fold_worktree_segments(&cwd_lossy);
        if strip_workspace_prefix(Path::new(cwd_str.as_ref()), root).is_some() {
            if let Ok(top) = git::toplevel(&cwd) {
                let top_path = PathBuf::from(&top);
                if let Ok(files) = git::staged_files(&top_path) {
                    let mut out = Vec::with_capacity(files.len());
                    for f in files {
                        let abs = top_path.join(&f);
                        let mut resolved = resolve_workspace_path_with_cwd(
                            &abs.to_string_lossy(),
                            Some(&cwd),
                            Some(root),
                        );
                        resolved.physical = Some(abs);
                        out.push(resolved);
                    }
                    return Ok(out);
                }
            }
        }
    }
    let files = git::staged_files(root)?;
    Ok(files
        .into_iter()
        .map(|f| resolve_workspace_path_with_cwd(&f, Some(root), Some(root)))
        .collect())
}

pub fn attach(
    conn: &Connection,
    item_type: &str,
    item_id: i64,
    paths: &[String],
    root: Option<&Path>,
) -> Result<Vec<String>> {
    let timestamp = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    for path in paths {
        let cleaned = normalize_path(path, root);
        conn.execute(
            "INSERT OR IGNORE INTO item_anchors (item_type, item_id, path, timestamp) VALUES (?1, ?2, ?3, ?4)",
            params![item_type, item_id, cleaned, timestamp],
        )?;
    }
    // Anchors changed the graph: refresh this item's derived edges.
    crate::ops::graph::rebuild::touch_item(conn, item_type, item_id, root)?;
    anchors_for(conn, item_type, item_id)
}

pub fn anchors_for(conn: &Connection, item_type: &str, item_id: i64) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT path FROM item_anchors WHERE item_type=?1 AND item_id=?2 ORDER BY id ASC",
    )?;
    let rows = stmt.query_map(params![item_type, item_id], |row| row.get(0))?;
    let mut paths = Vec::new();
    for r in rows {
        paths.push(r?);
    }
    Ok(paths)
}

pub fn anchors_map(conn: &Connection, item_type: &str) -> Result<HashMap<i64, Vec<String>>> {
    let mut stmt =
        conn.prepare("SELECT item_id, path FROM item_anchors WHERE item_type=?1 ORDER BY id ASC")?;
    let rows = stmt.query_map(params![item_type], |row| {
        let id: i64 = row.get(0)?;
        let path: String = row.get(1)?;
        Ok((id, path))
    })?;
    let mut map = HashMap::new();
    for r in rows {
        let (id, path) = r?;
        map.entry(id).or_insert_with(Vec::new).push(path);
    }
    Ok(map)
}

fn ref_item_exists(conn: &Connection, item_type: &RefItemType, id: i64) -> Result<bool> {
    let table = match item_type {
        RefItemType::Decision => "decisions",
        RefItemType::ProgressEntry => "progress_entries",
        RefItemType::SystemPattern => "system_patterns",
    };
    let count: i64 = conn.query_row(
        &format!("SELECT count(*) FROM {} WHERE id = ?", table),
        params![id],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

pub fn handle(conn: &Connection, cmd: AnchorCmd, root: Option<&Path>) -> Result<Value> {
    match cmd {
        AnchorCmd::Add {
            item_type,
            id,
            paths,
        } => {
            if !ref_item_exists(conn, &item_type, id)? {
                anyhow::bail!("{} {} does not exist", item_type.as_str(), id);
            }
            let urls = attach(conn, item_type.as_str(), id, &paths, root)?;
            Ok(serde_json::to_value(urls)?)
        }
        AnchorCmd::List { item_type, id } => {
            if !ref_item_exists(conn, &item_type, id)? {
                anyhow::bail!("{} {} does not exist", item_type.as_str(), id);
            }
            let mut stmt = conn.prepare(
                "SELECT path, timestamp FROM item_anchors \
                 WHERE item_type = ?1 AND item_id = ?2 \
                 ORDER BY id ASC",
            )?;
            let rows = stmt.query_map(params![item_type.as_str(), id], |row| {
                let path: String = row.get(0)?;
                let timestamp: String = row.get(1)?;
                Ok(serde_json::json!({
                    "path": path,
                    "timestamp": timestamp,
                }))
            })?;
            let mut list = Vec::new();
            for r in rows {
                list.push(r?);
            }
            Ok(Value::Array(list))
        }
        AnchorCmd::Remove {
            item_type,
            id,
            path,
        } => {
            if !ref_item_exists(conn, &item_type, id)? {
                anyhow::bail!("{} {} does not exist", item_type.as_str(), id);
            }
            let cleaned = normalize_path(&path, root);
            let mut count = conn.execute(
                "DELETE FROM item_anchors \
                 WHERE item_type = ?1 AND item_id = ?2 AND path = ?3",
                params![item_type.as_str(), id, cleaned],
            )?;
            if count == 0 && cleaned != path {
                count = conn.execute(
                    "DELETE FROM item_anchors \
                     WHERE item_type = ?1 AND item_id = ?2 AND path = ?3",
                    params![item_type.as_str(), id, path],
                )?;
            }
            if count == 0 {
                anyhow::bail!(
                    "anchor '{}' is not attached to {} {}",
                    path,
                    item_type.as_str(),
                    id
                );
            }
            Ok(serde_json::json!({
                "deleted": true,
                "path": cleaned,
            }))
        }
    }
}

pub fn query_relevant_ids(conn: &Connection, paths: &[String]) -> Result<Vec<(String, i64)>> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    let mut sql = "SELECT DISTINCT item_type, item_id FROM item_anchors WHERE ".to_string();
    let mut conditions = Vec::new();
    let mut params_vec = Vec::<&dyn rusqlite::ToSql>::new();
    for path in paths {
        conditions.push("(path = ? OR path LIKE ? || '/%' OR ? LIKE path || '/%')");
        params_vec.push(path);
        params_vec.push(path);
        params_vec.push(path);
    }
    sql.push_str(&conditions.join(" OR "));

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(params_vec), |row| {
        let itype: String = row.get(0)?;
        let id: i64 = row.get(1)?;
        Ok((itype, id))
    })?;

    let mut results = Vec::new();
    for r in rows {
        results.push(r?);
    }
    Ok(results)
}

pub fn handle_relevant(
    conn: &Connection,
    paths: Vec<String>,
    staged: bool,
    all: bool,
    root: Option<&Path>,
) -> Result<Value> {
    let mut cleaned_paths: Vec<String> = Vec::with_capacity(paths.len());
    for p in &paths {
        cleaned_paths.push(normalize_path(p, root));
    }
    if staged {
        let r = root.ok_or_else(|| {
            anyhow::anyhow!(
                "relevant --staged needs a workspace: pass --workspace <dir> or keep the database at <dir>/engrams/context.db"
            )
        })?;
        let staged_files = staged_files_for_workspace(r)
            .map_err(|e| anyhow::anyhow!("cannot read staged files: {}", e))?;
        for sf in staged_files {
            cleaned_paths.push(sf.logical);
        }
    }

    if cleaned_paths.is_empty() {
        if staged {
            return Ok(serde_json::json!({
                "decisions": [],
                "patterns": [],
            }));
        } else {
            anyhow::bail!("provide at least one path or --staged");
        }
    }

    let matched = query_relevant_ids(conn, &cleaned_paths)?;

    let mut decision_ids = Vec::new();
    let mut pattern_ids = Vec::new();
    for (itype, id) in matched {
        if itype == "decision" {
            decision_ids.push(id);
        } else if itype == "system_pattern" {
            pattern_ids.push(id);
        }
    }

    let mut decisions = Vec::new();
    if !decision_ids.is_empty() {
        let placeholders = crate::ops::sql_placeholders(decision_ids.len());
        let score = crate::ops::scoring::score_expr("timestamp", "importance");
        let sql = if all {
            format!("SELECT {}, {score} AS score FROM decisions WHERE id IN ({}) AND archived = 0 ORDER BY score DESC", crate::models::DECISION_COLS, placeholders)
        } else {
            format!("SELECT {}, {score} AS score FROM decisions WHERE id IN ({}) AND status = 'active' AND archived = 0 ORDER BY score DESC", crate::models::DECISION_COLS, placeholders)
        };
        let mut stmt = conn.prepare(&sql)?;
        let mut p = Vec::<&dyn rusqlite::ToSql>::new();
        for id in &decision_ids {
            p.push(id);
        }
        let rows = stmt.query_map(rusqlite::params_from_iter(p), |row| {
            let tags_str: Option<String> = row.get("tags")?;
            let tags = match tags_str {
                Some(s) => serde_json::from_str(&s).unwrap_or(Value::Null),
                None => Value::Null,
            };
            Ok(Decision {
                id: row.get("id")?,
                uuid: row.get("uuid")?,
                summary: row.get("summary")?,
                rationale: row.get("rationale")?,
                implementation_details: row.get("implementation_details")?,
                tags: if tags.is_null() { None } else { Some(tags) },
                timestamp: row.get("timestamp")?,
                status: row.get("status")?,
                commit_sha: row.get("commit_sha")?,
                pr_urls: Vec::new(),
                anchors: Vec::new(),
                importance: row.get("importance")?,
                access_count: row.get("access_count")?,
                last_accessed_at: row.get("last_accessed_at")?,
                archived: row.get("archived")?,
                contract: row.get("contract")?,
                score: Some(row.get("score")?),
            })
        })?;
        for r in rows {
            let mut d = r?;
            d.pr_urls = crate::ops::pr::pr_urls_for(conn, "decision", d.id)?;
            d.anchors = anchors_for(conn, "decision", d.id)?;
            decisions.push(d);
        }
    }

    let mut patterns = Vec::new();
    if !pattern_ids.is_empty() {
        let placeholders = crate::ops::sql_placeholders(pattern_ids.len());
        let pat_score = format!(
            "({} * {})",
            crate::ops::scoring::score_expr("timestamp", "importance"),
            crate::ops::scoring::confidence_expr("confidence", "last_confirmed_at", "timestamp")
        );
        let sql = format!("SELECT id, uuid, name, description, tags, timestamp, check_kind, check_expr, severity, importance, access_count, last_accessed_at, archived, confidence, last_confirmed_at, {pat_score} AS score FROM system_patterns WHERE id IN ({}) AND archived = 0 ORDER BY score DESC", placeholders);
        let mut stmt = conn.prepare(&sql)?;
        let mut p = Vec::<&dyn rusqlite::ToSql>::new();
        for id in &pattern_ids {
            p.push(id);
        }
        let rows = stmt.query_map(rusqlite::params_from_iter(p), |row| {
            let tags_str: Option<String> = row.get(4)?;
            let tags = match tags_str {
                Some(s) => serde_json::from_str(&s).unwrap_or(Value::Null),
                None => Value::Null,
            };
            Ok(Pattern {
                id: row.get(0)?,
                uuid: row.get(1)?,
                name: row.get(2)?,
                description: row.get(3)?,
                tags: if tags.is_null() { None } else { Some(tags) },
                timestamp: row.get(5)?,
                check_kind: row.get(6)?,
                check_expr: row.get(7)?,
                severity: row.get(8)?,
                pr_urls: Vec::new(),
                anchors: Vec::new(),
                importance: row.get(9)?,
                access_count: row.get(10)?,
                last_accessed_at: row.get(11)?,
                archived: row.get(12)?,
                confidence: row.get(13)?,
                effective_confidence: crate::ops::scoring::effective_confidence(
                    row.get::<_, f64>(13)?,
                    row.get::<_, Option<String>>(14)?.as_deref(),
                    &row.get::<_, String>(5)?,
                ),
                last_confirmed_at: row.get(14)?,
                score: Some(row.get(15)?),
            })
        })?;
        for r in rows {
            let mut pat = r?;
            pat.pr_urls = crate::ops::pr::pr_urls_for(conn, "system_pattern", pat.id)?;
            pat.anchors = anchors_for(conn, "system_pattern", pat.id)?;
            patterns.push(pat);
        }
    }
    // Reinforce-on-read (v0.10.0).
    crate::ops::scoring::reinforce(
        conn,
        "decisions",
        &decisions.iter().map(|d| d.id).collect::<Vec<_>>(),
    )?;
    crate::ops::scoring::reinforce(
        conn,
        "system_patterns",
        &patterns.iter().map(|p| p.id).collect::<Vec<_>>(),
    )?;

    // Tier-4 telemetry: one usage row per retrieval call.
    crate::ops::usage::record(
        conn,
        "relevant",
        &cleaned_paths.join(","),
        decisions.len(),
        false,
    );

    let mut out = serde_json::json!({
        "decisions": decisions,
        "patterns": patterns,
    });
    // Staleness drift (2.3): per-decision trust signal — anchored files
    // committed after the decision. Null (stripped on read) when no signal.
    if let Some(root) = root {
        let mut drift = crate::ops::drift::Drift::scan(root);
        if let Some(arr) = out.get_mut("decisions").and_then(Value::as_array_mut) {
            for d in arr.iter_mut() {
                let id = d.get("id").and_then(Value::as_i64);
                let ts = d.get("timestamp").and_then(Value::as_str);
                if let (Some(id), Some(ts)) = (id, ts) {
                    let sha = d.get("commit_sha").and_then(Value::as_str);
                    let report = drift.report(conn, "decision", id, ts, sha);
                    if !report.is_null() {
                        d["drift"] = report;
                    }
                }
            }
        }
    }
    Ok(out)
}

fn discover_child_repos(ws_root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(l1) = std::fs::read_dir(ws_root) else {
        return out;
    };
    for e1 in l1.flatten() {
        let p1 = e1.path();
        let Some(n1) = p1.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if n1.starts_with('.') || n1 == "target" || n1 == "node_modules" || n1 == "engrams" {
            continue;
        }
        if p1.is_dir() {
            out.push(n1.to_string());
            if let Ok(l2) = std::fs::read_dir(&p1) {
                for e2 in l2.flatten() {
                    let p2 = e2.path();
                    let Some(n2) = p2.file_name().and_then(|n| n.to_str()) else {
                        continue;
                    };
                    if n2.starts_with('.') || n2.ends_with(".worktrees") || !p2.is_dir() {
                        continue;
                    }
                    out.push(format!("{n1}/{n2}"));
                }
            }
        }
    }
    out.sort();
    out
}

/// Normalize stored paths across `item_anchors`, `schemas.scope_json`, and
/// `schema_proposals.scope_json`, recompute active schema centroids, and
/// refresh derived anchor graph edges (`doctor --fix` / Idea 4).
pub fn normalize_stored_paths(conn: &mut Connection, root: Option<&Path>) -> Result<Value> {
    let mut anchors_updated = 0usize;
    let mut anchors_deduped = 0usize;
    let mut schemas_updated = 0usize;
    let mut proposals_updated = 0usize;
    let mut unresolved_anchors = Vec::new();

    let child_repos = root.map(discover_child_repos).unwrap_or_default();
    let tx = conn.unchecked_transaction()?;

    // 1. Normalize `item_anchors.path`, deduplicating collisions on `(item_type, item_id, path)`.
    let rows: Vec<(i64, String, i64, String)> = {
        let mut stmt =
            conn.prepare("SELECT id, item_type, item_id, path FROM item_anchors ORDER BY id ASC")?;
        let mapped = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?;
        mapped.collect::<rusqlite::Result<Vec<_>>>()?
    };

    // Collect known canonical paths so legacy subproject-relative paths can
    // also check if they match a unique known prefix when not ambiguous.
    let mut known_canonical: Vec<String> = rows
        .iter()
        .map(|(_, _, _, p)| resolve_workspace_path_with_cwd(p, None, root).logical)
        .collect();
    known_canonical.sort();
    known_canonical.dedup();

    for (row_id, item_type, item_id, old_path) in rows {
        let mut norm = resolve_workspace_path_with_cwd(&old_path, None, root).logical;
        if let Some(ws_root) = root {
            if !Path::new(&norm).is_absolute()
                && !norm.starts_with('~')
                && !ws_root.join(&norm).exists()
            {
                // Check if `norm` uniquely resolves under a child repository on disk
                // or matches a unique known canonical suffix.
                let suffix = format!("/{norm}");
                let mut candidates: Vec<String> = child_repos
                    .iter()
                    .filter(|sub| ws_root.join(sub).join(&norm).exists())
                    .map(|sub| format!("{sub}/{norm}"))
                    .collect();
                for kc in &known_canonical {
                    if kc.ends_with(&suffix) && !candidates.contains(kc) {
                        candidates.push(kc.clone());
                    }
                }
                if candidates.len() == 1 {
                    norm = candidates.remove(0);
                } else if candidates.len() > 1 {
                    unresolved_anchors.push(serde_json::json!({
                        "item_type": item_type,
                        "item_id": item_id,
                        "path": norm,
                        "reason": "ambiguous",
                        "candidates": candidates,
                    }));
                } else {
                    unresolved_anchors.push(serde_json::json!({
                        "item_type": item_type,
                        "item_id": item_id,
                        "path": norm,
                        "reason": "not_found",
                    }));
                }
            }
        }
        if norm != old_path {
            let existing: i64 = conn.query_row(
                "SELECT COUNT(*) FROM item_anchors WHERE item_type = ?1 AND item_id = ?2 AND path = ?3",
                params![item_type, item_id, norm],
                |r| r.get(0),
            )?;
            if existing > 0 {
                conn.execute("DELETE FROM item_anchors WHERE id = ?1", params![row_id])?;
                anchors_deduped += 1;
            } else {
                conn.execute(
                    "UPDATE item_anchors SET path = ?1 WHERE id = ?2",
                    params![norm, row_id],
                )?;
                anchors_updated += 1;
            }
        }
    }

    // 2. Normalize `schemas.scope_json` paths and refresh centroids.
    let schema_rows: Vec<(i64, String)> = {
        let mut stmt = conn.prepare("SELECT id, scope_json FROM schemas ORDER BY id ASC")?;
        let mapped = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        mapped.collect::<rusqlite::Result<Vec<_>>>()?
    };
    for (sid, scope_json) in &schema_rows {
        let mut scope = crate::ops::schemas::retrieval::parse_scope(scope_json);
        let mut norm_paths: Vec<String> = Vec::with_capacity(scope.paths.len());
        let mut changed = false;
        for p in &scope.paths {
            let np = resolve_workspace_path_with_cwd(p, None, root).logical;
            if &np != p {
                changed = true;
            }
            if !norm_paths.contains(&np) {
                norm_paths.push(np);
            } else {
                changed = true;
            }
        }
        if changed {
            scope.paths = norm_paths;
            let new_json = serde_json::to_string(&scope)?;
            conn.execute(
                "UPDATE schemas SET scope_json = ?1 WHERE id = ?2",
                params![new_json, sid],
            )?;
            schemas_updated += 1;
        }
        crate::ops::schemas::confirm::refresh_schema_centroid(conn, *sid)?;
    }

    // 3. Normalize `schema_proposals.scope_json` paths.
    let prop_rows: Vec<(i64, String)> = {
        let mut stmt =
            conn.prepare("SELECT id, scope_json FROM schema_proposals ORDER BY id ASC")?;
        let mapped = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        mapped.collect::<rusqlite::Result<Vec<_>>>()?
    };
    for (pid, scope_json) in prop_rows {
        let mut scope = crate::ops::schemas::retrieval::parse_scope(&scope_json);
        let mut norm_paths: Vec<String> = Vec::with_capacity(scope.paths.len());
        let mut changed = false;
        for p in &scope.paths {
            let np = resolve_workspace_path_with_cwd(p, None, root).logical;
            if &np != p {
                changed = true;
            }
            if !norm_paths.contains(&np) {
                norm_paths.push(np);
            } else {
                changed = true;
            }
        }
        if changed {
            scope.paths = norm_paths;
            let new_json = serde_json::to_string(&scope)?;
            conn.execute(
                "UPDATE schema_proposals SET scope_json = ?1 WHERE id = ?2",
                params![new_json, pid],
            )?;
            proposals_updated += 1;
        }
    }
    // 4. Normalize manual `context_links.relationship_type` synonyms/hyphens (Idea 8).
    let mut links_normalized = 0usize;
    let link_rows: Vec<(i64, String, String, String, String, String)> = {
        let mut stmt = tx.prepare(
            "SELECT id, source_item_type, source_item_id, target_item_type, target_item_id, relationship_type \
             FROM context_links WHERE origin = 'manual' ORDER BY id ASC",
        )?;
        let mapped = stmt.query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
            ))
        })?;
        mapped.collect::<rusqlite::Result<Vec<_>>>()?
    };
    for (lid, src_t, src_id, tgt_t, tgt_id, rel) in link_rows {
        let (canon_rel, swap) = crate::ops::graph::rel::normalize(&rel);
        if canon_rel != rel || swap {
            let (ns_t, ns_id, nt_t, nt_id) = if swap {
                (tgt_t, tgt_id, src_t, src_id)
            } else {
                (src_t, src_id, tgt_t, tgt_id)
            };
            tx.execute(
                "UPDATE context_links SET source_item_type = ?1, source_item_id = ?2, \
                 target_item_type = ?3, target_item_id = ?4, relationship_type = ?5 WHERE id = ?6",
                params![ns_t, ns_id, nt_t, nt_id, canon_rel, lid],
            )?;
            links_normalized += 1;
        }
    }

    // 5. Decline legacy auto-fired `progress-entry` rows in `schema_suggestions` (Idea 2).
    let progress_suggestions_declined = tx.execute(
        "UPDATE schema_suggestions SET status = 'declined' \
         WHERE status = 'suggested' AND item_kind IN ('progress-entry', 'progress_entry')",
        [],
    )?;

    tx.commit()?;
    if anchors_updated > 0 || anchors_deduped > 0 || links_normalized > 0 {
        let opts = crate::ops::graph::rebuild::RebuildOpts {
            no_git: true,
            min_cochange: 2,
            max_commits: 200,
        };
        crate::ops::graph::rebuild::rebuild(conn, &opts, root)?;
    }

    Ok(serde_json::json!({
        "anchors_updated": anchors_updated,
        "anchors_deduped": anchors_deduped,
        "schemas_updated": schemas_updated,
        "proposals_updated": proposals_updated,
        "links_normalized": links_normalized,
        "progress_suggestions_declined": progress_suggestions_declined,
        "unresolved_anchors": unresolved_anchors,
    }))
}
