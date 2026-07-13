pub mod activity;
pub mod anchor;
pub mod batch;
pub mod context;
pub mod custom;
pub mod decision;
pub mod doctor;
pub mod git;
pub mod link;
pub mod pattern;
pub mod pr;
pub mod prime;
pub mod progress;
pub mod query;
pub mod report;
pub mod transfer;
use anyhow::Result;
use rusqlite::Connection;
use serde_json::json;
use serde_json::Value;
use std::path::Path;

use crate::cli::{Command, ContextCmd, HistoryDoc, ReportCmd};

pub fn dispatch(
    conn: &mut Connection,
    cmd: Command,
    db_path: &Path,
    created: bool,
) -> Result<Value> {
    match cmd {
        Command::Init => Ok(json!({"db_path": db_path.display().to_string(), "created": created})),
        Command::Migrate => {
            crate::db::run_migrations(conn)?;
            Ok(json!({"status": "success", "message": "Database migrated to the latest version"}))
        }
        Command::ProductContext { cmd } => match cmd {
            ContextCmd::Get => context::get(conn, "product_context"),
            ContextCmd::Update(args) => context::update(conn, "product_context", args),
        },
        Command::ActiveContext { cmd } => match cmd {
            ContextCmd::Get => context::get(conn, "active_context"),
            ContextCmd::Update(args) => context::update(conn, "active_context", args),
        },
        Command::History {
            doc,
            version,
            limit,
        } => {
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
        Command::Report { cmd, topic, limit } => match cmd {
            Some(ReportCmd::Open { no_browser, out }) => {
                report::open(conn, db_path, no_browser, out)
            }
            None => report::handle(conn, topic, limit),
        },
        Command::Pr { cmd } => pr::handle(conn, cmd),
        Command::Anchor { cmd } => anchor::handle(conn, cmd),
        Command::Relevant { paths, staged, all } => {
            anchor::handle_relevant(conn, paths, staged, all)
        }
        Command::Prime {
            budget,
            paths,
            tags,
        } => prime::handle(conn, budget, paths, tags),
        Command::Doctor => doctor::handle(conn),
        Command::Instructions => unreachable!("handled in main before dispatch"),
        Command::Query {
            query,
            types,
            tags,
            since,
            limit,
            all,
        } => query::handle(conn, query, types, tags, since, limit, all),
    }
}

pub(crate) fn fts_match_expr(query: &str) -> String {
    query
        .split_whitespace()
        .map(|token| format!("\"{}\"", token.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" ")
}
