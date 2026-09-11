//! `schema` command family: schema formation (spec 0002).
//!
//! Detection lives in `super::graph::louvain` (the orchestrator-approved
//! deviation from the spec's `detect.rs` path); this module hosts the
//! command handlers. `src/ops/schemas/` stays the home for staging,
//! apply, and the later list/show/refine surface.

pub mod assimilate;
pub mod confirm;
#[allow(dead_code)]
// labeler core; scan wiring lands in phase 3, after the phase-2 replay gate (spec 0003 design choice 4)
pub mod kind;
pub mod list;
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
        SchemaCmd::List { status } => list::list(conn, status.as_deref()),
        SchemaCmd::Show { target } => list::show(conn, &target),
        SchemaCmd::Refine { target, summary } => list::refine(conn, &target, &summary),
    }
}
