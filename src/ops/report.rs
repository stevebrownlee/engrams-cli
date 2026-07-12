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
    match topic {
        None => {
            let active_context = query_active_context(conn)?;
            let progress = query_progress(conn, limit)?;
            let decisions = query_decisions(conn, limit)?;
            let patterns = query_patterns(conn, limit)?;
            let links = query_links(conn, limit)?;
            Ok(serde_json::json!({
                "active_context": active_context,
                "progress": progress,
                "decisions": decisions,
                "patterns": patterns,
                "links": links,
            }))
        }
        Some(ReportTopic::Context) => {
            let active_context = query_active_context(conn)?;
            Ok(serde_json::to_value(active_context)?)
        }
        Some(ReportTopic::Progress) => {
            let progress = query_progress(conn, limit)?;
            Ok(serde_json::to_value(progress)?)
        }
        Some(ReportTopic::Decisions) => {
            let decisions = query_decisions(conn, limit)?;
            Ok(serde_json::to_value(decisions)?)
        }
        Some(ReportTopic::Patterns) => {
            let patterns = query_patterns(conn, limit)?;
            Ok(serde_json::to_value(patterns)?)
        }
        Some(ReportTopic::Links) => {
            let links = query_links(conn, limit)?;
            Ok(serde_json::to_value(links)?)
        }
    }
}

fn handle_human(conn: &Connection, topic: Option<ReportTopic>, limit: i64) -> Result<()> {
    let stdout = io::stdout();
    let mut w = stdout.lock();
    
    if topic.is_none() {
        writeln!(w, "================================================================================")?;
        writeln!(w, "                             ENGRAMS PROJECT REPORT                             ")?;
        writeln!(w, "================================================================================")?;
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
            writeln!(w, "Active Context")?;
            writeln!(w, "└── (not set)")?;
            return Ok(());
        }
    };
    
    let updated = doc.updated_at.as_deref().map(format_time).unwrap_or_else(|| "unknown".to_string());
    writeln!(w, "Active Context (v{}) • Updated {}", doc.version, updated)?;
    
    match &doc.content {
        Value::Object(map) => {
            let len = map.len();
            for (idx, (k, v)) in map.iter().enumerate() {
                let is_last = idx == len - 1;
                let prefix = if is_last { "└── " } else { "├── " };
                let child_prefix = if is_last { "    " } else { "│   " };
                
                match v {
                    Value::String(s) => {
                        writeln!(w, "{}{}: {}", prefix, k, s)?;
                    }
                    Value::Null => {
                        writeln!(w, "{}{}: (null)", prefix, k)?;
                    }
                    Value::Object(_) | Value::Array(_) => {
                        let pretty = serde_json::to_string_pretty(v).unwrap_or_else(|_| v.to_string());
                        writeln!(w, "{}{}:", prefix, k)?;
                        for line in pretty.lines() {
                            writeln!(w, "{}{}", child_prefix, line)?;
                        }
                    }
                    _ => {
                        writeln!(w, "{}{}: {}", prefix, k, v)?;
                    }
                }
            }
        }
        Value::String(s) => {
            writeln!(w, "└── {}", s)?;
        }
        Value::Null => {
            writeln!(w, "└── (null)")?;
        }
        other => {
            writeln!(w, "└── {}", other)?;
        }
    }
    
    Ok(())
}

fn write_progress_section(w: &mut impl Write, conn: &Connection, limit: i64) -> io::Result<()> {
    let entries = query_progress(conn, limit).unwrap_or_default();
    let count = entries.len();
    let entry_word = if count == 1 { "entry" } else { "entries" };
    writeln!(w, "Progress ({} {})", count, entry_word)?;
    
    if entries.is_empty() {
        writeln!(w, "└── (none)")?;
    } else {
        for (idx, p) in entries.iter().enumerate() {
            let is_last = idx == count - 1;
            let prefix = if is_last { "└── " } else { "├── " };
            let child_prefix = if is_last { "    " } else { "│   " };
            
            let status_icon = format_status(&p.status);
            let parent = p.parent_id.map(|id| format!(" [parent: #{}]", id)).unwrap_or_default();
            
            writeln!(w, "{}{} {}{}", prefix, status_icon, p.description, parent)?;
            writeln!(w, "{}    {}", child_prefix, format_time(&p.timestamp))?;
        }
    }
    Ok(())
}

fn write_decisions_section(w: &mut impl Write, conn: &Connection, limit: i64) -> io::Result<()> {
    let entries = query_decisions(conn, limit).unwrap_or_default();
    let count = entries.len();
    let entry_word = if count == 1 { "entry" } else { "entries" };
    writeln!(w, "Decisions ({} {})", count, entry_word)?;
    
    if entries.is_empty() {
        writeln!(w, "└── (none)")?;
    } else {
        for (idx, d) in entries.iter().enumerate() {
            let is_last = idx == count - 1;
            let prefix = if is_last { "└── " } else { "├── " };
            let child_prefix = if is_last { "    " } else { "│   " };
            
            writeln!(w, "{}{} {}", prefix, format!("#{}", d.id), d.summary)?;
            if let Some(r) = &d.rationale {
                if !r.trim().is_empty() {
                    writeln!(w, "{}Rationale: {}", child_prefix, r)?;
                }
            }
            if let Some(tags_val) = &d.tags {
                if let Some(arr) = tags_val.as_array() {
                    let tags_joined = arr.iter()
                        .filter_map(|v| v.as_str())
                        .collect::<Vec<_>>()
                        .join(", ");
                    if !tags_joined.is_empty() {
                        writeln!(w, "{}Tags:      {}", child_prefix, tags_joined)?;
                    }
                }
            }
            writeln!(w, "{}Date:      {}", child_prefix, format_time(&d.timestamp))?;
        }
    }
    Ok(())
}

fn write_patterns_section(w: &mut impl Write, conn: &Connection, limit: i64) -> io::Result<()> {
    let entries = query_patterns(conn, limit).unwrap_or_default();
    let count = entries.len();
    let entry_word = if count == 1 { "entry" } else { "entries" };
    writeln!(w, "Patterns ({} {})", count, entry_word)?;
    
    if entries.is_empty() {
        writeln!(w, "└── (none)")?;
    } else {
        for (idx, p) in entries.iter().enumerate() {
            let is_last = idx == count - 1;
            let prefix = if is_last { "└── " } else { "├── " };
            let child_prefix = if is_last { "    " } else { "│   " };
            
            writeln!(w, "{}{} {}", prefix, format!("#{}", p.id), p.name)?;
            if let Some(desc) = &p.description {
                if !desc.trim().is_empty() {
                    writeln!(w, "{}Detail: {}", child_prefix, desc)?;
                }
            }
            if let Some(tags_val) = &p.tags {
                if let Some(arr) = tags_val.as_array() {
                    let tags_joined = arr.iter()
                        .filter_map(|v| v.as_str())
                        .collect::<Vec<_>>()
                        .join(", ");
                    if !tags_joined.is_empty() {
                        writeln!(w, "{}Tags:   {}", child_prefix, tags_joined)?;
                    }
                }
            }
            writeln!(w, "{}Date:   {}", child_prefix, format_time(&p.timestamp))?;
        }
    }
    Ok(())
}

fn write_links_section(w: &mut impl Write, conn: &Connection, limit: i64) -> io::Result<()> {
    let entries = query_links(conn, limit).unwrap_or_default();
    let count = entries.len();
    let entry_word = if count == 1 { "entry" } else { "entries" };
    writeln!(w, "Links ({} {})", count, entry_word)?;
    
    if entries.is_empty() {
        writeln!(w, "└── (none)")?;
    } else {
        for (idx, l) in entries.iter().enumerate() {
            let is_last = idx == count - 1;
            let prefix = if is_last { "└── " } else { "├── " };
            let child_prefix = if is_last { "    " } else { "│   " };
            
            writeln!(w, "{}{} #{} ➔ {} #{} [{}]", prefix, l.source_item_type, l.source_item_id, l.target_item_type, l.target_item_id, l.relationship_type)?;
            if let Some(desc) = &l.description {
                if !desc.trim().is_empty() {
                    writeln!(w, "{}Detail: {}", child_prefix, desc)?;
                }
            }
            writeln!(w, "{}Date:   {}", child_prefix, format_time(&l.timestamp))?;
        }
    }
    Ok(())
}
