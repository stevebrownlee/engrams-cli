//! Schema opportunity detection and naming at session close (spec 0005).

use anyhow::Result;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::collections::BTreeMap;

use super::gates::{claimed_territory, gates_pass};

#[derive(Debug, Serialize)]
pub(crate) struct SchemaOpportunity {
    pub signature: String,
    pub member_count: usize,
    pub stability_count: i64,
    pub reward_hits: i64,
    pub suggested_name: String,
    pub confirm_command: String,
}

/// Find unconfirmed, gate-passing schema candidates eligible for notification.
pub(crate) fn find_opportunities(conn: &Connection) -> Result<Vec<SchemaOpportunity>> {
    let mut stmt = conn.prepare(
        "SELECT cluster_sig, member_keys_json, density, stability_count, reward_hits, \
                first_seen_at, last_seen_at, kind, notified_at \
         FROM schema_candidates \
         WHERE kind = 'schema' \
           AND (notified_at IS NULL OR notified_at < last_seen_at) \
         ORDER BY stability_count DESC, reward_hits DESC",
    )?;

    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, f64>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, i64>(4)?,
        ))
    })?;

    let mut opportunities = Vec::new();

    for row in rows {
        let (sig, members_json, density, stability, reward) = row?;
        if !gates_pass(density, stability, reward) {
            continue;
        }

        let members: Vec<String> = serde_json::from_str(&members_json).unwrap_or_default();
        let knowledge_members: Vec<String> = members
            .iter()
            .filter(|m| !m.starts_with("schema:"))
            .cloned()
            .collect();
        // `claimed_territory` scores via `jaccard`, whose merge walk requires
        // ascending-sorted operands; filtered stored order is sorted, but the
        // precondition is pinned here rather than trusted.
        let mut knowledge_members = knowledge_members;
        knowledge_members.sort_unstable();

        // Territory check: skip if claimed by an existing confirmed schema
        if claimed_territory(conn, &knowledge_members)?.is_some() {
            continue;
        }

        let suggested_name = suggest_schema_name(conn, &members)?;

        // Take unique prefix of cluster_sig: up to first 2 members or 30 chars
        let sig_prefix = if let Some(idx) = sig.match_indices(',').nth(1) {
            &sig[..idx.0]
        } else {
            &sig
        };
        let confirm_command =
            format!("engrams schema confirm {sig_prefix} --name {suggested_name}");

        opportunities.push(SchemaOpportunity {
            signature: sig,
            member_count: members.len(),
            stability_count: stability,
            reward_hits: reward,
            suggested_name,
            confirm_command,
        });
    }

    Ok(opportunities)
}

/// Stamp `notified_at = datetime('now')` on the nominated candidates.
pub(crate) fn stamp_notified(conn: &Connection, signatures: &[&str]) -> Result<()> {
    if signatures.is_empty() {
        return Ok(());
    }
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let sql = format!(
        "UPDATE schema_candidates SET notified_at = ?1 WHERE cluster_sig IN ({})",
        crate::ops::sql_placeholders(signatures.len())
    );
    let mut params_vec = Vec::<&dyn rusqlite::ToSql>::with_capacity(signatures.len() + 1);
    params_vec.push(&now);
    for sig in signatures {
        params_vec.push(sig);
    }
    conn.execute(&sql, rusqlite::params_from_iter(params_vec))?;
    Ok(())
}

/// Propose a base kebab-case schema name from members' dominant tags and anchor
/// directories, without uniqueness suffixing (so `schema propose` can surface
/// `name_conflict` when the base name is already taken).
pub(crate) fn suggest_base_schema_name(conn: &Connection, members: &[String]) -> Result<String> {
    let mut tag_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut dir_counts: BTreeMap<String, usize> = BTreeMap::new();

    for m in members {
        let Some((kind, id_str)) = m.split_once(':') else {
            continue;
        };
        let Ok(id) = id_str.parse::<i64>() else {
            continue;
        };

        let table = match kind {
            "decision" => Some("decisions"),
            "system_pattern" => Some("system_patterns"),
            _ => None,
        };
        if let Some(table) = table {
            let tags_raw: Option<Option<String>> = conn
                .query_row(
                    &format!("SELECT tags FROM {table} WHERE id = ?1"),
                    params![id],
                    |r| r.get(0),
                )
                .ok();
            if let Some(Some(raw)) = tags_raw {
                for t in crate::models::parse_tags(Some(&raw)) {
                    *tag_counts.entry(t).or_default() += 1;
                }
            }
        }

        let mut stmt =
            conn.prepare("SELECT path FROM item_anchors WHERE item_type = ?1 AND item_id = ?2")?;
        let paths = stmt.query_map(params![kind, id], |r| r.get::<_, String>(0))?;
        for p in paths {
            let Ok(p) = p else {
                continue;
            };
            if let Some(parent) = std::path::Path::new(&p).parent() {
                if let Some(dir_name) = parent.file_name().and_then(|f| f.to_str()) {
                    if !dir_name.is_empty() && dir_name != "." && dir_name != "src" {
                        *dir_counts.entry(dir_name.to_string()).or_default() += 1;
                    }
                }
            }
        }
    }

    let dominant_tag = tag_counts
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.as_str().cmp(a.0.as_str())))
        .map(|(t, _)| t);
    let dominant_dir = dir_counts
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.as_str().cmp(a.0.as_str())))
        .map(|(d, _)| d);

    let raw_name = match (dominant_tag, dominant_dir) {
        (Some(t), _) => t,
        (None, Some(d)) => d,
        (None, None) => "proposed-schema".to_string(),
    };

    Ok(sanitize_kebab(&raw_name))
}

/// Propose a kebab-case schema name from members' dominant tags and anchor directories.
pub(crate) fn suggest_schema_name(conn: &Connection, members: &[String]) -> Result<String> {
    let kebab_base = suggest_base_schema_name(conn, members)?;
    resolve_unique_name(conn, &kebab_base)
}

fn sanitize_kebab(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_dash = true;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    if out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "schema".to_string()
    } else {
        out
    }
}

fn resolve_unique_name(conn: &Connection, base: &str) -> Result<String> {
    let mut candidate = base.to_string();
    let mut counter = 2;
    loop {
        let exists: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM schemas WHERE name = ?1",
            params![&candidate],
            |r| r.get(0),
        )?;
        if !exists {
            return Ok(candidate);
        }
        candidate = format!("{base}-{counter}");
        counter += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::schema::SCHEMA).unwrap();
        conn
    }

    /// One `schema_candidates` row shape; `Default` is a passing, unnotified
    /// `schema`-kind candidate over `decision:1`.
    struct CandidateRow<'a> {
        sig: &'a str,
        members: &'a [&'a str],
        density: f64,
        stability: i64,
        reward: i64,
        kind: &'a str,
        last_seen: &'a str,
        notified: Option<&'a str>,
    }

    impl Default for CandidateRow<'_> {
        fn default() -> Self {
            Self {
                sig: "decision:1",
                members: &["decision:1"],
                density: 0.8,
                stability: 3,
                reward: 1,
                kind: "schema",
                last_seen: "2026-10-04T12:00:00Z",
                notified: None,
            }
        }
    }

    fn seed_candidate(conn: &Connection, c: CandidateRow<'_>) {
        let members_json = serde_json::to_string(c.members).unwrap();
        conn.execute(
            "INSERT INTO schema_candidates \
             (cluster_sig, member_keys_json, density, stability_count, reward_hits, \
              first_seen_at, last_seen_at, kind, notified_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, '2026-10-01T00:00:00Z', ?6, ?7, ?8)",
            params![
                c.sig,
                members_json,
                c.density,
                c.stability,
                c.reward,
                c.last_seen,
                c.kind,
                c.notified
            ],
        )
        .unwrap();
    }

    fn seed_decision(conn: &Connection, id: i64, tags_json: &str) {
        conn.execute(
            "INSERT INTO decisions (id, uuid, timestamp, summary, tags) \
             VALUES (?1, ?2, 't', 's', ?3)",
            params![id, format!("u{id}"), tags_json],
        )
        .unwrap();
    }

    /// Insert a confirmed `schemas` row plus `member_of` context links so
    /// `confirmed_schemas` reports exactly `members`.
    fn seed_confirmed_schema(conn: &Connection, id: i64, name: &str, members: &[(&str, i64)]) {
        conn.execute(
            "INSERT INTO schemas (id, uuid, name, summary, centroid_json, created_at, updated_at) \
             VALUES (?1, ?2, ?3, 's', '{}', 't', 't')",
            params![id, format!("schema-u{id}"), name],
        )
        .unwrap();
        for (kind, member_id) in members {
            conn.execute(
                "INSERT INTO context_links \
                 (source_item_type, source_item_id, target_item_type, target_item_id, \
                  relationship_type, timestamp) \
                 VALUES (?1, ?2, 'schema', ?3, 'member_of', 't')",
                params![kind, member_id.to_string(), id.to_string()],
            )
            .unwrap();
        }
    }

    #[test]
    fn test_suggest_schema_name_prefers_dominant_tag() {
        let conn = setup_test_db();
        seed_decision(&conn, 1, r#"["auth", "jwt"]"#);
        seed_decision(&conn, 2, r#"["auth", "crypto"]"#);

        let members = vec!["decision:1".to_string(), "decision:2".to_string()];
        let name = suggest_schema_name(&conn, &members).unwrap();
        assert_eq!(name, "auth");
    }

    #[test]
    fn test_suggest_schema_name_falls_back_to_anchor_path() {
        let conn = setup_test_db();
        seed_decision(&conn, 1, "[]");
        conn.execute(
            "INSERT INTO item_anchors (item_type, item_id, path, timestamp) \
             VALUES ('decision', 1, 'src/ops/auth.rs', 't')",
            [],
        )
        .unwrap();

        let members = vec!["decision:1".to_string()];
        let name = suggest_schema_name(&conn, &members).unwrap();
        assert_eq!(name, "ops");
    }

    #[test]
    fn test_suggest_schema_name_suffixes_on_collision() {
        let conn = setup_test_db();
        seed_decision(&conn, 1, r#"["auth"]"#);
        seed_confirmed_schema(&conn, 1, "auth", &[]);
        let members = vec!["decision:1".to_string()];
        assert_eq!(suggest_schema_name(&conn, &members).unwrap(), "auth-2");

        seed_confirmed_schema(&conn, 2, "auth-2", &[]);
        assert_eq!(suggest_schema_name(&conn, &members).unwrap(), "auth-3");
    }

    #[test]
    fn test_find_opportunities_emits_qualified_candidate() {
        let conn = setup_test_db();
        seed_decision(&conn, 1, r#"["auth"]"#);
        seed_decision(&conn, 2, r#"["auth"]"#);
        seed_decision(&conn, 3, r#"["auth"]"#);
        seed_candidate(
            &conn,
            CandidateRow {
                sig: "decision:1,decision:2,decision:3",
                members: &["decision:1", "decision:2", "decision:3"],
                stability: 4,
                reward: 2,
                ..Default::default()
            },
        );

        let opps = find_opportunities(&conn).unwrap();
        assert_eq!(opps.len(), 1);
        let o = &opps[0];
        assert_eq!(o.signature, "decision:1,decision:2,decision:3");
        assert_eq!(o.member_count, 3);
        assert_eq!(o.stability_count, 4);
        assert_eq!(o.reward_hits, 2);
        assert_eq!(o.suggested_name, "auth");
        // Confirm command carries the unique two-member prefix of the signature.
        assert_eq!(
            o.confirm_command,
            "engrams schema confirm decision:1,decision:2 --name auth"
        );
    }

    #[test]
    fn test_find_opportunities_skips_gate_failures() {
        let conn = setup_test_db();
        // stability below STABILITY_GATE (3) must not qualify, even unnotified
        seed_candidate(
            &conn,
            CandidateRow {
                stability: 2,
                ..Default::default()
            },
        );
        assert!(find_opportunities(&conn).unwrap().is_empty());
    }

    #[test]
    fn test_find_opportunities_suppresses_already_notified() {
        let conn = setup_test_db();
        // notified_at == last_seen_at: already surfaced, no drift since
        seed_candidate(
            &conn,
            CandidateRow {
                notified: Some("2026-10-04T12:00:00Z"),
                ..Default::default()
            },
        );
        assert!(find_opportunities(&conn).unwrap().is_empty());
    }

    #[test]
    fn test_find_opportunities_renotifies_on_drift() {
        let conn = setup_test_db();
        // last_seen_at advanced past notified_at: the candidate drifted, resurface
        seed_candidate(
            &conn,
            CandidateRow {
                notified: Some("2026-10-04T10:00:00Z"),
                ..Default::default()
            },
        );
        let opps = find_opportunities(&conn).unwrap();
        assert_eq!(opps.len(), 1);
        assert_eq!(opps[0].signature, "decision:1");
    }

    #[test]
    fn test_find_opportunities_skips_non_schema_kinds() {
        let conn = setup_test_db();
        seed_candidate(
            &conn,
            CandidateRow {
                sig: "progress_entry:1,progress_entry:2",
                members: &["progress_entry:1", "progress_entry:2"],
                reward: 2,
                kind: "story",
                ..Default::default()
            },
        );
        assert!(find_opportunities(&conn).unwrap().is_empty());
    }

    #[test]
    fn test_find_opportunities_skips_claimed_territory() {
        let conn = setup_test_db();
        // Confirmed schema "existing" already owns decision:1
        seed_confirmed_schema(&conn, 1, "existing", &[("decision", 1)]);
        seed_candidate(
            &conn,
            CandidateRow {
                sig: "decision:1,decision:2",
                members: &["decision:1", "decision:2"],
                ..Default::default()
            },
        );
        assert!(find_opportunities(&conn).unwrap().is_empty());
    }

    #[test]
    fn test_find_opportunities_excludes_schema_refs_from_territory() {
        let conn = setup_test_db();
        // schema:1 sits inside confirmed schema "container"; the candidate's
        // knowledge members (schema: refs filtered) remain unclaimed, so the
        // candidate must still surface with its full member count.
        seed_confirmed_schema(&conn, 2, "container", &[("schema", 1)]);
        seed_candidate(
            &conn,
            CandidateRow {
                sig: "decision:1,schema:1",
                members: &["decision:1", "schema:1"],
                ..Default::default()
            },
        );
        let opps = find_opportunities(&conn).unwrap();
        assert_eq!(opps.len(), 1);
        assert_eq!(opps[0].member_count, 2);
        assert_eq!(opps[0].signature, "decision:1,schema:1");
    }

    #[test]
    fn test_stamp_notified_marks_candidates() {
        let conn = setup_test_db();
        seed_candidate(&conn, CandidateRow::default());
        seed_candidate(
            &conn,
            CandidateRow {
                sig: "decision:2",
                members: &["decision:2"],
                ..Default::default()
            },
        );

        // Empty nomination is a no-op, not a SQL error
        stamp_notified(&conn, &[]).unwrap();
        let stamped: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM schema_candidates WHERE notified_at IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(stamped, 0);

        stamp_notified(&conn, &["decision:1", "decision:2"]).unwrap();
        let stamped: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM schema_candidates WHERE notified_at IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(stamped, 2);

        // And once stamped at/after last_seen, find_opportunities suppresses them
        assert!(find_opportunities(&conn).unwrap().is_empty());
    }
}
