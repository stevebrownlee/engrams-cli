use anyhow::Result;
use rusqlite::{params, Connection};
use serde_json::Value;
use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

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

struct ParsedFile {
    rel_file: String,
    full_path: PathBuf,
    json: Value,
}

fn parse_dir(path: &Path, dir_name: &str, errors: &mut Vec<String>) -> Result<Vec<ParsedFile>> {
    let dir_path = path.join(dir_name);
    let mut out = Vec::new();
    if dir_path.exists() && dir_path.is_dir() {
        let mut entries: Vec<fs::DirEntry> = Vec::new();
        for entry in fs::read_dir(dir_path)? {
            let entry = entry?;
            if entry.path().extension().and_then(|s| s.to_str()) == Some("md") {
                entries.push(entry);
            }
        }
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let full_path = entry.path();
            let rel_file = format!("{dir_name}/{}", entry.file_name().to_string_lossy());
            let content = fs::read_to_string(&full_path)?;
            if let Some(json) = extract_json_block(&content) {
                out.push(ParsedFile {
                    rel_file,
                    full_path,
                    json,
                });
            } else {
                errors.push(format!("No valid JSON in {}", full_path.display()));
            }
        }
    }
    Ok(out)
}

fn canonical_kind(raw: &str) -> &str {
    match raw {
        "pattern" | "system-pattern" | "system_pattern" => "system_pattern",
        "progress-entry" | "progress_entry" => "progress_entry",
        "custom-data" | "custom_data" => "custom_data",
        other => other,
    }
}

fn kind_table_normalized(raw: &str) -> Option<&'static str> {
    crate::models::kind_table(canonical_kind(raw))
}

fn add_candidate_id(buckets: &mut BTreeMap<&'static str, Vec<i64>>, kind: &str, id: i64) {
    if let Some(table) = kind_table_normalized(kind) {
        buckets.entry(table).or_default().push(id);
    }
}

fn existing_ids_in_table(
    conn: &Connection,
    table: &'static str,
    ids: &[i64],
) -> Result<HashSet<i64>> {
    if ids.is_empty() {
        return Ok(HashSet::new());
    }
    let mut out = HashSet::with_capacity(ids.len());
    for chunk in ids.chunks(900) {
        let sql = format!(
            "SELECT id FROM {table} WHERE id IN ({})",
            crate::ops::sql_placeholders(chunk.len())
        );
        let params: Vec<&dyn rusqlite::ToSql> =
            chunk.iter().map(|id| id as &dyn rusqlite::ToSql).collect();
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(params), |r| r.get::<_, i64>(0))?;
        for r in rows {
            out.insert(r?);
        }
    }
    Ok(out)
}

fn entity_exists(
    existing_by_table: &HashMap<&'static str, HashSet<i64>>,
    kind: &str,
    id: i64,
) -> bool {
    let Some(table) = kind_table_normalized(kind) else {
        return false;
    };
    existing_by_table
        .get(table)
        .is_some_and(|set| set.contains(&id))
}

pub fn handle(conn: &Connection, path: &Path, root: Option<&Path>) -> Result<Value> {
    if !path.exists() {
        anyhow::bail!("Export path does not exist: {}", path.display());
    }

    let mut imported = serde_json::Map::new();
    let mut skipped_references = Vec::new();
    let mut errors = Vec::new();

    // Parse all export files before writing so reference validation sees the
    // full export alongside existing database rows.
    let product_context_json = {
        let p = path.join("product_context.md");
        if p.exists() {
            let content = fs::read_to_string(p)?;
            match extract_json_block(&content) {
                Some(json) => Some(json),
                None => {
                    errors.push("No valid JSON in product_context.md".to_string());
                    None
                }
            }
        } else {
            None
        }
    };
    let active_context_json = {
        let p = path.join("active_context.md");
        if p.exists() {
            let content = fs::read_to_string(p)?;
            match extract_json_block(&content) {
                Some(json) => Some(json),
                None => {
                    errors.push("No valid JSON in active_context.md".to_string());
                    None
                }
            }
        } else {
            None
        }
    };

    let decisions_files = parse_dir(path, "decisions", &mut errors)?;
    let progress_files = parse_dir(path, "progress", &mut errors)?;
    let patterns_files = parse_dir(path, "patterns", &mut errors)?;
    let custom_data_files = parse_dir(path, "custom_data", &mut errors)?;
    let links_files = parse_dir(path, "links", &mut errors)?;
    let anchors_files = parse_dir(path, "anchors", &mut errors)?;
    let schemas_files = parse_dir(path, "schemas", &mut errors)?;

    let suggestions_path = path.join("schemas").join("suggestions.json");
    let standalone_suggestions: Option<Vec<Value>> = if suggestions_path.exists() {
        let content = fs::read_to_string(&suggestions_path)?;
        Some(serde_json::from_str(&content)?)
    } else {
        None
    };

    let surfaces_path = path.join("schemas").join("retrieval_surfaces.json");
    let surface_rows: Option<Vec<Value>> = if surfaces_path.exists() {
        let content = fs::read_to_string(&surfaces_path)?;
        Some(serde_json::from_str(&content)?)
    } else {
        None
    };

    let tx = conn.unchecked_transaction()?;

    if let Some(json) = &product_context_json {
        let content_obj = json.get("content").unwrap_or(&Value::Null);
        let version = json.get("version").and_then(|v| v.as_i64()).unwrap_or(1);
        let updated_at = json
            .get("updated_at")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        tx.execute(
            "INSERT INTO product_context(id, content, version, updated_at) VALUES (1, ?1, ?2, ?3) ON CONFLICT(id) DO UPDATE SET content=excluded.content, version=excluded.version, updated_at=excluded.updated_at",
            params![serde_json::to_string(content_obj)?, version, updated_at],
        )?;
        imported.insert("product_context".to_string(), serde_json::json!(1));
    }

    // Active context lives in the name-keyed multi-track table (schema v4);
    // the exported file always represents the 'default' track.
    if let Some(json) = &active_context_json {
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
    }

    let mut decisions_count = 0;
    for pf in &decisions_files {
        let res: Result<()> = (|| {
            let json = &pf.json;
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
            let implementation_details =
                json.get("implementation_details").and_then(|v| v.as_str());
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
        })();
        match res {
            Ok(()) => decisions_count += 1,
            Err(e) => errors.push(format!("Error importing {}: {}", pf.full_path.display(), e)),
        }
    }
    imported.insert("decisions".to_string(), serde_json::json!(decisions_count));

    let mut progress_count = 0;
    let mut pending_parents: Vec<(&str, i64, i64)> = Vec::new();
    for pf in &progress_files {
        let res: Result<Option<(i64, i64)>> = (|| {
            let json = &pf.json;
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
                "INSERT INTO progress_entries (id, timestamp, status, description, parent_id, commit_sha) \
                 VALUES (?1, ?2, ?3, ?4, NULL, ?5) \
                 ON CONFLICT(id) DO UPDATE SET \
                 timestamp=excluded.timestamp, status=excluded.status, \
                 description=excluded.description, parent_id=NULL, commit_sha=excluded.commit_sha",
                params![id, timestamp, status, description, commit_sha],
            )?;
            Ok(parent_id.map(|pid| (id, pid)))
        })();
        match res {
            Ok(opt_parent) => {
                progress_count += 1;
                if let Some((id, pid)) = opt_parent {
                    pending_parents.push((&pf.rel_file, id, pid));
                }
            }
            Err(e) => errors.push(format!("Error importing {}: {}", pf.full_path.display(), e)),
        }
    }
    imported.insert("progress".to_string(), serde_json::json!(progress_count));

    if !pending_parents.is_empty() {
        let mut parent_ids: Vec<i64> = pending_parents.iter().map(|&(_, _, pid)| pid).collect();
        parent_ids.sort_unstable();
        parent_ids.dedup();
        let existing_parents = existing_ids_in_table(&tx, "progress_entries", &parent_ids)?;
        for (rel_file, id, pid) in pending_parents {
            if existing_parents.contains(&pid) {
                tx.execute(
                    "UPDATE progress_entries SET parent_id = ?1 WHERE id = ?2",
                    params![pid, id],
                )?;
            } else {
                skipped_references.push(serde_json::json!({
                    "file": rel_file,
                    "reference": format!("progress_entry:{pid}"),
                    "reason": "parent missing; entry imported without its parent link",
                }));
            }
        }
    }

    let mut patterns_count = 0;
    for pf in &patterns_files {
        let res: Result<()> = (|| {
            let json = &pf.json;
            let id = json
                .get("id")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| anyhow::anyhow!("missing id"))?;
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
        })();
        match res {
            Ok(()) => patterns_count += 1,
            Err(e) => errors.push(format!("Error importing {}: {}", pf.full_path.display(), e)),
        }
    }
    imported.insert("patterns".to_string(), serde_json::json!(patterns_count));

    let mut custom_data_count = 0;
    for pf in &custom_data_files {
        let res: Result<()> = (|| {
            let json = &pf.json;
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
        })();
        match res {
            Ok(()) => custom_data_count += 1,
            Err(e) => errors.push(format!("Error importing {}: {}", pf.full_path.display(), e)),
        }
    }
    imported.insert(
        "custom_data".to_string(),
        serde_json::json!(custom_data_count),
    );

    // Insert schema identity rows before validating reference rows so
    // `schema:<id>` endpoints in links, memberships, suggestions, and
    // retrieval surfaces see all imported schemas.
    let mut valid_schemas: Vec<(&ParsedFile, i64)> = Vec::with_capacity(schemas_files.len());
    for pf in &schemas_files {
        let res: Result<i64> = (|| {
            let json = &pf.json;
            let id = json
                .get("id")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| anyhow::anyhow!("missing id"))?;
            tx.execute(
                "INSERT OR REPLACE INTO schemas (id, uuid, name, summary, summary_source, \
                 status, centroid_json, confidence, importance, access_count, \
                 last_accessed_at, last_confirmed_at, created_at, updated_at, \
                 kind, kind_reasons_json) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
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
                    // spec 0004 AC-6: kind/kind_reasons_json are the exported
                    // confirm-time snapshot — written verbatim, never re-derived.
                    // Exports older than the columns fall back to the defaults.
                    json.get("kind")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unclear"),
                    json.get("kind_reasons_json")
                        .and_then(|v| v.as_str())
                        .unwrap_or("[]"),
                ],
            )?;
            Ok(id)
        })();
        match res {
            Ok(id) => valid_schemas.push((pf, id)),
            Err(e) => errors.push(format!("Error importing {}: {}", pf.full_path.display(), e)),
        }
    }

    // Collect all candidate entity IDs referenced by links, anchors, schema
    // memberships, schema suggestions, and retrieval surfaces, then batch-check
    // their existence against the post-insert tables.
    let mut candidate_ids: BTreeMap<&'static str, Vec<i64>> = BTreeMap::new();
    for pf in &links_files {
        let json = &pf.json;
        if let (Some(st), Some(si)) = (
            json.get("source_item_type").and_then(|v| v.as_str()),
            json.get("source_item_id").and_then(|v| v.as_str()),
        ) {
            let has_path = st == "code"
                && json
                    .get("source_path")
                    .and_then(|v| v.as_str())
                    .is_some_and(|s| !s.is_empty());
            if st != "pr" && !has_path {
                if let Ok(id) = si.parse::<i64>() {
                    add_candidate_id(&mut candidate_ids, st, id);
                }
            }
        }
        if let (Some(tt), Some(ti)) = (
            json.get("target_item_type").and_then(|v| v.as_str()),
            json.get("target_item_id").and_then(|v| v.as_str()),
        ) {
            let has_path = tt == "code"
                && json
                    .get("target_path")
                    .and_then(|v| v.as_str())
                    .is_some_and(|s| !s.is_empty());
            if tt != "pr" && !has_path {
                if let Ok(id) = ti.parse::<i64>() {
                    add_candidate_id(&mut candidate_ids, tt, id);
                }
            }
        }
    }
    for pf in &anchors_files {
        let json = &pf.json;
        if let (Some(it), Some(id)) = (
            json.get("item_type").and_then(|v| v.as_str()),
            json.get("item_id").and_then(|v| v.as_i64()),
        ) {
            add_candidate_id(&mut candidate_ids, it, id);
        }
    }
    for (pf, _) in &valid_schemas {
        let json = &pf.json;
        if let Some(members) = json.get("members").and_then(|v| v.as_array()) {
            for m in members {
                if let (Some(kind), Some(raw_mid)) = (
                    m.get("kind").and_then(|v| v.as_str()),
                    m.get("id").and_then(|v| v.as_str()),
                ) {
                    let has_path = kind == "code"
                        && m.get("path")
                            .and_then(|v| v.as_str())
                            .is_some_and(|s| !s.is_empty());
                    if kind != "pr" && !has_path {
                        if let Ok(mid) = raw_mid.parse::<i64>() {
                            add_candidate_id(&mut candidate_ids, kind, mid);
                        }
                    }
                }
            }
        }
        if let Some(suggestions) = json.get("suggestions").and_then(|v| v.as_array()) {
            for s in suggestions {
                let item_kind = s
                    .get("item_kind")
                    .and_then(|v| v.as_str())
                    .unwrap_or("decision");
                if let Some(item_id) = s.get("item_id").and_then(|v| v.as_i64()) {
                    add_candidate_id(&mut candidate_ids, item_kind, item_id);
                }
            }
        }
    }
    if let Some(rows) = &standalone_suggestions {
        for s in rows {
            if let Some(sid) = s.get("schema_id").and_then(|v| v.as_i64()) {
                add_candidate_id(&mut candidate_ids, "schema", sid);
            }
            let item_kind = s
                .get("item_kind")
                .and_then(|v| v.as_str())
                .unwrap_or("decision");
            if let Some(item_id) = s.get("item_id").and_then(|v| v.as_i64()) {
                add_candidate_id(&mut candidate_ids, item_kind, item_id);
            }
        }
    }
    if let Some(rows) = &surface_rows {
        for r in rows {
            if let Some(sid) = r.get("schema_id").and_then(|v| v.as_i64()) {
                add_candidate_id(&mut candidate_ids, "schema", sid);
            }
            let node_kind = r.get("node_kind").and_then(|v| v.as_str()).unwrap_or("");
            let has_path = node_kind == "code"
                && r.get("path")
                    .and_then(|v| v.as_str())
                    .is_some_and(|s| !s.is_empty());
            if !has_path {
                if let Some(node_id) = r.get("node_id").and_then(|v| v.as_i64()) {
                    add_candidate_id(&mut candidate_ids, node_kind, node_id);
                }
            }
        }
    }

    let mut existing_by_table: HashMap<&'static str, HashSet<i64>> =
        HashMap::with_capacity(candidate_ids.len());
    for (table, mut ids) in candidate_ids {
        ids.sort_unstable();
        ids.dedup();
        existing_by_table.insert(table, existing_ids_in_table(&tx, table, &ids)?);
    }

    let mut links_count = 0;
    for pf in &links_files {
        let res: Result<bool> = (|| {
            let json = &pf.json;
            let id = json
                .get("id")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| anyhow::anyhow!("missing id"))?;
            let source_item_type = json
                .get("source_item_type")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing source_item_type"))?;
            let raw_source_id = json
                .get("source_item_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing source_item_id"))?;
            let target_item_type = json
                .get("target_item_type")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing target_item_type"))?;
            let raw_target_id = json
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

            let source_path = if source_item_type == "code" {
                json.get("source_path")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
            } else {
                None
            };
            let target_path = if target_item_type == "code" {
                json.get("target_path")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
            } else {
                None
            };

            let source_valid = source_item_type == "pr"
                || source_path.is_some()
                || raw_source_id
                    .parse::<i64>()
                    .is_ok_and(|sid| entity_exists(&existing_by_table, source_item_type, sid));
            let target_valid = target_item_type == "pr"
                || target_path.is_some()
                || raw_target_id
                    .parse::<i64>()
                    .is_ok_and(|tid| entity_exists(&existing_by_table, target_item_type, tid));

            if !source_valid || !target_valid {
                if !source_valid {
                    skipped_references.push(serde_json::json!({
                        "file": pf.rel_file,
                        "reference": format!("{source_item_type}:{raw_source_id}"),
                        "reason": format!("no {source_item_type} {raw_source_id}"),
                    }));
                }
                if !target_valid {
                    skipped_references.push(serde_json::json!({
                        "file": pf.rel_file,
                        "reference": format!("{target_item_type}:{raw_target_id}"),
                        "reason": format!("no {target_item_type} {raw_target_id}"),
                    }));
                }
                return Ok(false);
            }

            let source_item_id: Cow<'_, str> = if let Some(sp) = source_path {
                let new_id = crate::ops::graph::code::upsert_file(&tx, sp, timestamp, root)?;
                existing_by_table
                    .entry("code_nodes")
                    .or_default()
                    .insert(new_id);
                Cow::Owned(new_id.to_string())
            } else {
                Cow::Borrowed(raw_source_id)
            };
            let target_item_id: Cow<'_, str> = if let Some(tp) = target_path {
                let new_id = crate::ops::graph::code::upsert_file(&tx, tp, timestamp, root)?;
                existing_by_table
                    .entry("code_nodes")
                    .or_default()
                    .insert(new_id);
                Cow::Owned(new_id.to_string())
            } else {
                Cow::Borrowed(raw_target_id)
            };
            tx.execute(
                "INSERT OR REPLACE INTO context_links (id, source_item_type, source_item_id, target_item_type, target_item_id, relationship_type, description, timestamp, origin, source) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![id, source_item_type, source_item_id, target_item_type, target_item_id, relationship_type, description, timestamp, origin, source],
            )?;
            Ok(true)
        })();
        match res {
            Ok(true) => links_count += 1,
            Ok(false) => {}
            Err(e) => errors.push(format!("Error importing {}: {}", pf.full_path.display(), e)),
        }
    }
    imported.insert("links".to_string(), serde_json::json!(links_count));

    let mut anchors_count = 0;
    for pf in &anchors_files {
        let res: Result<bool> = (|| {
            let json = &pf.json;
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

            if !entity_exists(&existing_by_table, item_type, item_id) {
                skipped_references.push(serde_json::json!({
                    "file": pf.rel_file,
                    "reference": format!("{item_type}:{item_id}"),
                    "reason": format!("no {item_type} {item_id}"),
                }));
                return Ok(false);
            }

            tx.execute(
                "INSERT OR REPLACE INTO item_anchors (id, item_type, item_id, path, timestamp) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, item_type, item_id, path, timestamp],
            )?;
            Ok(true)
        })();
        match res {
            Ok(true) => anchors_count += 1,
            Ok(false) => {}
            Err(e) => errors.push(format!("Error importing {}: {}", pf.full_path.display(), e)),
        }
    }
    imported.insert("anchors".to_string(), serde_json::json!(anchors_count));

    let mut schemas_count = 0;
    for (pf, id) in valid_schemas {
        let res: Result<()> = (|| {
            let json = &pf.json;
            // Membership travels with the schema (the generic links export only
            // carries manual edges).
            if let Some(members) = json.get("members").and_then(|v| v.as_array()) {
                let created_at = json
                    .get("created_at")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                for m in members {
                    let kind = m
                        .get("kind")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| anyhow::anyhow!("missing member kind"))?;
                    let raw_mid = m
                        .get("id")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| anyhow::anyhow!("missing member id"))?;
                    let member_path = if kind == "code" {
                        m.get("path")
                            .and_then(|v| v.as_str())
                            .filter(|s| !s.is_empty())
                    } else {
                        None
                    };
                    let member_valid = kind == "pr"
                        || member_path.is_some()
                        || raw_mid
                            .parse::<i64>()
                            .is_ok_and(|mid| entity_exists(&existing_by_table, kind, mid));
                    if !member_valid {
                        skipped_references.push(serde_json::json!({
                            "file": pf.rel_file,
                            "reference": format!("{kind}:{raw_mid}"),
                            "reason": format!("no {kind} {raw_mid}"),
                        }));
                        continue;
                    }
                    let mid: Cow<'_, str> = if let Some(mp) = member_path {
                        let new_id =
                            crate::ops::graph::code::upsert_file(&tx, mp, created_at, root)?;
                        existing_by_table
                            .entry("code_nodes")
                            .or_default()
                            .insert(new_id);
                        Cow::Owned(new_id.to_string())
                    } else {
                        Cow::Borrowed(raw_mid)
                    };
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
                        params![kind, mid, id, created_at, origin, provenance],
                    )?;
                }
            }
            // Fired-suggestion resolutions (accepted/declined are user intent,
            // not recomputable).
            if let Some(suggestions) = json.get("suggestions").and_then(|v| v.as_array()) {
                for s in suggestions {
                    let item_kind = s
                        .get("item_kind")
                        .and_then(|v| v.as_str())
                        .unwrap_or("decision");
                    let item_id = s
                        .get("item_id")
                        .and_then(|v| v.as_i64())
                        .ok_or_else(|| anyhow::anyhow!("missing suggestion item_id"))?;
                    if !entity_exists(&existing_by_table, item_kind, item_id) {
                        skipped_references.push(serde_json::json!({
                            "file": pf.rel_file,
                            "reference": format!("{item_kind}:{item_id}"),
                            "reason": format!("no {item_kind} {item_id}"),
                        }));
                        continue;
                    }
                    tx.execute(
                        "INSERT OR REPLACE INTO schema_suggestions (ts, schema_id, item_kind, item_id, fit, status) \
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                        params![
                            s.get("ts").and_then(|v| v.as_str()).unwrap_or(""),
                            id,
                            item_kind,
                            item_id,
                            s.get("fit").and_then(|v| v.as_f64()).unwrap_or(0.0),
                            s.get("status").and_then(|v| v.as_str()).unwrap_or("suggested"),
                        ],
                    )?;
                }
            }
            Ok(())
        })();
        match res {
            Ok(()) => schemas_count += 1,
            Err(e) => errors.push(format!("Error importing {}: {}", pf.full_path.display(), e)),
        }
    }
    imported.insert("schemas".to_string(), serde_json::json!(schemas_count));

    if let Some(rows) = standalone_suggestions {
        for s in rows {
            let schema_id = s.get("schema_id").and_then(|v| v.as_i64()).unwrap_or(0);
            let item_kind = s
                .get("item_kind")
                .and_then(|v| v.as_str())
                .unwrap_or("decision");
            let item_id = s.get("item_id").and_then(|v| v.as_i64()).unwrap_or(0);
            let schema_ok = entity_exists(&existing_by_table, "schema", schema_id);
            let item_ok = entity_exists(&existing_by_table, item_kind, item_id);
            if !schema_ok {
                skipped_references.push(serde_json::json!({
                    "file": "schemas/suggestions.json",
                    "reference": format!("schema:{schema_id}"),
                    "reason": format!("no schema {schema_id}"),
                }));
            }
            if !item_ok {
                skipped_references.push(serde_json::json!({
                    "file": "schemas/suggestions.json",
                    "reference": format!("{item_kind}:{item_id}"),
                    "reason": format!("no {item_kind} {item_id}"),
                }));
            }
            if !schema_ok || !item_ok {
                continue;
            }
            tx.execute(
                "INSERT OR REPLACE INTO schema_suggestions (ts, schema_id, item_kind, item_id, fit, status) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    s.get("ts").and_then(|v| v.as_str()).unwrap_or(""),
                    schema_id,
                    item_kind,
                    item_id,
                    s.get("fit").and_then(|v| v.as_f64()).unwrap_or(0.0),
                    s.get("status").and_then(|v| v.as_str()).unwrap_or("suggested"),
                ],
            )?;
        }
    }

    // Reward telemetry (AC-11): restored wholesale — bounded rolling window.
    if let Some(rows) = surface_rows {
        for r in rows {
            let ts = r.get("ts").and_then(|v| v.as_str()).unwrap_or("");
            let cmd = r.get("cmd").and_then(|v| v.as_str()).unwrap_or("");
            let arg = r.get("arg").and_then(|v| v.as_str());
            let node_kind = r.get("node_kind").and_then(|v| v.as_str()).unwrap_or("");
            let raw_node_id = r.get("node_id").and_then(|v| v.as_i64()).unwrap_or(0);

            let schema_ok = match r.get("schema_id").and_then(|v| v.as_i64()) {
                Some(sid) if !entity_exists(&existing_by_table, "schema", sid) => {
                    skipped_references.push(serde_json::json!({
                        "file": "schemas/retrieval_surfaces.json",
                        "reference": format!("schema:{sid}"),
                        "reason": format!("no schema {sid}"),
                    }));
                    false
                }
                _ => true,
            };

            let surface_path = if node_kind == "code" {
                r.get("path")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
            } else {
                None
            };
            let node_ok =
                surface_path.is_some() || entity_exists(&existing_by_table, node_kind, raw_node_id);
            if !node_ok {
                skipped_references.push(serde_json::json!({
                    "file": "schemas/retrieval_surfaces.json",
                    "reference": format!("{node_kind}:{raw_node_id}"),
                    "reason": format!("no {node_kind} {raw_node_id}"),
                }));
            }
            if !schema_ok || !node_ok {
                continue;
            }

            let node_id = if let Some(sp) = surface_path {
                let new_id = crate::ops::graph::code::upsert_file(&tx, sp, ts, root)?;
                existing_by_table
                    .entry("code_nodes")
                    .or_default()
                    .insert(new_id);
                new_id
            } else {
                raw_node_id
            };
            // No unique key on this table; the natural key (ts, cmd, arg,
            // node_kind, node_id) guards against double-import duplication.
            tx.execute(
                "INSERT INTO retrieval_surfaces (ts, cmd, arg, node_kind, node_id) \
                 SELECT ?1, ?2, ?3, ?4, ?5 WHERE NOT EXISTS (\
                   SELECT 1 FROM retrieval_surfaces WHERE ts = ?1 AND cmd = ?2 \
                   AND arg IS ?3 AND node_kind = ?4 AND node_id = ?5)",
                params![ts, cmd, arg, node_kind, node_id],
            )?;
        }
    }

    tx.commit()?;

    Ok(serde_json::json!({
        "path": path.display().to_string(),
        "imported": imported,
        "skipped_references": skipped_references,
        "errors": errors,
    }))
}
