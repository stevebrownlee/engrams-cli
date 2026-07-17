use anyhow::{Context, Result};
use rusqlite::Connection;
use std::env;
use std::path::{Path, PathBuf};

use crate::schema::SCHEMA;

pub const LATEST_VERSION: i32 = 3;

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

pub fn resolve_db_path(db_arg: Option<&str>, workspace_arg: Option<&str>) -> Result<PathBuf> {
    if let Some(path) = db_arg {
        return Ok(PathBuf::from(path));
    }

    if let Some(workspace) = workspace_arg {
        return Ok(Path::new(workspace).join("engrams").join("context.db"));
    }

    // Auto-detect workspace
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
            return Ok(current.join("engrams").join("context.db"));
        }

        match current.parent() {
            Some(parent) => {
                current = parent.to_path_buf();
                check_path = current.clone();
            }
            None => return Ok(cwd.join("engrams").join("context.db")), // Default to cwd if none found
        }
    }
}

pub fn open(db_path: &Path) -> Result<Connection> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let conn = Connection::open(db_path)?;
    conn.pragma_update(None, "foreign_keys", "ON")?;

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
