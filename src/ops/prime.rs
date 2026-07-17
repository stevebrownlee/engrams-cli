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
    let active_context = crate::ops::report::query_context_doc(conn, "active_context")?;

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
        let mut sql = "SELECT id, uuid, summary, rationale, implementation_details, tags, timestamp, status, commit_sha FROM decisions WHERE status = 'active'".to_string();
        let mut params_vec = Vec::<&dyn rusqlite::ToSql>::new();

        if !paths.is_empty() {
            let placeholders = decision_ids
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(",");
            sql.push_str(&format!(" AND id IN ({})", placeholders));
        }

        if !tags.is_empty() {
            let placeholders = tags.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            sql.push_str(&format!(" AND EXISTS (SELECT 1 FROM json_each(decisions.tags) WHERE json_each.value IN ({}))", placeholders));
        }

        sql.push_str(" ORDER BY id DESC LIMIT ?");

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
            let tags_str: Option<String> = row.get(5)?;
            let tags = match tags_str {
                Some(s) => serde_json::from_str(&s).unwrap_or(Value::Null),
                None => Value::Null,
            };
            Ok(crate::models::Decision {
                id: row.get(0)?,
                uuid: row.get(1)?,
                summary: row.get(2)?,
                rationale: row.get(3)?,
                implementation_details: row.get(4)?,
                tags: if tags.is_null() { None } else { Some(tags) },
                timestamp: row.get(6)?,
                status: row.get(7)?,
                commit_sha: row.get(8)?,
                pr_urls: Vec::new(),
                anchors: Vec::new(),
            })
        })?;

        for r in rows {
            decisions.push(r?);
        }
    }

    let mut patterns = Vec::new();
    let skip_patterns_query = !paths.is_empty() && pattern_ids.is_empty();
    if !skip_patterns_query {
        let mut sql =
            "SELECT id, uuid, name, description, tags, timestamp FROM system_patterns WHERE 1=1"
                .to_string();
        let mut params_vec = Vec::<&dyn rusqlite::ToSql>::new();

        if !paths.is_empty() {
            let placeholders = pattern_ids
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(",");
            sql.push_str(&format!(" AND id IN ({})", placeholders));
        }

        if !tags.is_empty() {
            let placeholders = tags.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            sql.push_str(&format!(" AND EXISTS (SELECT 1 FROM json_each(system_patterns.tags) WHERE json_each.value IN ({}))", placeholders));
        }

        sql.push_str(" ORDER BY id DESC LIMIT ?");

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
                pr_urls: Vec::new(),
                anchors: Vec::new(),
            })
        })?;

        for r in rows {
            patterns.push(r?);
        }
    }

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
    let active_context_val = serde_json::to_value(active_context)?;

    // Compact graph summary; tiny, so include whenever the payload is built.
    // Under an explicit --budget it is the first section dropped.
    let mut graph_val = Some(crate::ops::graph::model::summary(conn)?);

    let est =
        |val: &Value| -> usize { serde_json::to_string(val).map(|s| s.len() / 4).unwrap_or(0) };

    if let Some(budget) = budget_opt {
        while est(&build_payload(
            &product_context_val,
            &active_context_val,
            &decisions,
            &patterns,
            &progress,
            graph_val.as_ref(),
            None,
        )) > budget
        {
            if graph_val.is_some() {
                graph_val = None;
            } else if !progress.is_empty() {
                progress.pop();
            } else if !patterns.is_empty() {
                patterns.pop();
            } else if !decisions.is_empty() {
                decisions.pop();
            } else if !product_context_val.is_null() {
                product_context_val = Value::Null;
            } else {
                break;
            }
        }
    }

    let payload = if let Some(n) = budget_opt {
        let temp_payload = build_payload(
            &product_context_val,
            &active_context_val,
            &decisions,
            &patterns,
            &progress,
            graph_val.as_ref(),
            None,
        );
        let m = est(&temp_payload);
        build_payload(
            &product_context_val,
            &active_context_val,
            &decisions,
            &patterns,
            &progress,
            graph_val.as_ref(),
            Some(serde_json::json!({
                "limit": n,
                "estimated_tokens": m
            })),
        )
    } else {
        build_payload(
            &product_context_val,
            &active_context_val,
            &decisions,
            &patterns,
            &progress,
            graph_val.as_ref(),
            None,
        )
    };

    Ok(payload)
}

fn build_payload(
    product_context: &Value,
    active_context: &Value,
    decisions: &[crate::models::Decision],
    patterns: &[crate::models::Pattern],
    progress: &[crate::models::Progress],
    graph: Option<&Value>,
    budget_info: Option<Value>,
) -> Value {
    let mut map = serde_json::Map::new();
    map.insert("product_context".to_string(), product_context.clone());
    map.insert("active_context".to_string(), active_context.clone());
    map.insert(
        "decisions".to_string(),
        serde_json::to_value(decisions).unwrap_or(Value::Null),
    );
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
