use crate::schema::SCHEMA;
use anyhow::{Context, Result};
use rusqlite::Connection;
use std::env;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub const LATEST_VERSION: i32 = 17;

pub fn get_user_version(conn: &Connection) -> Result<i32> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    Ok(version)
}

pub fn set_user_version(conn: &Connection, version: i32) -> Result<()> {
    conn.execute(&format!("PRAGMA user_version = {}", version), [])?;
    Ok(())
}

pub fn has_tables(conn: &Connection) -> Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
        [],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

pub fn validate_version(conn: &Connection, is_migrate_or_init: bool) -> Result<()> {
    let user_ver = get_user_version(conn)?;
    if user_ver < LATEST_VERSION {
        if !is_migrate_or_init {
            anyhow::bail!(
                "Database schema is out of date (version {}, latest is {}). Please run 'engrams migrate' to upgrade.",
                user_ver,
                LATEST_VERSION
            );
        }
    } else if user_ver > LATEST_VERSION {
        anyhow::bail!(
            "Database schema is newer than this CLI version (version {}, CLI supports up to {}). Please upgrade the engrams CLI.",
            user_ver,
            LATEST_VERSION
        );
    }
    Ok(())
}

pub fn run_migrations(conn: &mut Connection) -> Result<()> {
    let current = get_user_version(conn)?;
    if current >= LATEST_VERSION {
        return Ok(());
    }

    let tx = conn.transaction()?;
    for v in (current + 1)..=LATEST_VERSION {
        match v {
            2 => {
                tx.execute_batch(crate::schema::MIGRATION_V2)?;
            }
            3 => {
                tx.execute_batch(crate::schema::MIGRATION_V3)?;
            }
            4 => {
                tx.execute_batch(crate::schema::MIGRATION_V4)?;
            }
            5 => {
                tx.execute_batch(crate::schema::MIGRATION_V5)?;
            }
            6 => {
                tx.execute_batch(crate::schema::MIGRATION_V6)?;
            }
            7 => {
                tx.execute_batch(crate::schema::MIGRATION_V7)?;
            }
            8 => {
                tx.execute_batch(crate::schema::MIGRATION_V8)?;
            }
            9 => {
                tx.execute_batch(crate::schema::MIGRATION_V9)?;
            }
            10 => {
                tx.execute_batch(crate::schema::MIGRATION_V10)?;
            }
            11 => {
                tx.execute_batch(crate::schema::MIGRATION_V11)?;
            }
            12 => {
                tx.execute_batch(crate::schema::MIGRATION_V12)?;
            }
            13 => {
                tx.execute_batch(crate::schema::MIGRATION_V13)?;
            }
            14 => {
                tx.execute_batch(crate::schema::MIGRATION_V14)?;
            }
            15 => {
                tx.execute_batch(crate::schema::MIGRATION_V15)?;
            }
            16 => {
                tx.execute_batch(crate::schema::MIGRATION_V16)?;
            }
            17 => {
                tx.execute_batch(crate::schema::MIGRATION_V17)?;
            }
            _ => anyhow::bail!("Unknown migration version {}", v),
        }
    }
    tx.execute(&format!("PRAGMA user_version = {}", LATEST_VERSION), [])?;
    tx.commit()?;
    Ok(())
}

pub fn resolve_git_worktree(path: &Path) -> Option<PathBuf> {
    let git_file = if path.file_name().and_then(|n| n.to_str()) == Some(".git") {
        path.to_path_buf()
    } else {
        path.join(".git")
    };
    if !git_file.is_file() {
        return None;
    }
    let content = std::fs::read_to_string(&git_file).ok()?;
    let gitdir_str = content.trim().strip_prefix("gitdir:")?.trim();
    let mut gitdir = PathBuf::from(gitdir_str);
    if gitdir.is_relative() {
        gitdir = git_file.parent()?.join(gitdir);
    }
    let gitdir = std::fs::canonicalize(gitdir).ok()?;
    let parent = gitdir.parent()?;
    if parent.file_name()?.to_str()? == "worktrees" {
        let main_repo = parent.parent()?.parent()?;
        if main_repo.exists() {
            return Some(main_repo.to_path_buf());
        }
    }
    None
}

/// If `dir` sits inside a `<name>.worktrees/<branch>` directory (or is a git
/// worktree with a `.git` file), map it to the corresponding `<name>` main
/// checkout path when that checkout exists.
pub fn fold_worktree_dir(dir: &Path) -> Option<PathBuf> {
    if let Some(main_repo) = resolve_git_worktree(dir) {
        return Some(main_repo);
    }
    let mut prefix = PathBuf::new();
    let mut comps = dir.components().peekable();
    let mut folded = false;
    while let Some(c) = comps.next() {
        if let std::path::Component::Normal(os) = c {
            if let Some(s) = os.to_str() {
                if let Some(stem) = s.strip_suffix(".worktrees") {
                    if !stem.is_empty() && comps.peek().is_some() {
                        let _branch = comps.next();
                        prefix.push(stem);
                        folded = true;
                        continue;
                    }
                }
            }
        }
        prefix.push(c.as_os_str());
    }
    folded.then_some(prefix)
}

fn has_engrams_store(dir: &Path) -> bool {
    dir.join("engrams").join("context.db").exists() || dir.join(".engrams").exists()
}

/// Walk upward from `start_dir` (and across any git worktree link to its main
/// checkout and that checkout's ancestors) looking strictly for an existing
/// `engrams/context.db` or `.engrams` marker.
pub fn find_existing_store_root(start_dir: &Path) -> Option<PathBuf> {
    let mut cur = Some(start_dir.to_path_buf());
    while let Some(dir) = cur {
        if has_engrams_store(&dir) {
            return Some(dir);
        }
        if let Some(wt_main) = fold_worktree_dir(&dir) {
            let mut wt_cur = Some(wt_main);
            while let Some(wdir) = wt_cur {
                if has_engrams_store(&wdir) {
                    return Some(wdir);
                }
                wt_cur = wdir.parent().map(Path::to_path_buf);
            }
        }
        cur = dir.parent().map(Path::to_path_buf);
    }
    None
}

/// If `target_root` does not have an existing store yet, check whether any
/// strict parent (or worktree main-checkout ancestor) already owns one.
pub fn find_parent_store_root(target_root: &Path) -> Option<PathBuf> {
    let canonical_target =
        std::fs::canonicalize(target_root).unwrap_or_else(|_| target_root.to_path_buf());
    if let Some(wt_main) = fold_worktree_dir(&canonical_target) {
        if let Some(found) = find_existing_store_root(&wt_main) {
            if std::fs::canonicalize(&found).unwrap_or_else(|_| found.clone()) != canonical_target {
                return Some(found);
            }
        }
    }
    let parent = canonical_target.parent()?;
    find_existing_store_root(parent)
}

pub struct Workspace {
    pub db_path: PathBuf,
    pub root: Option<PathBuf>,
    head_sha: OnceLock<Option<String>>,
}

impl Workspace {
    pub fn new(db_path: PathBuf, root: Option<PathBuf>) -> Self {
        Self {
            db_path,
            root,
            head_sha: OnceLock::new(),
        }
    }

    pub fn head_sha(&self) -> Option<String> {
        self.head_sha
            .get_or_init(|| {
                let root = self.root.as_deref()?;
                crate::ops::git::head_sha(root)
            })
            .clone()
    }

    /// Resolve `HEAD` SHA from the git repository owning the anchored files
    /// when all anchors share one logical repo prefix (so child repos under an
    /// umbrella workspace stamp their own commit SHA), falling back to
    /// `self.head_sha()`.
    pub fn head_sha_for_anchors(&self, anchors: &[String]) -> Option<String> {
        let root = self.root.as_deref()?;
        if anchors.is_empty() {
            return self.head_sha();
        }
        let mut resolved_list: Vec<crate::ops::anchor::ResolvedPath> = anchors
            .iter()
            .map(|a| crate::ops::anchor::resolve_workspace_path(a, Some(root)))
            .collect();
        resolved_list.sort_by(|a, b| a.logical.cmp(&b.logical));
        let first_prefix =
            crate::ops::graph::louvain::anchor_repo_prefix(&resolved_list[0].logical);
        if resolved_list
            .iter()
            .any(|r| crate::ops::graph::louvain::anchor_repo_prefix(&r.logical) != first_prefix)
        {
            return self.head_sha();
        }
        if let Some(phys) = &resolved_list[0].physical {
            let mut dir = if phys.is_dir() {
                Some(phys.as_path())
            } else {
                phys.parent()
            };
            while let Some(d) = dir {
                if d.exists() {
                    if let Some(sha) = crate::ops::git::head_sha(d) {
                        return Some(sha);
                    }
                    break;
                }
                dir = d.parent();
            }
        }
        if !first_prefix.is_empty() {
            let sub_repo = resolve_active_subrepo_root(root, first_prefix);
            if sub_repo.join(".git").exists() {
                if let Some(sha) = crate::ops::git::head_sha(&sub_repo) {
                    return Some(sha);
                }
            }
        }
        self.head_sha()
    }
}

/// Resolve the physical git repository root for `repo_prefix` under `ws_root`:
/// if `cwd` sits inside a git worktree whose folded logical path starts with
/// `repo_prefix`, return that worktree's checkout root so `HEAD` and `git diff`
/// inspect the active worktree branch rather than the main checkout.
pub fn resolve_active_subrepo_root(ws_root: &Path, repo_prefix: &str) -> PathBuf {
    if let Ok(cwd) = env::current_dir() {
        let resolved_cwd = crate::ops::anchor::resolve_workspace_path_with_cwd(
            &cwd.to_string_lossy(),
            Some(&cwd),
            Some(ws_root),
        );
        if resolved_cwd.logical == repo_prefix
            || resolved_cwd
                .logical
                .strip_prefix(repo_prefix)
                .is_some_and(|rest| rest.starts_with('/'))
        {
            if let Ok(top) = crate::ops::git::toplevel(&cwd) {
                let top_path = PathBuf::from(top);
                if top_path.exists() {
                    return top_path;
                }
            }
        }
    }
    ws_root.join(repo_prefix)
}

pub fn resolve_workspace(db_arg: Option<&str>, workspace_arg: Option<&str>) -> Result<Workspace> {
    // ENGRAMS_DB routes every command at an explicit database without a
    // per-invocation flag; semantics are identical to --db, including
    // creating a nonexistent path on first use. An empty value is unset.
    let db_override = match db_arg {
        Some(path) => Some(PathBuf::from(path)),
        None => env::var("ENGRAMS_DB")
            .ok()
            .filter(|p| !p.is_empty())
            .map(PathBuf::from),
    };

    if let Some(workspace) = workspace_arg {
        let root = PathBuf::from(workspace);
        let db_path = db_override.unwrap_or_else(|| root.join("engrams").join("context.db"));
        return Ok(Workspace::new(db_path, Some(root)));
    }

    if let Some(db_path) = db_override {
        let root = workspace_root_from_db(&db_path);
        return Ok(Workspace::new(db_path, root));
    }

    let root = discover_workspace_root()?;
    let db_path = root.join("engrams").join("context.db");
    Ok(Workspace::new(db_path, Some(root)))
}

fn discover_workspace_root() -> Result<PathBuf> {
    let cwd = env::current_dir().context("Failed to get current directory")?;
    Ok(discover_workspace_root_from(&cwd))
}

pub fn discover_workspace_root_from(cwd: &Path) -> PathBuf {
    // Pass 1: Walk all ancestors (and worktree main-checkout ancestors) looking
    // strictly for an existing `engrams/context.db` or `.engrams` store so child
    // `package.json`, `Cargo.toml`, or `.git` markers never shadow a parent DB.
    if let Some(store_root) = find_existing_store_root(cwd) {
        return store_root;
    }

    // Pass 2: No existing store in any ancestor; fall back to project markers.
    let mut current = cwd.to_path_buf();
    loop {
        if let Some(worktree_root) = fold_worktree_dir(&current) {
            if worktree_root != current && worktree_root.exists() {
                if let Some(store_root) = find_existing_store_root(&worktree_root) {
                    return store_root;
                }
                current = worktree_root;
                continue;
            }
        }

        for marker in &[
            ".git",
            "pyproject.toml",
            "package.json",
            "Cargo.toml",
            "go.mod",
        ] {
            if current.join(marker).exists() {
                return current;
            }
        }

        match current.parent() {
            Some(parent) => current = parent.to_path_buf(),
            None => return cwd.to_path_buf(),
        }
    }
}

/// Derive the workspace root from a resolved db path of the form `<root>/engrams/context.db`.
/// Returns `None` for `--db` paths that don't follow this convention.
pub fn workspace_root_from_db(db_path: &Path) -> Option<PathBuf> {
    if db_path.file_name()?.to_str()? != "context.db" {
        return None;
    }
    let engrams_dir = db_path.parent()?;
    if engrams_dir.file_name()?.to_str()? != "engrams" {
        return None;
    }
    let root = engrams_dir.parent()?;
    if root.as_os_str().is_empty() {
        return Some(PathBuf::from("."));
    }
    Some(root.to_path_buf())
}

pub fn open(db_path: &Path) -> Result<Connection> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let conn = Connection::open(db_path)?;
    conn.pragma_update(None, "foreign_keys", "ON")?;

    // Register exp() — bundled SQLite may lack math functions; used by scoring.rs
    conn.create_scalar_function(
        "exp",
        1,
        rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            let x: f64 = ctx.get(0)?;
            Ok(x.exp())
        },
    )?;

    let has_tbls = has_tables(&conn)?;
    let user_ver = get_user_version(&conn)?;

    if !has_tbls {
        conn.execute_batch(SCHEMA)?;
        set_user_version(&conn, LATEST_VERSION)?;
    } else if user_ver == 0 {
        set_user_version(&conn, LATEST_VERSION)?;
    }

    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migration_v17_replaces_candidate_notified_at_with_prompt_notices() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::schema::SCHEMA).unwrap();
        // Baseline SCHEMA is DDL only; open() stamps user_version separately.
        set_user_version(&conn, LATEST_VERSION).unwrap();
        let user_ver = get_user_version(&conn).unwrap();
        assert_eq!(user_ver, 17);

        let has_cand_notified: bool = conn
            .prepare("PRAGMA table_info(schema_candidates)")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .any(|c| c.as_deref() == Ok("notified_at"));
        assert!(
            !has_cand_notified,
            "schema_candidates.notified_at must be dropped in v17 baseline"
        );

        let notice_cols: Vec<String> = conn
            .prepare("PRAGMA table_info(schema_prompt_notices)")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .map(|c| c.unwrap())
            .collect();
        assert_eq!(
            notice_cols,
            vec!["source_key", "evidence_json", "notified_at"]
        );
    }

    #[test]
    fn test_proposal_migration_v16_adds_table_and_schema_columns() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::schema::SCHEMA).unwrap();
        let schema_cols: Vec<String> = conn
            .prepare("PRAGMA table_info(schemas)")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .map(|c| c.unwrap())
            .collect();
        assert!(schema_cols.contains(&"statement".to_string()));
        assert!(schema_cols.contains(&"scope_json".to_string()));

        let prop_cols: Vec<String> = conn
            .prepare("PRAGMA table_info(schema_proposals)")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .map(|c| c.unwrap())
            .collect();
        for col in [
            "id",
            "uuid",
            "name",
            "statement",
            "summary",
            "scope_json",
            "members_json",
            "contrary_json",
            "evidence_json",
            "source_kind",
            "source_ref",
            "session",
            "status",
            "decision_note",
            "schema_id",
            "created_at",
            "decided_at",
            "lens",
        ] {
            assert!(
                prop_cols.contains(&col.to_string()),
                "schema_proposals missing column {col}"
            );
        }
    }

    #[test]
    fn test_migration_v17_carries_over_suppressed_schema_candidates_ac12() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::schema::SCHEMA).unwrap();
        // Rewind to v16 shape: drop schema_prompt_notices, drop schema_proposals.lens,
        // and re-add schema_candidates.notified_at.
        conn.execute_batch(
            "DROP TABLE schema_prompt_notices;
             ALTER TABLE schema_proposals DROP COLUMN lens;
             ALTER TABLE schema_candidates ADD COLUMN notified_at TEXT;
             PRAGMA user_version = 16;",
        )
        .unwrap();

        let insert_cand =
            |sig: &str, members: &str, kind: &str, last_seen: &str, notified: Option<&str>| {
                conn.execute(
                    "INSERT INTO schema_candidates \
                 (cluster_sig, member_keys_json, density, stability_count, reward_hits, \
                  first_seen_at, last_seen_at, kind, notified_at) \
                 VALUES (?1, ?2, 0.8, 3, 1, '2026-10-01T00:00:00Z', ?3, ?4, ?5)",
                    rusqlite::params![sig, members, last_seen, kind, notified],
                )
                .unwrap();
            };

        // 1. Suppressed schema candidate (notified_at == last_seen_at) with non-knowledge refs to strip
        insert_cand(
            "code:1,decision:1,decision:2,pr:7,schema:3",
            r#"["code:1","decision:1","decision:2","pr:7","schema:3"]"#,
            "schema",
            "2026-10-10T10:00:00Z",
            Some("2026-10-10T10:00:00Z"),
        );
        // 2. Suppressed schema candidate (notified_at > last_seen_at)
        insert_cand(
            "decision:3,decision:4,system_pattern:1",
            r#"["decision:3","decision:4","system_pattern:1"]"#,
            "schema",
            "2026-10-10T10:00:00Z",
            Some("2026-10-10T12:00:00Z"),
        );
        // 3. Refreshed schema candidate (notified_at < last_seen_at) -> must NOT carry over
        insert_cand(
            "decision:5,decision:6",
            r#"["decision:5","decision:6"]"#,
            "schema",
            "2026-10-11T00:00:00Z",
            Some("2026-10-10T12:00:00Z"),
        );
        // 4. Never notified schema candidate -> must NOT carry over
        insert_cand(
            "decision:7,decision:8",
            r#"["decision:7","decision:8"]"#,
            "schema",
            "2026-10-10T10:00:00Z",
            None,
        );
        // 5. Non-schema kind with notified_at >= last_seen_at -> must NOT carry over
        insert_cand(
            "decision:9,decision:10",
            r#"["decision:9","decision:10"]"#,
            "unclear",
            "2026-10-10T10:00:00Z",
            Some("2026-10-10T12:00:00Z"),
        );

        run_migrations(&mut conn).unwrap();
        assert_eq!(get_user_version(&conn).unwrap(), 17);

        let has_cand_notified: bool = conn
            .prepare("PRAGMA table_info(schema_candidates)")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .any(|c| c.as_deref() == Ok("notified_at"));
        assert!(!has_cand_notified);

        let notices: Vec<(String, String, String)> = conn
            .prepare(
                "SELECT source_key, evidence_json, notified_at \
                 FROM schema_prompt_notices ORDER BY source_key ASC",
            )
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();

        assert_eq!(
            notices,
            vec![
                (
                    "candidate:code:1,decision:1,decision:2,pr:7,schema:3".to_string(),
                    r#"["decision:1","decision:2"]"#.to_string(),
                    "2026-10-10T10:00:00Z".to_string(),
                ),
                (
                    "candidate:decision:3,decision:4,system_pattern:1".to_string(),
                    r#"["decision:3","decision:4","system_pattern:1"]"#.to_string(),
                    "2026-10-10T12:00:00Z".to_string(),
                ),
            ]
        );
    }
}
