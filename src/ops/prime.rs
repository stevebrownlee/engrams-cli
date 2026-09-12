use anyhow::Result;
use rusqlite::Connection;
use serde_json::Value;

pub fn handle(
    conn: &Connection,
    budget_opt: Option<usize>,
    paths: Vec<String>,
    tags: Vec<String>,
) -> Result<Value> {
    let product_context = crate::ops::report::query_context_doc(conn, "product_context")?;
    let active_context_val = load_active_context(conn, &paths, &tags)?;
    let schema_block = crate::ops::schemas::retrieval::prime_block(
        conn,
        crate::ops::schemas::retrieval::PRIME_SCHEMA_K,
    )?;

    let is_scoped = !paths.is_empty() || !tags.is_empty();
    let limit = if is_scoped { 50 } else { 10 };
    let limit_i64 = limit as i64;

    let mut decision_ids = Vec::new();
    let mut pattern_ids = Vec::new();
    if !paths.is_empty() {
        let cleaned_paths: Vec<String> = paths
            .iter()
            .map(|p| crate::ops::anchor::clean_path(p))
            .collect();
        let matched = crate::ops::anchor::query_relevant_ids(conn, &cleaned_paths)?;
        for (itype, id) in matched {
            if itype == "decision" {
                decision_ids.push(id);
            } else if itype == "system_pattern" {
                pattern_ids.push(id);
            }
        }
    }

    let mut decisions = Vec::new();
    let skip_decisions_query = !paths.is_empty() && decision_ids.is_empty();
    if !skip_decisions_query {
        let score = crate::ops::scoring::score_expr("timestamp", "importance");
        let mut sql = format!(
            "SELECT {}, {score} AS score FROM decisions WHERE status = 'active' AND archived = 0",
            crate::models::DECISION_COLS
        );
        let mut params_vec = Vec::<&dyn rusqlite::ToSql>::new();

        if !paths.is_empty() {
            let placeholders = crate::ops::sql_placeholders(decision_ids.len());
            sql.push_str(&format!(" AND id IN ({})", placeholders));
        }

        if !tags.is_empty() {
            let placeholders = crate::ops::sql_placeholders(tags.len());
            sql.push_str(&format!(" AND EXISTS (SELECT 1 FROM json_each(decisions.tags) WHERE json_each.value IN ({}))", placeholders));
        }

        sql.push_str(" ORDER BY score DESC, id DESC LIMIT ?");

        let mut stmt = conn.prepare(&sql)?;

        if !paths.is_empty() {
            for id in &decision_ids {
                params_vec.push(id);
            }
        }
        if !tags.is_empty() {
            for tag in &tags {
                params_vec.push(tag);
            }
        }
        params_vec.push(&limit_i64);

        let rows = stmt.query_map(rusqlite::params_from_iter(params_vec), |row| {
            let tags_str: Option<String> = row.get("tags")?;
            let tags = match tags_str {
                Some(s) => serde_json::from_str(&s).unwrap_or(Value::Null),
                None => Value::Null,
            };
            Ok(crate::models::Decision {
                id: row.get("id")?,
                uuid: row.get("uuid")?,
                summary: row.get("summary")?,
                rationale: row.get("rationale")?,
                implementation_details: row.get("implementation_details")?,
                tags: if tags.is_null() { None } else { Some(tags) },
                timestamp: row.get("timestamp")?,
                status: row.get("status")?,
                commit_sha: row.get("commit_sha")?,
                pr_urls: Vec::new(),
                anchors: Vec::new(),
                importance: row.get("importance")?,
                access_count: row.get("access_count")?,
                last_accessed_at: row.get("last_accessed_at")?,
                archived: row.get("archived")?,
                contract: row.get("contract")?,
                score: Some(row.get("score")?),
            })
        })?;

        for r in rows {
            decisions.push(r?);
        }
    }

    // Decisions reachable via a supersedes chain from an active decision are
    // demoted: fetched even when no longer status='active', annotated with
    // "superseded_by" (their direct superseder), and ordered after the active
    // ones — never hidden.
    let mut superseded_by: std::collections::HashMap<i64, i64> = std::collections::HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT source_item_id, target_item_id FROM context_links \
             WHERE source_item_type = 'decision' AND target_item_type = 'decision' \
             AND relationship_type = 'supersedes'",
        )?;
        let edges: Vec<(String, String)> = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        if !edges.is_empty() && !decisions.is_empty() {
            let graph = crate::ops::graph::model::load(conn)?;
            let mut chain_targets: std::collections::HashSet<i64> =
                std::collections::HashSet::new();
            for d in &decisions {
                if d.status == "active" {
                    for ((_, tgt_id), _) in graph.transitive_reachable(
                        &("decision".to_string(), d.id.to_string()),
                        "supersedes",
                    ) {
                        if let Ok(id) = tgt_id.parse::<i64>() {
                            chain_targets.insert(id);
                        }
                    }
                }
            }
            for (src, tgt) in &edges {
                if let (Ok(t), Ok(s)) = (tgt.parse::<i64>(), src.parse::<i64>()) {
                    if chain_targets.contains(&t) {
                        superseded_by.entry(t).or_insert(s);
                    }
                }
            }
            // Fetch demoted decisions not already listed (status filter above
            // only returns active ones).
            let have: std::collections::HashSet<i64> = decisions.iter().map(|d| d.id).collect();
            let missing: Vec<i64> = chain_targets
                .iter()
                .filter(|id| !have.contains(id))
                .cloned()
                .collect();
            if !missing.is_empty() {
                let placeholders = crate::ops::sql_placeholders(missing.len());
                let sql = format!(
                    "SELECT {} FROM decisions WHERE id IN ({}) ORDER BY id DESC",
                    crate::models::DECISION_COLS,
                    placeholders
                );
                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map(rusqlite::params_from_iter(missing.iter()), |row| {
                    let tags_str: Option<String> = row.get("tags")?;
                    let tags = match tags_str {
                        Some(s) => serde_json::from_str(&s).unwrap_or(Value::Null),
                        None => Value::Null,
                    };
                    Ok(crate::models::Decision {
                        id: row.get("id")?,
                        uuid: row.get("uuid")?,
                        summary: row.get("summary")?,
                        rationale: row.get("rationale")?,
                        implementation_details: row.get("implementation_details")?,
                        tags: if tags.is_null() { None } else { Some(tags) },
                        timestamp: row.get("timestamp")?,
                        status: row.get("status")?,
                        commit_sha: row.get("commit_sha")?,
                        pr_urls: Vec::new(),
                        anchors: Vec::new(),
                        importance: row.get("importance")?,
                        access_count: row.get("access_count")?,
                        last_accessed_at: row.get("last_accessed_at")?,
                        archived: row.get("archived")?,
                        contract: row.get("contract")?,
                        score: None,
                    })
                })?;
                for r in rows {
                    decisions.push(r?);
                }
            }
            // Order: non-demoted (active) first, demoted last.
            let (mut active, demoted): (Vec<_>, Vec<_>) = decisions
                .into_iter()
                .partition(|d| !superseded_by.contains_key(&d.id));
            active.extend(demoted);
            decisions = active;
        }
    }

    let mut patterns = Vec::new();
    let skip_patterns_query = !paths.is_empty() && pattern_ids.is_empty();
    if !skip_patterns_query {
        let pat_score = format!(
            "({} * {})",
            crate::ops::scoring::score_expr("timestamp", "importance"),
            crate::ops::scoring::confidence_expr("confidence", "last_confirmed_at", "timestamp")
        );
        let mut sql =
            format!("SELECT id, uuid, name, description, tags, timestamp, check_kind, check_expr, severity, importance, access_count, last_accessed_at, archived, confidence, last_confirmed_at, {pat_score} AS score FROM system_patterns WHERE archived = 0");
        let mut params_vec = Vec::<&dyn rusqlite::ToSql>::new();

        if !paths.is_empty() {
            let placeholders = crate::ops::sql_placeholders(pattern_ids.len());
            sql.push_str(&format!(" AND id IN ({})", placeholders));
        }

        if !tags.is_empty() {
            let placeholders = crate::ops::sql_placeholders(tags.len());
            sql.push_str(&format!(" AND EXISTS (SELECT 1 FROM json_each(system_patterns.tags) WHERE json_each.value IN ({}))", placeholders));
        }

        sql.push_str(" ORDER BY score DESC, id DESC LIMIT ?");

        let mut stmt = conn.prepare(&sql)?;

        if !paths.is_empty() {
            for id in &pattern_ids {
                params_vec.push(id);
            }
        }
        if !tags.is_empty() {
            for tag in &tags {
                params_vec.push(tag);
            }
        }
        params_vec.push(&limit_i64);

        let rows = stmt.query_map(rusqlite::params_from_iter(params_vec), |row| {
            let tags_str: Option<String> = row.get(4)?;
            let tags = match tags_str {
                Some(s) => serde_json::from_str(&s).unwrap_or(Value::Null),
                None => Value::Null,
            };
            Ok(crate::models::Pattern {
                id: row.get(0)?,
                uuid: row.get(1)?,
                name: row.get(2)?,
                description: row.get(3)?,
                tags: if tags.is_null() { None } else { Some(tags) },
                timestamp: row.get(5)?,
                check_kind: row.get(6)?,
                check_expr: row.get(7)?,
                severity: row.get(8)?,
                pr_urls: Vec::new(),
                anchors: Vec::new(),
                importance: row.get(9)?,
                access_count: row.get(10)?,
                last_accessed_at: row.get(11)?,
                archived: row.get(12)?,
                confidence: row.get(13)?,
                last_confirmed_at: row.get(14)?,
                effective_confidence: crate::ops::scoring::effective_confidence(
                    row.get::<_, f64>(13)?,
                    row.get::<_, Option<String>>(14)?.as_deref(),
                    &row.get::<_, String>(5)?,
                ),
                score: Some(row.get(15)?),
            })
        })?;

        for r in rows {
            patterns.push(r?);
        }
    }
    // Reinforce-on-read (v0.10.0): bump access_count / last_accessed_at for surfaced records.
    let decision_ids: Vec<i64> = decisions.iter().map(|d| d.id).collect();
    crate::ops::scoring::reinforce(conn, "decisions", &decision_ids)?;
    let pat_ids: Vec<i64> = patterns.iter().map(|p| p.id).collect();
    crate::ops::scoring::reinforce(conn, "system_patterns", &pat_ids)?;

    let mut progress = if is_scoped {
        Vec::new()
    } else {
        crate::ops::report::query_progress(conn, 10)?
    };

    let prs_map = crate::ops::pr::pr_urls_map(conn, "decision")?;
    let anchors_map = crate::ops::anchor::anchors_map(conn, "decision")?;
    for d in &mut decisions {
        if let Some(urls) = prs_map.get(&d.id) {
            d.pr_urls = urls.clone();
        }
        if let Some(paths) = anchors_map.get(&d.id) {
            d.anchors = paths.clone();
        }
    }

    let pat_prs_map = crate::ops::pr::pr_urls_map(conn, "system_pattern")?;
    let pat_anchors_map = crate::ops::anchor::anchors_map(conn, "system_pattern")?;
    for p in &mut patterns {
        if let Some(urls) = pat_prs_map.get(&p.id) {
            p.pr_urls = urls.clone();
        }
        if let Some(paths) = pat_anchors_map.get(&p.id) {
            p.anchors = paths.clone();
        }
    }

    let mut product_context_val = serde_json::to_value(product_context)?;

    // Compact graph summary; tiny, so include whenever the payload is built.
    // Under an explicit --budget it is the first section dropped.
    let mut graph_val = Some(crate::ops::graph::model::summary(conn)?);

    // Token estimate: bytes/4 of the serialized JSON. Generic so any
    // serializable section (Value or a slice of Serialize items) is measured
    // in one pass, with no intermediate Value round-trip.
    fn tok_cost<T: serde::Serialize>(v: &T) -> usize {
        serde_json::to_string(v).map(|s| s.len() / 4).unwrap_or(0)
    }

    if let Some(budget) = budget_opt {
        // Anchor the total with a single full serialization, then subtract
        // each dropped section/item's marginal cost. Only the dropped item is
        // re-serialized per iteration — never the whole nested payload.
        let mut total = tok_cost(&build_payload(PayloadParts {
            product_context: &product_context_val,
            active_context: &active_context_val,
            decisions: &decisions,
            patterns: &patterns,
            progress: &progress,
            graph: graph_val.as_ref(),
            budget_info: None,
            superseded_by: &superseded_by,
            schemas: &schema_block,
        }));
        while total > budget {
            if graph_val.is_some() {
                total = total.saturating_sub(tok_cost(graph_val.as_ref().unwrap()));
                graph_val = None;
            } else if let Some(dropped) = progress.pop() {
                total = total.saturating_sub(tok_cost(&dropped));
            } else if let Some(dropped) = patterns.pop() {
                total = total.saturating_sub(tok_cost(&dropped));
            } else if let Some(dropped) = decisions.pop() {
                // Match build_payload: a superseded decision carries an extra
                // "superseded_by" field in the emitted JSON.
                let mut dv = serde_json::to_value(&dropped).unwrap_or(Value::Null);
                if let Some(by) = superseded_by.get(&dropped.id) {
                    dv["superseded_by"] = serde_json::json!(by);
                }
                total = total.saturating_sub(tok_cost(&dv));
            } else if !product_context_val.is_null() {
                total = total.saturating_sub(tok_cost(&product_context_val));
                product_context_val = Value::Null;
            } else {
                break;
            }
        }
    }

    let payload = if let Some(n) = budget_opt {
        let temp_payload = build_payload(PayloadParts {
            product_context: &product_context_val,
            active_context: &active_context_val,
            decisions: &decisions,
            patterns: &patterns,
            progress: &progress,
            graph: graph_val.as_ref(),
            budget_info: None,
            superseded_by: &superseded_by,
            schemas: &schema_block,
        });
        let m = tok_cost(&temp_payload);
        build_payload(PayloadParts {
            product_context: &product_context_val,
            active_context: &active_context_val,
            decisions: &decisions,
            patterns: &patterns,
            progress: &progress,
            graph: graph_val.as_ref(),
            budget_info: Some(serde_json::json!({
                "limit": n,
                "estimated_tokens": m
            })),
            superseded_by: &superseded_by,
            schemas: &schema_block,
        })
    } else {
        build_payload(PayloadParts {
            product_context: &product_context_val,
            active_context: &active_context_val,
            decisions: &decisions,
            patterns: &patterns,
            progress: &progress,
            graph: graph_val.as_ref(),
            budget_info: None,
            superseded_by: &superseded_by,
            schemas: &schema_block,
        })
    };

    // Surfacing telemetry (AC-10): prime surfaces schemas alongside the
    // regular payload — one event, co-surfaced set bounded by the cap.
    let schema_ids: Vec<i64> = schema_block
        .iter()
        .filter_map(|s| s["id"].as_i64())
        .collect();
    if !schema_ids.is_empty() {
        let co: Vec<(&str, i64)> = decisions
            .iter()
            .map(|d| ("decision", d.id))
            .chain(patterns.iter().map(|p| ("system_pattern", p.id)))
            .chain(progress.iter().map(|p| ("progress_entry", p.id)))
            .collect();
        crate::ops::schemas::retrieval::record_surface(conn, "prime", None, &schema_ids, &co)?;
    }

    Ok(payload)
}

/// Load all active-context tracks. The payload lists every track with a
/// one-line focus; only the scope-matching track (or 'default') is expanded.
fn load_active_context(conn: &Connection, paths: &[String], tags: &[String]) -> Result<Value> {
    let mut stmt = conn
        .prepare("SELECT name, content, version, updated_at FROM active_contexts ORDER BY name")?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;

    let mut tracks = Vec::new();
    for r in rows {
        let (name, content_str, version, updated_at) = r?;
        let content: Value = serde_json::from_str(&content_str).unwrap_or(Value::Null);
        tracks.push((name, content, version, updated_at));
    }

    // Pick the track to expand: a track named by a scope tag, else a track
    // whose name appears in a scoped path, else 'default'.
    let selected = tags
        .iter()
        .find_map(|t| {
            tracks
                .iter()
                .find(|(n, ..)| n == t)
                .map(|(n, ..)| n.clone())
        })
        .or_else(|| {
            paths.iter().find_map(|p| {
                tracks
                    .iter()
                    .find(|(n, ..)| n != "default" && p.contains(n.as_str()))
                    .map(|(n, ..)| n.clone())
            })
        })
        .unwrap_or_else(|| "default".to_string());

    let mut track_list = Vec::with_capacity(tracks.len());
    let mut expanded = Value::Null;
    for (name, content, version, updated_at) in &tracks {
        track_list.push(serde_json::json!({
            "name": name,
            "focus": one_line_focus(content),
            "selected": *name == selected,
        }));
        if *name == selected {
            expanded = serde_json::json!({
                "name": name,
                "content": content,
                "version": version,
                "updated_at": updated_at,
            });
        }
    }

    Ok(serde_json::json!({
        "tracks": track_list,
        "selected": selected,
        "expanded": expanded,
    }))
}

/// One-line focus for a track: its `focus`/`current_focus` field if present,
/// else the first 120 chars of the serialized content.
fn one_line_focus(content: &Value) -> String {
    if let Some(f) = content
        .get("focus")
        .or_else(|| content.get("current_focus"))
        .and_then(|v| v.as_str())
    {
        return f.lines().next().unwrap_or("").chars().take(120).collect();
    }
    match content {
        Value::Null => String::new(),
        other => other
            .to_string()
            .lines()
            .next()
            .unwrap_or("")
            .chars()
            .take(120)
            .collect(),
    }
}

struct PayloadParts<'a> {
    product_context: &'a Value,
    active_context: &'a Value,
    decisions: &'a [crate::models::Decision],
    patterns: &'a [crate::models::Pattern],
    progress: &'a [crate::models::Progress],
    graph: Option<&'a Value>,
    budget_info: Option<Value>,
    superseded_by: &'a std::collections::HashMap<i64, i64>,
    schemas: &'a [Value],
}

fn build_payload(parts: PayloadParts) -> Value {
    let PayloadParts {
        product_context,
        active_context,
        decisions,
        patterns,
        progress,
        graph,
        budget_info,
        superseded_by,
        schemas,
    } = parts;
    let mut map = serde_json::Map::new();
    // The schemas block leads: concepts prime individual facts (spec 0002).
    map.insert("schemas".to_string(), Value::Array(schemas.to_vec()));
    map.insert("product_context".to_string(), product_context.clone());
    map.insert("active_context".to_string(), active_context.clone());
    let decisions_json: Vec<Value> = decisions
        .iter()
        .map(|d| {
            let mut v = serde_json::to_value(d).unwrap_or(Value::Null);
            if let Some(by) = superseded_by.get(&d.id) {
                v["superseded_by"] = serde_json::json!(by);
            }
            v
        })
        .collect();
    map.insert("decisions".to_string(), Value::Array(decisions_json));
    map.insert(
        "patterns".to_string(),
        serde_json::to_value(patterns).unwrap_or(Value::Null),
    );
    map.insert(
        "progress".to_string(),
        serde_json::to_value(progress).unwrap_or(Value::Null),
    );
    if let Some(g) = graph {
        map.insert("graph".to_string(), g.clone());
    }
    if let Some(b) = budget_info {
        map.insert("budget".to_string(), b);
    }
    Value::Object(map)
}
