pub mod activity;
pub mod advise;
pub mod anchor;
pub mod batch;
pub mod brief;
pub mod check;
pub mod consolidate;
pub mod context;
pub mod coverage;
pub mod custom;
pub mod decision;
pub mod doctor;
pub mod drift;
pub mod git;
pub mod graph;
pub mod install;
pub mod link;
pub mod pattern;
pub mod pr;
pub mod prime;
pub mod progress;
pub mod prune;
pub mod query;
pub mod report;
pub mod rules;
pub mod schemas;
pub mod scoring;
pub mod session;
pub mod status;
pub mod transfer;
pub mod usage;
use anyhow::Result;
use rusqlite::Connection;
use serde_json::json;
use serde_json::Value;
use std::path::Path;

use crate::cli::{ActiveContextCmd, Command, ContextCmd, HistoryDoc, ReportCmd};

pub fn dispatch(
    conn: &mut Connection,
    cmd: Command,
    db_path: &Path,
    created: bool,
) -> Result<Value> {
    match cmd {
        Command::Init => Ok(json!({"db_path": db_path.display().to_string(), "created": created})),
        Command::Migrate => {
            crate::db::run_migrations(conn)?;
            Ok(json!({
                "status": "success",
                "version": crate::db::LATEST_VERSION,
                "message": "Database migrated to the latest version"
            }))
        }
        Command::ProductContext { cmd } => match cmd {
            ContextCmd::Get => context::get(conn, "product_context"),
            ContextCmd::Update(args) => context::update(conn, "product_context", &args),
        },
        Command::ActiveContext { cmd } => match cmd {
            ActiveContextCmd::Get { name } => context::get_track(conn, &name),
            ActiveContextCmd::Update { name, args } => context::update_track(conn, &name, &args),
            ActiveContextCmd::List => context::list_tracks(conn),
        },
        Command::History {
            doc,
            version,
            limit,
            name,
            all,
        } => match doc {
            HistoryDoc::ProductContext => context::history(conn, "product_context", version, limit),
            HistoryDoc::ActiveContext => {
                if all {
                    context::history_all_tracks(conn)
                } else {
                    let track = name.as_deref().unwrap_or("default");
                    context::history_track(conn, track, version, limit)
                }
            }
        },
        Command::Decision { cmd } => decision::handle(conn, cmd),
        Command::Progress { cmd } => progress::handle(conn, cmd),
        Command::Pattern { cmd } => pattern::handle(conn, cmd, db_path),
        Command::Rules { cmd } => rules::handle(conn, cmd, db_path),
        Command::Install { harness, hooks } => install::handle(conn, &harness, hooks, db_path),
        Command::Check { staged, paths } => check::handle(conn, staged, &paths, db_path),
        Command::Custom { cmd } => custom::handle(conn, cmd),
        Command::Link { cmd } => link::handle(conn, cmd),
        Command::Activity(args) => activity::handle(conn, args),
        Command::Batch { r#type, items } => batch::handle(conn, r#type, items, db_path),
        Command::Export { path } => transfer::export::handle(conn, &path),
        Command::Import { path } => transfer::import::handle(conn, &path),
        Command::Report { cmd, topic, limit } => match cmd {
            Some(ReportCmd::Open { no_browser, out }) => {
                report::open(conn, db_path, no_browser, out)
            }
            None => report::handle(conn, topic, limit),
        },
        Command::Pr { cmd } => pr::handle(conn, cmd),
        Command::Anchor { cmd } => anchor::handle(conn, cmd),
        Command::Relevant { paths, staged, all } => {
            anchor::handle_relevant(conn, paths, staged, all)
        }
        Command::Advise { paths, staged } => advise::handle(conn, paths, staged, db_path),
        Command::Prune(cmd) => prune::handle(conn, cmd),
        Command::Consolidate(cmd) => {
            consolidate::handle(conn, cmd.apply, cmd.min_repeats, cmd.min_days)
        }
        Command::Prime {
            budget,
            paths,
            tags,
        } => prime::handle(conn, budget, paths, tags),
        Command::Doctor => doctor::handle(conn, db_path),
        Command::Graph { cmd } => graph::handle(conn, cmd, db_path),
        Command::Schema { cmd } => schemas::handle(conn, cmd),
        Command::Query {
            query,
            types,
            tags,
            since,
            limit,
            all,
            full,
        } => query::handle(conn, query, types, tags, since, limit, all, full),
        Command::Brief { target, depth } => brief::handle(conn, &target, depth),
        Command::Usage {
            since,
            daily,
            misses,
        } => usage::handle(conn, since, daily, misses),
        Command::Coverage { paths, diff } => coverage::handle(conn, paths, diff),
        Command::Session { cmd } => session::handle(conn, cmd),
        Command::Instructions => unreachable!("handled in main before dispatch"),
    }
}
/// Split an identifier into word parts at identifier boundaries:
/// snake_case, kebab-case, dotted paths, camelCase and acronym runs
/// (`HTTPServer` → http server), and alpha/digit edges (`v2Beta` → v2 beta).
/// Non-ASCII word characters pass through lowercased.
pub(crate) fn ident_parts(token: &str) -> Vec<String> {
    let chars: Vec<char> = token.chars().collect();
    let mut parts = Vec::new();
    let mut cur = String::new();
    let mut prev_class: Option<u8> = None; // 1 lower, 2 upper, 3 digit, 4 other-word
    for (i, &c) in chars.iter().enumerate() {
        let class = if c.is_ascii_lowercase() {
            1
        } else if c.is_ascii_uppercase() {
            2
        } else if c.is_ascii_digit() {
            3
        } else if c.is_alphanumeric() {
            4
        } else {
            0 // separator: _, -, ., whitespace, punctuation
        };
        if class == 0 {
            if !cur.is_empty() {
                parts.push(std::mem::take(&mut cur));
            }
            prev_class = None;
            continue;
        }
        // Boundary: camel hump, end of acronym run, or alpha/digit edge.
        let split = match prev_class {
            None => false,
            Some(1) => class == 2 || class == 3,
            Some(2) => {
                class == 3
                    || (class == 2 && chars.get(i + 1).is_some_and(|n| n.is_ascii_lowercase()))
            }
            Some(3) => class != 3,
            _ => true,
        };
        if split && !cur.is_empty() {
            parts.push(std::mem::take(&mut cur));
        }
        for lc in c.to_lowercase() {
            cur.push(lc);
        }
        prev_class = Some(class);
    }
    if !cur.is_empty() {
        parts.push(cur);
    }
    parts
}

/// Build an FTS5 MATCH expression from a free-text query.
///
/// Each whitespace token becomes `(parts-phrase OR concat*)` so a query for
/// `http_server` matches `http_server`/`http-server` (index splits separators
/// → phrase) and `httpServerUtil` (single concatenated index token; the `*`
/// prefix absorbs the unpredictable tail). Pure-symbol queries that normalize
/// to nothing fall back to the legacy quoted form.
pub(crate) fn fts_match_expr(query: &str) -> String {
    let mut clauses = Vec::new();
    for token in query.split_whitespace() {
        let parts = ident_parts(token);
        if parts.is_empty() {
            continue;
        }
        if parts.len() == 1 {
            clauses.push(format!("\"{}\"", parts[0]));
        } else {
            clauses.push(format!("\"{}\" OR {}*", parts.join(" "), parts.concat()));
        }
    }
    if clauses.is_empty() {
        // Preserve "matches nothing" semantics without FTS syntax errors.
        return format!("\"{}\"", query.trim().replace('"', "\"\""));
    }
    clauses.join(" ")
}
/// N comma-separated SQL placeholders (`?,?,…`), built with one allocation.
/// Supersedes the per-item `"?"` map-collect-join idiom (rule 005).
pub(crate) fn sql_placeholders(n: usize) -> String {
    let mut s = String::with_capacity(n + n.saturating_sub(1));
    for i in 0..n {
        if i > 0 {
            s.push(',');
        }
        s.push('?');
    }
    s
}
#[cfg(test)]
mod fts_expr_tests {
    use super::*;

    #[test]
    fn ident_parts_splits_conventions() {
        assert_eq!(ident_parts("system_user_id"), vec!["system", "user", "id"]);
        assert_eq!(ident_parts("system-user-id"), vec!["system", "user", "id"]);
        assert_eq!(ident_parts("systemUserId"), vec!["system", "user", "id"]);
        assert_eq!(ident_parts("SystemUserId"), vec!["system", "user", "id"]);
        // Acronym runs split at run boundaries (HTTPServer → http server).
        assert_eq!(
            ident_parts("parseHTTPServer"),
            vec!["parse", "http", "server"]
        );
        assert_eq!(ident_parts("HTTPServer"), vec!["http", "server"]);
        assert_eq!(ident_parts("plain"), vec!["plain"]);
        assert!(ident_parts("---___").is_empty());
    }

    #[test]
    fn match_expr_cross_convention() {
        // Multi-part tokens: phrase (separator-indexed docs) OR unquoted
        // prefix term (concatenated index tokens like systemUserId).
        assert_eq!(
            fts_match_expr("system_user_id"),
            "\"system user id\" OR systemuserid*"
        );
        assert_eq!(
            fts_match_expr("systemUserId"),
            "\"system user id\" OR systemuserid*"
        );
        // Single-word tokens stay a plain quoted term (no noisy prefix).
        assert_eq!(fts_match_expr("resolver"), "\"resolver\"");
        // Multi-token queries AND their clauses.
        assert_eq!(
            fts_match_expr("tok_a tok_b"),
            "\"tok a\" OR toka* \"tok b\" OR tokb*"
        );
        // Pure-symbol queries fall back to the quoted literal.
        assert_eq!(fts_match_expr("--- ___"), "\"--- ___\"");
    }
}
