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

    // Active context lives in the name-keyed multi-track table (schema v4);
    // the exported file always represents the 'default' track.
    let active_path = path.join("active_context.md");
    if active_path.exists() {
        let content = fs::read_to_string(&active_path)?;
        if let Some(json) = extract_json_block(&content) {
            let content_obj = json.get("content").unwrap_or(&Value::Null);
            let version = json.get("version").and_then(|v| v.as_i64()).unwrap_or(1);
            let updated_at = json
                .get("updated_at")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            tx.execute(
                "INSERT INTO active_contexts(name, content, version, updated_at) VALUES ('default', ?1, ?2, ?3) ON CONFLICT(name) DO UPDATE SET content=excluded.content, version=excluded.version, updated_at=excluded.updated_at",
                params![serde_json::to_string(content_obj)?, version, updated_at],
            )?;
            imported.insert("active_context".to_string(), serde_json::json!(1));
        } else {
            errors.push("No valid JSON in active_context.md".to_string());
        }
    }

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
        let importance = json.get("importance").and_then(|v| v.as_i64()).unwrap_or(5);
        let access_count = json
            .get("access_count")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let last_accessed_at = json.get("last_accessed_at").and_then(|v| v.as_str());
        let archived = json.get("archived").and_then(|v| v.as_i64()).unwrap_or(0);
        let contract = json.get("contract").and_then(|v| v.as_str());

        tx.execute(
            "INSERT OR REPLACE INTO decisions (id, uuid, summary, rationale, implementation_details, tags, timestamp, status, commit_sha, importance, access_count, last_accessed_at, archived, contract) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![id, uuid, summary, rationale, implementation_details, tags_json, timestamp, status, commit_sha, importance, access_count, last_accessed_at, archived, contract],
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
        let check_kind = json.get("check_kind").and_then(|v| v.as_str());
        let check_expr = json.get("check_expr").and_then(|v| v.as_str());
        let severity = json
            .get("severity")
            .and_then(|v| v.as_str())
            .unwrap_or("warn");
        let importance = json.get("importance").and_then(|v| v.as_i64()).unwrap_or(5);
        let access_count = json
            .get("access_count")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let last_accessed_at = json.get("last_accessed_at").and_then(|v| v.as_str());
        let archived = json.get("archived").and_then(|v| v.as_i64()).unwrap_or(0);
        let confidence = json
            .get("confidence")
            .and_then(|v| v.as_f64())
            .unwrap_or(1.0);
        let last_confirmed_at = json.get("last_confirmed_at").and_then(|v| v.as_str());

        tx.execute(
            "INSERT INTO system_patterns (id, uuid, name, description, tags, timestamp, check_kind, check_expr, severity, importance, access_count, last_accessed_at, archived, confidence, last_confirmed_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15) ON CONFLICT(name) DO UPDATE SET description=excluded.description, tags=excluded.tags, timestamp=excluded.timestamp, check_kind=excluded.check_kind, check_expr=excluded.check_expr, severity=excluded.severity, importance=excluded.importance, access_count=excluded.access_count, last_accessed_at=excluded.last_accessed_at, archived=excluded.archived, confidence=excluded.confidence, last_confirmed_at=excluded.last_confirmed_at",
            params![id, uuid, name, description, tags_json, timestamp, check_kind, check_expr, severity, importance, access_count, last_accessed_at, archived, confidence, last_confirmed_at],
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
        let origin = json
            .get("origin")
            .and_then(|v| v.as_str())
            .unwrap_or("manual");

        let source = json.get("source").and_then(|v| v.as_str());
        tx.execute(
            "INSERT OR REPLACE INTO context_links (id, source_item_type, source_item_id, target_item_type, target_item_id, relationship_type, description, timestamp, origin, source) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![id, source_item_type, source_item_id, target_item_type, target_item_id, relationship_type, description, timestamp, origin, source],
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
    // Schemas (spec 0002, AC-11): identity rows, restored with their
    // original ids — knowledge items import with original ids above, so
    // member_of endpoints need no remapping. Plain INSERTs fire the
    // schemas_fts sync triggers (S14 asserts the index works post-import).
    process_dir("schemas", "schemas", "schemas", &|json| {
        let id = json
            .get("id")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| anyhow::anyhow!("missing id"))?;
        tx.execute(
            "INSERT OR REPLACE INTO schemas (id, uuid, name, summary, summary_source, \
             status, centroid_json, confidence, importance, access_count, \
             last_accessed_at, last_confirmed_at, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                id,
                json.get("uuid")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing uuid"))?,
                json.get("name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing name"))?,
                json.get("summary").and_then(|v| v.as_str()).unwrap_or(""),
                json.get("summary_source")
                    .and_then(|v| v.as_str())
                    .unwrap_or("drafted"),
                json.get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("active"),
                json.get("centroid_json")
                    .and_then(|v| v.as_str())
                    .unwrap_or("{}"),
                json.get("confidence")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0),
                json.get("importance")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0),
                json.get("access_count")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0),
                json.get("last_accessed_at").and_then(|v| v.as_str()),
                json.get("last_confirmed_at").and_then(|v| v.as_str()),
                json.get("created_at")
                    .and_then(|v| v.as_str())
                    .unwrap_or(""),
                json.get("updated_at")
                    .and_then(|v| v.as_str())
                    .unwrap_or(""),
            ],
        )?;
        // Membership travels with the schema (the generic links export only
        // carries manual edges).
        if let Some(members) = json.get("members").and_then(|v| v.as_array()) {
            for m in members {
                let kind = m
                    .get("kind")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing member kind"))?;
                let mid = m
                    .get("id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing member id"))?;
                let origin = m
                    .get("origin")
                    .and_then(|v| v.as_str())
                    .unwrap_or("schema_confirm");
                let provenance = m.get("source").and_then(|v| v.as_str());
                // The generic links export already carries manual-origin
                // member_of edges (confirm writes origin='manual') — insert
                // only when the membership is genuinely absent.
                tx.execute(
                    "INSERT INTO context_links (source_item_type, source_item_id, \
                     target_item_type, target_item_id, relationship_type, timestamp, origin, source, weight) \
                     SELECT ?1, ?2, 'schema', ?3, 'member_of', ?4, ?5, ?6, 1.0 \
                     WHERE NOT EXISTS (\
                       SELECT 1 FROM context_links WHERE relationship_type = 'member_of' \
                       AND target_item_type = 'schema' AND target_item_id = ?3 \
                       AND source_item_type = ?1 AND source_item_id = ?2)",
                    params![
                        kind,
                        mid,
                        id,
                        json.get("created_at").and_then(|v| v.as_str()).unwrap_or(""),
                        origin,
                        provenance,
                    ],
                )?;
            }
        }
        // Fired-suggestion resolutions (accepted/declined are user intent,
        // not recomputable).
        if let Some(suggestions) = json.get("suggestions").and_then(|v| v.as_array()) {
            for s in suggestions {
                tx.execute(
                    "INSERT OR REPLACE INTO schema_suggestions (ts, schema_id, item_kind, item_id, fit, status) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        s.get("ts").and_then(|v| v.as_str()).unwrap_or(""),
                        id,
                        s.get("item_kind").and_then(|v| v.as_str()).unwrap_or("decision"),
                        s.get("item_id").and_then(|v| v.as_i64()),
                        s.get("fit").and_then(|v| v.as_f64()).unwrap_or(0.0),
                        s.get("status").and_then(|v| v.as_str()).unwrap_or("suggested"),
                    ],
                )?;
            }
        }
        Ok(())
    })?;
    // Reward telemetry (AC-11): restored wholesale — bounded rolling window.
    let surfaces_path = path.join("schemas").join("retrieval_surfaces.json");
    if surfaces_path.exists() {
        let content = fs::read_to_string(&surfaces_path)?;
        let rows: Vec<Value> = serde_json::from_str(&content)?;
        for r in rows {
            // No unique key on this table; the natural key (ts, cmd, arg,
            // node_kind, node_id) guards against double-import duplication.
            tx.execute(
                "INSERT INTO retrieval_surfaces (ts, cmd, arg, node_kind, node_id) \
                 SELECT ?1, ?2, ?3, ?4, ?5 WHERE NOT EXISTS (\
                   SELECT 1 FROM retrieval_surfaces WHERE ts = ?1 AND cmd = ?2 \
                   AND arg IS ?3 AND node_kind = ?4 AND node_id = ?5)",
                params![
                    r.get("ts").and_then(|v| v.as_str()).unwrap_or(""),
                    r.get("cmd").and_then(|v| v.as_str()).unwrap_or(""),
                    r.get("arg").and_then(|v| v.as_str()),
                    r.get("node_kind").and_then(|v| v.as_str()).unwrap_or(""),
                    r.get("node_id").and_then(|v| v.as_i64()).unwrap_or(0),
                ],
            )?;
        }
    }

    tx.commit()?;

    Ok(serde_json::json!({
        "path": path.display().to_string(),
        "imported": imported,
        "errors": errors,
    }))
}
