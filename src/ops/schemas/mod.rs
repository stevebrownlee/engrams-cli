//! `schema` command family: schema formation (spec 0002).
//!
//! Detection lives in `super::graph::louvain` (the orchestrator-approved
//! deviation from the spec's `detect.rs` path); this module hosts the
//! command handlers. `src/ops/schemas/` stays the home for staging,
//! apply, and the later list/show/refine surface.

pub mod scan;

use anyhow::Result;
use rusqlite::Connection;
use serde_json::Value;

use crate::cli::SchemaCmd;

pub fn handle(conn: &Connection, cmd: SchemaCmd) -> Result<Value> {
    match cmd {
        SchemaCmd::Scan => scan::scan(conn),
    }
}
