use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::env;

use crate::schema::SCHEMA;

pub fn resolve_db_path(db_arg: Option<&str>, workspace_arg: Option<&str>) -> Result<PathBuf> {
    if let Some(path) = db_arg {
        return Ok(PathBuf::from(path));
    }
    
    if let Some(workspace) = workspace_arg {
        return Ok(Path::new(workspace).join("engrams").join("context.db"));
    }
    
    // Auto-detect workspace
    let cwd = env::current_dir().context("Failed to get current directory")?;
    let mut current = cwd.as_path();
    
    loop {
        if current.join(".engrams").exists() ||
           current.join("engrams/context.db").exists() ||
           current.join(".git").exists() ||
           current.join("pyproject.toml").exists() ||
           current.join("package.json").exists() ||
           current.join("Cargo.toml").exists() ||
           current.join("go.mod").exists()
        {
            return Ok(current.join("engrams").join("context.db"));
        }
        
        match current.parent() {
            Some(parent) => current = parent,
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
    conn.execute_batch(SCHEMA)?;
    Ok(conn)
}
