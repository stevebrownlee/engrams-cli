pub mod context;
pub mod batch;
pub mod transfer;
pub mod activity;
pub mod custom;
pub mod decision;
pub mod link;
pub mod pattern;
pub mod progress;

use anyhow::Result;
use rusqlite::Connection;
use serde_json::Value;
use serde_json::json;
use std::path::Path;

use crate::cli::{Command, ContextCmd, HistoryDoc};

pub fn dispatch(conn: &Connection, cmd: Command, db_path: &Path, created: bool) -> Result<Value> {
    match cmd {
        Command::Init => {
            Ok(json!({"db_path": db_path.display().to_string(), "created": created}))
        }
        Command::ProductContext { cmd } => match cmd {
            ContextCmd::Get => context::get(conn, "product_context"),
            ContextCmd::Update(args) => context::update(conn, "product_context", args),
        },
        Command::ActiveContext { cmd } => match cmd {
            ContextCmd::Get => context::get(conn, "active_context"),
            ContextCmd::Update(args) => context::update(conn, "active_context", args),
        },
        Command::History { doc, version, limit } => {
            let table = match doc {
                HistoryDoc::ProductContext => "product_context",
                HistoryDoc::ActiveContext => "active_context",
            };
            context::history(conn, table, version, limit)
        }
        Command::Decision { cmd } => decision::handle(conn, cmd),
        Command::Progress { cmd } => progress::handle(conn, cmd),
        Command::Pattern { cmd } => pattern::handle(conn, cmd),
        Command::Custom { cmd } => custom::handle(conn, cmd),
        Command::Link { cmd } => link::handle(conn, cmd),
        Command::Activity(args) => activity::handle(conn, args),
        Command::Batch { r#type, items } => batch::handle(conn, r#type, items),
        Command::Export { path } => transfer::export::handle(conn, &path),
        Command::Import { path } => transfer::import::handle(conn, &path),
    }
}
