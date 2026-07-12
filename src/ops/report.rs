use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;
use std::io::{self, Write};

use crate::cli::{Format, ReportTopic};
use crate::models::{ContextDoc, Decision, Link, Pattern, Progress};

pub fn handle(conn: &Connection, topic: Option<ReportTopic>, limit: i64, format: Format) -> Result<Value> {
    match format {
        Format::Json => handle_json(conn, topic, limit),
        Format::Human => {
            handle_human(conn, topic, limit)?;
            Ok(Value::Null)
        }
    }
}

fn query_active_context(conn: &Connection) -> Result<Option<ContextDoc>> {
    let row: Option<(String, i64, String)> = conn
        .query_row(
            "SELECT content, version, updated_at FROM active_context WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
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
    let mut progress = Vec::with_capacity(limit as usize);
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
    let mut decisions = Vec::with_capacity(limit as usize);
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
    let mut patterns = Vec::with_capacity(limit as usize);
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
    let mut links = Vec::with_capacity(limit as usize);
    for r in rows {
        links.push(r?);
    }
    Ok(links)
}

fn handle_json(conn: &Connection, topic: Option<ReportTopic>, limit: i64) -> Result<Value> {
    let active_context = query_active_context(conn)?;
    let progress = query_progress(conn, limit)?;
    let decisions = query_decisions(conn, limit)?;
    let patterns = query_patterns(conn, limit)?;
    let links = query_links(conn, limit)?;
    match topic {
        None => {
            Ok(serde_json::json!({
                "active_context": active_context,
                "progress": progress,
                "decisions": decisions,
                "patterns": patterns,
                "links": links,
            }))
        }
        Some(ReportTopic::Context) => {
            Ok(serde_json::to_value(active_context)?)
        }
        Some(ReportTopic::Progress) => {
            Ok(serde_json::to_value(progress)?)
        }
        Some(ReportTopic::Decisions) => {
            Ok(serde_json::to_value(decisions)?)
        }
        Some(ReportTopic::Patterns) => {
            Ok(serde_json::to_value(patterns)?)
        }
        Some(ReportTopic::Links) => {
            Ok(serde_json::to_value(links)?)
        }
    }
}

fn handle_human(conn: &Connection, topic: Option<ReportTopic>, limit: i64) -> Result<()> {
    let stdout = io::stdout();
    let mut w = stdout.lock();
    
    if topic.is_none() {
        writeln!(w, "# Engrams Project Report")?;
        writeln!(w)?;
    }

    match topic {
        None => {
            write_active_context_section(&mut w, conn)?;
            writeln!(w)?;
            write_progress_section(&mut w, conn, limit)?;
            writeln!(w)?;
            write_decisions_section(&mut w, conn, limit)?;
            writeln!(w)?;
            write_patterns_section(&mut w, conn, limit)?;
            writeln!(w)?;
            write_links_section(&mut w, conn, limit)?;
        }
        Some(ReportTopic::Context) => {
            write_active_context_section(&mut w, conn)?;
        }
        Some(ReportTopic::Progress) => {
            write_progress_section(&mut w, conn, limit)?;
        }
        Some(ReportTopic::Decisions) => {
            write_decisions_section(&mut w, conn, limit)?;
        }
        Some(ReportTopic::Patterns) => {
            write_patterns_section(&mut w, conn, limit)?;
        }
        Some(ReportTopic::Links) => {
            write_links_section(&mut w, conn, limit)?;
        }
    }
    
    Ok(())
}

fn format_status(status: &str) -> &'static str {
    match status.to_lowercase().as_str() {
        "d" | "done" | "c" | "complete" => "[✓]",
        "ip" | "inprogress" | "started" | "doing" | "p" => "[▶]",
        "t" | "todo" | "planned" => "[ ]",
        _ => "[•]",
    }
}

fn format_time(timestamp: &str) -> String {
    timestamp
        .replace('T', " ")
        .replace('Z', "")
        .split('.')
        .next()
        .unwrap_or(timestamp)
        .to_string()
}

fn write_active_context_section(w: &mut impl Write, conn: &Connection) -> io::Result<()> {
    let doc = match query_active_context(conn) {
        Ok(Some(d)) => d,
        _ => {
            writeln!(w, "### Active Context")?;
            writeln!(w)?;
            writeln!(w, "*(not set)*")?;
            return Ok(());
        }
    };
    
    let updated = doc.updated_at.as_deref().map(format_time).unwrap_or_else(|| "unknown".to_string());
    writeln!(w, "### Active Context (v{})", doc.version)?;
    writeln!(w)?;
    writeln!(w, "| Key | Value |")?;
    writeln!(w, "| :--- | :--- |")?;
    writeln!(w, "| **updated_at** | {} |", updated)?;
    
    match &doc.content {
        Value::Object(map) => {
            for (k, v) in map {
                match v {
                    Value::String(s) => {
                        writeln!(w, "| **{}** | {} |", k, s.replace('|', "\\|").replace('\n', " "))?;
                    }
                    Value::Null => {
                        writeln!(w, "| **{}** | *(null)* |", k)?;
                    }
                    Value::Object(_) | Value::Array(_) => {
                        let pretty = serde_json::to_string(v).unwrap_or_else(|_| v.to_string());
                        writeln!(w, "| **{}** | `{}` |", k, pretty.replace('|', "\\|"))?;
                    }
                    _ => {
                        writeln!(w, "| **{}** | {} |", k, v.to_string().replace('|', "\\|").replace('\n', " "))?;
                    }
                }
            }
        }
        Value::String(s) => {
            writeln!(w, "| **content** | {} |", s.replace('|', "\\|").replace('\n', " "))?;
        }
        Value::Null => {
            writeln!(w, "| **content** | *(null)* |")?;
        }
        other => {
            writeln!(w, "| **content** | {} |", other.to_string().replace('|', "\\|").replace('\n', " "))?;
        }
    }
    
    Ok(())
}

fn write_progress_section(w: &mut impl Write, conn: &Connection, limit: i64) -> io::Result<()> {
    let entries = query_progress(conn, limit).unwrap_or_default();
    let count = entries.len();
    let entry_word = if count == 1 { "entry" } else { "entries" };
    writeln!(w, "### Progress ({} {})", count, entry_word)?;
    writeln!(w)?;
    
    if entries.is_empty() {
        writeln!(w, "*(none)*")?;
    } else {
        writeln!(w, "| Status | Description | Parent | Timestamp |")?;
        writeln!(w, "| :---: | :--- | :---: | :--- |")?;
        for p in entries {
            let status_icon = format_status(&p.status);
            let parent_str = p.parent_id.map(|id| format!("#{}", id)).unwrap_or_else(|| "-".to_string());
            let desc = p.description.replace('|', "\\|").replace('\n', " ");
            let ts = format_time(&p.timestamp);
            writeln!(w, "| {} | {} | {} | {} |", status_icon, desc, parent_str, ts)?;
        }
    }
    Ok(())
}

fn write_decisions_section(w: &mut impl Write, conn: &Connection, limit: i64) -> io::Result<()> {
    let entries = query_decisions(conn, limit).unwrap_or_default();
    let count = entries.len();
    let entry_word = if count == 1 { "entry" } else { "entries" };
    writeln!(w, "### Decisions ({} {})", count, entry_word)?;
    writeln!(w)?;
    
    if entries.is_empty() {
        writeln!(w, "*(none)*")?;
    } else {
        writeln!(w, "| ID | Summary | Rationale | Tags | Date |")?;
        writeln!(w, "| :---: | :--- | :--- | :--- | :--- |")?;
        for d in entries {
            let summary = d.summary.replace('|', "\\|").replace('\n', " ");
            let rationale = d.rationale.as_deref().unwrap_or("-").replace('|', "\\|").replace('\n', " ");
            let tags_str = if let Some(tags_val) = &d.tags {
                if let Some(arr) = tags_val.as_array() {
                    arr.iter()
                        .filter_map(|v| v.as_str())
                        .map(|t| format!("`{}`", t))
                        .collect::<Vec<_>>()
                        .join(", ")
                } else {
                    "-".to_string()
                }
            } else {
                "-".to_string()
            };
            let date = format_time(&d.timestamp);
            writeln!(w, "| #{} | {} | {} | {} | {} |", d.id, summary, rationale, tags_str, date)?;
        }
    }
    Ok(())
}

fn write_patterns_section(w: &mut impl Write, conn: &Connection, limit: i64) -> io::Result<()> {
    let entries = query_patterns(conn, limit).unwrap_or_default();
    let count = entries.len();
    let entry_word = if count == 1 { "entry" } else { "entries" };
    writeln!(w, "### Patterns ({} {})", count, entry_word)?;
    writeln!(w)?;
    
    if entries.is_empty() {
        writeln!(w, "*(none)*")?;
    } else {
        writeln!(w, "| ID | Name | Description | Tags | Date |")?;
        writeln!(w, "| :---: | :--- | :--- | :--- | :--- |")?;
        for p in entries {
            let name = p.name.replace('|', "\\|").replace('\n', " ");
            let desc = p.description.as_deref().unwrap_or("-").replace('|', "\\|").replace('\n', " ");
            let tags_str = if let Some(tags_val) = &p.tags {
                if let Some(arr) = tags_val.as_array() {
                    arr.iter()
                        .filter_map(|v| v.as_str())
                        .map(|t| format!("`{}`", t))
                        .collect::<Vec<_>>()
                        .join(", ")
                } else {
                    "-".to_string()
                }
            } else {
                "-".to_string()
            };
            let date = format_time(&p.timestamp);
            writeln!(w, "| #{} | {} | {} | {} | {} |", p.id, name, desc, tags_str, date)?;
        }
    }
    Ok(())
}

fn write_links_section(w: &mut impl Write, conn: &Connection, limit: i64) -> io::Result<()> {
    let entries = query_links(conn, limit).unwrap_or_default();
    let count = entries.len();
    let entry_word = if count == 1 { "entry" } else { "entries" };
    writeln!(w, "### Links ({} {})", count, entry_word)?;
    writeln!(w)?;
    
    if entries.is_empty() {
        writeln!(w, "*(none)*")?;
    } else {
        writeln!(w, "| Source | Target | Relationship | Description | Date |")?;
        writeln!(w, "| :--- | :--- | :---: | :--- | :--- |")?;
        for l in entries {
            let source = format!("`{}` #{}", l.source_item_type, l.source_item_id);
            let target = format!("`{}` #{}", l.target_item_type, l.target_item_id);
            let rel = l.relationship_type.replace('|', "\\|").replace('\n', " ");
            let desc = l.description.as_deref().unwrap_or("-").replace('|', "\\|").replace('\n', " ");
            let date = format_time(&l.timestamp);
            writeln!(w, "| {} | {} | {} | {} | {} |", source, target, rel, desc, date)?;
        }
    }
    Ok(())
}
