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

fn resolve_git_worktree(path: &Path) -> Option<PathBuf> {
    if !path.is_file() || path.file_name()?.to_str()? != ".git" {
        return None;
    }
    let content = std::fs::read_to_string(path).ok()?;
    let content = content.trim();
    if !content.starts_with("gitdir:") {
        return None;
    }
    let gitdir_str = content.strip_prefix("gitdir:")?.trim();
    let mut gitdir = PathBuf::from(gitdir_str);
    if gitdir.is_relative() {
        let git_file_parent = path.parent()?;
        gitdir = git_file_parent.join(gitdir);
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
    let mut current = cwd.clone();
    let mut check_path = current.clone();

    loop {
        let mut found = false;
        let mut found_worktree = None;
        for marker in &[
            ".engrams",
            "engrams/context.db",
            ".git",
            "pyproject.toml",
            "package.json",
            "Cargo.toml",
            "go.mod",
        ] {
            if *marker == "engrams/context.db" {
                check_path.push("engrams");
                check_path.push("context.db");
                let exists = check_path.exists();
                check_path.pop();
                check_path.pop();
                if exists {
                    found = true;
                    break;
                }
            } else {
                check_path.push(marker);
                let exists = check_path.exists();
                if exists && *marker == ".git" {
                    if let Some(worktree_root) = resolve_git_worktree(&check_path) {
                        found_worktree = Some(worktree_root);
                    }
                }
                check_path.pop();
                if exists && found_worktree.is_none() {
                    found = true;
                    break;
                }
            }
        }

        if let Some(worktree_root) = found_worktree {
            current = worktree_root;
            check_path = current.clone();
            continue;
        }

        if found {
            return Ok(current);
        }

        match current.parent() {
            Some(parent) => {
                current = parent.to_path_buf();
                check_path = current.clone();
            }
            None => return Ok(cwd), // Default to cwd if none found
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
