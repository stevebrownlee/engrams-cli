use anyhow::Result;
use rusqlite::{params, Connection};
use serde_json::Value;
use std::fs;
use std::path::Path;

fn extract_json_block(content: &str) -> Option<Value> {
    let start_tag = "```json";
    let end_tag = "```";

    if let Some(start_idx) = content.find(start_tag) {
        let after_start = &content[start_idx + start_tag.len()..];
        if let Some(end_idx) = after_start.find(end_tag) {
            let json_str = &after_start[..end_idx].trim();
            if let Ok(val) = serde_json::from_str(json_str) {
                return Some(val);
            }
        }
    }
    None
}

pub fn handle(conn: &Connection, path: &Path) -> Result<Value> {
    if !path.exists() {
        anyhow::bail!("Export path does not exist: {}", path.display());
    }

    let mut imported = serde_json::Map::new();
    let mut errors = Vec::new();

    let tx = conn.unchecked_transaction()?;

    // Import contexts
    let mut import_context = |file_name: &str, table: &str| -> Result<()> {
        let p = path.join(file_name);
        if p.exists() {
            let content = fs::read_to_string(p)?;
            if let Some(json) = extract_json_block(&content) {
                let content_obj = json.get("content").unwrap_or(&Value::Null);
                let version = json.get("version").and_then(|v| v.as_i64()).unwrap_or(1);
                let updated_at = json
                    .get("updated_at")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                tx.execute(
                    &format!("INSERT INTO {}(id, content, version, updated_at) VALUES (1, ?1, ?2, ?3) ON CONFLICT(id) DO UPDATE SET content=excluded.content, version=excluded.version, updated_at=excluded.updated_at", table),
                    params![serde_json::to_string(content_obj)?, version, updated_at],
                )?;
                imported.insert(table.to_string(), serde_json::json!(1));
            } else {
                errors.push(format!("No valid JSON in {}", file_name));
            }
        }
        Ok(())
    };
    import_context("product_context.md", "product_context")?;
    import_context("active_context.md", "active_context")?;

    // helper to read a dir and process
    let mut process_dir = |dir_name: &str,
                           _table: &str,
                           type_name: &str,
                           f: &dyn Fn(&Value) -> Result<()>|
     -> Result<()> {
        let dir_path = path.join(dir_name);
        let mut count = 0;
        if dir_path.exists() && dir_path.is_dir() {
            for entry in fs::read_dir(dir_path)? {
                let entry = entry?;
                if entry.path().extension().and_then(|s| s.to_str()) == Some("md") {
                    let content = fs::read_to_string(entry.path())?;
                    if let Some(json) = extract_json_block(&content) {
                        match f(&json) {
                            Ok(_) => count += 1,
                            Err(e) => errors.push(format!(
                                "Error importing {}: {}",
                                entry.path().display(),
                                e
                            )),
                        }
                    } else {
                        errors.push(format!("No valid JSON in {}", entry.path().display()));
                    }
                }
            }
        }
        imported.insert(type_name.to_string(), serde_json::json!(count));
        Ok(())
    };

    process_dir("decisions", "decisions", "decisions", &|json| {
        let id = json
            .get("id")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| anyhow::anyhow!("missing id"))?;
        let uuid = json
            .get("uuid")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing uuid"))?;
        let summary = json
            .get("summary")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing summary"))?;
        let rationale = json.get("rationale").and_then(|v| v.as_str());
        let implementation_details = json.get("implementation_details").and_then(|v| v.as_str());
        let tags = json.get("tags").and_then(|v| v.as_array());
        let timestamp = json
            .get("timestamp")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing timestamp"))?;

        let tags_json = if let Some(t) = tags {
            Some(serde_json::to_string(t)?)
        } else {
            None
        };
        let status = json
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("active");
        let commit_sha = json.get("commit_sha").and_then(|v| v.as_str());

        tx.execute(
            "INSERT OR REPLACE INTO decisions (id, uuid, summary, rationale, implementation_details, tags, timestamp, status, commit_sha) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![id, uuid, summary, rationale, implementation_details, tags_json, timestamp, status, commit_sha],
        )?;
        Ok(())
    })?;
    process_dir("progress", "progress_entries", "progress", &|json| {
        let id = json
            .get("id")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| anyhow::anyhow!("missing id"))?;
        let timestamp = json
            .get("timestamp")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing timestamp"))?;
        let status = json
            .get("status")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing status"))?;
        let description = json
            .get("description")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing description"))?;
        let parent_id = json.get("parent_id").and_then(|v| v.as_i64());

        let commit_sha = json.get("commit_sha").and_then(|v| v.as_str());

        tx.execute(
            "INSERT OR REPLACE INTO progress_entries (id, timestamp, status, description, parent_id, commit_sha) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, timestamp, status, description, parent_id, commit_sha],
        )?;
        Ok(())
    })?;
    process_dir("patterns", "system_patterns", "patterns", &|json| {
        let id = json
            .get("id")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| anyhow::anyhow!("missing id"))?; // Not really needed due to upsert logic but keeps ID if no conflict
        let uuid = json
            .get("uuid")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing uuid"))?;
        let name = json
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing name"))?;
        let description = json.get("description").and_then(|v| v.as_str());
        let tags = json.get("tags").and_then(|v| v.as_array());
        let timestamp = json
            .get("timestamp")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing timestamp"))?;

        let tags_json = if let Some(t) = tags {
            Some(serde_json::to_string(t)?)
        } else {
            None
        };

        tx.execute(
            "INSERT INTO system_patterns (id, uuid, name, description, tags, timestamp) VALUES (?1, ?2, ?3, ?4, ?5, ?6) ON CONFLICT(name) DO UPDATE SET description=excluded.description, tags=excluded.tags, timestamp=excluded.timestamp",
            params![id, uuid, name, description, tags_json, timestamp],
        )?;
        Ok(())
    })?;

    process_dir("custom_data", "custom_data", "custom_data", &|json| {
        let id = json
            .get("id")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| anyhow::anyhow!("missing id"))?;
        let timestamp = json
            .get("timestamp")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing timestamp"))?;
        let category = json
            .get("category")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing category"))?;
        let key = json
            .get("key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing key"))?;
        let value = json
            .get("value")
            .ok_or_else(|| anyhow::anyhow!("missing value"))?;

        let value_str = serde_json::to_string(value)?;

        tx.execute(
            "INSERT INTO custom_data (id, timestamp, category, key, value) VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT(category, key) DO UPDATE SET value=excluded.value, timestamp=excluded.timestamp",
            params![id, timestamp, category, key, value_str],
        )?;
        Ok(())
    })?;

    process_dir("links", "context_links", "links", &|json| {
        let id = json
            .get("id")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| anyhow::anyhow!("missing id"))?;
        let source_item_type = json
            .get("source_item_type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing source_item_type"))?;
        let source_item_id = json
            .get("source_item_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing source_item_id"))?;
        let target_item_type = json
            .get("target_item_type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing target_item_type"))?;
        let target_item_id = json
            .get("target_item_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing target_item_id"))?;
        let relationship_type = json
            .get("relationship_type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing relationship_type"))?;
        let description = json.get("description").and_then(|v| v.as_str());
        let timestamp = json
            .get("timestamp")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing timestamp"))?;

        tx.execute(
            "INSERT OR REPLACE INTO context_links (id, source_item_type, source_item_id, target_item_type, target_item_id, relationship_type, description, timestamp) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![id, source_item_type, source_item_id, target_item_type, target_item_id, relationship_type, description, timestamp],
        )?;
        Ok(())
    })?;
    process_dir("anchors", "item_anchors", "anchors", &|json| {
        let id = json
            .get("id")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| anyhow::anyhow!("missing id"))?;
        let item_type = json
            .get("item_type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing item_type"))?;
        let item_id = json
            .get("item_id")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| anyhow::anyhow!("missing item_id"))?;
        let path = json
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing path"))?;
        let timestamp = json
            .get("timestamp")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing timestamp"))?;

        tx.execute(
            "INSERT OR REPLACE INTO item_anchors (id, item_type, item_id, path, timestamp) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, item_type, item_id, path, timestamp],
        )?;
        Ok(())
    })?;

    tx.commit()?;

    Ok(serde_json::json!({
        "path": path.display().to_string(),
        "imported": imported,
        "errors": errors,
    }))
}
