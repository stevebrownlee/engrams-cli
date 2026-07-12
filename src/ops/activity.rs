use anyhow::Result;
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use rusqlite::{params, Connection};
use serde_json::Value;
use std::str::FromStr;

use crate::cli::ActivityArgs;
use crate::models::{CustomData, Decision, Pattern, Progress};

pub fn handle(conn: &Connection, args: ActivityArgs) -> Result<Value> {
    let cutoff = if let Some(since) = args.since {
        DateTime::<Utc>::from_str(&since)?.to_rfc3339_opts(SecondsFormat::Secs, true)
    } else {
        (Utc::now() - Duration::hours(args.hours)).to_rfc3339_opts(SecondsFormat::Secs, true)
    };

    let limit = args.limit_per_type;

    let mut decisions = Vec::new();
    {
        let mut stmt = conn.prepare("SELECT id, uuid, summary, rationale, implementation_details, tags, timestamp FROM decisions WHERE timestamp >= ? ORDER BY timestamp DESC LIMIT ?")?;
        let rows = stmt.query_map(params![cutoff, limit], |row| {
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
        for r in rows {
            decisions.push(r?);
        }
    }

    let mut progress = Vec::new();
    {
        let mut stmt = conn.prepare("SELECT id, timestamp, status, description, parent_id FROM progress_entries WHERE timestamp >= ? ORDER BY timestamp DESC LIMIT ?")?;
        let rows = stmt.query_map(params![cutoff, limit], |row| {
            Ok(Progress {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                status: row.get(2)?,
                description: row.get(3)?,
                parent_id: row.get(4)?,
            })
        })?;
        for r in rows {
            progress.push(r?);
        }
    }

    let mut patterns = Vec::new();
    {
        let mut stmt = conn.prepare("SELECT id, uuid, name, description, tags, timestamp FROM system_patterns WHERE timestamp >= ? ORDER BY timestamp DESC LIMIT ?")?;
        let rows = stmt.query_map(params![cutoff, limit], |row| {
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
        for r in rows {
            patterns.push(r?);
        }
    }

    let mut custom_data = Vec::new();
    {
        let mut stmt = conn.prepare("SELECT id, timestamp, category, key, value FROM custom_data WHERE timestamp >= ? ORDER BY timestamp DESC LIMIT ?")?;
        let rows = stmt.query_map(params![cutoff, limit], |row| {
            let value_str: String = row.get(4)?;
            let value = serde_json::from_str(&value_str).unwrap_or(Value::String(value_str));
            Ok(CustomData {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                category: row.get(2)?,
                key: row.get(3)?,
                value,
            })
        })?;
        for r in rows {
            custom_data.push(r?);
        }
    }

    Ok(serde_json::json!({
        "since": cutoff,
        "decisions": decisions,
        "progress": progress,
        "patterns": patterns,
        "custom_data": custom_data
    }))
}
