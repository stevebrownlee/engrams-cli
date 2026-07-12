use anyhow::Result;
use chrono::{SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;
use std::io::Write;
use std::path::{Path, PathBuf};

const TEMPLATE: &str = include_str!("../assets/report/template.html");
const APP_CSS: &str = include_str!("../assets/report/app.css");
const VENDOR_JS: &str = include_str!("../assets/report/vendor/cytoscape.min.js");
const APP_JS: &str = include_str!("../assets/report/app.js");

use crate::cli::ReportTopic;
use crate::models::{ContextDoc, CustomData, Decision, Link, Pattern, Progress};

#[derive(serde::Serialize)]
struct DashboardPayload {
    generated_at: String,
    db_path: String,
    version: &'static str,
    product_context: Option<ContextDoc>,
    active_context: Option<ContextDoc>,
    decisions: Vec<Decision>,
    progress: Vec<Progress>,
    patterns: Vec<Pattern>,
    custom_data: Vec<CustomData>,
    links: Vec<Link>,
}
pub fn open(
    conn: &Connection,
    db_path: &Path,
    no_browser: bool,
    out: Option<PathBuf>,
) -> Result<Value> {
    let generated_at = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);

    let product_context = query_context_doc(conn, "product_context")?;
    let active_context = query_context_doc(conn, "active_context")?;
    let decisions = query_decisions(conn, -1)?;
    let progress = query_progress(conn, -1)?;
    let patterns = query_patterns(conn, -1)?;
    let custom_data = query_custom_data(conn)?;
    let links = query_links(conn, -1)?;

    let counts = serde_json::json!({
        "decisions": decisions.len(),
        "progress": progress.len(),
        "patterns": patterns.len(),
        "custom_data": custom_data.len(),
        "links": links.len(),
    });

    let payload = DashboardPayload {
        generated_at,
        db_path: db_path.display().to_string(),
        version: env!("CARGO_PKG_VERSION"),
        product_context,
        active_context,
        decisions,
        progress,
        patterns,
        custom_data,
        links,
    };

    let html_path = match out {
        Some(path) => path,
        None => {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            db_path.hash(&mut hasher);
            let hash = hasher.finish() as u32;
            std::env::temp_dir().join(format!("engrams-report-{:08x}.html", hash))
        }
    };

    write_html(&html_path, &payload)?;

    let mut opened = false;
    if !no_browser {
        opened = launch_browser(&html_path);
    }

    Ok(serde_json::json!({
        "path": html_path.display().to_string(),
        "opened": opened,
        "counts": counts,
    }))
}

struct ScriptSafe<W: std::io::Write>(W);
impl<W: std::io::Write> std::io::Write for ScriptSafe<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut rest = buf;
        while let Some(i) = rest.iter().position(|&b| b == b'<') {
            self.0.write_all(&rest[..i])?;
            self.0.write_all(b"\\u003c")?;
            rest = &rest[i + 1..];
        }
        self.0.write_all(rest)?;
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}

fn write_html(path: &Path, payload: &DashboardPayload) -> Result<()> {
    let file = std::fs::File::create(path)?;
    let mut writer = std::io::BufWriter::new(file);

    let seg: Vec<&str> = TEMPLATE.split("/*__ENGRAMS_SLOT__*/").collect();
    debug_assert_eq!(seg.len(), 5);

    if seg.len() == 5 {
        writer.write_all(seg[0].as_bytes())?;
        writer.write_all(APP_CSS.as_bytes())?;
        writer.write_all(seg[1].as_bytes())?;
        writer.write_all(VENDOR_JS.as_bytes())?;
        writer.write_all(seg[2].as_bytes())?;

        let mut safe_writer = ScriptSafe(&mut writer);
        serde_json::to_writer(&mut safe_writer, payload)?;

        writer.write_all(seg[3].as_bytes())?;
        writer.write_all(APP_JS.as_bytes())?;
        writer.write_all(seg[4].as_bytes())?;
    }
    writer.flush()?;
    Ok(())
}

fn launch_browser(path: &Path) -> bool {
    let mut cmd = if cfg!(target_os = "windows") {
        let mut c = std::process::Command::new("cmd");
        c.arg("/C").arg("start").arg("").arg(path);
        c
    } else if cfg!(target_os = "macos") {
        let mut c = std::process::Command::new("open");
        c.arg(path);
        c
    } else {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(path);
        c
    };

    cmd.spawn().is_ok()
}

pub fn handle(conn: &Connection, topic: Option<ReportTopic>, limit: i64) -> Result<Value> {
    let active_context = query_context_doc(conn, "active_context")?;
    let progress = query_progress(conn, limit)?;
    let decisions = query_decisions(conn, limit)?;
    let patterns = query_patterns(conn, limit)?;
    let links = query_links(conn, limit)?;
    match topic {
        None => Ok(serde_json::json!({
            "active_context": active_context,
            "progress": progress,
            "decisions": decisions,
            "patterns": patterns,
            "links": links,
        })),
        Some(ReportTopic::Context) => Ok(serde_json::to_value(active_context)?),
        Some(ReportTopic::Progress) => Ok(serde_json::to_value(progress)?),
        Some(ReportTopic::Decisions) => Ok(serde_json::to_value(decisions)?),
        Some(ReportTopic::Patterns) => Ok(serde_json::to_value(patterns)?),
        Some(ReportTopic::Links) => Ok(serde_json::to_value(links)?),
    }
}
fn query_context_doc(conn: &Connection, table: &str) -> Result<Option<ContextDoc>> {
    let sql = format!(
        "SELECT content, version, updated_at FROM {} WHERE id = 1",
        table
    );
    let row: Option<(String, i64, String)> = conn
        .query_row(&sql, [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .optional()?;

    let doc = row.map(|(content_str, version, updated_at)| ContextDoc {
        content: serde_json::from_str(&content_str).unwrap_or(Value::Null),
        version,
        updated_at: Some(updated_at),
    });

    Ok(doc)
}

fn query_progress(conn: &Connection, limit: i64) -> Result<Vec<Progress>> {
    let mut stmt = conn.prepare("SELECT id, timestamp, status, description, parent_id FROM progress_entries ORDER BY id DESC LIMIT ?")?;
    let rows = stmt.query_map(params![limit], |row| {
        Ok(Progress {
            id: row.get(0)?,
            timestamp: row.get(1)?,
            status: row.get(2)?,
            description: row.get(3)?,
            parent_id: row.get(4)?,
        })
    })?;
    let mut progress = Vec::with_capacity(limit.max(0) as usize);
    for r in rows {
        progress.push(r?);
    }
    Ok(progress)
}

fn query_decisions(conn: &Connection, limit: i64) -> Result<Vec<Decision>> {
    let mut stmt = conn.prepare("SELECT id, uuid, summary, rationale, implementation_details, tags, timestamp FROM decisions ORDER BY id DESC LIMIT ?")?;
    let rows = stmt.query_map(params![limit], |row| {
        let tags_str: Option<String> = row.get(5)?;
        let tags = match tags_str {
            Some(s) => serde_json::from_str(&s).unwrap_or(Value::Null),
            None => Value::Null,
        };
        Ok(Decision {
            id: row.get(0)?,
            uuid: row.get(1)?,
            summary: row.get(2)?,
            rationale: row.get(3)?,
            implementation_details: row.get(4)?,
            tags: if tags.is_null() { None } else { Some(tags) },
            timestamp: row.get(6)?,
        })
    })?;
    let mut decisions = Vec::with_capacity(limit.max(0) as usize);
    for r in rows {
        decisions.push(r?);
    }
    Ok(decisions)
}

fn query_patterns(conn: &Connection, limit: i64) -> Result<Vec<Pattern>> {
    let mut stmt = conn.prepare("SELECT id, uuid, name, description, tags, timestamp FROM system_patterns ORDER BY id DESC LIMIT ?")?;
    let rows = stmt.query_map(params![limit], |row| {
        let tags_str: Option<String> = row.get(4)?;
        let tags = match tags_str {
            Some(s) => serde_json::from_str(&s).unwrap_or(Value::Null),
            None => Value::Null,
        };
        Ok(Pattern {
            id: row.get(0)?,
            uuid: row.get(1)?,
            name: row.get(2)?,
            description: row.get(3)?,
            tags: if tags.is_null() { None } else { Some(tags) },
            timestamp: row.get(5)?,
        })
    })?;
    let mut patterns = Vec::with_capacity(limit.max(0) as usize);
    for r in rows {
        patterns.push(r?);
    }
    Ok(patterns)
}

fn query_links(conn: &Connection, limit: i64) -> Result<Vec<Link>> {
    let mut stmt = conn.prepare("SELECT id, source_item_type, source_item_id, target_item_type, target_item_id, relationship_type, description, timestamp FROM context_links ORDER BY id DESC LIMIT ?")?;
    let rows = stmt.query_map(params![limit], |row| {
        Ok(Link {
            id: row.get(0)?,
            source_item_type: row.get(1)?,
            source_item_id: row.get(2)?,
            target_item_type: row.get(3)?,
            target_item_id: row.get(4)?,
            relationship_type: row.get(5)?,
            description: row.get(6)?,
            timestamp: row.get(7)?,
            direction: None,
        })
    })?;
    let mut links = Vec::with_capacity(limit.max(0) as usize);
    for r in rows {
        links.push(r?);
    }
    Ok(links)
}

fn query_custom_data(conn: &Connection) -> Result<Vec<CustomData>> {
    let mut stmt = conn
        .prepare("SELECT id, timestamp, category, key, value FROM custom_data ORDER BY id ASC")?;
    let rows = stmt.query_map([], crate::ops::custom::parse_custom_row)?;
    let mut results = Vec::new();
    for r in rows {
        results.push(r?);
    }
    Ok(results)
}
