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

    let db_path = db::resolve_db_path(cli.db.as_deref(), cli.workspace.as_deref())?;

    let db_existed = db_path.exists();

    // We open (and create) the db for all commands.
    let mut conn = db::open(&db_path)?;

    // Instantiate UpdateChecker (spawns background thread if check is needed)
    let checker = release::UpdateChecker::new(&db_path);

    let is_migrate_or_init = matches!(cli.command, cli::Command::Migrate | cli::Command::Init);
    db::validate_version(&conn, is_migrate_or_init)?;

    let result = ops::dispatch(&mut conn, cli.command, &db_path, !db_existed)?;

    let mut out = std::io::stdout().lock();
    serde_json::to_writer_pretty(&mut out, &result)?;
    out.write_all(b"\n")?;

    // Print notification if a new version is available
    checker.print_notification();

    Ok(())
}
