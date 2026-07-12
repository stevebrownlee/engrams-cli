use anyhow::Result;
use rusqlite::Connection;
use serde_json::Value;

pub fn handle(conn: &Connection, budget_opt: Option<usize>) -> Result<Value> {
    let product_context = crate::ops::report::query_context_doc(conn, "product_context")?;
    let active_context = crate::ops::report::query_context_doc(conn, "active_context")?;

    let mut decisions = crate::ops::report::query_decisions(conn, 10, true)?;
    let mut patterns = crate::ops::report::query_patterns(conn, 10)?;
    let mut progress = crate::ops::report::query_progress(conn, 10)?;

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

    let est =
        |val: &Value| -> usize { serde_json::to_string(val).map(|s| s.len() / 4).unwrap_or(0) };

    if let Some(budget) = budget_opt {
        while est(&build_payload(
            &product_context_val,
            &active_context_val,
            &decisions,
            &patterns,
            &progress,
            None,
        )) > budget
        {
            if !progress.is_empty() {
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
            None,
        );
        let m = est(&temp_payload);
        build_payload(
            &product_context_val,
            &active_context_val,
            &decisions,
            &patterns,
            &progress,
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
    if let Some(b) = budget_info {
        map.insert("budget".to_string(), b);
    }
    Value::Object(map)
}
