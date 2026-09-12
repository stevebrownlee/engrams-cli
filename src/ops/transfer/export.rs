use anyhow::Result;
use chrono::Utc;
use rusqlite::Connection;
use serde_json::Value;
use std::fs;
use std::path::Path;

/// One YAML frontmatter field: a scalar or a (possibly empty) list of scalars.
enum FmValue {
    Scalar(String),
    List(Vec<String>),
}

/// Render a `---` delimited YAML frontmatter block from ordered fields.
/// Empty lists are omitted entirely.
fn yaml_frontmatter(fields: &[(&str, FmValue)]) -> String {
    let mut out = String::from("---\n");
    for (key, value) in fields {
        match value {
            FmValue::Scalar(s) => {
                out.push_str(key);
                out.push_str(": ");
                out.push_str(&yaml_scalar(s));
                out.push('\n');
            }
            FmValue::List(items) => {
                if items.is_empty() {
                    continue;
                }
                out.push_str(key);
                out.push_str(":\n");
                for item in items {
                    out.push_str("  - ");
                    out.push_str(&yaml_scalar(item));
                    out.push('\n');
                }
            }
        }
    }
    out.push_str("---\n");
    out
}

/// Render a YAML scalar, quoting only when a bare value would be misparsed.
fn yaml_scalar(s: &str) -> String {
    if !yaml_needs_quoting(s) {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn yaml_needs_quoting(s: &str) -> bool {
    if s.is_empty() || s != s.trim() {
        return true;
    }
    if s.contains(": ")
        || s.ends_with(':')
        || s.contains('#')
        || s.contains('\n')
        || s.contains('\r')
        || s.contains('\t')
    {
        return true;
    }
    // YAML indicator characters are unsafe in leading position.
    const SPECIAL_LEADERS: &[char] = &[
        '-', '?', ':', ',', '[', ']', '{', '}', '&', '*', '!', '|', '>', '\'', '"', '%', '@', '`',
    ];
    if SPECIAL_LEADERS.contains(&s.chars().next().unwrap()) {
        return true;
    }
    // Bare words that a YAML parser would read as a non-string scalar.
    let lower = s.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "true" | "false" | "yes" | "no" | "on" | "off" | "null" | "~" | ".nan" | ".inf"
    ) || s.parse::<f64>().is_ok()
    {
        return true;
    }
    false
}

/// Pull the string list out of a JSON item's `tags` field.
fn tags_list(v: &Value) -> Vec<String> {
    v.get("tags")
        .and_then(|t| t.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|t| t.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Non-empty string field from a JSON item, when present.
fn opt_str(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(|x| x.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Map a canonical progress status to its schema.org ActionStatus.
/// Non-canonical (legacy/unmigrated) statuses export without actionStatus.
fn action_status(status: &str) -> Option<&'static str> {
    match status {
        "Done" => Some("CompletedActionStatus"),
        "InProgress" | "InReview" => Some("ActiveActionStatus"),
        "Todo" | "Blocked" => Some("PotentialActionStatus"),
        "Dropped" => Some("FailedActionStatus"),
        _ => None,
    }
}

pub fn handle(conn: &Connection, path: &Path) -> Result<Value> {
    fs::create_dir_all(path)?;
    fs::create_dir_all(path.join("decisions"))?;
    fs::create_dir_all(path.join("progress"))?;
    fs::create_dir_all(path.join("patterns"))?;
    fs::create_dir_all(path.join("custom_data"))?;
    fs::create_dir_all(path.join("links"))?;
    fs::create_dir_all(path.join("anchors"))?;
    fs::create_dir_all(path.join("schemas"))?;

    let mut counts = serde_json::Map::new();

    // Export Product Context
    let product_context = crate::ops::context::get(conn, "product_context")?;
    if product_context
        .get("version")
        .and_then(|v| v.as_i64())
        .unwrap_or(0)
        > 0
    {
        let mut fields = vec![
            ("identifier", FmValue::Scalar("product_context".to_string())),
            ("title", FmValue::Scalar("Product Context".to_string())),
        ];
        if let Some(updated) = opt_str(&product_context, "updated_at") {
            fields.push(("created", FmValue::Scalar(updated)));
        }
        let content = format!(
            "{}# Product Context\n\n```json\n{}\n```\n",
            yaml_frontmatter(&fields),
            serde_json::to_string_pretty(&product_context)?
        );
        fs::write(path.join("product_context.md"), content)?;
        counts.insert("product_context".to_string(), serde_json::json!(1));
    }

    // Export Active Context (the 'default' track in the multi-track schema)
    let active_context = crate::ops::context::get_track(conn, "default")?;
    if active_context
        .get("version")
        .and_then(|v| v.as_i64())
        .unwrap_or(0)
        > 0
    {
        let mut fields = vec![
            ("identifier", FmValue::Scalar("active_context".to_string())),
            ("title", FmValue::Scalar("Active Context".to_string())),
        ];
        if let Some(updated) = opt_str(&active_context, "updated_at") {
            fields.push(("created", FmValue::Scalar(updated)));
        }
        let content = format!(
            "{}# Active Context\n\n```json\n{}\n```\n",
            yaml_frontmatter(&fields),
            serde_json::to_string_pretty(&active_context)?
        );
        fs::write(path.join("active_context.md"), content)?;
        counts.insert("active_context".to_string(), serde_json::json!(1));
    }

    // Export Decisions
    let mut stmt = conn.prepare(&format!(
        "SELECT {} FROM decisions",
        crate::models::DECISION_COLS
    ))?;
    let rows = stmt.query_map([], |row| {
        let tags_str: Option<String> = row.get("tags")?;
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
            "status": row.get::<_, String>(7)?,
            "commit_sha": row.get::<_, Option<String>>(8)?,
            "importance": row.get::<_, i64>(9)?,
            "access_count": row.get::<_, i64>(10)?,
            "last_accessed_at": row.get::<_, Option<String>>(11)?,
            "archived": row.get::<_, i64>(12)?,
            "contract": row.get::<_, Option<String>>(13)?,
        }))
    })?;
    let mut decisions_count = 0;
    for r in rows {
        let r = r?;
        let id = r.get("id").unwrap().as_i64().unwrap();
        let summary = r.get("summary").unwrap().as_str().unwrap();
        let mut fields = vec![
            ("'@type'", FmValue::Scalar("CreativeWork".to_string())),
            ("identifier", FmValue::Scalar(id.to_string())),
            ("title", FmValue::Scalar(summary.to_string())),
        ];
        if let Some(ts) = opt_str(&r, "timestamp") {
            fields.push(("created", FmValue::Scalar(ts)));
        }
        let tags = tags_list(&r);
        if !tags.is_empty() {
            fields.push(("subject", FmValue::List(tags)));
        }
        if let Some(rationale) = opt_str(&r, "rationale") {
            fields.push(("description", FmValue::Scalar(rationale)));
        }
        let content = format!(
            "{}# {}\n\n```json\n{}\n```\n",
            yaml_frontmatter(&fields),
            summary,
            serde_json::to_string_pretty(&r)?
        );
        fs::write(path.join("decisions").join(format!("{}.md", id)), content)?;
        decisions_count += 1;
    }
    counts.insert("decisions".to_string(), serde_json::json!(decisions_count));

    // Export Progress
    let mut stmt = conn.prepare(
        "SELECT id, timestamp, status, description, parent_id, commit_sha FROM progress_entries",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(serde_json::json!({
            "id": row.get::<_, i64>(0)?,
            "timestamp": row.get::<_, String>(1)?,
            "status": row.get::<_, String>(2)?,
            "description": row.get::<_, String>(3)?,
            "parent_id": row.get::<_, Option<i64>>(4)?,
            "commit_sha": row.get::<_, Option<String>>(5)?,
        }))
    })?;
    let mut progress_count = 0;
    for r in rows {
        let r = r?;
        let id = r.get("id").unwrap().as_i64().unwrap();
        let description = r.get("description").unwrap().as_str().unwrap();
        let status = r.get("status").and_then(|s| s.as_str()).unwrap_or("");
        let mut fields = vec![("'@type'", FmValue::Scalar("Action".to_string()))];
        if let Some(action_status) = action_status(status) {
            fields.push(("actionStatus", FmValue::Scalar(action_status.to_string())));
        }
        fields.push(("identifier", FmValue::Scalar(id.to_string())));
        fields.push(("title", FmValue::Scalar(description.to_string())));
        if let Some(ts) = opt_str(&r, "timestamp") {
            fields.push(("created", FmValue::Scalar(ts)));
        }
        fields.push(("description", FmValue::Scalar(description.to_string())));
        let content = format!(
            "{}# {}\n\n```json\n{}\n```\n",
            yaml_frontmatter(&fields),
            description,
            serde_json::to_string_pretty(&r)?
        );
        fs::write(path.join("progress").join(format!("{}.md", id)), content)?;
        progress_count += 1;
    }
    counts.insert("progress".to_string(), serde_json::json!(progress_count));

    // Export Patterns
    let mut stmt =
        conn.prepare("SELECT id, uuid, name, description, tags, timestamp, check_kind, check_expr, severity, importance, access_count, last_accessed_at, archived, confidence, last_confirmed_at FROM system_patterns")?;
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
            "check_kind": row.get::<_, Option<String>>(6)?,
            "check_expr": row.get::<_, Option<String>>(7)?,
            "severity": row.get::<_, String>(8)?,
            "importance": row.get::<_, i64>(9)?,
            "access_count": row.get::<_, i64>(10)?,
            "last_accessed_at": row.get::<_, Option<String>>(11)?,
            "archived": row.get::<_, i64>(12)?,
            "confidence": row.get::<_, f64>(13)?,
            "last_confirmed_at": row.get::<_, Option<String>>(14)?,
        }))
    })?;
    let mut patterns_count = 0;
    for r in rows {
        let r = r?;
        let id = r.get("id").unwrap().as_i64().unwrap();
        let name = r.get("name").unwrap().as_str().unwrap();
        let mut fields = vec![
            ("identifier", FmValue::Scalar(id.to_string())),
            ("title", FmValue::Scalar(name.to_string())),
        ];
        if let Some(ts) = opt_str(&r, "timestamp") {
            fields.push(("created", FmValue::Scalar(ts)));
        }
        let tags = tags_list(&r);
        if !tags.is_empty() {
            fields.push(("subject", FmValue::List(tags)));
        }
        if let Some(desc) = opt_str(&r, "description") {
            fields.push(("description", FmValue::Scalar(desc)));
        }
        let content = format!(
            "{}# {}\n\n```json\n{}\n```\n",
            yaml_frontmatter(&fields),
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
        let mut fields = vec![
            ("identifier", FmValue::Scalar(id.to_string())),
            ("title", FmValue::Scalar(format!("{}:{}", category, key))),
        ];
        if let Some(ts) = opt_str(&r, "timestamp") {
            fields.push(("created", FmValue::Scalar(ts)));
        }
        let content = format!(
            "{}# {}:{}\n\n```json\n{}\n```\n",
            yaml_frontmatter(&fields),
            category,
            key,
            serde_json::to_string_pretty(&r)?
        );
        fs::write(path.join("custom_data").join(format!("{}.md", id)), content)?;
        custom_count += 1;
    }
    counts.insert("custom_data".to_string(), serde_json::json!(custom_count));

    // Export Links (manual edges only; derived edges are regenerable).
    // Confirm-origin member_of edges land here too: confirm writes
    // origin='manual' (with provenance source='schema_confirm').
    let mut stmt = conn.prepare("SELECT id, source_item_type, source_item_id, target_item_type, target_item_id, relationship_type, description, timestamp, origin, source FROM context_links WHERE origin = 'manual'")?;
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
            "origin": row.get::<_, String>(8)?,
            "source": row.get::<_, Option<String>>(9)?,
        }))
    })?;
    let mut links_count = 0;
    for r in rows {
        let r = r?;
        let id = r.get("id").unwrap().as_i64().unwrap();
        let rel = r.get("relationship_type").unwrap().as_str().unwrap();
        let mut fields = vec![
            ("identifier", FmValue::Scalar(id.to_string())),
            ("title", FmValue::Scalar(rel.to_string())),
        ];
        if let Some(ts) = opt_str(&r, "timestamp") {
            fields.push(("created", FmValue::Scalar(ts)));
        }
        if let Some(desc) = opt_str(&r, "description") {
            fields.push(("description", FmValue::Scalar(desc)));
        }
        let content = format!(
            "{}# {}\n\n```json\n{}\n```\n",
            yaml_frontmatter(&fields),
            rel,
            serde_json::to_string_pretty(&r)?
        );
        fs::write(path.join("links").join(format!("{}.md", id)), content)?;
        links_count += 1;
    }
    counts.insert("links".to_string(), serde_json::json!(links_count));
    // Export Anchors
    let mut stmt =
        conn.prepare("SELECT id, item_type, item_id, path, timestamp FROM item_anchors")?;
    let rows = stmt.query_map([], |row| {
        Ok(serde_json::json!({
            "id": row.get::<_, i64>(0)?,
            "item_type": row.get::<_, String>(1)?,
            "item_id": row.get::<_, i64>(2)?,
            "path": row.get::<_, String>(3)?,
            "timestamp": row.get::<_, String>(4)?,
        }))
    })?;
    let mut anchors_count = 0;
    for r in rows {
        let r = r?;
        let id = r.get("id").unwrap().as_i64().unwrap();
        let item_type = r.get("item_type").unwrap().as_str().unwrap();
        let mut fields = vec![
            ("identifier", FmValue::Scalar(id.to_string())),
            ("title", FmValue::Scalar(item_type.to_string())),
        ];
        if let Some(ts) = opt_str(&r, "timestamp") {
            fields.push(("created", FmValue::Scalar(ts)));
        }
        let content = format!(
            "{}# {}\n\n```json\n{}\n```\n",
            yaml_frontmatter(&fields),
            item_type,
            serde_json::to_string_pretty(&r)?
        );
        fs::write(path.join("anchors").join(format!("{}.md", id)), content)?;
        anchors_count += 1;
    }
    counts.insert("anchors".to_string(), serde_json::json!(anchors_count));

    // Export Schemas (spec 0002, AC-11): identity data, not regenerable —
    // re-running detection on the target would mint a new uuid. Each file
    // carries the full row plus its member_of membership, fired
    // suggestion resolutions, and reward telemetry.
    // `schema_candidates` (re-staged by scan) is excluded.
    let mut stmt = conn.prepare(
        "SELECT id, uuid, name, summary, summary_source, status, centroid_json, \
         confidence, importance, access_count, last_accessed_at, last_confirmed_at, \
         created_at, updated_at FROM schemas ORDER BY id",
    )?;
    let schema_rows = stmt.query_map([], |row| {
        Ok(serde_json::json!({
            "id": row.get::<_, i64>(0)?,
            "uuid": row.get::<_, String>(1)?,
            "name": row.get::<_, String>(2)?,
            "summary": row.get::<_, String>(3)?,
            "summary_source": row.get::<_, String>(4)?,
            "status": row.get::<_, String>(5)?,
            "centroid_json": row.get::<_, String>(6)?,
            "confidence": row.get::<_, f64>(7)?,
            "importance": row.get::<_, f64>(8)?,
            "access_count": row.get::<_, i64>(9)?,
            "last_accessed_at": row.get::<_, Option<String>>(10)?,
            "last_confirmed_at": row.get::<_, Option<String>>(11)?,
            "created_at": row.get::<_, String>(12)?,
            "updated_at": row.get::<_, String>(13)?,
        }))
    })?;
    let mut schemas_count = 0;
    for row in schema_rows {
        let row = row?;
        let id = row.get("id").unwrap().as_i64().unwrap();
        // Membership travels with the schema: member_of edges pointing at it,
        // with the origin that wrote them and the provenance `source`
        // (confirm writes source='schema_confirm'). The generic links export
        // already carries these confirm edges — origin='manual' — so this
        // members array is a fallback; the importer's NOT EXISTS guard
        // against existing links is the actual dedup and is load-bearing.
        let mut members = Vec::new();
        let mut link_stmt = conn.prepare(
            "SELECT source_item_type, source_item_id, origin, source FROM context_links \
             WHERE relationship_type = 'member_of' AND target_item_type = 'schema' \
             AND target_item_id = ?1 ORDER BY source_item_type, \
             CAST(source_item_id AS INTEGER)",
        )?;
        let member_rows = link_stmt.query_map([id], |l| {
            Ok(serde_json::json!({
                "kind": l.get::<_, String>(0)?,
                "id": l.get::<_, String>(1)?,
                "origin": l.get::<_, String>(2)?,
                "source": l.get::<_, Option<String>>(3)?,
            }))
        })?;
        for m in member_rows {
            members.push(m?);
        }

        let mut suggestions = Vec::new();
        let mut sug_stmt = conn.prepare(
            "SELECT ts, item_kind, item_id, fit, status FROM schema_suggestions \
             WHERE schema_id = ?1 ORDER BY item_kind, item_id",
        )?;
        let sug_rows = sug_stmt.query_map([id], |s| {
            Ok(serde_json::json!({
                "ts": s.get::<_, String>(0)?,
                "item_kind": s.get::<_, String>(1)?,
                "item_id": s.get::<_, i64>(2)?,
                "fit": s.get::<_, f64>(3)?,
                "status": s.get::<_, String>(4)?,
            }))
        })?;
        for s in sug_rows {
            suggestions.push(s?);
        }

        let mut row = row;
        row["members"] = Value::Array(members);
        row["suggestions"] = Value::Array(suggestions);

        let name = row.get("name").unwrap().as_str().unwrap().to_string();
        let content = format!(
            "{}# {}\n\n```json\n{}\n```\n",
            {
                let mut fields = vec![
                    ("identifier", FmValue::Scalar(id.to_string())),
                    ("title", FmValue::Scalar(name.clone())),
                ];
                if let Some(ts) = opt_str(&row, "created_at") {
                    fields.push(("created", FmValue::Scalar(ts)));
                }
                yaml_frontmatter(&fields)
            },
            name,
            serde_json::to_string_pretty(&row)?
        );
        fs::write(path.join("schemas").join(format!("{id}.md")), content)?;
        schemas_count += 1;
    }
    counts.insert("schemas".to_string(), serde_json::json!(schemas_count));

    // Reward telemetry (AC-11 "telemetry intact"): the rolling-window
    // table is bounded, so the whole thing exports as one file.
    let mut stmt = conn.prepare(
        "SELECT ts, cmd, arg, node_kind, node_id FROM retrieval_surfaces ORDER BY rowid",
    )?;
    let surface_rows = stmt.query_map([], |r| {
        Ok(serde_json::json!({
            "ts": r.get::<_, String>(0)?,
            "cmd": r.get::<_, String>(1)?,
            "arg": r.get::<_, Option<String>>(2)?,
            "node_kind": r.get::<_, String>(3)?,
            "node_id": r.get::<_, i64>(4)?,
        }))
    })?;
    let mut surfaces: Vec<Value> = Vec::new();
    for r in surface_rows {
        surfaces.push(r?);
    }
    fs::write(
        path.join("schemas").join("retrieval_surfaces.json"),
        serde_json::to_string_pretty(&surfaces)?,
    )?;

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
