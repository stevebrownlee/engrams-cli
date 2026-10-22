//! `schema` command family: schema formation (spec 0002).
//!
//! Detection lives in `super::graph::louvain` (the orchestrator-approved
//! deviation from the spec's `detect.rs` path); this module hosts the
//! command handlers. `src/ops/schemas/` stays the home for staging,
//! apply, and the later list/show/refine surface.

pub mod assimilate;
pub mod confirm;
pub(crate) mod gates;
pub mod kind;
pub mod list;
pub(crate) mod opportunities;
pub mod propose;
pub mod retrieval;
pub mod scan;

use anyhow::Result;
use rusqlite::Connection;
use serde_json::Value;

use crate::cli::SchemaCmd;

pub fn handle(conn: &Connection, cmd: SchemaCmd) -> Result<Value> {
    match cmd {
        SchemaCmd::Scan { apply } => scan::scan(conn, apply),
        SchemaCmd::Confirm { target, name } => confirm::confirm(conn, &target, name.as_deref()),
        SchemaCmd::Propose {
            statement,
            summary,
            trigger,
            rule,
            members,
            from,
            name,
            scope_tags,
            scope_paths,
            contrary,
            session,
            force,
        } => propose::propose(
            conn,
            &statement,
            summary.as_deref(),
            trigger.as_deref(),
            rule.as_deref(),
            &members,
            from.as_deref(),
            name.as_deref(),
            scope_tags.as_deref(),
            scope_paths.as_deref(),
            &contrary,
            session.as_deref(),
            force,
        ),
        SchemaCmd::Reject { target, reason } => propose::reject(conn, &target, reason.as_deref()),
        SchemaCmd::Proposals { status } => propose::proposals(conn, status.as_deref()),
        SchemaCmd::List { status } => list::list(conn, status.as_deref()),
        SchemaCmd::Show { target } => list::show(conn, &target),
        SchemaCmd::Refine {
            target,
            summary,
            trigger,
            rule,
        } => list::refine(
            conn,
            &target,
            summary.as_deref(),
            trigger.as_deref(),
            rule.as_deref(),
        ),
    }
}
