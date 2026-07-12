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

    let items: Vec<Value> =
        serde_json::from_str(&items_json).context("items must be a JSON array")?;

    let tx = conn.unchecked_transaction()?;
    let mut ids = Vec::new();
    let mut created = 0;

    for item in items {
        if let Value::Object(mut map) = item {
            match batch_type {
                BatchType::Decision => {
                    let summary = map
                        .remove("summary")
                        .and_then(|v| match v {
                            Value::String(s) => Some(s),
                            _ => None,
                        })
                        .context("missing summary")?;
                    let rationale = map.remove("rationale").and_then(|v| match v {
                        Value::String(s) => Some(s),
                        _ => None,
                    });
                    let details = map.remove("details").and_then(|v| match v {
                        Value::String(s) => Some(s),
                        _ => None,
                    });
                    let tags = map.remove("tags").and_then(|v| match v {
                        Value::Array(arr) => Some(arr),
                        _ => None,
                    });

                    let mut cmd_tags = Vec::new();
                    if let Some(t_arr) = tags {
                        for t in t_arr {
                            if let Value::String(s) = t {
                                cmd_tags.push(s);
                            }
                        }
                    }

                    // Call decision logic
                    let res = crate::ops::decision::handle(
                        &tx,
                        crate::cli::DecisionCmd::Log {
                            summary,
                            rationale,
                            details,
                            tags: cmd_tags,
                            force: true,
                        },
                    )?;
                    if let Value::Object(mut res_map) = res {
                        if let Some(id_val) = res_map.remove("id") {
                            ids.push(id_val);
                        }
                    }
                    created += 1;
                }
                BatchType::Progress => {
                    let status = map
                        .remove("status")
                        .and_then(|v| match v {
                            Value::String(s) => Some(s),
                            _ => None,
                        })
                        .context("missing status")?;
                    let description = map
                        .remove("description")
                        .and_then(|v| match v {
                            Value::String(s) => Some(s),
                            _ => None,
                        })
                        .context("missing description")?;
                    let parent_id = map.remove("parent_id").and_then(|v| v.as_i64());

                    let res = crate::ops::progress::handle(
                        &tx,
                        crate::cli::ProgressCmd::Log {
                            status,
                            description,
                            parent_id,
                            check_similar: false,
                        },
                    )?;
                    if let Value::Object(mut res_map) = res {
                        if let Some(id_val) = res_map.remove("id") {
                            ids.push(id_val);
                        }
                    }
                    created += 1;
                }
                BatchType::Pattern => {
                    let name = map
                        .remove("name")
                        .and_then(|v| match v {
                            Value::String(s) => Some(s),
                            _ => None,
                        })
                        .context("missing name")?;
                    let description = map.remove("description").and_then(|v| match v {
                        Value::String(s) => Some(s),
                        _ => None,
                    });
                    let tags = map.remove("tags").and_then(|v| match v {
                        Value::Array(arr) => Some(arr),
                        _ => None,
                    });

                    let mut cmd_tags = Vec::new();
                    if let Some(t_arr) = tags {
                        for t in t_arr {
                            if let Value::String(s) = t {
                                cmd_tags.push(s);
                            }
                        }
                    }

                    let res = crate::ops::pattern::handle(
                        &tx,
                        crate::cli::PatternCmd::Log {
                            name,
                            description,
                            tags: cmd_tags,
                        },
                    )?;
                    if let Value::Object(mut res_map) = res {
                        if let Some(id_val) = res_map.remove("id") {
                            ids.push(id_val);
                        }
                    }
                    created += 1;
                }
                BatchType::CustomData => {
                    let category = map
                        .remove("category")
                        .and_then(|v| match v {
                            Value::String(s) => Some(s),
                            _ => None,
                        })
                        .context("missing category")?;
                    let key = map
                        .remove("key")
                        .and_then(|v| match v {
                            Value::String(s) => Some(s),
                            _ => None,
                        })
                        .context("missing key")?;
                    let json = map
                        .remove("json")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let value = if json {
                        let val = map.remove("value").context("missing value")?;
                        serde_json::to_string(&val)?
                    } else {
                        map.remove("value")
                            .and_then(|v| match v {
                                Value::String(s) => Some(s),
                                _ => None,
                            })
                            .context("missing value (string)")?
                    };

                    let res = crate::ops::custom::handle(
                        &tx,
                        crate::cli::CustomCmd::Set {
                            category,
                            key,
                            value,
                            json,
                        },
                    )?;
                    if let Value::Object(mut res_map) = res {
                        if let Some(id_val) = res_map.remove("id") {
                            ids.push(id_val);
                        }
                    }
                    created += 1;
                }
            }
        }
    }

    tx.commit()?;

    Ok(serde_json::json!({
        "created": created,
        "ids": ids,
    }))
}
