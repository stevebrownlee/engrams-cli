use anyhow::Result;
use chrono::Utc;
use rusqlite::Connection;
use serde_json::Value;
use std::fs;
use std::path::Path;

pub fn handle(conn: &Connection, path: &Path) -> Result<Value> {
    fs::create_dir_all(path)?;
    fs::create_dir_all(path.join("decisions"))?;
    fs::create_dir_all(path.join("progress"))?;
    fs::create_dir_all(path.join("patterns"))?;
    fs::create_dir_all(path.join("custom_data"))?;
    fs::create_dir_all(path.join("links"))?;

    let mut counts = serde_json::Map::new();

    // Export Product Context
    let product_context = crate::ops::context::get(conn, "product_context")?;
    if product_context
        .get("version")
        .and_then(|v| v.as_i64())
        .unwrap_or(0)
        > 0
    {
        let content = format!(
            "# Product Context\n\n```json\n{}\n```\n",
            serde_json::to_string_pretty(&product_context)?
        );
        fs::write(path.join("product_context.md"), content)?;
        counts.insert("product_context".to_string(), serde_json::json!(1));
    }

    // Export Active Context
    let active_context = crate::ops::context::get(conn, "active_context")?;
    if active_context
        .get("version")
        .and_then(|v| v.as_i64())
        .unwrap_or(0)
        > 0
    {
        let content = format!(
            "# Active Context\n\n```json\n{}\n```\n",
            serde_json::to_string_pretty(&active_context)?
        );
        fs::write(path.join("active_context.md"), content)?;
        counts.insert("active_context".to_string(), serde_json::json!(1));
    }

    // Export Decisions
    let mut stmt = conn.prepare("SELECT id, uuid, summary, rationale, implementation_details, tags, timestamp FROM decisions")?;
    let rows = stmt.query_map([], |row| {
        let tags_str: Option<String> = row.get(5)?;
        let tags = match tags_str {
            Some(s) => serde_json::from_str(&s).unwrap_or(Value::Null),
            None => Value::Null,
        };
        Ok(serde_json::json!({
            "id": row.get::<_, i64>(0)?,
            "uuid": row.get::<_, String>(1)?,
            "summary": row.get::<_, String>(2)?,
            "rationale": row.get::<_, Option<String>>(3)?,
            "implementation_details": row.get::<_, Option<String>>(4)?,
            "tags": if tags.is_null() { None } else { Some(tags) },
            "timestamp": row.get::<_, String>(6)?,
        }))
    })?;
    let mut decisions_count = 0;
    for r in rows {
        let r = r?;
        let id = r.get("id").unwrap().as_i64().unwrap();
        let summary = r.get("summary").unwrap().as_str().unwrap();
        let content = format!(
            "# {}\n\n```json\n{}\n```\n",
            summary,
            serde_json::to_string_pretty(&r)?
        );
        fs::write(path.join("decisions").join(format!("{}.md", id)), content)?;
        decisions_count += 1;
    }
    counts.insert("decisions".to_string(), serde_json::json!(decisions_count));

    // Export Progress
    let mut stmt =
        conn.prepare("SELECT id, timestamp, status, description, parent_id FROM progress_entries")?;
    let rows = stmt.query_map([], |row| {
        Ok(serde_json::json!({
            "id": row.get::<_, i64>(0)?,
            "timestamp": row.get::<_, String>(1)?,
            "status": row.get::<_, String>(2)?,
            "description": row.get::<_, String>(3)?,
            "parent_id": row.get::<_, Option<i64>>(4)?,
        }))
    })?;
    let mut progress_count = 0;
    for r in rows {
        let r = r?;
        let id = r.get("id").unwrap().as_i64().unwrap();
        let description = r.get("description").unwrap().as_str().unwrap();
        let content = format!(
            "# {}\n\n```json\n{}\n```\n",
            description,
            serde_json::to_string_pretty(&r)?
        );
        fs::write(path.join("progress").join(format!("{}.md", id)), content)?;
        progress_count += 1;
    }
    counts.insert("progress".to_string(), serde_json::json!(progress_count));

    // Export Patterns
    let mut stmt =
        conn.prepare("SELECT id, uuid, name, description, tags, timestamp FROM system_patterns")?;
    let rows = stmt.query_map([], |row| {
        let tags_str: Option<String> = row.get(4)?;
        let tags = match tags_str {
            Some(s) => serde_json::from_str(&s).unwrap_or(Value::Null),
            None => Value::Null,
        };
        Ok(serde_json::json!({
            "id": row.get::<_, i64>(0)?,
            "uuid": row.get::<_, String>(1)?,
            "name": row.get::<_, String>(2)?,
            "description": row.get::<_, Option<String>>(3)?,
            "tags": if tags.is_null() { None } else { Some(tags) },
            "timestamp": row.get::<_, String>(5)?,
        }))
    })?;
    let mut patterns_count = 0;
    for r in rows {
        let r = r?;
        let id = r.get("id").unwrap().as_i64().unwrap();
        let name = r.get("name").unwrap().as_str().unwrap();
        let content = format!(
            "# {}\n\n```json\n{}\n```\n",
            name,
            serde_json::to_string_pretty(&r)?
        );
        fs::write(path.join("patterns").join(format!("{}.md", id)), content)?;
        patterns_count += 1;
    }
    counts.insert("patterns".to_string(), serde_json::json!(patterns_count));

    // Export Custom Data
    let mut stmt = conn.prepare("SELECT id, timestamp, category, key, value FROM custom_data")?;
    let rows = stmt.query_map([], |row| {
        let value_str: String = row.get(4)?;
        let value = serde_json::from_str(&value_str).unwrap_or(Value::String(value_str));
        Ok(serde_json::json!({
            "id": row.get::<_, i64>(0)?,
            "timestamp": row.get::<_, String>(1)?,
            "category": row.get::<_, String>(2)?,
            "key": row.get::<_, String>(3)?,
            "value": value,
        }))
    })?;
    let mut custom_count = 0;
    for r in rows {
        let r = r?;
        let id = r.get("id").unwrap().as_i64().unwrap();
        let category = r.get("category").unwrap().as_str().unwrap();
        let key = r.get("key").unwrap().as_str().unwrap();
        let content = format!(
            "# {}:{}\n\n```json\n{}\n```\n",
            category,
            key,
            serde_json::to_string_pretty(&r)?
        );
        fs::write(path.join("custom_data").join(format!("{}.md", id)), content)?;
        custom_count += 1;
    }
    counts.insert("custom_data".to_string(), serde_json::json!(custom_count));

    // Export Links
    let mut stmt = conn.prepare("SELECT id, source_item_type, source_item_id, target_item_type, target_item_id, relationship_type, description, timestamp FROM context_links")?;
    let rows = stmt.query_map([], |row| {
        Ok(serde_json::json!({
            "id": row.get::<_, i64>(0)?,
            "source_item_type": row.get::<_, String>(1)?,
            "source_item_id": row.get::<_, String>(2)?,
            "target_item_type": row.get::<_, String>(3)?,
            "target_item_id": row.get::<_, String>(4)?,
            "relationship_type": row.get::<_, String>(5)?,
            "description": row.get::<_, Option<String>>(6)?,
            "timestamp": row.get::<_, String>(7)?,
        }))
    })?;
    let mut links_count = 0;
    for r in rows {
        let r = r?;
        let id = r.get("id").unwrap().as_i64().unwrap();
        let rel = r.get("relationship_type").unwrap().as_str().unwrap();
        let content = format!(
            "# {}\n\n```json\n{}\n```\n",
            rel,
            serde_json::to_string_pretty(&r)?
        );
        fs::write(path.join("links").join(format!("{}.md", id)), content)?;
        links_count += 1;
    }
    counts.insert("links".to_string(), serde_json::json!(links_count));

    let manifest = serde_json::json!({
        "exported_at": Utc::now().to_rfc3339(),
        "counts": counts,
    });
    fs::write(
        path.join("manifest.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;

    Ok(serde_json::json!({
        "path": path.display().to_string(),
        "counts": counts,
    }))
}
