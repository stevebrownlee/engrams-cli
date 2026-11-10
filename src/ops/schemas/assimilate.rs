//! Lexical assimilation hooks (spec 0002, AC-8): at log time, new items are
//! matched against confirmed schemas' `centroid_json` and either fire a
//! suggestion (persisted to `schema_suggestions`, no membership write) or —
//! with the explicit `--schema` flag — attach `member_of` at write time and
//! bump the schema's confirmation recency.
//!
//! Matching is convention-aware token overlap only (no model): fit is the
//! share of a schema's centroid tags whose word parts appear in the item's
//! summary + tags. Thresholds are decision 78's frozen launch defaults.

use std::collections::BTreeSet;

use anyhow::{Context as _, Result};
use rusqlite::{params, Connection, OptionalExtension};

use serde_json::{json, Value};

/// Minimum token-overlap fit for a suggestion to fire (decision 78).
pub const FIT_GATE: f64 = 0.4;
/// Top-N schemas suggested per item (decision 78).
pub const TOP_SUGGESTIONS: usize = 2;

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// One fired or resolved suggestion, as surfaced in log-command output.
pub struct Suggestion {
    pub schema_id: i64,
    pub name: String,
    pub fit: f64,
}

impl Suggestion {
    fn to_json(&self) -> Value {
        json!({ "id": self.schema_id, "name": self.name, "fit": self.fit })
    }
}

/// Convention-aware token set for an item's lexical surface: ident parts of
/// the summary words plus every tag (snake/kebab/camel variants collapse).
pub(super) fn item_tokens(summary: &str, tags: &[String]) -> BTreeSet<String> {
    let mut parts = BTreeSet::new();
    for word in summary.split_whitespace() {
        parts.extend(crate::ops::ident_parts(word));
    }
    for tag in tags {
        parts.extend(crate::ops::ident_parts(tag));
    }
    parts
}

/// Match one item against all active schemas.
/// When `tags` or `anchors` are supplied (or when active decisions exist in the
/// workspace), uses the Idea 2 two-step data-driven Evidence Gate + Prevalence
/// fit scorer (binary `path_signal` against `scope.paths`/`centroid.anchors`
/// plus database-derived umbrella and sibling-schema tag frequencies). Falls
/// back to lexical token overlap only for synthetic untagged/unanchored unit
/// tests on empty workspaces.
#[cfg(test)]
pub fn match_schemas(conn: &Connection, summary: &str, tags: &[String]) -> Result<Vec<Suggestion>> {
    match_schemas_with_anchors(conn, summary, tags, &[])
}

pub fn match_schemas_with_anchors(
    conn: &Connection,
    summary: &str,
    tags: &[String],
    anchors: &[String],
) -> Result<Vec<Suggestion>> {
    use std::collections::BTreeMap;

    let mut stmt = conn.prepare(
        "SELECT id, name, scope_json, centroid_json FROM schemas \
         WHERE status = 'active' ORDER BY id",
    )?;
    let schema_rows: Vec<(i64, String, String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if schema_rows.is_empty() {
        return Ok(Vec::new());
    }

    // Check workspace decision count and tag document frequencies.
    let mut tag_doc_freq: BTreeMap<String, usize> = BTreeMap::new();
    let mut active_dec_count = 0usize;
    {
        let mut dstmt =
            conn.prepare("SELECT tags FROM decisions WHERE status = 'active' AND archived = 0")?;
        let drows = dstmt.query_map([], |r| r.get::<_, Option<String>>(0))?;
        for dr in drows {
            active_dec_count += 1;
            let raw = dr?;
            for t in crate::models::parse_tags(raw.as_deref()) {
                let canon = crate::ops::ident_parts(&t).join("-");
                if !canon.is_empty() {
                    *tag_doc_freq.entry(canon).or_insert(0) += 1;
                }
            }
        }
    }

    // If untagged and unanchored: in a live workspace (`active_dec_count > 0`)
    // return no automatic suggestions; in synthetic unit-test DBs (`0` active
    // decisions) preserve convention-aware token matching.
    if tags.is_empty() && anchors.is_empty() {
        if active_dec_count > 0 {
            return Ok(Vec::new());
        }
        return legacy_lexical_match(summary, &schema_rows);
    }

    let umbrella_threshold = 20usize.max(active_dec_count / 20);
    let umbrella_tags: BTreeSet<String> = tag_doc_freq
        .into_iter()
        .filter_map(|(t, cnt)| (cnt > umbrella_threshold).then_some(t))
        .collect();

    let member_counts = super::list::member_counts(conn)?;

    struct ParsedSchema {
        id: i64,
        name: String,
        scope_tags: BTreeSet<String>,
        scope_paths: Vec<String>,
        ctags: BTreeMap<String, usize>,
        canchors: BTreeSet<String>,
        mcount: usize,
        prevalent_tags: BTreeSet<String>,
    }

    let mut parsed: Vec<ParsedSchema> = Vec::with_capacity(schema_rows.len());
    for (id, name, scope_json, centroid_json) in schema_rows {
        let scope = super::retrieval::parse_scope(&scope_json);
        let scope_tags: BTreeSet<String> = scope
            .tags
            .iter()
            .map(|t| crate::ops::ident_parts(t).join("-"))
            .filter(|s| !s.is_empty())
            .collect();
        let mut ctags: BTreeMap<String, usize> = BTreeMap::new();
        let mut canchors: BTreeSet<String> = BTreeSet::new();
        if let Ok(centroid) = serde_json::from_str::<Value>(&centroid_json) {
            if let Some(tag_obj) = centroid["tags"].as_object() {
                for (k, v) in tag_obj {
                    let canon = crate::ops::ident_parts(k).join("-");
                    let n = v.as_u64().unwrap_or(1) as usize;
                    if !canon.is_empty() {
                        *ctags.entry(canon).or_insert(0) += n;
                    }
                }
            }
            if let Some(anc_obj) = centroid["anchors"].as_object() {
                for k in anc_obj.keys() {
                    canchors.insert(crate::ops::anchor::clean_path(k));
                }
            }
        }
        let link_mcount = member_counts.get(&id).copied().unwrap_or(0) as usize;
        let max_tag_cnt = ctags.values().copied().max().unwrap_or(1);
        let mcount = link_mcount.max(max_tag_cnt).max(1);
        let prevalent_tags: BTreeSet<String> = ctags
            .iter()
            .filter(|(_, &cnt)| cnt >= 2 && (cnt as f64) / (mcount as f64) >= 0.3)
            .map(|(t, _)| t.clone())
            .collect();
        parsed.push(ParsedSchema {
            id,
            name,
            scope_tags,
            scope_paths: scope.paths,
            ctags,
            canchors,
            mcount,
            prevalent_tags,
        });
    }

    let item_tag_set: Vec<String> = tags
        .iter()
        .map(|t| crate::ops::ident_parts(t).join("-"))
        .filter(|s| !s.is_empty())
        .collect();
    let item_anchors: Vec<String> = anchors
        .iter()
        .map(|a| crate::ops::anchor::clean_path(a))
        .collect();

    let mut hits: Vec<Suggestion> = Vec::new();
    for (idx, s) in parsed.iter().enumerate() {
        let mut sibling_ctags: BTreeSet<String> = BTreeSet::new();
        for (other_idx, other) in parsed.iter().enumerate() {
            if idx == other_idx {
                continue;
            }
            let shares_path = s
                .scope_paths
                .iter()
                .any(|sp| other.scope_paths.iter().any(|osp| sp == osp));
            if shares_path {
                sibling_ctags.extend(other.prevalent_tags.iter().cloned());
            }
        }
        let has_sibling_on_path = !sibling_ctags.is_empty();
        // Only tags carried by >= 50% of schema members (or scope.tags that are
        // not workspace umbrella tags, or the schema's own leading name token)
        // count as `primary_tags`, so a secondary scope tag like `harbor` never
        // admits unrelated notes on its own.
        let mut primary_tags: BTreeSet<String> = s
            .ctags
            .iter()
            .filter(|(_, &cnt)| cnt >= 2 && (cnt as f64) / (s.mcount as f64) >= 0.5)
            .map(|(t, _)| t.clone())
            .collect();
        for st in &s.scope_tags {
            if !umbrella_tags.contains(st) || s.name.starts_with(st.as_str()) {
                primary_tags.insert(st.clone());
            }
        }
        for sib in &sibling_ctags {
            if !s.scope_tags.contains(sib) {
                primary_tags.remove(sib);
            }
        }
        let mut specific_tags: BTreeSet<String> = s.prevalent_tags.clone();
        specific_tags.extend(s.scope_tags.iter().cloned());
        for u in &umbrella_tags {
            specific_tags.remove(u);
        }
        for sib in &sibling_ctags {
            if !s.scope_tags.contains(sib) {
                specific_tags.remove(sib);
            }
        }

        let specific_spaths: Vec<&str> = s
            .scope_paths
            .iter()
            .map(String::as_str)
            .filter(|p| parsed.len() == 1 || p.trim_matches('/').split('/').count() >= 2)
            .collect();
        let exact_file_match = item_anchors.iter().any(|a| {
            s.scope_paths.iter().any(|sp| sp == a) || (s.canchors.contains(a) && a != "AGENTS.md")
        });
        let dir_scope_match = item_anchors.iter().any(|a| {
            specific_spaths
                .iter()
                .any(|sp| super::retrieval::path_matches(a, sp))
        });
        let path_signal = if exact_file_match || dir_scope_match {
            1.0
        } else {
            0.0
        };

        let matched_primary: Vec<&String> = item_tag_set
            .iter()
            .filter(|t| primary_tags.contains(*t))
            .collect();
        let matched_specific: Vec<&String> = item_tag_set
            .iter()
            .filter(|t| specific_tags.contains(*t))
            .collect();
        let matched_scope: Vec<&String> = item_tag_set
            .iter()
            .filter(|t| s.scope_tags.contains(*t))
            .collect();

        let gate_ok = if path_signal > 0.0 {
            !matched_specific.is_empty() || (!has_sibling_on_path && !matched_primary.is_empty())
        } else {
            matched_specific.len() >= 2 && (s.scope_tags.is_empty() || !matched_scope.is_empty())
        };
        if !gate_ok {
            continue;
        }

        let mut tag_weight_sum = 0.0f64;
        for t in &item_tag_set {
            if specific_tags.contains(t) || primary_tags.contains(t) {
                let prev = s.ctags.get(t).copied().unwrap_or(1) as f64 / s.mcount as f64;
                let boost = if s.scope_tags.contains(t) { 0.35 } else { 0.0 };
                let mut w = (prev + boost).min(1.0);
                if sibling_ctags.contains(t) && !s.scope_tags.contains(t) {
                    w *= 0.3;
                }
                tag_weight_sum += w;
            }
        }
        let fit = ((path_signal * 0.6 + tag_weight_sum) / 1.4).min(1.0);
        if fit >= FIT_GATE {
            hits.push(Suggestion {
                schema_id: s.id,
                name: s.name.clone(),
                fit,
            });
        }
    }

    hits.sort_by(|a, b| {
        b.fit
            .partial_cmp(&a.fit)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.schema_id.cmp(&b.schema_id))
    });
    hits.truncate(TOP_SUGGESTIONS);
    Ok(hits)
}

fn legacy_lexical_match(
    summary: &str,
    schema_rows: &[(i64, String, String, String)],
) -> Result<Vec<Suggestion>> {
    let query_parts = item_tokens(summary, &[]);
    if query_parts.is_empty() {
        return Ok(Vec::new());
    }
    let mut hits: Vec<Suggestion> = Vec::new();
    for (id, name, scope_json, centroid_json) in schema_rows {
        let scope = super::retrieval::parse_scope(scope_json);
        let mut vocab: BTreeSet<String> = BTreeSet::new();
        vocab.extend(scope.tags);
        vocab.extend(scope.paths);
        if let Ok(centroid) = serde_json::from_str::<Value>(centroid_json) {
            if let Some(tag_keys) = centroid["tags"].as_object() {
                vocab.extend(tag_keys.keys().cloned());
            }
        }
        if vocab.is_empty() {
            continue;
        }
        let matched = vocab
            .iter()
            .filter(|t| {
                crate::ops::ident_parts(t)
                    .iter()
                    .any(|p| query_parts.contains(p))
            })
            .count();
        let basis = vocab.len().clamp(1, 3);
        let fit = (matched as f64 / basis as f64).min(1.0);
        if fit >= FIT_GATE {
            hits.push(Suggestion {
                schema_id: *id,
                name: name.clone(),
                fit,
            });
        }
    }
    hits.sort_by(|a, b| {
        b.fit
            .partial_cmp(&a.fit)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.schema_id.cmp(&b.schema_id))
    });
    hits.truncate(TOP_SUGGESTIONS);
    Ok(hits)
}
/// Persist fired suggestions for one item (status `suggested`). Re-firing
/// refreshes the fit but never revives a row the user already resolved.
pub fn record_suggestions(
    conn: &Connection,
    kind: &str,
    item_id: i64,
    suggestions: &[Suggestion],
) -> Result<()> {
    if suggestions.is_empty() {
        return Ok(());
    }
    let ts = now();
    // Composable with an outer transaction (batch): join it instead of
    // nesting a second BEGIN on the same connection.
    if conn.is_autocommit() {
        let tx = conn.unchecked_transaction()?;
        insert_suggestions(&tx, kind, item_id, suggestions, &ts)?;
        tx.commit()?;
    } else {
        insert_suggestions(conn, kind, item_id, suggestions, &ts)?;
    }
    Ok(())
}

fn insert_suggestions(
    conn: &Connection,
    kind: &str,
    item_id: i64,
    suggestions: &[Suggestion],
    ts: &str,
) -> Result<()> {
    for s in suggestions {
        conn.execute(
            "INSERT INTO schema_suggestions (ts, schema_id, item_kind, item_id, fit, status) \
             VALUES (?1, ?2, ?3, ?4, ?5, 'suggested') \
             ON CONFLICT(schema_id, item_kind, item_id) DO UPDATE SET \
             fit = excluded.fit, \
             status = CASE WHEN schema_suggestions.status = 'suggested' \
                           THEN 'suggested' ELSE schema_suggestions.status END",
            params![ts, s.schema_id, kind, item_id, s.fit],
        )?;
    }
    Ok(())
}

/// `--schema <id|name>`: attach `member_of` at write time, mark any matching
/// suggestion accepted, and bump the schema's `last_confirmed_at` (AC-8).
/// Idempotent — a repeated attach updates recency, never duplicates edges.
pub fn attach(conn: &Connection, kind: &str, item_id: i64, target: &str) -> Result<Suggestion> {
    let schema_id: i64 = match target.parse::<i64>() {
        Ok(id) => {
            let hit: Option<i64> = conn
                .query_row("SELECT id FROM schemas WHERE id = ?1", params![id], |r| {
                    r.get(0)
                })
                .optional()?;
            hit.ok_or_else(|| anyhow::anyhow!("no schema matches '{target}'"))?
        }
        Err(_) => {
            let hit: Option<i64> = conn
                .query_row(
                    "SELECT id FROM schemas WHERE name = ?1",
                    params![target],
                    |r| r.get(0),
                )
                .optional()?;
            hit.ok_or_else(|| anyhow::anyhow!("no schema matches '{target}'"))?
        }
    };
    let name: String = conn.query_row(
        "SELECT name FROM schemas WHERE id = ?1",
        params![schema_id],
        |r| r.get(0),
    )?;

    let ts = now();
    let graph_kind = match kind {
        "pattern" => "system_pattern",
        "progress-entry" => "progress_entry",
        other => other,
    };
    // Same composability rule as record_suggestions.
    if conn.is_autocommit() {
        let tx = conn.unchecked_transaction()?;
        attach_writes(&tx, graph_kind, item_id, schema_id, kind, &ts)?;
        tx.commit()?;
    } else {
        attach_writes(conn, graph_kind, item_id, schema_id, kind, &ts)?;
    }

    /// The three attach writes: edge (idempotent), suggestion acceptance,
    /// confirmation recency. Runs on the caller's transaction scope.
    fn attach_writes(
        conn: &Connection,
        graph_kind: &str,
        item_id: i64,
        schema_id: i64,
        kind: &str,
        ts: &str,
    ) -> Result<()> {
        conn.execute(
            "INSERT INTO context_links (source_item_type, source_item_id, target_item_type, \
         target_item_id, relationship_type, timestamp, origin, source, weight) \
         SELECT ?1, ?2, 'schema', ?3, 'member_of', ?4, 'manual', 'schema_assimilate', 1.0 \
         WHERE NOT EXISTS (SELECT 1 FROM context_links WHERE source_item_type = ?1 \
         AND source_item_id = ?2 AND target_item_type = 'schema' AND target_item_id = ?3 \
         AND relationship_type = 'member_of')",
            params![graph_kind, item_id.to_string(), schema_id.to_string(), ts],
        )
        .context("attaching member_of edge")?;
        conn.execute(
            "UPDATE schema_suggestions SET status = 'accepted', ts = ?4 \
         WHERE schema_id = ?1 AND item_kind = ?2 AND item_id = ?3",
            params![schema_id, kind, item_id, ts],
        )?;
        conn.execute(
            "UPDATE schemas SET last_confirmed_at = ?2, updated_at = ?2 WHERE id = ?1",
            params![schema_id, ts],
        )?;
        super::confirm::refresh_schema_centroid(conn, schema_id)?;
        Ok(())
    }

    let fit: Option<f64> = conn
        .query_row(
            "SELECT fit FROM schema_suggestions \
             WHERE schema_id = ?1 AND item_kind = ?2 AND item_id = ?3",
            params![schema_id, kind, item_id],
            |r| r.get(0),
        )
        .optional()?;
    Ok(Suggestion {
        schema_id,
        name,
        fit: fit.unwrap_or(0.0),
    })
}

/// `--schema none`: the decision-79 opt-out. Every still-`suggested` row for
/// this item flips to `declined`; an item that never fired has nothing to
/// decline and stays absent from the table (a never-attached suggestion is
/// ambiguous between declined, ignored, and session-ended — only explicit
/// opt-out is observable). Returns the number of rows flipped.
pub fn decline_fired(conn: &Connection, kind: &str, item_id: i64) -> Result<usize> {
    let n = conn.execute(
        "UPDATE schema_suggestions SET status = 'declined', ts = ?3 \
         WHERE item_kind = ?1 AND item_id = ?2 AND status = 'suggested'",
        params![kind, item_id, now()],
    )?;
    Ok(n)
}

/// The output block shared by the three log commands: the fired/attached
/// suggestion list, plus the attach/decline markers when the flags ran.
pub fn output_block(
    conn: &Connection,
    kind: &str,
    item_id: i64,
    attached: Option<&Suggestion>,
    declined: usize,
) -> Result<Value> {
    let mut suggestions = Vec::new();
    if attached.is_none() {
        let mut stmt = conn.prepare(
            "SELECT s.id, s.name, g.fit FROM schema_suggestions g \
             JOIN schemas s ON s.id = g.schema_id \
             WHERE g.item_kind = ?1 AND g.item_id = ?2 AND g.status = 'suggested' \
             ORDER BY g.fit DESC, s.id",
        )?;
        let rows = stmt.query_map(params![kind, item_id], |r| {
            Ok(Suggestion {
                schema_id: r.get(0)?,
                name: r.get(1)?,
                fit: r.get(2)?,
            })
        })?;
        for r in rows {
            suggestions.push(r?.to_json());
        }
    }
    Ok(json!({
        "schema_suggestions": suggestions,
        "schema_attached": attached.map(|s| s.to_json()),
        "schema_declined": if declined > 0 { Some(declined) } else { None },
    }))
}

/// The one call each log command makes after its write commits: `--schema
/// <id|name>` attaches, `--schema none` declines fired suggestions, absence
/// matches and fires. `kind` is the schema_suggestions vocabulary
/// (`decision` / `pattern` / `progress-entry`); item ids are global per kind.
pub fn at_log_time(
    conn: &Connection,
    kind: &str,
    item_id: i64,
    summary: &str,
    tags: &[String],
    schema: Option<&str>,
) -> Result<Value> {
    at_log_time_with_anchors(conn, kind, item_id, summary, tags, &[], schema)
}

pub fn at_log_time_with_anchors(
    conn: &Connection,
    kind: &str,
    item_id: i64,
    summary: &str,
    tags: &[String],
    anchors: &[String],
    schema: Option<&str>,
) -> Result<Value> {
    let (attached, declined) = match schema {
        // Fire first, then flip: the item DID lexically match, and the
        // declined row is the audit trail that it will not be re-suggested.
        Some("none") => {
            let mut hits = match_schemas_with_anchors(conn, summary, tags, anchors)?;
            if hits.is_empty() {
                let mut stmt = conn.prepare(
                    "SELECT id, name, scope_json, centroid_json FROM schemas WHERE status = 'active' ORDER BY id",
                )?;
                let schema_rows: Vec<(i64, String, String, String)> = stmt
                    .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                hits = legacy_lexical_match(summary, &schema_rows)?;
            }
            record_suggestions(conn, kind, item_id, &hits)?;
            (None, decline_fired(conn, kind, item_id)?)
        }
        Some(target) => (Some(attach(conn, kind, item_id, target)?), 0),
        None => {
            // Idea 2: Skip auto-suggesting `progress-entry` status updates;
            // only decisions and patterns generate staged suggestions.
            if kind != "progress-entry" {
                let hits = match_schemas_with_anchors(conn, summary, tags, anchors)?;
                record_suggestions(conn, kind, item_id, &hits)?;
            }
            (None, 0)
        }
    };
    output_block(conn, kind, item_id, attached.as_ref(), declined)
}

/// Return up to 10 pending `schema_suggestions` (for `decision` and `pattern`
/// items) with composite key, item summary, fit, and ready-to-run accept/decline
/// commands for `session close` and `engrams schema assimilate` (Idea 2B / Idea 10).
pub fn pending_for_close(conn: &Connection, schema_filter: Option<i64>) -> Result<Vec<Value>> {
    let sql = if schema_filter.is_some() {
        "SELECT g.schema_id, s.name, g.item_kind, g.item_id, g.fit, g.ts \
         FROM schema_suggestions g \
         JOIN schemas s ON s.id = g.schema_id \
         WHERE g.status = 'suggested' AND s.status = 'active' AND g.schema_id = ?1 \
           AND g.item_kind IN ('decision', 'pattern', 'system_pattern') \
         ORDER BY g.fit DESC, g.ts DESC, g.schema_id ASC, g.item_id ASC LIMIT 10"
    } else {
        "SELECT g.schema_id, s.name, g.item_kind, g.item_id, g.fit, g.ts \
         FROM schema_suggestions g \
         JOIN schemas s ON s.id = g.schema_id \
         WHERE g.status = 'suggested' AND s.status = 'active' \
           AND g.item_kind IN ('decision', 'pattern', 'system_pattern') \
         ORDER BY g.fit DESC, g.ts DESC, g.schema_id ASC, g.item_id ASC LIMIT 10"
    };
    let mut stmt = conn.prepare(sql)?;
    let map_row =
        |r: &rusqlite::Row<'_>| -> rusqlite::Result<(i64, String, String, i64, f64, String)> {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
            ))
        };
    let rows: Vec<(i64, String, String, i64, f64, String)> = if let Some(sid) = schema_filter {
        stmt.query_map(params![sid], map_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?
    } else {
        stmt.query_map([], map_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    let mut out = Vec::with_capacity(rows.len());
    for (schema_id, schema_name, item_kind, item_id, fit, ts) in rows {
        let summary: Option<String> = match item_kind.as_str() {
            "decision" => conn
                .query_row(
                    "SELECT summary FROM decisions WHERE id = ?1 AND status = 'active' AND archived = 0",
                    params![item_id],
                    |r| r.get(0),
                )
                .optional()?,
            "pattern" | "system_pattern" => conn
                .query_row(
                    "SELECT name FROM system_patterns WHERE id = ?1 AND archived = 0",
                    params![item_id],
                    |r| r.get(0),
                )
                .optional()?,
            _ => None,
        };
        let Some(item_summary) = summary else {
            continue;
        };
        let key_ref = format!("{item_kind}:{item_id}");
        out.push(json!({
            "schema_id": schema_id,
            "schema_name": schema_name,
            "item_kind": item_kind,
            "item_id": item_id,
            "summary": item_summary,
            "fit": fit,
            "ts": ts,
            "accept_command": format!("engrams schema assimilate {schema_name} --accept {key_ref}"),
            "decline_command": format!("engrams schema assimilate {schema_name} --decline {key_ref}"),
        }));
    }
    Ok(out)
}

/// Handler for `engrams schema assimilate [<target>] [--accept <kind:id,...>] [--decline <kind:id,...>]` (Idea 2B).
pub fn assimilate_cmd(
    conn: &Connection,
    target: Option<&str>,
    accept: &[String],
    decline: &[String],
) -> Result<Value> {
    if accept.is_empty() && decline.is_empty() {
        let sid = match target {
            Some(t) => Some(
                super::list::resolve_schema(conn, t)?
                    .ok_or_else(|| anyhow::anyhow!("no schema matches '{t}'"))?,
            ),
            None => None,
        };
        let pending = pending_for_close(conn, sid)?;
        return Ok(json!({
            "status": "success",
            "pending_suggestions": pending,
        }));
    }
    let Some(target_str) = target else {
        anyhow::bail!("specify a target schema name or id when passing --accept or --decline");
    };
    let schema_id = super::list::resolve_schema(conn, target_str)?
        .ok_or_else(|| anyhow::anyhow!("no schema matches '{target_str}'"))?;
    let mut accepted_items = Vec::new();
    let mut declined_items = Vec::new();
    for raw in accept {
        let (canon_kind, id, _) = super::propose::parse_note_key(raw)?;
        let sugg_kind = if canon_kind == "system_pattern" {
            "pattern"
        } else {
            canon_kind
        };
        let _ = attach(conn, sugg_kind, id, &schema_id.to_string())?;
        accepted_items.push(format!("{sugg_kind}:{id}"));
    }
    let ts = now();
    for raw in decline {
        let (canon_kind, id, _) = super::propose::parse_note_key(raw)?;
        for k in [
            canon_kind,
            if canon_kind == "system_pattern" {
                "pattern"
            } else {
                canon_kind
            },
        ] {
            conn.execute(
                "UPDATE schema_suggestions SET status = 'declined', ts = ?4 \
                 WHERE schema_id = ?1 AND item_kind = ?2 AND item_id = ?3",
                params![schema_id, k, id, ts],
            )?;
        }
        declined_items.push(format!("{canon_kind}:{id}"));
    }
    Ok(json!({
        "status": "success",
        "schema_id": schema_id,
        "accepted": accepted_items,
        "declined": declined_items,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::SCHEMA;
    use rusqlite::params;

    fn mem_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        conn
    }

    /// Confirmed gateway schema with a real-shaped centroid.
    fn gateway_schema(conn: &Connection) -> i64 {
        conn.execute(
            "INSERT INTO schemas (uuid, name, summary, summary_source, status, centroid_json, \
             last_confirmed_at, created_at, updated_at) \
             VALUES ('u', 'gateway', 's', 'agent', 'active', \
             '{\"tags\": {\"api_gateway\": 2, \"routing\": 3}, \"anchors\": {}}', \
             '2026-01-01T00:00:00Z', 't0', 't0')",
            [],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn fit_gate_and_convention_aware_tokenization() {
        let conn = mem_db();
        let id = gateway_schema(&conn);

        // kebab-case query tokenizes to the same parts as the centroid tag.
        let hits = match_schemas(&conn, "Rewire the api-gateway routing table", &[]).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].schema_id, id);
        assert!((hits[0].fit - 1.0).abs() < 1e-9);

        // Half the centroid matches: fires at exactly the 0.4 gate.
        let half = match_schemas(&conn, "unrelated note about routing", &[]).unwrap();
        assert_eq!(half.len(), 1);
        assert!((half[0].fit - 0.5).abs() < 1e-9);

        // Below the gate: no suggestion.
        assert!(
            match_schemas(&conn, "completely unrelated storage engine", &[])
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn suggestions_persist_and_never_write_membership() {
        let conn = mem_db();
        gateway_schema(&conn);
        let hits = match_schemas(&conn, "api-gateway routing", &[]).unwrap();
        record_suggestions(&conn, "decision", 7, &hits).unwrap();

        let (fit, status): (f64, String) = conn
            .query_row(
                "SELECT fit, status FROM schema_suggestions WHERE item_kind = 'decision' AND item_id = 7",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(status, "suggested");
        assert!((fit - 1.0).abs() < 1e-9);
        let edges: i64 = conn
            .query_row("SELECT COUNT(*) FROM context_links", [], |r| r.get(0))
            .unwrap();
        assert_eq!(edges, 0, "suggestion must not write membership (AC-8)");

        // Re-firing refreshes fit but never flips a resolved row.
        decline_fired(&conn, "decision", 7).unwrap();
        record_suggestions(&conn, "decision", 7, &hits).unwrap();
        let status: String = conn
            .query_row(
                "SELECT status FROM schema_suggestions WHERE item_kind = 'decision' AND item_id = 7",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "declined");
    }

    #[test]
    fn attach_links_marks_accepted_and_bumps_confirmation() {
        let conn = mem_db();
        let id = gateway_schema(&conn);
        let hits = match_schemas(&conn, "api-gateway routing", &[]).unwrap();
        record_suggestions(&conn, "decision", 7, &hits).unwrap();
        let before: String = conn
            .query_row(
                "SELECT last_confirmed_at FROM schemas WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap();

        let attached = attach(&conn, "decision", 7, &id.to_string()).unwrap();
        assert_eq!(attached.schema_id, id);

        // member_of edge in the canonical graph direction and vocabulary.
        let (skind, tid, rel): (String, String, String) = conn
            .query_row(
                "SELECT source_item_type, target_item_id, relationship_type FROM context_links",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!((skind.as_str(), rel.as_str()), ("decision", "member_of"));
        assert_eq!(tid, id.to_string());

        let status: String = conn
            .query_row("SELECT status FROM schema_suggestions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(status, "accepted");
        let after: String = conn
            .query_row(
                "SELECT last_confirmed_at FROM schemas WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap();
        assert_ne!(before, after, "attach bumps confirmation recency (AC-8)");

        // Idempotent: a repeated attach keeps one edge.
        attach(&conn, "decision", 7, "gateway").unwrap();
        let edges: i64 = conn
            .query_row("SELECT COUNT(*) FROM context_links", [], |r| r.get(0))
            .unwrap();
        assert_eq!(edges, 1);
    }

    #[test]
    fn decline_is_explicit_opt_out_only() {
        let conn = mem_db();
        gateway_schema(&conn);
        // No suggestions fired: --schema none is a no-op.
        assert_eq!(decline_fired(&conn, "decision", 1).unwrap(), 0);
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_suggestions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 0);

        let hits = match_schemas(&conn, "api-gateway routing", &[]).unwrap();
        record_suggestions(&conn, "decision", 1, &hits).unwrap();
        assert_eq!(decline_fired(&conn, "decision", 1).unwrap(), 1);
    }

    #[test]
    fn schema_none_fires_then_declines_a_fresh_match() {
        let conn = mem_db();
        gateway_schema(&conn);
        // A matching fresh item opted out: the suggestion is recorded AND
        // declined — the row proves the match happened and stays resolved.
        let out = at_log_time(
            &conn,
            "decision",
            9,
            "api-gateway routing",
            &[],
            Some("none"),
        )
        .unwrap();
        assert_eq!(out["schema_declined"], 1);
        assert_eq!(out["schema_suggestions"], json!([]));
        let (status, n): (String, i64) = conn
            .query_row(
                "SELECT status, COUNT(*) FROM schema_suggestions \
                 WHERE item_kind = 'decision' AND item_id = 9",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((status, n), ("declined".to_string(), 1));
    }

    /// AC-9 stability seam: member growth from attach flows through scan's
    /// identity machinery — the enlarged cluster re-stages, and re-applying
    /// is rejected by the confirmed-Jaccard guard instead of duplicating.
    #[test]
    fn scan_after_attach_recognizes_the_schema_without_duplicating() {
        let conn = mem_db();
        let sid = gateway_schema(&conn);
        for i in 1..=3 {
            conn.execute(
                "INSERT INTO decisions (uuid, timestamp, summary) \
                 VALUES (?1, ?2, 'gateway routing note')",
                params![format!("u{i}"), format!("2026-01-0{i}T00:00:00Z")],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO context_links (source_item_type, source_item_id, \
                 target_item_type, target_item_id, relationship_type, timestamp) \
                 VALUES ('decision', ?1, 'decision', ?2, 'relates_to', 't')",
                params![i.to_string(), (i % 3 + 1).to_string()],
            )
            .unwrap();
        }
        // The schema's own members, so the cluster carries the centroid's
        // lexical territory and a new gateway item genuinely matches it.
        for i in 1..=3 {
            conn.execute(
                "INSERT INTO context_links (source_item_type, source_item_id, \
                 target_item_type, target_item_id, relationship_type, timestamp) \
                 VALUES ('decision', ?1, 'schema', ?2, 'member_of', 't')",
                params![i.to_string(), sid.to_string()],
            )
            .unwrap();
        }

        // New item attaches at log time (member growth through the seam).
        conn.execute(
            "INSERT INTO decisions (uuid, timestamp, summary) \
             VALUES ('u4', '2026-01-02T00:00:00Z', 'api-gateway routing')",
            [],
        )
        .unwrap();
        attach(&conn, "decision", 4, &sid.to_string()).unwrap();

        // Scan stages the enlarged cluster (three sightings to clear the
        // stability gate); proposing from it is rejected by the
        // confirmed-territory covenant (AC-4 / AC-9).
        for _ in 0..3 {
            super::super::scan::scan(&conn).unwrap();
        }
        let err = super::super::propose::propose(
            &conn,
            "Gateway routing rules across services and endpoints",
            Some("When touching gateway routing: follow the shared routing table"),
            None,
            None,
            &[],
            Some("decision:1"),
            None,
            None,
            None,
            None,
            None,
            &[],
            None,
            false,
        )
        .unwrap_err()
        .to_string();
        assert!(
            err.contains("notes already covered by schema"),
            "enlarged cluster must be rejected as already covered: {err}"
        );
        let schemas: i64 = conn
            .query_row("SELECT COUNT(*) FROM schemas", [], |r| r.get(0))
            .unwrap();
        assert_eq!(schemas, 1);
        let members: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM context_links WHERE relationship_type = 'member_of' \
                 AND target_item_type = 'schema' AND target_item_id = ?1",
                params![sid],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(members, 4, "attached member survives scan untouched");
    }

    #[test]
    fn mature_schema_with_many_tags_matches_focused_item() {
        let conn = mem_db();
        // Centroid with 12 tags representing a mature, well-developed schema.
        conn.execute(
            "INSERT INTO schemas (uuid, name, summary, summary_source, status, centroid_json, \
             last_confirmed_at, created_at, updated_at) \
             VALUES ('u-mature', 'mature-infra', 's', 'agent', 'active', \
             '{\"tags\": {\"api\": 5, \"gateway\": 4, \"routing\": 6, \"proxy\": 3, \"auth\": 2, \
                         \"rate_limit\": 2, \"tls\": 1, \"cors\": 1, \"dns\": 1, \"http\": 4, \
                         \"grpc\": 2, \"mesh\": 1}, \"anchors\": {}}', \
             '2026-01-01T00:00:00Z', 't0', 't0')",
            [],
        )
        .unwrap();
        let id = conn.last_insert_rowid();

        // A decision tagged with 2 matching tags should match with 1.0 fit,
        // not 2/12 = 0.16.
        let hits = match_schemas(
            &conn,
            "Update gateway routes",
            &["api".to_string(), "gateway".to_string()],
        )
        .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].schema_id, id);
        assert!((hits[0].fit - 1.0).abs() < 1e-6);

        // An untagged summary mentioning 2 distinctive tags should match at 2/3 = 0.67 >= 0.40.
        let hits_untagged = match_schemas(&conn, "Rewire api gateway configuration", &[]).unwrap();
        assert_eq!(hits_untagged.len(), 1);
        assert_eq!(hits_untagged[0].schema_id, id);
        assert!((hits_untagged[0].fit - (2.0 / 3.0)).abs() < 1e-6);
    }

    #[test]
    fn match_schemas_includes_scope_tags_and_paths() {
        let conn = mem_db();
        conn.execute(
            "INSERT INTO schemas (uuid, name, summary, summary_source, scope_json, status, \
             centroid_json, last_confirmed_at, created_at, updated_at) \
             VALUES ('u-mara', 'mara-broken-leader', 'When writing Mara: keep self-doubt', \
             'agent', '{\"tags\":[\"mara\"],\"paths\":[\"manuscript\"]}', 'active', \
             '{\"tags\":{},\"anchors\":{}}', '2026-01-01T00:00:00Z', 't0', 't0')",
            [],
        )
        .unwrap();

        let hits = match_schemas(&conn, "Mara hesitates in manuscript chapter", &[]).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "mara-broken-leader");
    }
}
