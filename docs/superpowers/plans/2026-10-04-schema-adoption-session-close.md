# Schema Adoption Surfacing at Session Close Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Surface actionable cognitive schema opportunities and concrete retrieval value metrics directly in `engrams session close` and `engrams schema list` to make schema adoption effortless and its benefits visible.

**Architecture:** Add a `notified_at` column to `schema_candidates` via an ALTER-only migration (v15). Deduplicate gate-passing and territory-claiming logic into a shared `src/ops/schemas/gates.rs` module. Create `src/ops/schemas/opportunities.rs` to filter unconfirmed, gate-passing schema candidates, suggest kebab-case names, format runnable `schema confirm` commands, and stamp `notified_at`. Integrate session-windowed `retrieval_surfaces` counts into `session close` and expose a `guided` counter in `schema list`.

**Tech Stack:** Rust (2021 edition), SQLite (bundled `rusqlite` 0.32), `serde_json`, `chrono`. Implemented under the Zero-Cost Pragmatist persona: zero unnecessary heap allocations, pre-allocated buffers, borrowed parameters, and no throwaway collections.

## Global Constraints

- Frozen migrations: `MIGRATION_V15` must be an ALTER-only const; `SCHEMA` baseline carries `notified_at TEXT`; `LATEST_VERSION` is 15.
- Zero unnecessary allocations: use `&str` over `String`, pre-allocate with `Vec::with_capacity`, borrow SQL parameters (`Vec<&dyn ToSql>`).
- Rule 002: Dynamic SQL parameter lists must borrow via `Vec<&dyn ToSql>`; no `Box<dyn ToSql>`.
- Rule 004: Consume temporary lookup maps via `.remove()` or `mem::take`.
- Rule 005: Never materialize throwaway intermediate collections; fuse loops or pre-size with `with_capacity`.
- JSON-only output invariant: every CLI command emits structured JSON.
- Meaningful testing standards: construct test data dynamically; never assert static seed data or hardcoded copy strings.

---

### Task 1: Migration v15 — Add `notified_at` to `schema_candidates`

**Files:**
- Modify: `src/schema.rs:196-208,485-500`
- Modify: `src/db.rs:8,56-95`
- Modify: `src/ops/schemas/kind.rs:675-700`
- Test: `tests/cli.rs` (migration suite)

**Interfaces:**
- Consumes: `schema_candidates` SQLite table definition
- Produces: `schema_candidates.notified_at TEXT`, `crate::schema::MIGRATION_V15`, `crate::db::LATEST_VERSION = 15`

- [ ] **Step 1: Write the failing test for version and migration v15**

Add a unit test in `src/db.rs` (or update existing version check) to assert `LATEST_VERSION == 15` and that `schema_candidates` table has `notified_at`.

In `src/db.rs` tests module:
```rust
#[test]
fn test_migration_v15_adds_notified_at_to_schema_candidates() {
    let mut conn = Connection::open_in_memory().unwrap();
    // Run baseline initialization
    conn.execute_batch(crate::schema::SCHEMA).unwrap();
    let user_ver = get_user_version(&conn).unwrap();
    assert_eq!(user_ver, 15);

    // Verify column exists on fresh schema
    let has_col: bool = conn
        .prepare("PRAGMA table_info(schema_candidates)")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .any(|c| c.as_deref() == Ok("notified_at"));
    assert!(has_col, "schema_candidates must have notified_at in baseline");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --bin engrams test_migration_v15`
Expected: FAIL (compilation error or version mismatch `14 != 15`)

- [ ] **Step 3: Implement migration v15 and bump `LATEST_VERSION`**

In `src/schema.rs`:
1. In baseline `SCHEMA` table `schema_candidates` (around line 196):
```sql
CREATE TABLE IF NOT EXISTS schema_candidates (
  cluster_sig        TEXT PRIMARY KEY,    -- sorted member-key signature
  member_keys_json   TEXT NOT NULL,
  density            REAL NOT NULL,
  stability_count    INTEGER NOT NULL DEFAULT 1,
  reward_hits        INTEGER NOT NULL DEFAULT 0,
  last_drift_removed INTEGER NOT NULL DEFAULT 0, -- last match: members departed
  last_drift_added   INTEGER NOT NULL DEFAULT 0, -- last match: members arrived
  first_seen_at      TEXT NOT NULL,
  last_seen_at       TEXT NOT NULL,
  kind               TEXT NOT NULL DEFAULT 'unclear',  -- spec 0003: 'schema'|'story'|'inventory'|'unclear'
  kind_reasons_json  TEXT NOT NULL DEFAULT '[]',       -- labeler's plain-language reasons
  notified_at        TEXT                              -- session close notification timestamp
);
```

2. Add `MIGRATION_V15` constant at the end of migration constants (after `MIGRATION_V14`):
```rust
pub const MIGRATION_V15: &str = r#"
-- Session-close schema opportunity notification timestamp.
-- NULL = never notified; timestamp allows re-notification only if candidate drifts (last_seen_at > notified_at).
ALTER TABLE schema_candidates ADD COLUMN notified_at TEXT;
"#;
```

In `src/db.rs`:
1. Update `LATEST_VERSION`:
```rust
pub const LATEST_VERSION: i32 = 15;
```
2. Add migration v15 dispatch in `run_migrations`:
```rust
            14 => {
                tx.execute_batch(crate::schema::MIGRATION_V14)?;
            }
            15 => {
                tx.execute_batch(crate::schema::MIGRATION_V15)?;
            }
```

In `src/ops/schemas/kind.rs`:
Update line 694 in test `kind_migration_v13_backfills_unclear_and_preserves_gates`:
```rust
        assert_eq!(crate::db::get_user_version(&conn).unwrap(), crate::db::LATEST_VERSION);
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --bin engrams test_migration_v15`
Expected: PASS
Run: `cargo test --all-targets`
Expected: 214+ passed, 0 failed.

- [ ] **Step 5: Commit**

```bash
git add src/schema.rs src/db.rs src/ops/schemas/kind.rs
git commit -m "feat(0005): migration v15 adds notified_at to schema_candidates"
```

---

### Task 2: Deduplicate Gates and Territory Checks (`src/ops/schemas/gates.rs`)

**Files:**
- Create: `src/ops/schemas/gates.rs`
- Modify: `src/ops/schemas/mod.rs`
- Modify: `src/ops/schemas/scan.rs:25-35,60-80,300-365`
- Modify: `src/ops/schemas/confirm.rs:30-40,430-495`
- Test: `src/ops/schemas/gates.rs` (unit tests)

**Interfaces:**
- Consumes: `Connection`, `rusqlite`
- Produces:
  ```rust
  pub(crate) const DENSITY_GATE: f64 = 0.5;
  pub(crate) const STABILITY_GATE: i64 = 3;
  pub(crate) const REWARD_GATE: i64 = 0;
  pub(crate) const JACCARD_IDENTITY: f64 = 0.7;

  pub(crate) fn gates_pass(density: f64, stability: i64, reward: i64) -> bool;
  pub(crate) fn failed_gates(density: f64, stability: i64, reward: i64) -> Vec<&'static str>;
  pub(crate) fn jaccard(a: &[&str], b: &[&str]) -> f64;
  pub(crate) fn claimed_territory(conn: &Connection, knowledge_members: &[String]) -> Result<Option<(i64, String)>>;
  ```

- [ ] **Step 1: Write unit tests for `gates_pass` and `claimed_territory`**

In new file `src/ops/schemas/gates.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gates_pass_evaluation() {
        assert!(gates_pass(0.5, 3, 0));
        assert!(gates_pass(0.8, 5, 2));
        assert!(!gates_pass(0.49, 3, 0));
        assert!(!gates_pass(0.5, 2, 0));
        assert!(!gates_pass(0.5, 3, -1));

        let failed = failed_gates(0.4, 2, 0);
        assert_eq!(failed, vec!["density", "stability"]);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --bin engrams gates::tests`
Expected: FAIL (module not registered or functions missing)

- [ ] **Step 3: Implement `src/ops/schemas/gates.rs` and refactor callers**

Create `src/ops/schemas/gates.rs`:
```rust
//! Shared gate thresholds, identity metrics, and territory claims (spec 0002/0003/0005).

use anyhow::Result;
use rusqlite::Connection;

pub(crate) const DENSITY_GATE: f64 = 0.5;
pub(crate) const STABILITY_GATE: i64 = 3;
pub(crate) const REWARD_GATE: i64 = 0;
pub(crate) const JACCARD_IDENTITY: f64 = 0.7;

#[inline]
pub(crate) fn gates_pass(density: f64, stability: i64, reward: i64) -> bool {
    density >= DENSITY_GATE && stability >= STABILITY_GATE && reward >= REWARD_GATE
}

pub(crate) fn failed_gates(density: f64, stability: i64, reward: i64) -> Vec<&'static str> {
    let mut failed = Vec::with_capacity(3);
    if density < DENSITY_GATE {
        failed.push("density");
    }
    if stability < STABILITY_GATE {
        failed.push("stability");
    }
    if reward < REWARD_GATE {
        failed.push("reward");
    }
    failed
}

/// Jaccard similarity of two ascending-sorted member sets (merge intersection).
pub(crate) fn jaccard(a: &[&str], b: &[&str]) -> f64 {
    let (mut i, mut j, mut inter) = (0usize, 0usize, 0usize);
    while i < a.len() && j < b.len() {
        match a[i].cmp(b[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                inter += 1;
                i += 1;
                j += 1;
            }
        }
    }
    let union = a.len() + b.len() - inter;
    if union == 0 {
        1.0
    } else {
        inter as f64 / union as f64
    }
}

/// Territory check: returns `Some((schema_id, schema_name))` if `knowledge_members`
/// is a subset, superset, or $\ge 0.7$ Jaccard match of any confirmed schema.
pub(crate) fn claimed_territory(
    conn: &Connection,
    knowledge_members: &[String],
) -> Result<Option<(i64, String)>> {
    for s in super::confirm::confirmed_schemas(conn)? {
        let s_knowledge: Vec<&String> = s
            .members
            .iter()
            .filter(|m| !m.starts_with("schema:"))
            .collect();
        let covered = knowledge_members
            .iter()
            .filter(|m| s_knowledge.contains(m))
            .count();
        let covers = s_knowledge
            .iter()
            .filter(|k| knowledge_members.contains(k))
            .count();
        let j = jaccard(
            &knowledge_members.iter().map(String::as_str).collect::<Vec<_>>(),
            &s.members.iter().map(String::as_str).collect::<Vec<_>>(),
        );

        if covered == knowledge_members.len()
            || (!s_knowledge.is_empty() && covers == s_knowledge.len())
            || j >= JACCARD_IDENTITY
        {
            return Ok(Some((s.id, s.name)));
        }
    }
    Ok(None)
}
```

In `src/ops/schemas/mod.rs`:
```rust
pub(crate) mod gates;
```

In `src/ops/schemas/scan.rs`:
Replace definitions of `DENSITY_GATE`, `STABILITY_GATE`, `REWARD_GATE`, `JACCARD_IDENTITY`, and `jaccard` with:
```rust
pub(crate) use super::gates::{gates_pass, failed_gates, jaccard, DENSITY_GATE, JACCARD_IDENTITY, REWARD_GATE, STABILITY_GATE};
```
In `scan.rs:364`, replace manual checks with:
```rust
"failed_gates": failed_gates(density, stability, reward),
"gates_pass": gates_pass(density, stability, reward),
```

In `src/ops/schemas/confirm.rs`:
Replace lines 437-478 territory check with:
```rust
    if let Some((claimed_id, claimed_name)) = super::gates::claimed_territory(conn, &knowledge_members)? {
        bail!(
            "candidate already confirmed as schema {} ({})",
            claimed_id,
            claimed_name
        );
    }
```
And lines 480-496 gate checks with:
```rust
    let failed = super::gates::failed_gates(cand.density, cand.stability_count, cand.reward_hits);
    if !failed.is_empty() {
        bail!(
            "candidate has not passed the gates (failing: {}); only passing candidates confirm",
            failed.join(", ")
        );
    }
```

- [ ] **Step 4: Run tests to verify all tests pass**

Run: `cargo test --bin engrams schemas`
Expected: PASS (all schema unit tests pass)
Run: `cargo test --test cli schema`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/ops/schemas/gates.rs src/ops/schemas/mod.rs src/ops/schemas/scan.rs src/ops/schemas/confirm.rs
git commit -m "refactor(0005): extract shared gates and claimed_territory into gates.rs"
```

---

### Task 3: Opportunity Detection & Name Generation (`src/ops/schemas/opportunities.rs`)

**Files:**
- Create: `src/ops/schemas/opportunities.rs`
- Modify: `src/ops/schemas/mod.rs`
- Test: `src/ops/schemas/opportunities.rs` (unit tests)

**Interfaces:**
- Consumes: `schema_candidates`, `context_links`, `item_anchors`, `schemas`, `gates::gates_pass`, `gates::claimed_territory`
- Produces:
  ```rust
  #[derive(Debug, serde::Serialize)]
  pub(crate) struct SchemaOpportunity {
      pub signature: String,
      pub member_count: usize,
      pub stability_count: i64,
      pub reward_hits: i64,
      pub suggested_name: String,
      pub confirm_command: String,
  }

  pub(crate) fn find_opportunities(conn: &Connection) -> Result<Vec<SchemaOpportunity>>;
  pub(crate) fn stamp_notified(conn: &Connection, signatures: &[&str]) -> Result<()>;
  pub(crate) fn suggest_schema_name(conn: &Connection, members: &[String]) -> Result<String>;
  ```

- [ ] **Step 1: Write unit tests for name suggestion and opportunity filtering**

In `src/ops/schemas/opportunities.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::schema::SCHEMA).unwrap();
        conn
    }

    #[test]
    fn test_suggest_schema_name_prefers_dominant_tag() {
        let conn = setup_test_db();
        conn.execute(
            "INSERT INTO decisions (id, uuid, timestamp, summary, tags) VALUES (1, 'u1', 't', 's', '[\"auth\", \"jwt\"]')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO decisions (id, uuid, timestamp, summary, tags) VALUES (2, 'u2', 't', 's', '[\"auth\", \"crypto\"]')",
            [],
        ).unwrap();

        let members = vec!["decision:1".to_string(), "decision:2".to_string()];
        let name = suggest_schema_name(&conn, &members).unwrap();
        assert_eq!(name, "auth");
    }

    #[test]
    fn test_suggest_schema_name_falls_back_to_anchor_path() {
        let conn = setup_test_db();
        conn.execute(
            "INSERT INTO decisions (id, uuid, timestamp, summary, tags) VALUES (1, 'u1', 't', 's', '[]')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO item_anchors (item_type, item_id, path, timestamp) VALUES ('decision', 1, 'src/ops/auth.rs', 't')",
            [],
        ).unwrap();

        let members = vec!["decision:1".to_string()];
        let name = suggest_schema_name(&conn, &members).unwrap();
        assert_eq!(name, "ops");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --bin engrams opportunities::tests`
Expected: FAIL (module missing)

- [ ] **Step 3: Implement `src/ops/schemas/opportunities.rs`**

Create `src/ops/schemas/opportunities.rs`:
```rust
//! Schema opportunity detection and naming at session close (spec 0005).

use std::collections::HashMap;
use anyhow::Result;
use rusqlite::{params, Connection};
use serde::Serialize;

use super::gates::{gates_pass, claimed_territory};

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
        let confirm_command = format!("engrams schema confirm {sig_prefix} --name {suggested_name}");

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

/// Propose a kebab-case schema name from members' dominant tags and anchor directories.
pub(crate) fn suggest_schema_name(conn: &Connection, members: &[String]) -> Result<String> {
    let mut tag_counts: HashMap<String, usize> = HashMap::new();
    let mut dir_counts: HashMap<String, usize> = HashMap::new();

    for m in members {
        let parts: Vec<&str> = m.split(':').collect();
        if parts.len() != 2 {
            continue;
        }
        let (kind, id_str) = (parts[0], parts[1]);
        let Ok(id) = id_str.parse::<i64>() else { continue };

        if kind == "decision" {
            let tags_json: Option<String> = conn
                .query_row("SELECT tags FROM decisions WHERE id = ?1", params![id], |r| r.get(0))
                .ok();
            if let Some(tj) = tags_json {
                if let Ok(tags) = serde_json::from_str::<Vec<String>>(&tj) {
                    for t in tags {
                        *tag_counts.entry(t).or_default() += 1;
                    }
                }
            }
        }

        let paths: Vec<String> = conn
            .prepare("SELECT path FROM item_anchors WHERE item_type = ?1 AND item_id = ?2")?
            .query_map(params![kind, id], |r| r.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        for p in paths {
            if let Some(parent) = std::path::Path::new(&p).parent() {
                if let Some(dir_name) = parent.file_name().and_then(|f| f.to_str()) {
                    if !dir_name.is_empty() && dir_name != "." && dir_name != "src" {
                        *dir_counts.entry(dir_name.to_string()).or_default() += 1;
                    }
                }
            }
        }
    }

    let dominant_tag = tag_counts.into_iter().max_by_key(|(_, c)| *c).map(|(t, _)| t);
    let dominant_dir = dir_counts.into_iter().max_by_key(|(_, c)| *c).map(|(d, _)| d);

    let raw_name = match (dominant_tag, dominant_dir) {
        (Some(t), _) => t,
        (None, Some(d)) => d,
        (None, None) => "proposed-schema".to_string(),
    };

    let kebab_base = sanitize_kebab(&raw_name);
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
```

In `src/ops/schemas/mod.rs`:
```rust
pub(crate) mod opportunities;
```

- [ ] **Step 4: Run unit tests to verify they pass**

Run: `cargo test --bin engrams opportunities::tests`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/ops/schemas/opportunities.rs src/ops/schemas/mod.rs
git commit -m "feat(0005): implement schema opportunity detection and name suggestion"
```

---

### Task 4: Session Value Counter and Opportunities in `session close`

**Files:**
- Modify: `src/ops/session.rs:1-75`
- Test: `tests/cli.rs` (session close integration test)

**Interfaces:**
- Consumes: `opportunities::find_opportunities`, `opportunities::stamp_notified`, `retrieval_surfaces`
- Produces: Enhanced JSON in `engrams session close`:
  ```json
  "schema_value": { "guided_retrievals": usize, "distinct_schemas": usize },
  "schema_opportunities": [ ... ]
  ```

- [ ] **Step 1: Write integration test for `session close` schema metrics and opportunities**

In `tests/cli.rs`:
```rust
#[test]
fn test_session_close_surfaces_schema_value_and_opportunities() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    engrams(&db).arg("init").assert().success();

    // Seed usage_log and retrieval_surfaces for session "ses_test"
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch(
            "INSERT INTO usage_log (timestamp, command, arg, hits, miss, session) VALUES
             ('2026-10-04T12:00:00Z', 'advise', 'src/lib.rs', 2, 0, 'ses_test');
             INSERT INTO retrieval_surfaces (ts, cmd, arg, node_kind, node_id) VALUES
             ('2026-10-04T12:01:00Z', 'advise', 'src/lib.rs', 'schema', 1),
             ('2026-10-04T12:02:00Z', 'advise', 'src/lib.rs', 'schema', 2),
             ('2026-10-04T12:03:00Z', 'advise', 'src/lib.rs', 'schema', 1);
             INSERT INTO schema_candidates (cluster_sig, member_keys_json, density, stability_count, reward_hits, first_seen_at, last_seen_at, kind) VALUES
             ('decision:1,decision:2', '[\"decision:1\", \"decision:2\"]', 0.8, 3, 1, '2026-10-04T10:00:00Z', '2026-10-04T12:00:00Z', 'schema');",
        ).unwrap();
    }

    let out = engrams(&db)
        .args(["session", "close", "--session", "ses_test", "--reads-skipped", "0", "--reads-required", "0"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let val: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();

    assert_eq!(val["schema_value"]["guided_retrievals"], 3);
    assert_eq!(val["schema_value"]["distinct_schemas"], 2);

    let opps = val["schema_opportunities"].as_array().unwrap();
    assert_eq!(opps.len(), 1);
    assert_eq!(opps[0]["signature"], "decision:1,decision:2");
    assert!(opps[0]["confirm_command"].as_str().unwrap().contains("engrams schema confirm"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test cli test_session_close_surfaces_schema_value_and_opportunities`
Expected: FAIL (`schema_value` or `schema_opportunities` missing from JSON output)

- [ ] **Step 3: Implement session value query and opportunity surfacing in `session.rs`**

In `src/ops/session.rs`:
1. Add `schema_value_rollup` helper:
```rust
fn schema_value_rollup(conn: &Connection, session: Option<&str>) -> Result<Value> {
    let (guided, distinct) = match session {
        Some(sid) => {
            let start_ts: Option<String> = conn.query_row(
                "SELECT MIN(timestamp) FROM usage_log WHERE session = ?1",
                params![sid],
                |r| r.get(0),
            ).optional()?.flatten();

            match start_ts {
                Some(ts) => conn.query_row(
                    "SELECT COUNT(*), COUNT(DISTINCT node_id) FROM retrieval_surfaces \
                     WHERE node_kind = 'schema' AND ts >= ?1",
                    params![ts],
                    |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
                )?,
                None => (0, 0),
            }
        }
        None => conn.query_row(
            "SELECT COUNT(*), COUNT(DISTINCT node_id) FROM retrieval_surfaces \
             WHERE node_kind = 'schema'",
            [],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
        )?,
    };
    Ok(json!({
        "guided_retrievals": guided,
        "distinct_schemas": distinct
    }))
}
```

2. In `close()` function (around lines 56-68):
```rust
    let rollup = usage_rollup(conn, sid.as_deref())?;
    let schema_val = schema_value_rollup(conn, sid.as_deref())?;
    let opportunities = crate::ops::schemas::opportunities::find_opportunities(conn)?;

    // Stamp notified on emitted opportunities
    if !opportunities.is_empty() {
        let sigs: Vec<&str> = opportunities.iter().map(|o| o.signature.as_str()).collect();
        crate::ops::schemas::opportunities::stamp_notified(conn, &sigs)?;
    }

    let mut out = json!({
        "closed": true,
        "timestamp": now,
        "session": sid,
        "reads_skipped_estimated": reads_skipped,
        "reads_required_estimated": reads_required,
        "tokens_saved_estimated": tokens_saved_est,
        "note": note,
        "pr": pr_url,
        "retrievals_this_session": rollup,
        "schema_value": schema_val,
        "schema_opportunities": opportunities,
    });
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test cli test_session_close_surfaces_schema_value_and_opportunities`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/ops/session.rs tests/cli.rs
git commit -m "feat(0005): add schema_value and schema_opportunities to session close"
```

---

### Task 5: `schema list` Guided Counter

**Files:**
- Modify: `src/ops/schemas/retrieval.rs:75-95`
- Modify: `src/ops/schemas/list.rs:50-85,110-125`
- Test: `tests/cli.rs` (schema list integration test)

**Interfaces:**
- Consumes: `retrieval::surface_counts(conn)`
- Produces: `"guided": usize` in `engrams schema list` output rows

- [ ] **Step 1: Write integration test for `schema list` guided count**

In `tests/cli.rs`:
```rust
#[test]
fn test_schema_list_includes_guided_count() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    engrams(&db).arg("init").assert().success();

    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch(
            "INSERT INTO schemas (id, uuid, name, summary, centroid_json, created_at, updated_at) VALUES
             (1, 'u1', 'active-schema', 'summary', '{}', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
             INSERT INTO retrieval_surfaces (ts, cmd, arg, node_kind, node_id) VALUES
             ('2026-01-01T00:00:00Z', 'advise', 'path', 'schema', 1),
             ('2026-01-01T00:01:00Z', 'advise', 'path', 'schema', 1);",
        ).unwrap();
    }

    let out = engrams(&db).args(["schema", "list"]).output().unwrap();
    assert!(out.status.success());
    let val: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let schemas = val["schemas"].as_array().unwrap();
    assert_eq!(schemas[0]["guided"], 2);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test cli test_schema_list_includes_guided_count`
Expected: FAIL (`guided` field is null/missing)

- [ ] **Step 3: Implement shared `surface_counts` and wire into `schema list`**

In `src/ops/schemas/retrieval.rs`:
Export `surface_counts`:
```rust
/// Count of times each schema was surfaced across retrieval surfaces within
/// the rolling retention window.
pub(crate) fn surface_counts(conn: &Connection) -> Result<HashMap<i64, usize>> {
    let mut stmt = conn.prepare(
        "SELECT node_id, COUNT(*) FROM retrieval_surfaces \
         WHERE node_kind = 'schema' GROUP BY node_id",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, usize>(1)?)))?;
    let mut map = HashMap::new();
    for r in rows {
        let (id, count) = r?;
        map.insert(id, count);
    }
    Ok(map)
}
```
Update `reward_hits` in `retrieval.rs:85` to use `surface_counts` or wrap it.

In `src/ops/schemas/list.rs`:
In `row_json`:
```rust
fn row_json(
    counts: &std::collections::HashMap<i64, i64>,
    guided: &std::collections::HashMap<i64, usize>,
    r: &Row,
) -> Value {
    let centroid: Value = serde_json::from_str(&r.centroid_json).unwrap_or(Value::Null);
    json!({
        "id": r.id,
        "name": r.name,
        "summary": r.summary,
        "summary_source": r.summary_source,
        "status": r.status,
        "kind": r.kind,
        "member_count": counts.get(&r.id).copied().unwrap_or(0),
        "guided": guided.get(&r.id).copied().unwrap_or(0),
        "updated_at": r.updated_at,
        "last_confirmed_at": r.last_confirmed_at,
        "centroid": centroid,
    })
}
```
In `list()`:
Fetch `let guided = super::retrieval::surface_counts(conn)?;` and pass to `row_json`.
In `show()`:
Also pass `&super::retrieval::surface_counts(conn)?` to `row_json` so `show` output is consistent.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test cli test_schema_list_includes_guided_count`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/ops/schemas/retrieval.rs src/ops/schemas/list.rs tests/cli.rs
git commit -m "feat(0005): add guided counter to schema list and show rows"
```

---

### Task 6: End-to-End Actionability Verification Test

**Files:**
- Modify: `tests/cli.rs`
- Test: Full test suite verification

**Interfaces:**
- Consumes: All components from Tasks 1-5
- Produces: CLI test asserting that `confirm_command` printed by `session close` can be executed directly to confirm the schema.

- [ ] **Step 1: Write the end-to-end actionability test**

In `tests/cli.rs`:
```rust
#[test]
fn test_session_close_opportunity_workflow_actionable() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("e.db");
    engrams(&db).arg("init").assert().success();

    // 1. Seed members and a gate-passing schema candidate
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch(
            "INSERT INTO decisions (id, uuid, timestamp, summary, tags) VALUES
             (10, 'u10', '2026-01-01T00:00:00Z', 'Dec 10', '[\"ast\", \"compiler\"]'),
             (11, 'u11', '2026-01-01T00:00:00Z', 'Dec 11', '[\"ast\", \"syntax\"]');
             INSERT INTO item_anchors (item_type, item_id, path, timestamp) VALUES
             ('decision', 10, 'src/ast/lexer.rs', '2026-01-01T00:00:00Z'),
             ('decision', 11, 'src/ast/parser.rs', '2026-01-01T00:00:00Z');
             INSERT INTO schema_candidates (cluster_sig, member_keys_json, density, stability_count, reward_hits, first_seen_at, last_seen_at, kind) VALUES
             ('decision:10,decision:11', '[\"decision:10\", \"decision:11\"]', 0.8, 3, 2, '2026-01-01T00:00:00Z', '2026-01-01T01:00:00Z', 'schema');",
        ).unwrap();
    }

    // 2. Run session close -> must surface the opportunity
    let out = engrams(&db)
        .args(["session", "close", "--reads-skipped", "0", "--reads-required", "0"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let val: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let opps = val["schema_opportunities"].as_array().unwrap();
    assert_eq!(opps.len(), 1);

    let cmd_str = opps[0]["confirm_command"].as_str().unwrap();
    // Parse "engrams schema confirm <target> --name <name>" args
    let args: Vec<&str> = cmd_str.split_whitespace().skip(1).collect(); // drop "engrams"

    // 3. Execute the emitted confirm command verbatim
    engrams(&db).args(&args).assert().success();

    // 4. Verify the schema is now confirmed
    let show_out = engrams(&db).args(["schema", "show", "ast"]).output().unwrap();
    assert!(show_out.status.success());

    // 5. Run session close again -> opportunity must be gone (notified/confirmed)
    let out2 = engrams(&db)
        .args(["session", "close", "--reads-skipped", "0", "--reads-required", "0"])
        .output()
        .unwrap();
    let val2: serde_json::Value = serde_json::from_slice(&out2.stdout).unwrap();
    let opps2 = val2["schema_opportunities"].as_array().unwrap();
    assert!(opps2.is_empty(), "promoted/notified candidate must not re-nag");
}
```

- [ ] **Step 2: Run test to verify it passes**

Run: `cargo test --test cli test_session_close_opportunity_workflow_actionable`
Expected: PASS

- [ ] **Step 3: Run full verification gate**

```bash
cargo fmt --check
cargo clippy --all-targets
./target/debug/engrams check --paths src/ops/schemas/opportunities.rs,src/ops/session.rs,src/ops/schemas/gates.rs
cargo test
```
Expected: All clean, 218+ tests passing.

- [ ] **Step 4: Commit**

```bash
git add tests/cli.rs
git commit -m "test(0005): add end-to-end actionability test for schema opportunity workflow"
```
