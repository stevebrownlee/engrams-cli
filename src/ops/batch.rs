use anyhow::{Context, Result};
use rusqlite::Connection;
use serde_json::Value;
use std::io::Read;

use crate::cli::BatchType;

pub fn handle(conn: &Connection, batch_type: BatchType, items_arg: String) -> Result<Value> {
    let items_json = if items_arg == "-" {
        let mut buffer = String::new();
        std::io::stdin().read_to_string(&mut buffer)?;
        buffer
    } else {
        items_arg
    };

    let items: Vec<Value> = serde_json::from_str(&items_json).context("items must be a JSON array")?;

    let tx = conn.unchecked_transaction()?;
    let mut ids = Vec::new();
    let mut created = 0;

    for item in items {
        match batch_type {
            BatchType::Decision => {
                let summary = item.get("summary").and_then(|v| v.as_str()).context("missing summary")?;
                let rationale = item.get("rationale").and_then(|v| v.as_str());
                let details = item.get("details").and_then(|v| v.as_str());
                let tags = item.get("tags").and_then(|v| v.as_array());

                let mut cmd_tags = Vec::new();
                if let Some(t_arr) = tags {
                    for t in t_arr {
                        if let Some(t_str) = t.as_str() {
                            cmd_tags.push(t_str.to_string());
                        }
                    }
                }

                // Call decision logic
                let res = crate::ops::decision::handle(&tx, crate::cli::DecisionCmd::Log {
                    summary: summary.to_string(),
                    rationale: rationale.map(|s| s.to_string()),
                    details: details.map(|s| s.to_string()),
                    tags: cmd_tags,
                })?;
                ids.push(res.get("id").unwrap().clone());
                created += 1;
            }
            BatchType::Progress => {
                let status = item.get("status").and_then(|v| v.as_str()).context("missing status")?;
                let description = item.get("description").and_then(|v| v.as_str()).context("missing description")?;
                let parent_id = item.get("parent_id").and_then(|v| v.as_i64());

                let res = crate::ops::progress::handle(&tx, crate::cli::ProgressCmd::Log {
                    status: status.to_string(),
                    description: description.to_string(),
                    parent_id,
                })?;
                ids.push(res.get("id").unwrap().clone());
                created += 1;
            }
            BatchType::Pattern => {
                let name = item.get("name").and_then(|v| v.as_str()).context("missing name")?;
                let description = item.get("description").and_then(|v| v.as_str());
                let tags = item.get("tags").and_then(|v| v.as_array());

                let mut cmd_tags = Vec::new();
                if let Some(t_arr) = tags {
                    for t in t_arr {
                        if let Some(t_str) = t.as_str() {
                            cmd_tags.push(t_str.to_string());
                        }
                    }
                }

                let res = crate::ops::pattern::handle(&tx, crate::cli::PatternCmd::Log {
                    name: name.to_string(),
                    description: description.map(|s| s.to_string()),
                    tags: cmd_tags,
                })?;
                ids.push(res.get("id").unwrap().clone());
                created += 1;
            }
            BatchType::CustomData => {
                let category = item.get("category").and_then(|v| v.as_str()).context("missing category")?;
                let key = item.get("key").and_then(|v| v.as_str()).context("missing key")?;
                let json = item.get("json").and_then(|v| v.as_bool()).unwrap_or(false);
                let value = if json {
                    serde_json::to_string(item.get("value").context("missing value")?)?
                } else {
                    item.get("value").and_then(|v| v.as_str()).context("missing value (string)")?.to_string()
                };

                let res = crate::ops::custom::handle(&tx, crate::cli::CustomCmd::Set {
                    category: category.to_string(),
                    key: key.to_string(),
                    value,
                    json,
                })?;
                ids.push(res.get("id").unwrap().clone());
                created += 1;
            }
        }
    }

    tx.commit()?;

    Ok(serde_json::json!({
        "created": created,
        "ids": ids,
    }))
}
