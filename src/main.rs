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
    // Print notification if a new version is available
    checker.print_notification();

    Ok(())
}

fn project_fields(val: &mut serde_json::Value, fields: &[String]) {
    match val {
        serde_json::Value::Object(map) => {
            let mut new_map = serde_json::Map::new();
            for f in fields {
                if let Some(v) = map.remove(f) {
                    new_map.insert(f.clone(), v);
                }
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
