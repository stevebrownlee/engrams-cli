use anyhow::Result;
use serde_json::Value;

use crate::cli::Format;

pub fn emit(format: Format, val: Value) -> Result<()> {
    match format {
        Format::Json => {
            println!("{}", serde_json::to_string_pretty(&val)?);
        }
        Format::Human => {
            if val.is_null() {
                return Ok(());
            }
            if let Some(created) = val.get("created").and_then(|v| v.as_bool()) {
                if let Some(path) = val.get("db_path").and_then(|v| v.as_str()) {
                    if val.as_object().is_some_and(|o| o.len() == 2) {
                        if created {
                            println!("Initialized engrams DB at {}", path);
                        } else {
                            println!("DB already initialized at {}", path);
                        }
                        return Ok(());
                    }
                }
            }
            print_human(&val);
        }
    }
    Ok(())
}

fn print_human(val: &Value) {
    if let Some(arr) = val.as_array() {
        if arr.is_empty() {
            println!("(empty)");
        } else {
            for (i, item) in arr.iter().enumerate() {
                if i > 0 {
                    println!("---");
                }
                print_object_human(item, 0);
            }
        }
    } else {
        print_object_human(val, 0);
    }
}

fn print_object_human(val: &Value, indent: usize) {
    match val {
        Value::Object(obj) => {
            for (k, v) in obj {
                if v.is_object() || v.is_array() {
                    println!("{:indent$}{}:", "", k, indent = indent);
                    print_object_human(v, indent + 2);
                } else {
                    let v_str = match v {
                        Value::String(s) => s.to_string(),
                        Value::Null => "(null)".to_string(),
                        _ => v.to_string(),
                    };
                    println!("{:indent$}{}: {}", "", k, v_str, indent = indent);
                }
            }
        }
        Value::Array(arr) => {
            for item in arr {
                if item.is_object() || item.is_array() {
                    print_object_human(item, indent + 2);
                } else {
                    let v_str = match item {
                        Value::String(s) => s.to_string(),
                        Value::Null => "(null)".to_string(),
                        _ => item.to_string(),
                    };
                    println!("{:indent$}- {}", "", v_str, indent = indent);
                }
            }
        }
        _ => {
            println!("{:indent$}{}", "", val, indent = indent);
        }
    }
}
