//! Prune-decay: archive records whose Ebbinghaus retention has decayed below a
//! threshold (v0.10.0 tier-1).
//!
//! Retention is `exp(-age_days / strength)` where
//! `strength = (importance + access_count) * STRENGTH_DAYS`.
//! A record that is important or has been read recently survives longer.
//! Items with zero strength (importance 0, never read) are always prunable.
//! Reference: MemoryBank (arXiv:2305.10250).

use crate::cli::PruneCmd;
use crate::ops::scoring::{DEFAULT_PRUNE_THRESHOLD, STRENGTH_DAYS};
use anyhow::Result;
use rusqlite::Connection;
use serde_json::{json, Value};

/// Build the SQL WHERE fragment that identifies prunable rows.
/// `ts` and `imp`, `ac` are the column names for timestamp, importance, access_count.
fn retention_where(ts: &str, imp: &str, ac: &str, threshold: f64) -> String {
    format!(
        "archived = 0 AND (\
         ({imp} + {ac}) = 0 \
         OR ({imp} + {ac}) > 0 \
           AND exp(-(julianday('now') - julianday({ts})) / (({imp} + {ac}) * {STR})) < {thr})",
        ts = ts,
        imp = imp,
        ac = ac,
        STR = STRENGTH_DAYS,
        thr = threshold,
    )
}

pub fn handle(conn: &Connection, cmd: PruneCmd) -> Result<Value> {
    let threshold = cmd.threshold.unwrap_or(DEFAULT_PRUNE_THRESHOLD);
    let dry_run = cmd.dry_run;

    let dec_where = retention_where("timestamp", "importance", "access_count", threshold);
    let pat_where = retention_where("timestamp", "importance", "access_count", threshold);

    // Collect prunable items for the report.
    struct Prunable {
        id: i64,
        label: String,
        item_type: &'static str,
    }

    let mut prunable: Vec<Prunable> = Vec::new();

    {
        let sql = format!("SELECT id, summary FROM decisions WHERE {}", dec_where);
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], |row| {
            Ok(Prunable {
                id: row.get(0)?,
                label: row.get(1)?,
                item_type: "decision",
            })
        })?;
        for r in rows {
            prunable.push(r?);
        }
    }
    {
        let sql = format!("SELECT id, name FROM system_patterns WHERE {}", pat_where);
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], |row| {
            Ok(Prunable {
                id: row.get(0)?,
                label: row.get(1)?,
                item_type: "system_pattern",
            })
        })?;
        for r in rows {
            prunable.push(r?);
        }
    }

    let count = prunable.len();

    if !dry_run && count > 0 {
        let dec_ids: Vec<i64> = prunable
            .iter()
            .filter(|p| p.item_type == "decision")
            .map(|p| p.id)
            .collect();
        let pat_ids: Vec<i64> = prunable
            .iter()
            .filter(|p| p.item_type == "system_pattern")
            .map(|p| p.id)
            .collect();

        if !dec_ids.is_empty() {
            let placeholders = crate::ops::sql_placeholders(dec_ids.len());
            let sql = format!(
                "UPDATE decisions SET archived = 1 WHERE id IN ({})",
                placeholders
            );
            let mut params: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(dec_ids.len());
            for id in &dec_ids {
                params.push(id);
            }
            conn.execute(&sql, rusqlite::params_from_iter(params))?;
        }
        if !pat_ids.is_empty() {
            let placeholders = crate::ops::sql_placeholders(pat_ids.len());
            let sql = format!(
                "UPDATE system_patterns SET archived = 1 WHERE id IN ({})",
                placeholders
            );
            let mut params: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(pat_ids.len());
            for id in &pat_ids {
                params.push(id);
            }
            conn.execute(&sql, rusqlite::params_from_iter(params))?;
        }
    }

    let items: Vec<Value> = prunable
        .into_iter()
        .map(|p| {
            json!({
                "id": p.id,
                "type": p.item_type,
                "label": p.label,
            })
        })
        .collect();

    Ok(json!({
        "threshold": threshold,
        "dry_run": dry_run,
        "count": count,
        "archived": if dry_run { 0 } else { count },
        "items": items,
    }))
}
