mod cli;
mod db;
mod models;
mod ops;
mod release;
mod schema;

use anyhow::Result;
use clap::Parser;
use std::io::Write;

fn main() {
    if let Err(e) = run() {
        eprintln!(r#"{{"error": "{}"}}"#, e.to_string().replace("\"", "\\\""));
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = cli::Cli::parse();

    if matches!(cli.command, cli::Command::Instructions) {
        print!("{}", include_str!("assets/instructions.md"));
        return Ok(());
    }

    let ws = db::resolve_workspace(cli.db.as_deref(), cli.workspace.as_deref())?;

    let db_existed = ws.db_path.exists();
    let is_init = matches!(cli.command, cli::Command::Init);
    let has_explicit_db =
        cli.db.is_some() || std::env::var("ENGRAMS_DB").is_ok_and(|p| !p.is_empty());

    // Fail closed before `db::open` (creating no directories or files) when a
    // non-`init` workspace/CWD command targets a path with no existing database
    // (Decision 538 / Idea 5).
    if !db_existed && !is_init {
        if let Some(root) = ws.root.as_deref() {
            if let Some(parent_store) = db::find_parent_store_root(root) {
                anyhow::bail!(
                    "no engrams database at {} (found parent workspace database at {}); pass --workspace {} or run 'engrams init' to create a separate store",
                    ws.db_path.display(),
                    parent_store.join("engrams").join("context.db").display(),
                    parent_store.display()
                );
            }
        }
        if !has_explicit_db {
            anyhow::bail!(
                "no engrams database at {}; run 'engrams init' to initialize this workspace or pass --workspace <dir>",
                ws.db_path.display()
            );
        }
    }

    // We open (and create) the db for all commands.
    let mut conn = db::open(&ws.db_path)?;
    // Instantiate UpdateChecker (spawns background thread if check is needed)
    let checker = release::UpdateChecker::new(&ws.db_path);

    let is_migrate_or_init = matches!(cli.command, cli::Command::Migrate | cli::Command::Init);
    let is_check = matches!(cli.command, cli::Command::Check { .. });
    let is_single_item_get = matches!(
        &cli.command,
        cli::Command::Decision {
            cmd: cli::DecisionCmd::Get { .. }
        } | cli::Command::Pattern {
            cmd: cli::PatternCmd::Get { .. }
        } | cli::Command::Custom {
            cmd: cli::CustomCmd::Get { .. }
        } | cli::Command::Schema {
            cmd: cli::SchemaCmd::Show { .. }
        }
    );
    db::validate_version(&conn, is_migrate_or_init)?;

    let mut result = ops::dispatch(&mut conn, cli.command, &ws, !db_existed)?;

    if !cli.fields.is_empty() {
        project_fields(&mut result, &cli.fields);
    }

    let mut out = std::io::stdout().lock();
    if cli.compact {
        strip_nulls(&mut result);
        if !is_single_item_get {
            clamp_compact_prose(&mut result);
        }
        write_line_safe_json(&mut out, &result, 0)?;
    } else {
        serde_json::to_writer_pretty(&mut out, &result)?;
    }
    out.write_all(b"\n")?;
    if is_check {
        if let Some(arr) = result.get("violations").and_then(|v| v.as_array()) {
            if !arr.is_empty() {
                // Exit 1 *after* printing the violations JSON so CI catches it
                // without an error-shape output obscuring the findings.
                checker.print_notification();
                std::process::exit(1);
            }
        }
    }
    // Print notification if a new version is available
    checker.print_notification();

    Ok(())
}

/// Canonical-name fallbacks: request `summary`/`name`, get a hit's `title` value.
const FIELD_ALIASES: &[(&str, &str)] = &[("summary", "title"), ("name", "title")];

fn project_fields(val: &mut serde_json::Value, fields: &[String]) {
    match val {
        serde_json::Value::Object(map) => {
            // Miss-guidance (query's empty-result advice) survives projection:
            // an agent requesting narrow fields still needs re-targeting help.
            let guidance = map.remove("miss_guidance");
            let mut new_map = serde_json::Map::new();
            for f in fields {
                if let Some(v) = map.remove(f) {
                    new_map.insert(f.clone(), v);
                } else if let Some((_, src)) = FIELD_ALIASES.iter().find(|(alias, _)| alias == f) {
                    if let Some(v) = map.remove(*src) {
                        new_map.insert(f.clone(), v);
                    }
                }
            }
            if let Some(g) = guidance {
                new_map.insert("miss_guidance".to_string(), g);
            }
            *map = new_map;
        }
        serde_json::Value::Array(arr) => {
            for item in arr {
                project_fields(item, fields);
            }
        }
        _ => {}
    }
}

fn strip_nulls(val: &mut serde_json::Value) {
    match val {
        serde_json::Value::Object(map) => {
            map.retain(|k, v| {
                if v.is_null() {
                    return false;
                }
                if let Some(arr) = v.as_array() {
                    if arr.is_empty() && matches!(k.as_str(), "pr_urls" | "anchors" | "overrides") {
                        return false;
                    }
                }
                true
            });
            for v in map.values_mut() {
                strip_nulls(v);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr {
                strip_nulls(v);
            }
        }
        _ => {}
    }
}

const COMPACT_LINE_MAX: usize = 500;
const COMPACT_PROSE_MAX: usize = 320;

fn clamp_compact_prose(val: &mut serde_json::Value) {
    match val {
        serde_json::Value::Object(map) => {
            let mut was_truncated = false;
            for (k, v) in map.iter_mut() {
                if let serde_json::Value::String(s) = v {
                    let max_escaped = COMPACT_LINE_MAX.saturating_sub(k.len() + 24);
                    let limit = COMPACT_PROSE_MAX.min(max_escaped);
                    let mut escaped_len = serde_json::to_string(s).map_or(0, |j| j.len());
                    if s.len() > limit || escaped_len > max_escaped {
                        let mut end = s.len().min(limit);
                        while end > 0 && !s.is_char_boundary(end) {
                            end -= 1;
                        }
                        s.truncate(end);
                        s.push('…');
                        escaped_len = serde_json::to_string(s).map_or(0, |j| j.len());
                        while escaped_len > max_escaped && s.len() > 4 {
                            s.pop(); // remove ellipsis
                            let mut next_end = s.len().saturating_sub(16);
                            while next_end > 0 && !s.is_char_boundary(next_end) {
                                next_end -= 1;
                            }
                            s.truncate(next_end);
                            s.push('…');
                            escaped_len = serde_json::to_string(s).map_or(0, |j| j.len());
                        }
                        was_truncated = true;
                    }
                }
            }
            if was_truncated && !map.contains_key("truncated") {
                map.insert("truncated".to_string(), serde_json::Value::Bool(true));
            }
            for v in map.values_mut() {
                clamp_compact_prose(v);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr {
                clamp_compact_prose(v);
            }
        }
        _ => {}
    }
}

fn write_line_safe_json<W: Write>(
    out: &mut W,
    val: &serde_json::Value,
    indent: usize,
) -> std::io::Result<()> {
    let single = serde_json::to_string(val).unwrap_or_default();
    if indent + single.len() <= COMPACT_LINE_MAX {
        return out.write_all(single.as_bytes());
    }
    match val {
        serde_json::Value::Array(arr) if !arr.is_empty() => {
            out.write_all(b"[\n")?;
            let child_indent = indent + 1;
            for (i, item) in arr.iter().enumerate() {
                for _ in 0..child_indent {
                    out.write_all(b" ")?;
                }
                write_line_safe_json(out, item, child_indent)?;
                if i + 1 < arr.len() {
                    out.write_all(b",\n")?;
                } else {
                    out.write_all(b"\n")?;
                }
            }
            for _ in 0..indent {
                out.write_all(b" ")?;
            }
            out.write_all(b"]")
        }
        serde_json::Value::Object(map) if !map.is_empty() => {
            out.write_all(b"{\n")?;
            let child_indent = indent + 1;
            let len = map.len();
            for (i, (k, v)) in map.iter().enumerate() {
                for _ in 0..child_indent {
                    out.write_all(b" ")?;
                }
                let k_json = serde_json::to_string(k).unwrap_or_default();
                out.write_all(k_json.as_bytes())?;
                out.write_all(b":")?;
                write_line_safe_json(out, v, child_indent + k_json.len() + 1)?;
                if i + 1 < len {
                    out.write_all(b",\n")?;
                } else {
                    out.write_all(b"\n")?;
                }
            }
            for _ in 0..indent {
                out.write_all(b" ")?;
            }
            out.write_all(b"}")
        }
        _ => out.write_all(single.as_bytes()),
    }
}
