//! `engrams session close` (tier-4) — the checkpoint that makes "the graph grew
//! this session" a machine-checked invariant instead of an aspiration.
//!
//! Persists one `session_closes` row (skipped vs required reads, the agent's
//! own token-savings estimate, optional PR, `ENGRAMS_SESSION` key matching
//! `usage_log.session`) and, when a PR is given, refuses to bless a PR whose
//! node lacks at least one linked decision and one anchored code file within
//! one hop.
//!
//! The savings figure is a pass-through estimate, never derived: invented
//! precision would poison the metric's credibility. Output keys say
//! `_estimated` where the number is the caller's word, not a measurement.

use anyhow::Result;
use chrono::{SecondsFormat, Utc};
use rusqlite::{params, Connection};
use serde_json::{json, Value};

#[allow(clippy::too_many_arguments)]
pub fn close(
    conn: &Connection,
    reads_skipped: i64,
    reads_required: i64,
    tokens_saved_est: Option<i64>,
    note: Option<String>,
    pr: Option<String>,
    session: Option<String>,
) -> Result<Value> {
    let (pr_url, pr_validation) = match &pr {
        Some(p) => {
            let url = crate::ops::pr::resolve_pr_url(p)?;
            let v = validate_pr(conn, &url)?;
            (Some(url), Some(v))
        }
        None => (None, None),
    };

    // Gate failure: nothing is recorded — the agent fixes linkage and retries,
    // so a failed close attempt never pollutes the rollup.
    if let Some(false) = pr_validation.as_ref().map(|v| v.gate_ok) {
        return Ok(json!({
            "closed": false,
            "pr_validation": pr_validation,
            "reason": "PR session gate failed: link a decision (session_close) and at least one anchored code file to the PR node, then retry",
        }));
    }

    let now = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let sid = session.or_else(|| std::env::var("ENGRAMS_SESSION").ok());
    conn.execute(
        "INSERT INTO session_closes (timestamp, session, reads_skipped, reads_required, tokens_saved, note, pr_url) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![now, sid, reads_skipped, reads_required, tokens_saved_est, note, pr_url],
    )?;

    let rollup = usage_rollup(conn, sid.as_deref())?;

    let mut out = json!({
        "closed": true,
        "timestamp": now,
        "session": sid,
        "reads_skipped_estimated": reads_skipped,
        "reads_required_estimated": reads_required,
        "tokens_saved_estimated": tokens_saved_est,
        "note": note,
        "pr": pr_url,
        "retrievals_this_session": rollup,
    });
    if let Some(v) = pr_validation {
        out["pr_validation"] = json!(v);
    }
    Ok(out)
}

/// PR gate: the pr node must have ≥1 linked decision and ≥1 code file in its
/// 1-hop neighborhood. Never errors on a missing node — reports the failure.
fn validate_pr(conn: &Connection, url: &str) -> Result<PrValidation> {
    let mut has_decision = false;
    let mut code_files: Vec<String> = Vec::new();
    let mut stmt = conn.prepare(
        "SELECT source_item_type, source_item_id FROM context_links \
         WHERE target_item_type = 'pr' AND (target_item_id = ?1 OR target_item_id LIKE ?2)",
    )?;
    let suffix = format!("%/{}", url.rsplit('/').next().unwrap_or(url));
    let rows = stmt.query_map(params![url, suffix], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })?;
    for r in rows {
        let (ty, id) = r?;
        match ty.as_str() {
            "decision" => {
                has_decision = true;
                if let Ok(id_int) = id.parse::<i64>() {
                    let mut stmt_a = conn.prepare(
                        "SELECT path FROM item_anchors WHERE item_type = 'decision' AND item_id = ?1",
                    )?;
                    let paths = stmt_a.query_map(params![id_int], |r| r.get::<_, String>(0))?;
                    for p in paths.flatten() {
                        if !code_files.contains(&p) {
                            code_files.push(p);
                        }
                    }
                }
            }
            "system_pattern" | "pattern" => {
                if let Ok(id_int) = id.parse::<i64>() {
                    let mut stmt_a = conn.prepare(
                        "SELECT path FROM item_anchors WHERE item_type = 'system_pattern' AND item_id = ?1",
                    )?;
                    let paths = stmt_a.query_map(params![id_int], |r| r.get::<_, String>(0))?;
                    for p in paths.flatten() {
                        if !code_files.contains(&p) {
                            code_files.push(p);
                        }
                    }
                }
            }
            "code" => {
                if let Ok(Some(path)) = conn.query_row(
                    "SELECT path FROM code_nodes WHERE id = ?1 AND kind = 'file'",
                    params![id],
                    |r| r.get::<_, Option<String>>(0),
                ) {
                    if !code_files.contains(&path) {
                        code_files.push(path);
                    }
                }
            }
            _ => {}
        }
    }
    Ok(PrValidation {
        gate_ok: has_decision && !code_files.is_empty(),
        linked_decision: has_decision,
        anchored_code_files: code_files,
    })
}

struct PrValidation {
    gate_ok: bool,
    linked_decision: bool,
    anchored_code_files: Vec<String>,
}

impl serde::Serialize for PrValidation {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        json!({
            "gate_ok": self.gate_ok,
            "linked_decision": self.linked_decision,
            "anchored_code_files": self.anchored_code_files,
        })
        .serialize(s)
    }
}

/// Recorded closes: totals plus the most recent rows.
pub fn history(conn: &Connection) -> Result<Value> {
    let (sum_skipped, sum_required, sum_saved, count): (i64, i64, i64, i64) = conn.query_row(
        "SELECT COALESCE(SUM(reads_skipped),0), COALESCE(SUM(reads_required),0), \
                COALESCE(SUM(tokens_saved),0), COUNT(*) FROM session_closes",
        [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )?;
    let mut stmt = conn.prepare(
        "SELECT timestamp, session, reads_skipped, reads_required, tokens_saved, note, pr_url \
         FROM session_closes ORDER BY timestamp DESC LIMIT 20",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(json!({
                "timestamp": r.get::<_, String>(0)?,
                "session": r.get::<_, Option<String>>(1)?,
                "reads_skipped_estimated": r.get::<_, i64>(2)?,
                "reads_required_estimated": r.get::<_, i64>(3)?,
                "tokens_saved_estimated": r.get::<_, Option<i64>>(4)?,
                "note": r.get::<_, Option<String>>(5)?,
                "pr": r.get::<_, Option<String>>(6)?,
            }))
        })?
        .filter_map(|r| r.ok())
        .collect::<Vec<_>>();
    Ok(json!({
        "sessions": count,
        "reads_skipped_estimated_total": sum_skipped,
        "reads_required_estimated_total": sum_required,
        "tokens_saved_estimated_total": sum_saved,
        "recent": rows,
    }))
}

fn usage_rollup(conn: &Connection, session: Option<&str>) -> Result<Value> {
    let (calls, misses) = match session {
        Some(sid) => conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(miss),0) FROM usage_log WHERE session = ?1",
            params![sid],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
        )?,
        None => conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(miss),0) FROM usage_log",
            [],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
        )?,
    };
    Ok(json!({"calls": calls, "zero_hit": misses}))
}

pub fn handle(conn: &Connection, cmd: crate::cli::SessionCmd) -> Result<Value> {
    match cmd {
        crate::cli::SessionCmd::Close {
            reads_skipped,
            reads_required,
            tokens_saved,
            note,
            pr,
            session,
        } => close(
            conn,
            reads_skipped,
            reads_required,
            tokens_saved,
            note,
            pr,
            session,
        ),
        crate::cli::SessionCmd::History => history(conn),
    }
}
