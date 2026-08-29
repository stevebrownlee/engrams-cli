mod cli;
mod db;
mod models;
mod ops;
mod release;
mod schema;

use anyhow::Result;
use clap::Parser;
use std::io::Write;

fn main() {
    if let Err(e) = run() {
        eprintln!(r#"{{"error": "{}"}}"#, e.to_string().replace("\"", "\\\""));
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = cli::Cli::parse();

    if matches!(cli.command, cli::Command::Instructions) {
        print!("{}", include_str!("assets/instructions.md"));
        return Ok(());
    }

    let db_path = db::resolve_db_path(cli.db.as_deref(), cli.workspace.as_deref())?;

    let db_existed = db_path.exists();

    // We open (and create) the db for all commands.
    let mut conn = db::open(&db_path)?;

    // Instantiate UpdateChecker (spawns background thread if check is needed)
    let checker = release::UpdateChecker::new(&db_path);

    let is_migrate_or_init = matches!(cli.command, cli::Command::Migrate | cli::Command::Init);
    let is_check = matches!(cli.command, cli::Command::Check { .. });
    db::validate_version(&conn, is_migrate_or_init)?;

    let mut result = ops::dispatch(&mut conn, cli.command, &db_path, !db_existed)?;

    if !cli.fields.is_empty() {
        project_fields(&mut result, &cli.fields);
    }

    let mut out = std::io::stdout().lock();
    if cli.compact {
        strip_nulls(&mut result);
        serde_json::to_writer(&mut out, &result)?;
    } else {
        serde_json::to_writer_pretty(&mut out, &result)?;
    }
    out.write_all(b"\n")?;
    if is_check {
        if let Some(arr) = result.get("violations").and_then(|v| v.as_array()) {
            if !arr.is_empty() {
                // Exit 1 *after* printing the violations JSON so CI catches it
                // without an error-shape output obscuring the findings.
                checker.print_notification();
                std::process::exit(1);
            }
        }
    }
    // Print notification if a new version is available
    checker.print_notification();

    Ok(())
}

/// Canonical-name fallbacks: request `summary`/`name`, get a hit's `title` value.
const FIELD_ALIASES: &[(&str, &str)] = &[("summary", "title"), ("name", "title")];

fn project_fields(val: &mut serde_json::Value, fields: &[String]) {
    match val {
        serde_json::Value::Object(map) => {
            // Miss-guidance (query's empty-result advice) survives projection:
            // an agent requesting narrow fields still needs re-targeting help.
            let guidance = map.remove("miss_guidance");
            let mut new_map = serde_json::Map::new();
            for f in fields {
                if let Some(v) = map.remove(f) {
                    new_map.insert(f.clone(), v);
                } else if let Some((_, src)) = FIELD_ALIASES.iter().find(|(alias, _)| alias == f) {
                    if let Some(v) = map.remove(*src) {
                        new_map.insert(f.clone(), v);
                    }
                }
            }
            if let Some(g) = guidance {
                new_map.insert("miss_guidance".to_string(), g);
            }
            *map = new_map;
        }
        serde_json::Value::Array(arr) => {
            for item in arr {
                project_fields(item, fields);
            }
        }
        _ => {}
    }
}

fn strip_nulls(val: &mut serde_json::Value) {
    match val {
        serde_json::Value::Object(map) => {
            map.retain(|_, v| !v.is_null());
            for v in map.values_mut() {
                strip_nulls(v);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr {
                strip_nulls(v);
            }
        }
        _ => {}
    }
}
