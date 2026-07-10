mod cli;
mod db;
mod models;
mod ops;
mod output;
mod schema;

use anyhow::Result;
use clap::Parser;

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
    let conn = db::open(&db_path)?;

    let result = ops::dispatch(&conn, cli.command, &db_path, !db_existed)?;

    output::emit(cli.format, result)?;

    Ok(())
}
